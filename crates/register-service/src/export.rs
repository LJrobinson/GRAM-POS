//! Versioned logical exports. Stored JSON is retained byte-for-byte for idempotency.
use super::*;
use std::{fs::OpenOptions, io::Write};

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
struct Backup {
    format: String,
    format_version: u32,
    schema_version: i64,
    settings: Option<SettingsBackup>,
    draft_json: Option<String>,
    sales: Vec<SaleBackup>,
    sales_sequence: String,
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
            || self.format_version != 1
            || self.schema_version != SCHEMA_VERSION
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
        let backup = Backup {
            format: "gram-pos-backup".into(),
            format_version: 1,
            schema_version: SCHEMA_VERSION,
            settings,
            draft_json,
            sales,
            sales_sequence: sequence.unwrap_or(0).to_string(),
        };
        backup.validate()?;
        tx.commit()?;
        Ok(backup)
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
        make_export(
            "gram-pos-sales.json",
            &json!({"format":"gram-pos-sales","format_version":1,"receipts":receipts}),
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
