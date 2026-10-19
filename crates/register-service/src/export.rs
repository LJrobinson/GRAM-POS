//! Versioned logical exports. Stored JSON is retained byte-for-byte for idempotency.
use super::*;
use std::{collections::BTreeMap, fs::OpenOptions, io::Write};

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportFile {
    pub filename: String,
    pub media_type: String,
    pub content: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsBackup {
    revision: String,
    config_json: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaleBackup {
    id: String,
    idempotency_key: String,
    request_json: String,
    created_at_unix: String,
    snapshot_json: String,
    receipt_json: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionBackup {
    id: String,
    idempotency_key: String,
    request_json: String,
    session_json: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CloseBackup {
    session_id: String,
    request_json: String,
    session_json: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RefundBackup {
    id: String,
    sale_id: String,
    session_id: String,
    idempotency_key: String,
    request_json: String,
    receipt_json: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    format: String,
    format_version: u32,
    schema_version: i64,
    settings: Option<SettingsBackup>,
    draft_json: Option<String>,
    sales: Vec<SaleBackup>,
    sales_sequence: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cash_sessions: Option<Vec<SessionBackup>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cash_closes: Option<Vec<CloseBackup>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sale_sessions: Option<Vec<SaleSession>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refunds: Option<Vec<RefundBackup>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cash_sessions_sequence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refunds_sequence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    legacy_sale_max_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    quote: Quote,
    config_snapshot: Config,
    tender_minor: String,
    change_minor: String,
    payment_method: String,
    demo: bool,
}
fn corrupt() -> ServiceError {
    ServiceError::new("INVALID_BACKUP", "Register data is incomplete, inconsistent, or unsupported. No existing database was changed.")
}
fn integer(value: &str) -> Result<i64, ServiceError> {
    let n = parse_integer(value, "Stored integer").map_err(|_| corrupt())?;
    let n = i64::try_from(n).map_err(|_| corrupt())?;
    if n.to_string() != value {
        return Err(corrupt());
    }
    Ok(n)
}
impl Backup {
    fn validate(&self) -> Result<(), ServiceError> {
        if self.format != "gram-pos-backup"
            || !matches!((self.format_version, self.schema_version), (1, 1) | (2, 2))
        {
            return Err(corrupt());
        }
        if let Some(settings) = &self.settings {
            if integer(&settings.revision)? < 1 {
                return Err(corrupt());
            }
            let config: Config = decode_stored(&settings.config_json)?;
            validate_config(&config).map_err(|_| corrupt())?;
        }
        if let Some(raw) = &self.draft_json {
            let draft: CartInput = decode_stored(raw)?;
            validate_draft(&draft.lines).map_err(|_| corrupt())?;
        }
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let sequence = integer(&self.sales_sequence)?;
        for sale in &self.sales {
            let id = integer(&sale.id)?;
            if id < 1 || id > sequence || !ids.insert(id) || !keys.insert(&sale.idempotency_key) {
                return Err(corrupt());
            }
            validate_id(&sale.idempotency_key, "Checkout key").map_err(|_| corrupt())?;
            let request: CheckoutInput = decode_stored(&sale.request_json)?;
            let receipt: Receipt = decode_stored(&sale.receipt_json)?;
            let snapshot: Snapshot = decode_stored(&sale.snapshot_json)?;
            let revision = integer(&receipt.quote.config_revision)?;
            let quote = quote_using_config(&request.lines, &receipt.config_snapshot, revision)
                .map_err(|_| corrupt())?;
            let total = parse_integer(&quote.total_minor, "Total").map_err(|_| corrupt())?;
            let tender = parse_integer(&request.tender_minor, "Tender").map_err(|_| corrupt())?;
            let change = tender.checked_sub(total).ok_or_else(corrupt)?;
            if revision < 1
                || integer(&sale.created_at_unix)? < 0
                || request.idempotency_key != sale.idempotency_key
                || parse_integer(&request.expected_total_minor, "Total").map_err(|_| corrupt())?
                    != total
                || parse_integer(&request.expected_config_revision, "Revision")
                    .map_err(|_| corrupt())?
                    != revision as u64
                || receipt.sale_id != sale.id
                || receipt.receipt_number != format!("DEMO-{id:08}")
                || receipt.created_at_unix != sale.created_at_unix
                || receipt.quote != quote
                || receipt.tender_minor != tender.to_string()
                || receipt.change_minor != change.to_string()
                || receipt.payment_method != "cash"
                || !receipt.demo
                || snapshot.quote != receipt.quote
                || snapshot.config_snapshot != receipt.config_snapshot
                || snapshot.tender_minor != receipt.tender_minor
                || snapshot.change_minor != receipt.change_minor
                || snapshot.payment_method != receipt.payment_method
                || snapshot.demo != receipt.demo
                || self
                    .settings
                    .as_ref()
                    .map(|s| integer(&s.revision))
                    .transpose()?
                    .unwrap_or(0)
                    < revision
            {
                return Err(corrupt());
            }
        }
        self.validate_ledger()
    }
    fn validate_ledger(&self) -> Result<(), ServiceError> {
        if self.format_version == 1 {
            if self.cash_sessions.is_some()
                || self.cash_closes.is_some()
                || self.sale_sessions.is_some()
                || self.refunds.is_some()
                || self.cash_sessions_sequence.is_some()
                || self.refunds_sequence.is_some()
                || self.legacy_sale_max_id.is_some()
            {
                return Err(corrupt());
            }
            return Ok(());
        }
        let sessions = self.cash_sessions.as_ref().ok_or_else(corrupt)?;
        if !sessions.is_empty() && self.settings.is_none() {
            return Err(corrupt());
        }
        let closes = self.cash_closes.as_ref().ok_or_else(corrupt)?;
        let mappings = self.sale_sessions.as_ref().ok_or_else(corrupt)?;
        let refunds = self.refunds.as_ref().ok_or_else(corrupt)?;
        let session_seq = integer(self.cash_sessions_sequence.as_deref().ok_or_else(corrupt)?)?;
        let refund_seq = integer(self.refunds_sequence.as_deref().ok_or_else(corrupt)?)?;
        let legacy = integer(self.legacy_sale_max_id.as_deref().ok_or_else(corrupt)?)?;
        if legacy > integer(&self.sales_sequence)? {
            return Err(corrupt());
        }
        let mut opened = BTreeMap::new();
        let mut keys = BTreeSet::new();
        for row in sessions {
            let id = integer(&row.id)?;
            validate_id(&row.idempotency_key, "Session key").map_err(|_| corrupt())?;
            let request: OpenCashSessionInput = decode_stored(&row.request_json)?;
            let session: CashSession = decode_stored(&row.session_json)?;
            currency(&session.currency).map_err(|_| corrupt())?;
            let amount =
                parse_integer(&request.opening_float_minor, "Float").map_err(|_| corrupt())?;
            if id < 1
                || id > session_seq
                || !keys.insert(&row.idempotency_key)
                || request.idempotency_key != row.idempotency_key
                || session.session_id != row.id
                || request
                    .expected_currency
                    .as_ref()
                    .is_some_and(|c| c != &session.currency)
                || session.opening_float_minor != amount.to_string()
                || session.expected_cash_minor != amount.to_string()
                || session.sales_total_minor != "0"
                || session.refunds_total_minor != "0"
                || session.movement_count != "0"
                || session.closed_at_unix.is_some()
                || session.counted_minor.is_some()
                || session.variance_minor.is_some()
                || opened.insert(id, session).is_some()
            {
                return Err(corrupt());
            }
        }
        let mut closed = BTreeMap::new();
        for row in closes {
            let id = integer(&row.session_id)?;
            let session: CashSession = decode_stored(&row.session_json)?;
            let request: CloseCashSessionInput = decode_stored(&row.request_json)?;
            let opening = opened.get(&id).ok_or_else(corrupt)?;
            integer(session.closed_at_unix.as_deref().ok_or_else(corrupt)?)?;
            let count = parse_integer(&request.counted_minor, "Count").map_err(|_| corrupt())?;
            let movements = parse_integer(&request.expected_movement_count, "Movements")
                .map_err(|_| corrupt())?;
            if parse_integer(&request.session_id, "Session ID").map_err(|_| corrupt())? != id as u64
                || session.session_id != row.session_id
                || session.opened_at_unix != opening.opened_at_unix
                || session.currency != opening.currency
                || session.opening_float_minor != opening.opening_float_minor
                || session.counted_minor.as_deref() != Some(count.to_string().as_str())
                || session.movement_count != movements.to_string()
                || closed.insert(id, session).is_some()
            {
                return Err(corrupt());
            }
        }
        // Wall-clock adjustments do not change transaction order. Session IDs do.
        let latest = opened.keys().next_back();
        for (id, session) in &opened {
            integer(&session.opened_at_unix)?;
            if Some(id) != latest && !closed.contains_key(id) {
                return Err(corrupt());
            }
        }
        if let Some((id, active)) = opened.iter().next_back() {
            if !closed.contains_key(id) {
                let settings = self.settings.as_ref().ok_or_else(corrupt)?;
                let config: Config = decode_stored(&settings.config_json)?;
                if config.currency != active.currency {
                    return Err(corrupt());
                }
            }
        }
        let mut sales = BTreeMap::new();
        for row in &self.sales {
            sales.insert(
                integer(&row.id)?,
                decode_stored::<Receipt>(&row.receipt_json)?,
            );
        }
        let mut totals: BTreeMap<i64, (i128, i128, u64)> =
            opened.keys().map(|id| (*id, (0, 0, 0))).collect();
        let mut mapped = BTreeMap::new();
        for row in mappings {
            let sale_id = integer(&row.sale_id)?;
            let session_id = integer(&row.session_id)?;
            let sale = sales.get(&sale_id).ok_or_else(corrupt)?;
            let session = opened.get(&session_id).ok_or_else(corrupt)?;
            let sale_row = self
                .sales
                .iter()
                .find(|s| s.id == row.sale_id)
                .ok_or_else(corrupt)?;
            let request: CheckoutInput = decode_stored(&sale_row.request_json)?;
            if mapped.insert(sale_id, session_id).is_some()
                || sale_id <= legacy
                || sale.quote.currency != session.currency
                || request
                    .expected_session_id
                    .as_ref()
                    .is_some_and(|id| id != &row.session_id)
                || request
                    .expected_currency
                    .as_ref()
                    .is_some_and(|c| c != &session.currency)
            {
                return Err(corrupt());
            }
            integer(&sale.created_at_unix)?;
            let sum = totals.get_mut(&session_id).ok_or_else(corrupt)?;
            sum.0 = sum
                .0
                .checked_add(i128::from(
                    parse_integer(&sale.quote.total_minor, "Total").map_err(|_| corrupt())?,
                ))
                .ok_or_else(corrupt)?;
            sum.2 = sum.2.checked_add(1).ok_or_else(corrupt)?;
        }
        if sales
            .keys()
            .any(|id| *id > legacy && !mapped.contains_key(id))
        {
            return Err(corrupt());
        }
        // Sale IDs and session IDs both follow serialized transaction order.
        if mapped
            .values()
            .zip(mapped.values().skip(1))
            .any(|(earlier, later)| earlier > later)
        {
            return Err(corrupt());
        }
        let mut refunded = BTreeSet::new();
        let mut refund_ids = BTreeSet::new();
        let mut refund_sessions = BTreeMap::new();
        keys.clear();
        for row in refunds {
            let id = integer(&row.id)?;
            let sale_id = integer(&row.sale_id)?;
            let session_id = integer(&row.session_id)?;
            let original = sales.get(&sale_id).ok_or_else(corrupt)?;
            let session = opened.get(&session_id).ok_or_else(corrupt)?;
            let request: RefundInput = decode_stored(&row.request_json)?;
            let receipt: RefundReceipt = decode_stored(&row.receipt_json)?;
            validate_id(&row.idempotency_key, "Refund key").map_err(|_| corrupt())?;
            if id < 1
                || id > refund_seq
                || !refund_ids.insert(id)
                || !refunded.insert(sale_id)
                || !keys.insert(&row.idempotency_key)
                || request.idempotency_key != row.idempotency_key
                || parse_integer(&request.sale_id, "Sale ID").map_err(|_| corrupt())?
                    != sale_id as u64
                || receipt.refund_id != row.id
                || receipt.refund_number != format!("DEMO-REFUND-{id:08}")
                || receipt.sale_id != row.sale_id
                || receipt.session_id != row.session_id
                || receipt.original_receipt_number != original.receipt_number
                || receipt.amount_minor != original.quote.total_minor
                || receipt.currency != original.quote.currency
                || receipt.currency != session.currency
                || receipt.payment_method != "cash"
                || receipt.reason != request.reason
                || request.reason.trim().is_empty()
                || request.reason.len() > 500
                || request.reason.chars().any(char::is_control)
                || request
                    .expected_session_id
                    .as_ref()
                    .is_some_and(|id| id != &row.session_id)
                || request
                    .expected_currency
                    .as_ref()
                    .is_some_and(|c| c != &session.currency)
                || mapped
                    .get(&sale_id)
                    .is_some_and(|sale_session| *sale_session > session_id)
            {
                return Err(corrupt());
            }
            refund_sessions.insert(id, session_id);
            integer(&receipt.created_at_unix)?;
            let sum = totals.get_mut(&session_id).ok_or_else(corrupt)?;
            sum.1 = sum
                .1
                .checked_add(i128::from(
                    parse_integer(&receipt.amount_minor, "Refund").map_err(|_| corrupt())?,
                ))
                .ok_or_else(corrupt)?;
            sum.2 = sum.2.checked_add(1).ok_or_else(corrupt)?;
        }
        if refund_sessions
            .values()
            .zip(refund_sessions.values().skip(1))
            .any(|(earlier, later)| earlier > later)
        {
            return Err(corrupt());
        }
        for (id, session) in closed {
            let (sales, refunds, count) = totals.get(&id).ok_or_else(corrupt)?;
            let opening = i128::from(
                parse_integer(&session.opening_float_minor, "Float").map_err(|_| corrupt())?,
            );
            let expected = opening
                .checked_add(*sales)
                .and_then(|v| v.checked_sub(*refunds))
                .ok_or_else(corrupt)?;
            let counted = i128::from(
                parse_integer(
                    session.counted_minor.as_deref().ok_or_else(corrupt)?,
                    "Count",
                )
                .map_err(|_| corrupt())?,
            );
            let variance = counted.checked_sub(expected).ok_or_else(corrupt)?;
            if session.sales_total_minor != sales.to_string()
                || session.refunds_total_minor != refunds.to_string()
                || session.movement_count != count.to_string()
                || session.expected_cash_minor != expected.to_string()
                || session.variance_minor.as_deref() != Some(variance.to_string().as_str())
            {
                return Err(corrupt());
            }
        }
        Ok(())
    }
}
impl RegisterService {
    fn logical_backup(&self) -> Result<Backup, ServiceError> {
        let tx = self.conn.unchecked_transaction()?;
        let integrity: String = tx.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(corrupt());
        }
        let foreign_errors: i64 =
            tx.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })?;
        if foreign_errors != 0 {
            return Err(corrupt());
        }
        let settings = tx
            .query_row(
                "SELECT revision,config_json FROM settings WHERE singleton=1",
                [],
                |r| {
                    Ok(SettingsBackup {
                        revision: r.get::<_, i64>(0)?.to_string(),
                        config_json: r.get(1)?,
                    })
                },
            )
            .optional()?;
        let draft_json = tx
            .query_row("SELECT draft_json FROM drafts WHERE singleton=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let mut sales = Vec::new();
        {
            let mut stmt = tx.prepare("SELECT s.id,s.idempotency_key,s.request_json,s.created_at_unix,s.snapshot_json,r.receipt_json FROM sales s LEFT JOIN receipts r ON s.id=r.sale_id ORDER BY s.id")?;
            for row in stmt.query_map([], |r| {
                Ok(SaleBackup {
                    id: r.get::<_, i64>(0)?.to_string(),
                    idempotency_key: r.get(1)?,
                    request_json: r.get(2)?,
                    created_at_unix: r.get(3)?,
                    snapshot_json: r.get(4)?,
                    receipt_json: r.get(5)?,
                })
            })? {
                sales.push(row?);
            }
        }
        let sequence: Option<i64> = tx
            .query_row(
                "SELECT seq FROM sqlite_sequence WHERE name='sales'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let cash_sessions = {
            let mut stmt = tx.prepare("SELECT id,idempotency_key,request_json,session_json FROM cash_sessions ORDER BY id")?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(SessionBackup {
                        id: r.get::<_, i64>(0)?.to_string(),
                        idempotency_key: r.get(1)?,
                        request_json: r.get(2)?,
                        session_json: r.get(3)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        let cash_closes = {
            let mut stmt = tx.prepare(
                "SELECT session_id,request_json,session_json FROM cash_closes ORDER BY session_id",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(CloseBackup {
                        session_id: r.get::<_, i64>(0)?.to_string(),
                        request_json: r.get(1)?,
                        session_json: r.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        let sale_sessions = {
            let mut stmt =
                tx.prepare("SELECT sale_id,session_id FROM sale_sessions ORDER BY sale_id")?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(SaleSession {
                        sale_id: r.get::<_, i64>(0)?.to_string(),
                        session_id: r.get::<_, i64>(1)?.to_string(),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        let refunds = {
            let mut stmt = tx.prepare("SELECT id,sale_id,session_id,idempotency_key,request_json,receipt_json FROM refunds ORDER BY id")?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(RefundBackup {
                        id: r.get::<_, i64>(0)?.to_string(),
                        sale_id: r.get::<_, i64>(1)?.to_string(),
                        session_id: r.get::<_, i64>(2)?.to_string(),
                        idempotency_key: r.get(3)?,
                        request_json: r.get(4)?,
                        receipt_json: r.get(5)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        let ledger_sequence = |name: &str| -> Result<String, ServiceError> {
            Ok(tx
                .query_row(
                    "SELECT seq FROM sqlite_sequence WHERE name=?1",
                    [name],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or(0)
                .to_string())
        };
        let legacy: i64 = tx.query_row(
            "SELECT legacy_sale_max_id FROM ledger_metadata WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        let backup = Backup {
            format: "gram-pos-backup".into(),
            format_version: 2,
            schema_version: SCHEMA_VERSION,
            settings,
            draft_json,
            sales,
            sales_sequence: sequence.unwrap_or(0).to_string(),
            cash_sessions: Some(cash_sessions),
            cash_closes: Some(cash_closes),
            sale_sessions: Some(sale_sessions),
            refunds: Some(refunds),
            cash_sessions_sequence: Some(ledger_sequence("cash_sessions")?),
            refunds_sequence: Some(ledger_sequence("refunds")?),
            legacy_sale_max_id: Some(legacy.to_string()),
        };
        backup.validate()?;
        tx.commit()?;
        Ok(backup)
    }
    // Reports share export/recovery validation and one consistent read snapshot,
    // but consume only event records, never drawer aggregates or live catalog prices.
    pub(super) fn report_records(&self) -> Result<super::report::ReportRecords, ServiceError> {
        let backup = self.logical_backup()?;
        Ok(super::report::ReportRecords {
            config_revision: backup
                .settings
                .as_ref()
                .map(|s| s.revision.clone())
                .unwrap_or_else(|| "0".into()),
            receipts: backup
                .sales
                .iter()
                .map(|row| decode_stored(&row.receipt_json))
                .collect::<Result<_, _>>()?,
            refunds: backup
                .refunds
                .as_ref()
                .ok_or_else(corrupt)?
                .iter()
                .map(|row| decode_stored(&row.receipt_json))
                .collect::<Result<_, _>>()?,
            sale_sessions: backup.sale_sessions.ok_or_else(corrupt)?,
        })
    }
    pub fn export_backup(&self) -> Result<ExportFile, ServiceError> {
        make_export("gram-pos-backup.json", &self.logical_backup()?)
    }
    pub fn export_sales(&self) -> Result<ExportFile, ServiceError> {
        let backup = self.logical_backup()?;
        let receipts = backup
            .sales
            .iter()
            .map(|s| decode_stored::<Value>(&s.receipt_json))
            .collect::<Result<Vec<_>, _>>()?;
        // Build every export section from the same validated database snapshot.
        let mut sessions: BTreeMap<String, CashSession> = backup
            .cash_sessions
            .as_ref()
            .ok_or_else(corrupt)?
            .iter()
            .map(|row| {
                Ok((
                    row.id.clone(),
                    decode_stored::<CashSession>(&row.session_json)?,
                ))
            })
            .collect::<Result<_, ServiceError>>()?;
        let refunds = backup
            .refunds
            .as_ref()
            .ok_or_else(corrupt)?
            .iter()
            .map(|row| decode_stored::<RefundReceipt>(&row.receipt_json))
            .collect::<Result<Vec<_>, _>>()?;
        let mappings = backup.sale_sessions.as_ref().ok_or_else(corrupt)?;
        for mapping in mappings {
            let sale = backup
                .sales
                .iter()
                .find(|sale| sale.id == mapping.sale_id)
                .ok_or_else(corrupt)?;
            let receipt: Receipt = decode_stored(&sale.receipt_json)?;
            let session = sessions.get_mut(&mapping.session_id).ok_or_else(corrupt)?;
            let amount = i128::from(
                parse_integer(&receipt.quote.total_minor, "Total").map_err(|_| corrupt())?,
            );
            let total: i128 = session.sales_total_minor.parse().map_err(|_| corrupt())?;
            session.sales_total_minor = total.checked_add(amount).ok_or_else(corrupt)?.to_string();
            session.movement_count =
                (parse_integer(&session.movement_count, "Count").map_err(|_| corrupt())? + 1)
                    .to_string();
        }
        for refund in &refunds {
            let session = sessions.get_mut(&refund.session_id).ok_or_else(corrupt)?;
            let amount =
                i128::from(parse_integer(&refund.amount_minor, "Refund").map_err(|_| corrupt())?);
            let total: i128 = session.refunds_total_minor.parse().map_err(|_| corrupt())?;
            session.refunds_total_minor =
                total.checked_add(amount).ok_or_else(corrupt)?.to_string();
            session.movement_count =
                (parse_integer(&session.movement_count, "Count").map_err(|_| corrupt())? + 1)
                    .to_string();
        }
        for session in sessions.values_mut() {
            let opening = i128::from(
                parse_integer(&session.opening_float_minor, "Float").map_err(|_| corrupt())?,
            );
            let sales: i128 = session.sales_total_minor.parse().map_err(|_| corrupt())?;
            let refunds: i128 = session.refunds_total_minor.parse().map_err(|_| corrupt())?;
            session.expected_cash_minor = opening
                .checked_add(sales)
                .and_then(|v| v.checked_sub(refunds))
                .ok_or_else(corrupt)?
                .to_string();
        }
        for close in backup.cash_closes.as_ref().ok_or_else(corrupt)? {
            sessions.insert(
                close.session_id.clone(),
                decode_stored(&close.session_json)?,
            );
        }
        let mut sessions: Vec<_> = sessions.into_values().collect();
        sessions.sort_by_key(|session| integer(&session.session_id).unwrap_or(0));
        make_export(
            "gram-pos-sales.json",
            &json!({"format":"gram-pos-sales","format_version":2,"receipts":receipts,
                "refunds":refunds,"sessions":sessions,"sale_sessions":mappings}),
        )
    }
    /// Offline recovery only. Never opens or replaces an existing destination.
    pub fn restore_backup(
        content: &str,
        destination: impl AsRef<Path>,
    ) -> Result<(), ServiceError> {
        let backup: Backup = serde_json::from_str(content).map_err(|_| corrupt())?;
        backup.validate()?;
        // Validate every row in an isolated in-memory database before touching a path.
        let mut restored = Self::in_memory()?;
        {
            let tx = restored.conn.transaction()?;
            let legacy = backup
                .legacy_sale_max_id
                .as_deref()
                .map(integer)
                .transpose()?
                .unwrap_or(
                    backup
                        .sales
                        .iter()
                        .map(|s| integer(&s.id))
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .max()
                        .unwrap_or(0),
                );
            // Only the isolated recovery database is modified here.
            tx.execute_batch("DROP TRIGGER ledger_metadata_no_update")?;
            tx.execute(
                "UPDATE ledger_metadata SET legacy_sale_max_id=?1 WHERE singleton=1",
                [legacy],
            )?;
            tx.execute_batch("CREATE TRIGGER ledger_metadata_no_update BEFORE UPDATE ON ledger_metadata BEGIN SELECT RAISE(ABORT, 'Ledger is immutable'); END;")?;
            if let Some(s) = backup.settings {
                tx.execute(
                    "INSERT INTO settings VALUES(1,?1,?2)",
                    params![integer(&s.revision)?, s.config_json],
                )?;
            }
            if let Some(draft) = backup.draft_json {
                tx.execute("INSERT INTO drafts VALUES(1,?1)", [draft])?;
            }
            for sale in backup.sales {
                tx.execute("INSERT INTO sales(id,idempotency_key,request_json,created_at_unix,snapshot_json) VALUES(?1,?2,?3,?4,?5)", params![integer(&sale.id)?,sale.idempotency_key,sale.request_json,sale.created_at_unix,sale.snapshot_json])?;
                tx.execute(
                    "INSERT INTO receipts VALUES(?1,?2)",
                    params![integer(&sale.id)?, sale.receipt_json],
                )?;
            }
            for row in backup.cash_sessions.unwrap_or_default() {
                tx.execute("INSERT INTO cash_sessions(id,idempotency_key,request_json,session_json) VALUES(?1,?2,?3,?4)",params![integer(&row.id)?,row.idempotency_key,row.request_json,row.session_json])?;
            }
            for row in backup.cash_closes.unwrap_or_default() {
                tx.execute("INSERT INTO cash_closes(session_id,request_json,session_json) VALUES(?1,?2,?3)",params![integer(&row.session_id)?,row.request_json,row.session_json])?;
            }
            for row in backup.sale_sessions.unwrap_or_default() {
                tx.execute(
                    "INSERT INTO sale_sessions(sale_id,session_id) VALUES(?1,?2)",
                    params![integer(&row.sale_id)?, integer(&row.session_id)?],
                )?;
            }
            for row in backup.refunds.unwrap_or_default() {
                tx.execute("INSERT INTO refunds(id,sale_id,session_id,idempotency_key,request_json,receipt_json) VALUES(?1,?2,?3,?4,?5,?6)",params![integer(&row.id)?,integer(&row.sale_id)?,integer(&row.session_id)?,row.idempotency_key,row.request_json,row.receipt_json])?;
            }
            for (name, sequence) in [
                (
                    "cash_sessions",
                    backup.cash_sessions_sequence.as_deref().unwrap_or("0"),
                ),
                ("refunds", backup.refunds_sequence.as_deref().unwrap_or("0")),
            ] {
                tx.execute("DELETE FROM sqlite_sequence WHERE name=?1", [name])?;
                tx.execute(
                    "INSERT INTO sqlite_sequence(name,seq) VALUES(?1,?2)",
                    params![name, integer(sequence)?],
                )?;
            }
            tx.execute("DELETE FROM sqlite_sequence WHERE name='sales'", [])?;
            tx.execute(
                "INSERT INTO sqlite_sequence(name,seq) VALUES('sales',?1)",
                [integer(&backup.sales_sequence)?],
            )?;
            tx.commit()?;
        }
        restored.logical_backup()?;
        // SQLite serialization produces a complete DB image, not a copy of a live file.
        let image = restored.conn.serialize(rusqlite::DatabaseName::Main)?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(destination.as_ref()).map_err(|_| ServiceError::new("RESTORE_DESTINATION", "Choose a new nonexistent destination file in a writable folder. Existing files are never replaced."))?;
        file.write_all(&image).and_then(|_| file.sync_all()).map_err(|_| ServiceError::new("RESTORE_WRITE", "Could not finish writing the new recovery file. Do not use that incomplete file; choose another new destination and retry."))?;
        Ok(())
    }
}
fn make_export(name: &str, value: &impl Serialize) -> Result<ExportFile, ServiceError> {
    let mut content = serde_json::to_string_pretty(value).map_err(|_| corrupt())?;
    content.push('\n');
    Ok(ExportFile {
        filename: name.into(),
        media_type: "application/json".into(),
        content,
    })
}
