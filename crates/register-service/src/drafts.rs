//! Durable cart lifecycles. A single revision fences all active/parked mutations.
//! Operation keys retain intent, not response snapshots: retries always read current state.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DraftSnapshot {
    pub line_id: String,
    pub item: Option<CatalogItem>,
    pub group: Option<GroupConfig>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredDraft {
    pub id: String,
    pub lines: Vec<CartInputLine>,
    pub snapshots: Vec<DraftSnapshot>,
    pub currency: CurrencyDto,
    pub label: Option<String>,
    pub parked_at_unix: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DraftMetadata {
    pub revision: String,
    pub next_id: String,
    pub guarded: bool,
    pub active_id: Option<String>,
    pub guarded_since_revision: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DraftRecord {
    pub id: String,
    pub status: String,
    pub data_json: String,
    pub sale_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DraftOperation {
    pub idempotency_key: String,
    pub command: String,
    pub request_json: String,
    pub applied_revision: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DraftBackup {
    pub metadata: DraftMetadata,
    pub records: Vec<DraftRecord>,
    pub operations: Vec<DraftOperation>,
}

pub(super) fn metadata(conn: &Connection) -> Result<DraftMetadata, ServiceError> {
    Ok(conn.query_row(
        "SELECT revision,next_id,guarded,active_id,guarded_since_revision FROM draft_metadata WHERE singleton=1",
        [],
        |r| {
            Ok(DraftMetadata {
                revision: r.get::<_, i64>(0)?.to_string(),
                next_id: r.get::<_, i64>(1)?.to_string(),
                guarded: r.get(2)?,
                active_id: r.get(3)?,
                guarded_since_revision: r.get::<_, Option<i64>>(4)?.map(|n| n.to_string()),
            })
        },
    )?)
}
fn record(conn: &Connection, id: &str) -> Result<(String, StoredDraft), ServiceError> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT status,data_json FROM draft_records WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (status, raw) = row.ok_or_else(|| {
        ServiceError::new(
            "DRAFT_NOT_FOUND",
            "This cart is no longer available. Reload the parked carts.",
        )
    })?;
    Ok((status, decode_stored(&raw)?))
}
fn active(conn: &Connection, meta: &DraftMetadata) -> Result<Option<StoredDraft>, ServiceError> {
    meta.active_id
        .as_deref()
        .map(|id| {
            let (status, draft) = record(conn, id)?;
            if status != "active" {
                return Err(ServiceError::new(
                    "STORAGE_ERROR",
                    "Saved cart identity is inconsistent.",
                ));
            }
            Ok(draft)
        })
        .transpose()
}
fn current_cart(conn: &Connection) -> Result<CartInput, ServiceError> {
    let raw: Option<String> = conn
        .query_row("SELECT draft_json FROM drafts WHERE singleton=1", [], |r| {
            r.get(0)
        })
        .optional()?;
    raw.map(|s| decode_stored(&s))
        .transpose()
        .map(|v| v.unwrap_or(CartInput { lines: vec![] }))
}
fn write_cart(conn: &Connection, lines: &[CartInputLine]) -> Result<(), ServiceError> {
    conn.execute("INSERT INTO drafts(singleton,draft_json) VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET draft_json=excluded.draft_json",[serialize(&CartInput {lines:lines.to_vec()})?])?;
    Ok(())
}
fn write_record(
    conn: &Connection,
    status: &str,
    draft: &StoredDraft,
    sale_id: Option<i64>,
) -> Result<(), ServiceError> {
    conn.execute("INSERT INTO draft_records(id,status,data_json,sale_id) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET status=excluded.status,data_json=excluded.data_json,sale_id=excluded.sale_id",params![draft.id,status,serialize(draft)?,sale_id])?;
    Ok(())
}
fn next_revision(meta: &DraftMetadata) -> Result<i64, ServiceError> {
    meta.revision
        .parse::<i64>()
        .ok()
        .and_then(|r| r.checked_add(1))
        .ok_or_else(|| ServiceError::new("STORAGE_ERROR", "Cart revision limit reached."))
}
fn write_metadata(conn: &Connection, meta: &DraftMetadata) -> Result<(), ServiceError> {
    conn.execute("UPDATE draft_metadata SET revision=?1,next_id=?2,guarded=?3,active_id=?4,guarded_since_revision=?5 WHERE singleton=1",params![meta.revision,meta.next_id,meta.guarded,meta.active_id,meta.guarded_since_revision])?;
    Ok(())
}
fn check_revision(meta: &DraftMetadata, expected: &str) -> Result<(), ServiceError> {
    let revision = parse_integer(expected, "Expected draft revision")?;
    if revision.to_string() != expected {
        return Err(ServiceError::new(
            "INVALID_INPUT",
            "Expected draft revision must use canonical decimal digits without leading zeros.",
        ));
    }
    if revision.to_string() != meta.revision {
        return Err(ServiceError::new("STALE_DRAFT","The cart changed in another window. Reload and review the current cart; this request was not applied."));
    }
    Ok(())
}
fn operation_seen(
    conn: &Connection,
    key: &str,
    command: &str,
    request: &str,
) -> Result<bool, ServiceError> {
    validate_id(key, "Cart operation key")?;
    let old: Option<(String, String)> = conn
        .query_row(
            "SELECT command,request_json FROM draft_operations WHERE idempotency_key=?1",
            [key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match old {
        Some((c,r)) if c==command && r==request=>Ok(true),
        Some(_)=>Err(ServiceError::new("IDEMPOTENCY_CONFLICT","This cart operation key belongs to a different request. Reconcile the original request before continuing.")),
        None=>Ok(false),
    }
}
fn finish_operation(
    conn: &Connection,
    meta: &mut DraftMetadata,
    key: &str,
    command: &str,
    request: &str,
) -> Result<(), ServiceError> {
    meta.revision = next_revision(meta)?.to_string();
    if !meta.guarded {
        meta.guarded_since_revision = Some(meta.revision.clone());
    }
    meta.guarded = true;
    write_metadata(conn, meta)?;
    conn.execute("INSERT INTO draft_operations(idempotency_key,command,request_json,applied_revision) VALUES(?1,?2,?3,?4)",params![key,command,request,meta.revision])?;
    Ok(())
}
fn validate_label(label: Option<&str>) -> Result<(), ServiceError> {
    if label.is_some_and(|s| {
        s.trim() != s || s.is_empty() || s.chars().count() > 80 || s.chars().any(char::is_control)
    }) {
        return Err(ServiceError::new("INVALID_INPUT","Use a nonempty cart label of at most 80 characters, without surrounding whitespace or controls. Do not include customer or personal information."));
    }
    Ok(())
}
fn now() -> Result<String, ServiceError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| {
            ServiceError::new(
                "CLOCK_ERROR",
                "The computer clock is before the Unix epoch.",
            )
        })?
        .as_secs()
        .to_string())
}
fn quantity_kind(q: &InputQuantity) -> &'static str {
    match q {
        InputQuantity::Measured { .. } => "measured",
        InputQuantity::Prepacked { .. } => "prepacked",
        InputQuantity::Unit { .. } => "unit",
    }
}
fn snapshot(line: &CartInputLine, config: &Config) -> DraftSnapshot {
    let item = config.catalog.iter().find(|i| i.sku == line.sku).cloned();
    let group = item.as_ref().and_then(|i| match &i.pricing {
        CatalogPricing::WeightGroup { group_id } => {
            config.groups.iter().find(|g| &g.id == group_id).cloned()
        }
        _ => None,
    });
    DraftSnapshot {
        line_id: line.id.clone(),
        item,
        group,
    }
}
fn preview(draft: &StoredDraft, config: &Config, revision: i64) -> ParkedDraftPreview {
    let mut issues = vec![];
    for line in &draft.lines {
        let mut add = |code: &str, message: String, blocking: bool| {
            issues.push(DraftIssue {
                line_id: line.id.clone(),
                code: code.into(),
                message,
                blocking,
            })
        };
        let quantity_valid = match &line.quantity {
            InputQuantity::Measured { grams } => {
                core::Weight::from_grams_str(grams).is_ok_and(|w| w.milligrams() > 0)
            }
            InputQuantity::Prepacked { count } | InputQuantity::Unit { count } => {
                parse_integer(count, "Quantity").is_ok_and(|n| n > 0)
            }
        };
        if !quantity_valid {
            add("INVALID_QUANTITY", "Enter a positive quantity in the original quantity type. The saved text has not been changed.".into(), true);
        }
        let Some(item) = config.catalog.iter().find(|i| i.sku == line.sku) else {
            add(
                "PRODUCT_MISSING",
                format!(
                    "{} is no longer in the catalog. Remove this line before checkout.",
                    line.sku
                ),
                true,
            );
            continue;
        };
        if item.archived {
            add(
                "PRODUCT_ARCHIVED",
                format!(
                    "{} is archived. Remove it or unarchive it before checkout.",
                    item.name
                ),
                true,
            );
        }
        if quantity_kind(&line.quantity) != item.kind {
            add("PRODUCT_TYPE_CHANGED",format!("{} now uses a different quantity type. The original quantity was preserved; remove this line and add the current product if needed.",item.name),true);
        }
        if let Some(original) = draft.snapshots.iter().find(|s| s.line_id == line.id) {
            if let Some(old) = &original.item {
                let old_group = &original.group;
                let new_group = match &item.pricing {
                    CatalogPricing::WeightGroup { group_id } => {
                        config.groups.iter().find(|g| &g.id == group_id)
                    }
                    _ => None,
                };
                if old.pricing != item.pricing || old_group.as_ref() != new_group {
                    add("PRICE_CHANGED",format!("{} has changed prices or pricing rules. The preview uses current prices.",item.name),false);
                }
                if old.name != item.name
                    || old.barcode != item.barcode
                    || old.kind != item.kind
                    || old.declared_content_grams != item.declared_content_grams
                {
                    add("PRODUCT_CHANGED",format!("{} has changed product details. Review the current product before checkout.",item.name),false);
                }
            } else {
                add("PRODUCT_CHANGED",format!("{} was not in the catalog when this line was saved. Review the current product.",item.name),false);
            }
        }
    }
    if draft.currency != config.currency {
        issues.push(DraftIssue{line_id:String::new(),code:"CURRENCY_CHANGED".into(),message:"The currency or precision changed. This preview uses the current currency, not a converted historic price.".into(),blocking:false});
    }
    let quote = if issues.iter().any(|i| i.blocking) {
        None
    } else {
        match quote_using_config(&draft.lines, config, revision) {
            Ok(q) => Some(q),
            Err(e) => {
                issues.push(DraftIssue {
                    line_id: String::new(),
                    code: "PRICE_ERROR".into(),
                    message: e.message,
                    blocking: true,
                });
                None
            }
        }
    };
    ParkedDraftPreview {
        draft_id: draft.id.clone(),
        label: draft.label.clone(),
        lines: draft.lines.clone(),
        config_revision: revision.to_string(),
        currency: config.currency.clone(),
        quote,
        issues,
    }
}

pub(super) fn state_fields(
    conn: &Connection,
    config: Option<&Config>,
    revision: i64,
) -> Result<(String, Option<ActiveDraft>, Vec<ParkedDraftSummary>), ServiceError> {
    let meta = metadata(conn)?;
    let active = active(conn, &meta)?.map(|draft| ActiveDraft {
        id: draft.id.clone(),
        revision: meta.revision.clone(),
        issues: config
            .map(|c| preview(&draft, c, revision).issues)
            .unwrap_or_default(),
    });
    let mut stmt=conn.prepare("SELECT data_json FROM draft_records WHERE status='parked' ORDER BY CAST(json_extract(data_json,'$.parked_at_unix') AS INTEGER) DESC,id")?;
    let raw = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let parked = raw
        .into_iter()
        .map(|s| {
            let d: StoredDraft = decode_stored(&s)?;
            Ok(ParkedDraftSummary {
                id: d.id,
                label: d.label,
                parked_at_unix: d.parked_at_unix.ok_or_else(|| {
                    ServiceError::new("STORAGE_ERROR", "Parked cart timestamp is missing.")
                })?,
                line_count: d.lines.len(),
            })
        })
        .collect::<Result<Vec<_>, ServiceError>>()?;
    Ok((meta.revision, active, parked))
}

impl RegisterService {
    pub fn save_active_draft(
        &mut self,
        input: SaveActiveDraftInput,
    ) -> Result<State, ServiceError> {
        validate_draft(&input.lines)?;
        let request = serialize(&input)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if operation_seen(&tx, &input.idempotency_key, "save_active_draft", &request)? {
            tx.commit()?;
            return self.state();
        }
        let mut meta = metadata(&tx)?;
        check_revision(&meta, &input.expected_draft_revision)?;
        let existing = active(&tx, &meta)?;
        if input.lines.is_empty() {
            if let Some(d) = existing {
                write_record(&tx, "discarded", &d, None)?;
            }
            meta.active_id = None;
        } else {
            let (_, config) = read_config(&tx)?;
            let mut draft = if let Some(d) = existing {
                d
            } else {
                let n = meta
                    .next_id
                    .parse::<i64>()
                    .map_err(|_| ServiceError::new("STORAGE_ERROR", "Invalid cart sequence."))?;
                meta.next_id = n
                    .checked_add(1)
                    .ok_or_else(|| {
                        ServiceError::new("STORAGE_ERROR", "Cart sequence limit reached.")
                    })?
                    .to_string();
                StoredDraft {
                    id: format!("draft-{n}"),
                    lines: vec![],
                    snapshots: vec![],
                    currency: config.currency.clone(),
                    label: None,
                    parked_at_unix: None,
                }
            };
            draft.snapshots = input
                .lines
                .iter()
                .map(|line| {
                    draft
                        .lines
                        .iter()
                        .find(|old| {
                            old.id == line.id
                                && old.sku == line.sku
                                && quantity_kind(&old.quantity) == quantity_kind(&line.quantity)
                        })
                        .and_then(|_| draft.snapshots.iter().find(|s| s.line_id == line.id))
                        .cloned()
                        .unwrap_or_else(|| snapshot(line, &config))
                })
                .collect();
            draft.lines = input.lines.clone();
            meta.active_id = Some(draft.id.clone());
            write_record(&tx, "active", &draft, None)?;
        }
        write_cart(&tx, &input.lines)?;
        finish_operation(
            &tx,
            &mut meta,
            &input.idempotency_key,
            "save_active_draft",
            &request,
        )?;
        tx.commit()?;
        self.state()
    }
    pub fn park_draft(&mut self, input: ParkDraftInput) -> Result<State, ServiceError> {
        validate_label(input.label.as_deref())?;
        let request = serialize(&input)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if operation_seen(&tx, &input.idempotency_key, "park_draft", &request)? {
            tx.commit()?;
            return self.state();
        }
        let mut meta = metadata(&tx)?;
        check_revision(&meta, &input.expected_draft_revision)?;
        let mut draft = active(&tx, &meta)?.ok_or_else(|| {
            ServiceError::new(
                "EMPTY_DRAFT",
                "Save a nonempty active cart before parking it.",
            )
        })?;
        if draft.lines.is_empty() {
            return Err(ServiceError::new(
                "EMPTY_DRAFT",
                "An empty cart cannot be parked.",
            ));
        }
        draft.label = input.label.clone();
        draft.parked_at_unix = Some(now()?);
        write_record(&tx, "parked", &draft, None)?;
        meta.active_id = None;
        write_cart(&tx, &[])?;
        finish_operation(
            &tx,
            &mut meta,
            &input.idempotency_key,
            "park_draft",
            &request,
        )?;
        tx.commit()?;
        self.state()
    }
    pub fn preview_parked_draft(&self, id: &str) -> Result<ParkedDraftPreview, ServiceError> {
        validate_id(id, "Draft ID")?;
        let tx = self.conn.unchecked_transaction()?;
        let (status, draft) = record(&tx, id)?;
        if status != "parked" {
            return Err(ServiceError::new(
                "DRAFT_NOT_FOUND",
                "This cart is not parked anymore. Reload the list.",
            ));
        }
        let (revision, config) = read_config(&tx)?;
        let result = preview(&draft, &config, revision);
        tx.commit()?;
        Ok(result)
    }
    pub fn resume_parked_draft(
        &mut self,
        input: ResumeParkedDraftInput,
    ) -> Result<State, ServiceError> {
        validate_id(&input.draft_id, "Draft ID")?;
        let request = serialize(&input)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if operation_seen(&tx, &input.idempotency_key, "resume_parked_draft", &request)? {
            tx.commit()?;
            return self.state();
        }
        let mut meta = metadata(&tx)?;
        check_revision(&meta, &input.expected_draft_revision)?;
        if !current_cart(&tx)?.lines.is_empty() || meta.active_id.is_some() {
            return Err(ServiceError::new(
                "ACTIVE_DRAFT_NOT_EMPTY",
                "Park or clear the current cart before resuming another. Nothing was overwritten.",
            ));
        }
        let (revision, _) = read_config(&tx)?;
        let expected_config = parse_integer(
            &input.expected_config_revision,
            "Expected configuration revision",
        )?;
        if expected_config.to_string() != input.expected_config_revision {
            return Err(ServiceError::new("INVALID_INPUT", "Expected configuration revision must use canonical decimal digits without leading zeros."));
        }
        if expected_config != revision as u64 {
            return Err(ServiceError::new(
                "STALE_QUOTE",
                "The catalog changed since preview. Review a new preview before resuming.",
            ));
        }
        let (status, draft) = record(&tx, &input.draft_id)?;
        if status != "parked" {
            return Err(ServiceError::new(
                "DRAFT_NOT_FOUND",
                "This cart is not parked anymore. Reload the list.",
            ));
        }
        meta.active_id = Some(draft.id.clone());
        write_record(&tx, "active", &draft, None)?;
        write_cart(&tx, &draft.lines)?;
        finish_operation(
            &tx,
            &mut meta,
            &input.idempotency_key,
            "resume_parked_draft",
            &request,
        )?;
        tx.commit()?;
        self.state()
    }
    pub fn discard_parked_draft(
        &mut self,
        input: DiscardParkedDraftInput,
    ) -> Result<State, ServiceError> {
        validate_id(&input.draft_id, "Draft ID")?;
        let request = serialize(&input)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if operation_seen(
            &tx,
            &input.idempotency_key,
            "discard_parked_draft",
            &request,
        )? {
            tx.commit()?;
            return self.state();
        }
        let mut meta = metadata(&tx)?;
        check_revision(&meta, &input.expected_draft_revision)?;
        let (status, draft) = record(&tx, &input.draft_id)?;
        if status != "parked" {
            return Err(ServiceError::new(
                "DRAFT_NOT_FOUND",
                "This cart is not parked anymore. Reload the list.",
            ));
        }
        write_record(&tx, "discarded", &draft, None)?;
        finish_operation(
            &tx,
            &mut meta,
            &input.idempotency_key,
            "discard_parked_draft",
            &request,
        )?;
        tx.commit()?;
        self.state()
    }
}

pub(super) fn check_checkout(conn: &Connection, input: &CheckoutInput) -> Result<(), ServiceError> {
    let meta = metadata(conn)?;
    if !meta.guarded && input.expected_draft_id.is_none() && input.expected_draft_revision.is_none()
    {
        return Ok(());
    }
    let (Some(id), Some(revision)) = (&input.expected_draft_id, &input.expected_draft_revision)
    else {
        return Err(ServiceError::new(
            "GUARDED_DRAFT_REQUIRED",
            "This register uses guarded carts. Reload the current cart before checkout.",
        ));
    };
    check_revision(&meta, revision)?;
    if meta.active_id.as_deref() != Some(id.as_str()) {
        return Err(ServiceError::new("STALE_DRAFT","This cart is no longer active. No sale was saved; reconcile the current cart and receipts."));
    }
    let draft = active(conn, &meta)?
        .ok_or_else(|| ServiceError::new("STALE_DRAFT", "No tracked active cart is available."))?;
    if draft.lines != input.lines {
        return Err(ServiceError::new("STALE_DRAFT","Checkout lines do not match the saved active cart. Save and review it before collecting cash."));
    }
    Ok(())
}
pub(super) fn consume_checkout(conn: &Connection, sale_id: i64) -> Result<(), ServiceError> {
    let mut meta = metadata(conn)?;
    if let Some(draft) = active(conn, &meta)? {
        write_record(conn, "consumed", &draft, Some(sale_id))?;
    }
    meta.active_id = None;
    meta.revision = next_revision(&meta)?.to_string();
    write_metadata(conn, &meta)?;
    Ok(())
}
pub(super) fn save_legacy(conn: &Connection, draft: &CartInput) -> Result<(), ServiceError> {
    let mut meta = metadata(conn)?;
    if meta.guarded {
        return Err(ServiceError::new("GUARDED_DRAFT_REQUIRED","This register uses guarded carts. Reload and save with a cart revision and operation key."));
    }
    write_cart(conn, &draft.lines)?;
    meta.revision = next_revision(&meta)?.to_string();
    write_metadata(conn, &meta)
}

pub(super) fn backup(conn: &Connection) -> Result<DraftBackup, ServiceError> {
    let records = {
        let mut stmt =
            conn.prepare("SELECT id,status,data_json,sale_id FROM draft_records ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DraftRecord {
                    id: r.get(0)?,
                    status: r.get(1)?,
                    data_json: r.get(2)?,
                    sale_id: r.get::<_, Option<i64>>(3)?.map(|n| n.to_string()),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let operations = {
        let mut stmt=conn.prepare("SELECT idempotency_key,command,request_json,applied_revision FROM draft_operations ORDER BY applied_revision")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DraftOperation {
                    idempotency_key: r.get(0)?,
                    command: r.get(1)?,
                    request_json: r.get(2)?,
                    applied_revision: r.get::<_, i64>(3)?.to_string(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    Ok(DraftBackup {
        metadata: metadata(conn)?,
        records,
        operations,
    })
}
fn invalid_backup() -> ServiceError {
    ServiceError::new(
        "INVALID_BACKUP",
        "Saved cart identities or operations are inconsistent. No existing database was changed.",
    )
}
fn integer(value: &str) -> Result<i64, ServiceError> {
    let n = parse_integer(value, "Stored cart integer").map_err(|_| invalid_backup())?;
    let n = i64::try_from(n).map_err(|_| invalid_backup())?;
    if n.to_string() != value {
        return Err(invalid_backup());
    }
    Ok(n)
}
impl DraftBackup {
    pub(super) fn validate(
        &self,
        draft: Option<&CartInput>,
        sales: &[(String, CheckoutInput)],
        config: Option<&Config>,
        config_revision: i64,
    ) -> Result<(), ServiceError> {
        let revision = integer(&self.metadata.revision)?;
        let next_id = integer(&self.metadata.next_id)?;
        if next_id < 1 || next_id as usize != self.records.len() + 1 {
            return Err(invalid_backup());
        }
        if self.metadata.guarded != self.metadata.guarded_since_revision.is_some() {
            return Err(invalid_backup());
        }
        if let Some(start) = &self.metadata.guarded_since_revision {
            if integer(start)? < 1 || integer(start)? > revision {
                return Err(invalid_backup());
            }
        }
        if !self.metadata.guarded
            && (self.metadata.active_id.is_some()
                || !self.records.is_empty()
                || !self.operations.is_empty())
        {
            return Err(invalid_backup());
        }
        if self.metadata.guarded && self.operations.is_empty() {
            return Err(invalid_backup());
        }
        let mut ids = BTreeSet::new();
        let mut active_id = None;
        let mut consumed_sales = BTreeSet::new();
        for record in &self.records {
            let n = integer(
                record
                    .id
                    .strip_prefix("draft-")
                    .ok_or_else(invalid_backup)?,
            )?;
            if n < 1 || n >= next_id || !ids.insert(record.id.clone()) {
                return Err(invalid_backup());
            }
            let stored: StoredDraft =
                decode_stored(&record.data_json).map_err(|_| invalid_backup())?;
            if stored.id != record.id
                || stored.lines.is_empty()
                || stored.snapshots.len() != stored.lines.len()
                || config.is_none()
            {
                return Err(invalid_backup());
            }
            validate_draft(&stored.lines).map_err(|_| invalid_backup())?;
            validate_label(stored.label.as_deref()).map_err(|_| invalid_backup())?;
            currency(&stored.currency).map_err(|_| invalid_backup())?;
            if let Some(t) = &stored.parked_at_unix {
                integer(t)?;
            }
            if stored.label.is_some() && stored.parked_at_unix.is_none() {
                return Err(invalid_backup());
            }
            let mut snapshots = BTreeSet::new();
            for snap in &stored.snapshots {
                if !snapshots.insert(&snap.line_id) {
                    return Err(invalid_backup());
                }
                let line = stored
                    .lines
                    .iter()
                    .find(|l| l.id == snap.line_id)
                    .ok_or_else(invalid_backup)?;
                if let Some(item) = &snap.item {
                    if item.sku != line.sku {
                        return Err(invalid_backup());
                    }
                    if let Some(group) = &snap.group {
                        if !matches!(&item.pricing,CatalogPricing::WeightGroup{group_id} if group_id==&group.id)
                        {
                            return Err(invalid_backup());
                        }
                    }
                    let c = Config {
                        store_settings: None,
                        schema_version: 1,
                        demo: true,
                        tax_mode: "demo_no_tax".into(),
                        currency: stored.currency.clone(),
                        groups: snap.group.clone().into_iter().collect(),
                        catalog: vec![item.clone()],
                    };
                    validate_config(&c).map_err(|_| invalid_backup())?;
                } else if snap.group.is_some() {
                    return Err(invalid_backup());
                }
            }
            match record.status.as_str() {
                "active" => {
                    if active_id.replace(record.id.clone()).is_some()
                        || draft.map(|d| &d.lines) != Some(&stored.lines)
                        || record.sale_id.is_some()
                    {
                        return Err(invalid_backup());
                    }
                }
                "parked" => {
                    if stored.parked_at_unix.is_none() || record.sale_id.is_some() {
                        return Err(invalid_backup());
                    }
                }
                "discarded" => {
                    if record.sale_id.is_some() {
                        return Err(invalid_backup());
                    }
                }
                "consumed" => {
                    let sale_id = record.sale_id.as_ref().ok_or_else(invalid_backup)?;
                    integer(sale_id)?;
                    let request = &sales
                        .iter()
                        .find(|(id, _)| id == sale_id)
                        .ok_or_else(invalid_backup)?
                        .1;
                    if !consumed_sales.insert(sale_id.clone())
                        || request.expected_draft_id.as_deref() != Some(record.id.as_str())
                        || request.lines != stored.lines
                    {
                        return Err(invalid_backup());
                    }
                }
                _ => return Err(invalid_backup()),
            }
        }
        if active_id != self.metadata.active_id
            || (self.metadata.guarded
                && active_id.is_none()
                && draft.is_some_and(|d| !d.lines.is_empty()))
        {
            return Err(invalid_backup());
        }
        for (id, request) in sales {
            match (&request.expected_draft_id, &request.expected_draft_revision) {
                (None, None) => {}
                (Some(d), Some(r)) => {
                    if !ids.contains(d) || !consumed_sales.contains(id) || integer(r)? >= revision {
                        return Err(invalid_backup());
                    }
                }
                _ => return Err(invalid_backup()),
            }
        }
        let mut keys = BTreeSet::new();
        let mut revisions = BTreeSet::new();
        for operation in &self.operations {
            validate_id(&operation.idempotency_key, "Operation key")
                .map_err(|_| invalid_backup())?;
            let applied = integer(&operation.applied_revision)?;
            if applied < 1
                || applied > revision
                || !keys.insert(&operation.idempotency_key)
                || !revisions.insert(applied)
            {
                return Err(invalid_backup());
            }
            let (key, expected, canonical) = match operation.command.as_str() {
                "save_active_draft" => {
                    let v: SaveActiveDraftInput =
                        decode_stored(&operation.request_json).map_err(|_| invalid_backup())?;
                    validate_draft(&v.lines).map_err(|_| invalid_backup())?;
                    let raw = serialize(&v)?;
                    (v.idempotency_key, v.expected_draft_revision, raw)
                }
                "park_draft" => {
                    let v: ParkDraftInput =
                        decode_stored(&operation.request_json).map_err(|_| invalid_backup())?;
                    validate_label(v.label.as_deref()).map_err(|_| invalid_backup())?;
                    let raw = serialize(&v)?;
                    (v.idempotency_key, v.expected_draft_revision, raw)
                }
                "resume_parked_draft" => {
                    let v: ResumeParkedDraftInput =
                        decode_stored(&operation.request_json).map_err(|_| invalid_backup())?;
                    if !ids.contains(&v.draft_id)
                        || integer(&v.expected_config_revision)? < 1
                        || integer(&v.expected_config_revision)? > config_revision
                    {
                        return Err(invalid_backup());
                    }
                    let raw = serialize(&v)?;
                    (v.idempotency_key, v.expected_draft_revision, raw)
                }
                "discard_parked_draft" => {
                    let v: DiscardParkedDraftInput =
                        decode_stored(&operation.request_json).map_err(|_| invalid_backup())?;
                    if !ids.contains(&v.draft_id) {
                        return Err(invalid_backup());
                    }
                    let raw = serialize(&v)?;
                    (v.idempotency_key, v.expected_draft_revision, raw)
                }
                _ => return Err(invalid_backup()),
            };
            if key != operation.idempotency_key
                || integer(&expected)?.checked_add(1) != Some(applied)
                || canonical != operation.request_json
            {
                return Err(invalid_backup());
            }
        }
        self.validate_history(sales)?;
        Ok(())
    }
    fn validate_history(&self, sales: &[(String, CheckoutInput)]) -> Result<(), ServiceError> {
        use std::collections::BTreeMap;
        // A legacy-only register has no guarded history. Its revision prefix is opaque.
        if !self.metadata.guarded {
            return Ok(());
        }
        enum Event<'a> {
            Operation(&'a DraftOperation),
            Sale(&'a String, &'a CheckoutInput),
        }
        struct ReplayDraft {
            status: &'static str,
            lines: Vec<CartInputLine>,
            label: Option<String>,
            parked: bool,
            sale_id: Option<String>,
        }
        let mut events = BTreeMap::new();
        for op in &self.operations {
            if events
                .insert(integer(&op.applied_revision)?, Event::Operation(op))
                .is_some()
            {
                return Err(invalid_backup());
            }
        }
        for (id, request) in sales {
            if let Some(revision) = &request.expected_draft_revision {
                let applied = integer(revision)?
                    .checked_add(1)
                    .ok_or_else(invalid_backup)?;
                if events.insert(applied, Event::Sale(id, request)).is_some() {
                    return Err(invalid_backup());
                }
            }
        }
        let mut expected = integer(
            self.metadata
                .guarded_since_revision
                .as_deref()
                .ok_or_else(invalid_backup)?,
        )?;
        let mut active: Option<String> = None;
        let mut next_id = 1i64;
        let mut records: BTreeMap<String, ReplayDraft> = BTreeMap::new();
        let mut last_sale_id = 0;
        for (revision, event) in events {
            if revision != expected {
                return Err(invalid_backup());
            }
            expected = expected.checked_add(1).ok_or_else(invalid_backup)?;
            match event {
                Event::Operation(op) => match op.command.as_str() {
                    "save_active_draft" => {
                        let request: SaveActiveDraftInput =
                            decode_stored(&op.request_json).map_err(|_| invalid_backup())?;
                        if request.lines.is_empty() {
                            if let Some(id) = active.take() {
                                records.get_mut(&id).ok_or_else(invalid_backup)?.status =
                                    "discarded";
                            }
                        } else if let Some(id) = &active {
                            records.get_mut(id).ok_or_else(invalid_backup)?.lines = request.lines;
                        } else {
                            let id = format!("draft-{next_id}");
                            next_id = next_id.checked_add(1).ok_or_else(invalid_backup)?;
                            records.insert(
                                id.clone(),
                                ReplayDraft {
                                    status: "active",
                                    lines: request.lines,
                                    label: None,
                                    parked: false,
                                    sale_id: None,
                                },
                            );
                            active = Some(id);
                        }
                    }
                    "park_draft" => {
                        let request: ParkDraftInput =
                            decode_stored(&op.request_json).map_err(|_| invalid_backup())?;
                        let id = active.take().ok_or_else(invalid_backup)?;
                        let record = records.get_mut(&id).ok_or_else(invalid_backup)?;
                        record.status = "parked";
                        record.label = request.label;
                        record.parked = true;
                    }
                    "resume_parked_draft" => {
                        if active.is_some() {
                            return Err(invalid_backup());
                        }
                        let request: ResumeParkedDraftInput =
                            decode_stored(&op.request_json).map_err(|_| invalid_backup())?;
                        let record = records
                            .get_mut(&request.draft_id)
                            .ok_or_else(invalid_backup)?;
                        if record.status != "parked" {
                            return Err(invalid_backup());
                        }
                        record.status = "active";
                        active = Some(request.draft_id);
                    }
                    "discard_parked_draft" => {
                        let request: DiscardParkedDraftInput =
                            decode_stored(&op.request_json).map_err(|_| invalid_backup())?;
                        let record = records
                            .get_mut(&request.draft_id)
                            .ok_or_else(invalid_backup)?;
                        if record.status != "parked" {
                            return Err(invalid_backup());
                        }
                        record.status = "discarded";
                    }
                    _ => return Err(invalid_backup()),
                },
                Event::Sale(id, request) => {
                    let active_id = active.take().ok_or_else(invalid_backup)?;
                    let record = records.get_mut(&active_id).ok_or_else(invalid_backup)?;
                    let sale_id = integer(id)?;
                    if request.expected_draft_id.as_ref() != Some(&active_id)
                        || record.lines != request.lines
                        || sale_id <= last_sale_id
                    {
                        return Err(invalid_backup());
                    }
                    record.status = "consumed";
                    record.sale_id = Some(id.clone());
                    last_sale_id = sale_id;
                }
            }
        }
        if expected.checked_sub(1) != Some(integer(&self.metadata.revision)?)
            || next_id != integer(&self.metadata.next_id)?
            || active != self.metadata.active_id
            || records.len() != self.records.len()
        {
            return Err(invalid_backup());
        }
        for row in &self.records {
            let expected = records.get(&row.id).ok_or_else(invalid_backup)?;
            let stored: StoredDraft =
                decode_stored(&row.data_json).map_err(|_| invalid_backup())?;
            if expected.status != row.status
                || expected.lines != stored.lines
                || expected.label != stored.label
                || expected.parked != stored.parked_at_unix.is_some()
                || expected.sale_id != row.sale_id
            {
                return Err(invalid_backup());
            }
        }
        Ok(())
    }
    pub(super) fn restore(&self, conn: &Connection) -> Result<(), ServiceError> {
        for r in &self.records {
            conn.execute(
                "INSERT INTO draft_records(id,status,data_json,sale_id) VALUES(?1,?2,?3,?4)",
                params![
                    r.id,
                    r.status,
                    r.data_json,
                    r.sale_id.as_deref().map(integer).transpose()?
                ],
            )?;
        }
        for o in &self.operations {
            conn.execute("INSERT INTO draft_operations(idempotency_key,command,request_json,applied_revision) VALUES(?1,?2,?3,?4)",params![o.idempotency_key,o.command,o.request_json,integer(&o.applied_revision)?])?;
        }
        write_metadata(conn, &self.metadata)
    }
}
