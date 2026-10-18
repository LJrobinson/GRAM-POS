//! Transport-neutral register service. Pricing lives only in `gram-pos-core`.
//! SQLite transactions store immutable demo cash sales and receipt snapshots.
//! This milestone deliberately has no production tax or payment integration.
#![forbid(unsafe_code)]

mod dto;
mod export;
pub use dto::*;
pub use export::ExportFile;

use gram_pos_core as core;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fmt,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const SCHEMA_VERSION: i64 = 1;
const APPLICATION_ID: i64 = 0x4752414D;
const MAX_LINES: usize = 500;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ServiceError {
    pub code: String,
    pub message: String,
}
impl ServiceError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ServiceError {}
impl From<rusqlite::Error> for ServiceError {
    fn from(_: rusqlite::Error) -> Self {
        // Do not expose local paths or SQL through frontend responses.
        Self::new("STORAGE_ERROR", "Local storage failed. Retry the same checkout key to reconcile any uncertain completion.")
    }
}
impl From<core::PricingError> for ServiceError {
    fn from(error: core::PricingError) -> Self {
        Self::new("PRICE_ERROR", error.to_string())
    }
}

pub struct RegisterService {
    conn: Connection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigInput {
    config: Config,
    expected_config_revision: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InitializeInput {
    currency: CurrencyDto,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptInput {
    sale_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyInput {
    idempotency_key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyInput {}

impl RegisterService {
    /// Open only the caller-selected local file. Never seeds catalog or policies.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ServiceError> {
        Self::from_connection(Connection::open(path)?)
    }
    pub fn in_memory() -> Result<Self, ServiceError> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(mut conn: Connection) -> Result<Self, ServiceError> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;")?;
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = transaction.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let app_id: i64 = transaction.query_row("PRAGMA application_id", [], |r| r.get(0))?;
        if !(0..=SCHEMA_VERSION).contains(&version) || (app_id != 0 && app_id != APPLICATION_ID) {
            return Err(ServiceError::new("SCHEMA_MISMATCH", "This local database belongs to an unsupported application or schema version. It was not modified."));
        }
        if version == 0 {
            let tables: i64 = transaction.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'", [], |r| r.get(0))?;
            if tables != 0 || app_id != 0 {
                return Err(ServiceError::new(
                    "SCHEMA_MISMATCH",
                    "Refusing to initialize a nonempty or unrecognized local database.",
                ));
            }
            transaction.execute_batch(
                "CREATE TABLE settings (singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL CHECK(revision>0), config_json TEXT NOT NULL);
                 CREATE TABLE drafts (singleton INTEGER PRIMARY KEY CHECK(singleton=1), draft_json TEXT NOT NULL);
                 CREATE TABLE sales (id INTEGER PRIMARY KEY AUTOINCREMENT, idempotency_key TEXT NOT NULL UNIQUE, request_json TEXT NOT NULL, created_at_unix TEXT NOT NULL, snapshot_json TEXT NOT NULL);
                 CREATE TABLE receipts (sale_id INTEGER PRIMARY KEY REFERENCES sales(id), receipt_json TEXT NOT NULL);
                 CREATE TRIGGER sales_no_update BEFORE UPDATE ON sales BEGIN SELECT RAISE(ABORT, 'Sales are immutable'); END;
                 CREATE TRIGGER sales_no_delete BEFORE DELETE ON sales BEGIN SELECT RAISE(ABORT, 'Sales are immutable'); END;
                 CREATE TRIGGER receipts_no_update BEFORE UPDATE ON receipts BEGIN SELECT RAISE(ABORT, 'Receipts are immutable'); END;
                 CREATE TRIGGER receipts_no_delete BEFORE DELETE ON receipts BEGIN SELECT RAISE(ABORT, 'Receipts are immutable'); END;
                 PRAGMA application_id = 1196573005;
                 PRAGMA user_version = 1;"
            )?;
        } else if app_id != APPLICATION_ID {
            return Err(ServiceError::new(
                "SCHEMA_MISMATCH",
                "This database is not a recognized GRAM register file.",
            ));
        }
        // Read required columns before accepting the file; corrupted/missing schema fails closed.
        transaction.prepare("SELECT s.revision, s.config_json, d.draft_json FROM settings s LEFT JOIN drafts d ON d.singleton=s.singleton")?;
        transaction.prepare("SELECT s.id, s.idempotency_key, s.request_json, s.created_at_unix, s.snapshot_json, r.receipt_json FROM sales s LEFT JOIN receipts r ON s.id=r.sale_id")?;
        transaction.commit()?;
        Ok(Self { conn })
    }

    /// JSON command boundary shared by desktop IPC and the opt-in development server.
    pub fn handle(&mut self, command: &str, payload: Value) -> Result<Value, ServiceError> {
        match command {
            "get_state" => {
                let _: EmptyInput = decode(payload)?;
                encode(self.state()?)
            }
            "initialize_demo" => {
                let _: EmptyInput = decode(payload)?;
                encode(self.initialize_demo()?)
            }
            "initialize_empty" => {
                let input: InitializeInput = decode(payload)?;
                encode(self.initialize_empty(input.currency)?)
            }
            "export_sales" => {
                let _: EmptyInput = decode(payload)?;
                encode(self.export_sales()?)
            }
            "export_backup" => {
                let _: EmptyInput = decode(payload)?;
                encode(self.export_backup()?)
            }
            "put_config" => {
                let input: ConfigInput = decode(payload)?;
                encode(self.put_config_checked(input.config, &input.expected_config_revision)?)
            }
            "quote_cart" => {
                let input: CartInput = decode(payload)?;
                encode(self.quote(&input.lines)?)
            }
            "complete_cash_sale" => {
                let input: CheckoutInput = decode(payload)?;
                encode(self.complete_cash_sale(input)?)
            }
            "save_draft" => {
                let input: CartInput = decode(payload)?;
                self.save_draft(input)?;
                Ok(json!({"saved":true}))
            }
            "get_receipt" => {
                let input: ReceiptInput = decode(payload)?;
                encode(self.receipt(&input.sale_id)?)
            }
            "get_sale_by_key" => {
                let input: KeyInput = decode(payload)?;
                validate_id(&input.idempotency_key, "Idempotency key")?;
                Ok(json!({"receipt": self.receipt_by_key(&input.idempotency_key)?}))
            }
            "list_sales" => {
                let _: EmptyInput = decode(payload)?;
                Ok(json!({"sales":self.list_sales()?}))
            }
            _ => Err(ServiceError::new(
                "UNKNOWN_COMMAND",
                "Unknown register command.",
            )),
        }
    }

    pub fn state(&self) -> Result<State, ServiceError> {
        // A single read transaction prevents configuration and draft being mixed across writes.
        let tx = self.conn.unchecked_transaction()?;
        let stored = read_config_optional(&tx)?;
        let draft_json: Option<String> = tx
            .query_row("SELECT draft_json FROM drafts WHERE singleton=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        let draft = draft_json
            .map(|s| decode_stored(&s))
            .transpose()?
            .unwrap_or(CartInput { lines: vec![] });
        let (revision, config) = stored
            .map(|(r, c)| (r.to_string(), Some(c)))
            .unwrap_or(("0".into(), None));
        let result = State {
            configured: config.is_some(),
            config_revision: revision,
            catalog: config
                .as_ref()
                .map(|c| c.catalog.clone())
                .unwrap_or_default(),
            config,
            draft,
        };
        tx.commit()?;
        Ok(result)
    }

    pub fn initialize_demo(&mut self) -> Result<State, ServiceError> {
        self.initialize(demo_config())
    }

    pub fn initialize_empty(&mut self, currency: CurrencyDto) -> Result<State, ServiceError> {
        self.initialize(Config {
            schema_version: 1,
            demo: true,
            tax_mode: "demo_no_tax".into(),
            currency,
            groups: vec![],
            catalog: vec![],
        })
    }

    fn initialize(&mut self, config: Config) -> Result<State, ServiceError> {
        validate_config(&config)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if read_config_optional(&tx)?.is_some() {
            return Err(ServiceError::new("ALREADY_CONFIGURED", "The register already has saved settings; demo initialization never overwrites them."));
        }
        tx.execute(
            "INSERT INTO settings(singleton,revision,config_json) VALUES(1,1,?1)",
            [serialize(&config)?],
        )?;
        tx.commit()?;
        self.state()
    }

    pub fn put_config(&mut self, config: Config) -> Result<State, ServiceError> {
        self.write_config(config, None)
    }

    pub fn put_config_checked(
        &mut self,
        config: Config,
        expected_revision: &str,
    ) -> Result<State, ServiceError> {
        let expected = parse_integer(expected_revision, "Expected configuration revision")?;
        self.write_config(config, Some(expected))
    }

    fn write_config(
        &mut self,
        config: Config,
        expected: Option<u64>,
    ) -> Result<State, ServiceError> {
        validate_config(&config)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<i64> = tx
            .query_row("SELECT revision FROM settings WHERE singleton=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        if expected.is_some_and(|value| value != previous.unwrap_or(0) as u64) {
            return Err(ServiceError::new("STALE_CONFIG", "Configuration changed in another window. Reload before editing; your changes were not saved."));
        }
        let revision = previous.unwrap_or(0).checked_add(1).ok_or_else(|| {
            ServiceError::new("INVALID_CONFIG", "Configuration revision limit reached.")
        })?;
        tx.execute("INSERT INTO settings(singleton,revision,config_json) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET revision=excluded.revision,config_json=excluded.config_json", params![revision, serialize(&config)?])?;
        tx.commit()?;
        self.state()
    }

    pub fn quote(&self, lines: &[CartInputLine]) -> Result<Quote, ServiceError> {
        let (revision, config) = read_config(&self.conn)?;
        quote_using_config(lines, &config, revision)
    }

    pub fn save_draft(&mut self, draft: CartInput) -> Result<(), ServiceError> {
        // A draft may be incomplete (e.g. blank weight while typing), but its shape and IDs must be safe.
        validate_draft(&draft.lines)?;
        self.conn.execute("INSERT INTO drafts(singleton,draft_json) VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET draft_json=excluded.draft_json", [serialize(&draft)?])?;
        Ok(())
    }

    pub fn complete_cash_sale(&mut self, input: CheckoutInput) -> Result<Receipt, ServiceError> {
        validate_id(&input.idempotency_key, "Idempotency key")?;
        let request_json = serialize(&input)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(String,String)> = tx.query_row(
            "SELECT s.request_json,r.receipt_json FROM sales s JOIN receipts r ON r.sale_id=s.id WHERE s.idempotency_key=?1", [&input.idempotency_key], |r| Ok((r.get(0)?,r.get(1)?))
        ).optional()?;
        if let Some((original, receipt)) = existing {
            if original != request_json {
                return Err(ServiceError::new("IDEMPOTENCY_CONFLICT", "This checkout key already belongs to a different completed request. Retrieve the original receipt; do not reuse this key."));
            }
            return decode_stored(&receipt);
        }
        let tender = parse_integer(&input.tender_minor, "Cash tender")?;
        let expected_total = parse_integer(&input.expected_total_minor, "Expected total")?;
        let expected_revision = parse_integer(
            &input.expected_config_revision,
            "Expected configuration revision",
        )?;
        let (revision, config) = read_config(&tx)?;
        let quote = quote_using_config(&input.lines, &config, revision)?;
        let total = parse_integer(&quote.total_minor, "Total")?;
        if expected_revision != revision as u64 || expected_total != total {
            return Err(ServiceError::new(
                "STALE_QUOTE",
                "Prices or policies changed. Review a fresh quote before collecting cash.",
            ));
        }
        let change = tender.checked_sub(total).ok_or_else(|| {
            ServiceError::new(
                "INSUFFICIENT_TENDER",
                "Cash tender is below the sale total. No sale was saved.",
            )
        })?;
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| {
                ServiceError::new(
                    "CLOCK_ERROR",
                    "The computer clock is before the Unix epoch.",
                )
            })?
            .as_secs()
            .to_string();
        let snapshot = json!({"quote":&quote,"config_snapshot":&config,"tender_minor":tender.to_string(),"change_minor":change.to_string(),"payment_method":"cash","demo":true});
        tx.execute("INSERT INTO sales(idempotency_key,request_json,created_at_unix,snapshot_json) VALUES(?1,?2,?3,?4)", params![input.idempotency_key,request_json,created_at,serialize(&snapshot)?])?;
        let id = tx.last_insert_rowid();
        let receipt = Receipt {
            sale_id: id.to_string(),
            receipt_number: format!("DEMO-{id:08}"),
            created_at_unix: created_at,
            quote,
            tender_minor: tender.to_string(),
            change_minor: change.to_string(),
            payment_method: "cash".into(),
            demo: true,
            config_snapshot: config,
        };
        tx.execute(
            "INSERT INTO receipts(sale_id,receipt_json) VALUES(?1,?2)",
            params![id, serialize(&receipt)?],
        )?;
        // Clearing the recovered cart is part of the same commit as the receipt.
        tx.execute("DELETE FROM drafts WHERE singleton=1", [])?;
        tx.commit()?;
        Ok(receipt)
    }

    pub fn receipt(&self, sale_id: &str) -> Result<Receipt, ServiceError> {
        let id = parse_integer(sale_id, "Sale ID")?;
        let id = i64::try_from(id)
            .map_err(|_| ServiceError::new("INVALID_INPUT", "Sale ID is out of range."))?;
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT receipt_json FROM receipts WHERE sale_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        value.map(|s| decode_stored(&s)).unwrap_or_else(|| {
            Err(ServiceError::new(
                "NOT_FOUND",
                "No local receipt was found for this sale.",
            ))
        })
    }

    pub fn receipt_by_key(&self, key: &str) -> Result<Option<Receipt>, ServiceError> {
        let value: Option<String> = self.conn.query_row("SELECT r.receipt_json FROM receipts r JOIN sales s ON s.id=r.sale_id WHERE s.idempotency_key=?1", [key], |r| r.get(0)).optional()?;
        value.map(|s| decode_stored(&s)).transpose()
    }

    pub fn list_sales(&self) -> Result<Vec<SaleSummary>, ServiceError> {
        let mut stmt = self
            .conn
            .prepare("SELECT receipt_json FROM receipts ORDER BY sale_id DESC")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut sales = Vec::new();
        for row in rows {
            let receipt: Receipt = decode_stored(&row?)?;
            sales.push(SaleSummary {
                sale_id: receipt.sale_id,
                receipt_number: receipt.receipt_number,
                created_at_unix: receipt.created_at_unix,
                total_minor: receipt.quote.total_minor,
                tender_minor: receipt.tender_minor,
                change_minor: receipt.change_minor,
                currency: receipt.quote.currency,
                line_count: receipt.quote.lines.len(),
            });
        }
        Ok(sales)
    }
}

fn encode<T: Serialize>(value: T) -> Result<Value, ServiceError> {
    serde_json::to_value(value)
        .map_err(|_| ServiceError::new("SERIALIZATION_ERROR", "Could not encode register data."))
}
fn serialize<T: Serialize>(value: &T) -> Result<String, ServiceError> {
    serde_json::to_string(value)
        .map_err(|_| ServiceError::new("SERIALIZATION_ERROR", "Could not encode register data."))
}
fn decode<T: DeserializeOwned>(value: Value) -> Result<T, ServiceError> {
    serde_json::from_value(value)
        .map_err(|e| ServiceError::new("INVALID_INPUT", format!("Invalid command payload: {e}")))
}
fn decode_stored<T: DeserializeOwned>(value: &str) -> Result<T, ServiceError> {
    serde_json::from_str(value).map_err(|_| {
        ServiceError::new(
            "STORAGE_ERROR",
            "Saved register data is invalid; the file was not overwritten.",
        )
    })
}
fn read_config_optional(conn: &Connection) -> Result<Option<(i64, Config)>, ServiceError> {
    let stored: Option<(i64, String)> = conn
        .query_row(
            "SELECT revision,config_json FROM settings WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    stored
        .map(|(revision, raw)| Ok((revision, decode_stored(&raw)?)))
        .transpose()
}
fn read_config(conn: &Connection) -> Result<(i64, Config), ServiceError> {
    read_config_optional(conn)?.ok_or_else(|| {
        ServiceError::new(
            "NOT_CONFIGURED",
            "Set up a catalog or load the explicit demo before quoting a sale.",
        )
    })
}
fn parse_integer(value: &str, field: &str) -> Result<u64, ServiceError> {
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ServiceError::new(
            "INVALID_INPUT",
            format!("{field} must be an exact nonnegative integer string."),
        ));
    }
    value.parse().map_err(|_| {
        ServiceError::new(
            "INVALID_INPUT",
            format!("{field} exceeds the supported integer range."),
        )
    })
}
fn validate_id(value: &str, field: &str) -> Result<(), ServiceError> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(ServiceError::new(
            "INVALID_INPUT",
            format!(
                "{field} must contain 1–128 characters without surrounding whitespace or controls."
            ),
        ));
    }
    Ok(())
}
fn validate_draft(lines: &[CartInputLine]) -> Result<(), ServiceError> {
    if lines.len() > MAX_LINES {
        return Err(ServiceError::new(
            "INVALID_INPUT",
            "A cart may contain at most 500 lines.",
        ));
    }
    let mut ids = BTreeSet::new();
    for line in lines {
        validate_id(&line.id, "Line ID")?;
        validate_id(&line.sku, "SKU")?;
        if !ids.insert(&line.id) {
            return Err(ServiceError::new(
                "INVALID_INPUT",
                "Cart line IDs must be unique.",
            ));
        }
        let text = match &line.quantity {
            InputQuantity::Measured { grams } => grams,
            InputQuantity::Prepacked { count } => count,
        };
        if text.len() > 32 || text.chars().any(char::is_control) {
            return Err(ServiceError::new(
                "INVALID_INPUT",
                "Draft quantity text is too long or contains control characters.",
            ));
        }
    }
    Ok(())
}
fn currency(dto: &CurrencyDto) -> Result<core::Currency, ServiceError> {
    Ok(core::Currency::new(&dto.code, dto.minor_unit_exponent)?)
}
fn grams(weight: core::Weight) -> String {
    format!(
        "{}.{:03}",
        weight.milligrams() / 1000,
        weight.milligrams() % 1000
    )
}
fn rule(rule: core::PricingRule) -> String {
    match rule {
        core::PricingRule::WithinNominalTolerance => "within_nominal_tolerance",
        core::PricingRule::BelowSmallestProportional => "below_smallest_proportional",
        core::PricingRule::FullActualAtTierRate => "full_actual_at_tier_rate",
        core::PricingRule::NominalPlusExcessBeyondTolerance => {
            "nominal_plus_excess_beyond_tolerance"
        }
    }
    .into()
}
fn group_to_core(
    group: &GroupConfig,
    currency: core::Currency,
) -> Result<core::PricingGroup, ServiceError> {
    let invalid = || {
        ServiceError::new(
            "INVALID_CONFIG",
            "Every pricing policy must be explicitly selected from supported values.",
        )
    };
    let overage_policy = match group.overage_policy.as_str() {
        "full_actual_at_tier_rate" => core::OveragePolicy::FullActualAtTierRate,
        "excess_beyond_tolerance_at_tier_rate" => {
            core::OveragePolicy::ExcessBeyondToleranceAtTierRate
        }
        _ => return Err(invalid()),
    };
    let rounding = match group.rounding.as_str() {
        "floor" => core::Rounding::Floor,
        "ceil" => core::Rounding::Ceil,
        "half_up" => core::Rounding::HalfUp,
        "half_even" => core::Rounding::HalfEven,
        _ => return Err(invalid()),
    };
    let below_smallest = match group.below_smallest.as_str() {
        "proportional_at_smallest_tier" => core::BelowSmallestPolicy::ProportionalAtSmallestTier,
        "reject" => core::BelowSmallestPolicy::Reject,
        _ => return Err(invalid()),
    };
    if group.tolerance_scope != "aggregate_pricing_group" {
        return Err(invalid());
    }
    let tiers = group
        .tiers
        .iter()
        .map(|tier| {
            Ok(core::Tier {
                nominal_weight: core::Weight::from_grams_str(&tier.nominal_grams)?,
                tolerance: core::Weight::from_grams_str(&tier.tolerance_grams)?,
                price: core::Money::from_minor_str(currency, &tier.price_minor)?,
            })
        })
        .collect::<Result<Vec<_>, ServiceError>>()?;
    let schedule = core::PriceSchedule {
        currency,
        tiers,
        overage_policy,
        rounding,
        below_smallest,
        tolerance_scope: core::ToleranceScope::AggregatePricingGroup,
    };
    schedule.validate()?;
    Ok(core::PricingGroup {
        id: group.id.clone(),
        schedule,
    })
}

pub fn validate_config(config: &Config) -> Result<(), ServiceError> {
    validate_config_inner(config).map_err(|e| ServiceError::new("INVALID_CONFIG", e.message))
}
fn validate_config_inner(config: &Config) -> Result<(), ServiceError> {
    if config.schema_version != 1 || !config.demo || config.tax_mode != "demo_no_tax" {
        return Err(ServiceError::new("INVALID_CONFIG", "This release supports explicit demo configuration only, with no tax calculated. Production sales are not enabled."));
    }
    if config.groups.len() > 100 || config.catalog.len() > 1000 {
        return Err(ServiceError::new(
            "INVALID_CONFIG",
            "Use 0–1000 catalog items and at most 100 price groups.",
        ));
    }
    let cur = currency(&config.currency)?;
    let mut groups = BTreeSet::new();
    for group in &config.groups {
        validate_id(&group.id, "Group ID")?;
        validate_id(&group.name, "Group name")?;
        if !groups.insert(group.id.as_str()) {
            return Err(ServiceError::new(
                "INVALID_CONFIG",
                "Pricing group IDs must be unique.",
            ));
        }
        if group.tiers.len() > 100 {
            return Err(ServiceError::new(
                "INVALID_CONFIG",
                "A group may contain at most 100 tiers.",
            ));
        }
        group_to_core(group, cur)?;
    }
    let mut skus = BTreeSet::new();
    for item in &config.catalog {
        validate_id(&item.sku, "SKU")?;
        validate_id(&item.name, "Product name")?;
        if !skus.insert(&item.sku) {
            return Err(ServiceError::new(
                "INVALID_CONFIG",
                "Catalog SKUs must be unique.",
            ));
        }
        match item.kind.as_str() {
            "measured" if item.declared_content_grams.is_none() => {},
            "prepacked" => {
                let declared = item.declared_content_grams.as_deref().ok_or_else(|| ServiceError::new("INVALID_CONFIG","A prepack needs declared content grams."))?;
                if core::Weight::from_grams_str(declared)? == core::Weight::ZERO { return Err(ServiceError::new("INVALID_CONFIG","Declared package content must be positive.")); }
            },
            _ => return Err(ServiceError::new("INVALID_CONFIG","Product kind must be measured or prepacked; measured products cannot have declared package content.")),
        }
        match &item.pricing {
            CatalogPricing::WeightGroup { group_id } => {
                if !groups.contains(group_id.as_str()) {
                    return Err(ServiceError::new(
                        "INVALID_CONFIG",
                        "Catalog product refers to an unknown price group.",
                    ));
                }
            }
            CatalogPricing::FixedUnit { unit_price_minor } => {
                if item.kind != "prepacked" {
                    return Err(ServiceError::new(
                        "INVALID_CONFIG",
                        "Only prepackages can use fixed unit prices.",
                    ));
                }
                core::Money::from_minor_str(cur, unit_price_minor)?;
            }
        }
    }
    Ok(())
}

fn quote_using_config(
    lines: &[CartInputLine],
    config: &Config,
    revision: i64,
) -> Result<Quote, ServiceError> {
    validate_config(config)?;
    validate_draft(lines)?;
    let cur = currency(&config.currency)?;
    let groups = config
        .groups
        .iter()
        .map(|g| group_to_core(g, cur))
        .collect::<Result<Vec<_>, _>>()?;
    let mut core_lines = Vec::new();
    let mut catalog_items = Vec::new();
    for line in lines {
        let item = config
            .catalog
            .iter()
            .find(|i| i.sku == line.sku)
            .ok_or_else(|| {
                ServiceError::new(
                    "INVALID_INPUT",
                    format!("Unknown catalog SKU: {}", line.sku),
                )
            })?;
        if item.archived {
            return Err(ServiceError::new(
                "ARCHIVED_PRODUCT",
                format!(
                    "{} is archived. Remove it from the cart or unarchive it before selling.",
                    item.name
                ),
            ));
        }
        let quantity = match &line.quantity {
            InputQuantity::Measured { grams } if item.kind == "measured" => {
                core::Quantity::Measured {
                    actual_weight: core::Weight::from_grams_str(grams)?,
                }
            }
            InputQuantity::Prepacked { count } if item.kind == "prepacked" => {
                core::Quantity::Prepacked {
                    count: parse_integer(count, "Package count")?,
                    declared_content: core::Weight::from_grams_str(
                        item.declared_content_grams.as_deref().ok_or_else(|| {
                            ServiceError::new("INVALID_CONFIG", "Missing declared content.")
                        })?,
                    )?,
                }
            }
            _ => {
                return Err(ServiceError::new(
                    "INVALID_INPUT",
                    "Quantity kind does not match this catalog product.",
                ))
            }
        };
        let pricing = match &item.pricing {
            CatalogPricing::WeightGroup { group_id } => core::LinePricing::WeightGroup {
                group_id: group_id.clone(),
            },
            CatalogPricing::FixedUnit { unit_price_minor } => core::LinePricing::FixedUnit {
                unit_price: core::Money::from_minor_str(cur, unit_price_minor)?,
            },
        };
        core_lines.push(core::CartLine {
            id: line.id.clone(),
            sku: line.sku.clone(),
            quantity,
            pricing,
        });
        catalog_items.push(item);
    }
    let priced = core::price_cart(&core_lines, &groups)?;
    let quote_lines = priced
        .lines
        .into_iter()
        .zip(lines)
        .zip(catalog_items)
        .map(|((line, input), item)| {
            let explanation = match line.explanation {
                core::LineExplanation::FixedUnit { unit_price, count } => Explanation::FixedUnit {
                    unit_price_minor: unit_price.minor_units.to_string(),
                    count: count.to_string(),
                },
                core::LineExplanation::GroupAllocation {
                    group_id,
                    contribution_weight,
                    weight_source,
                    group_eligible_weight,
                    selected_nominal_weight,
                    group_billable_weight,
                    rule: pricing_rule,
                    allocation_remainder,
                    received_remainder_unit,
                } => Explanation::GroupAllocation {
                    group_id,
                    contribution_grams: grams(contribution_weight),
                    weight_source: match weight_source {
                        core::WeightSource::ActualMeasured => "actual_measured",
                        core::WeightSource::DeclaredPackageContent => "declared_package_content",
                    }
                    .into(),
                    group_eligible_grams: grams(group_eligible_weight),
                    selected_nominal_grams: grams(selected_nominal_weight),
                    group_billable_grams: grams(group_billable_weight),
                    rule: rule(pricing_rule),
                    allocation_remainder: allocation_remainder.to_string(),
                    received_remainder_unit,
                },
            };
            QuoteLine {
                id: line.id,
                sku: line.sku,
                name: item.name.clone(),
                quantity: input.quantity.clone(),
                declared_content_grams: item.declared_content_grams.clone(),
                amount_minor: line.amount.minor_units.to_string(),
                explanation,
            }
        })
        .collect();
    let quote_groups = priced
        .groups
        .into_iter()
        .map(|group| {
            let schedule = config
                .groups
                .iter()
                .find(|g| g.id == group.group_id)
                .expect("core only returns configured groups")
                .clone();
            QuoteGroup {
                group_id: group.group_id,
                actual_measured_grams: grams(group.actual_measured_weight),
                declared_package_grams: grams(group.declared_package_weight),
                eligible_grams: grams(group.eligible_weight),
                selected_nominal_grams: grams(group.selected_tier.nominal_weight),
                tolerance_grams: grams(group.selected_tier.tolerance),
                billable_grams: grams(group.billable_weight),
                rule: rule(group.rule),
                amount_minor: group.amount.minor_units.to_string(),
                exact_numerator: group.exact_amount.numerator.to_string(),
                exact_denominator: group.exact_amount.denominator.to_string(),
                schedule,
            }
        })
        .collect();
    Ok(Quote {
        config_revision: revision.to_string(),
        currency: config.currency.clone(),
        lines: quote_lines,
        groups: quote_groups,
        total_minor: priced.total.minor_units.to_string(),
        tax_mode: config.tax_mode.clone(),
    })
}

/// Illustrative fixtures only. Callers must explicitly choose initialization.
pub fn demo_config() -> Config {
    Config {
        schema_version: 1,
        demo: true,
        tax_mode: "demo_no_tax".into(),
        currency: CurrencyDto {
            code: "USD".into(),
            minor_unit_exponent: 2,
        },
        groups: vec![GroupConfig {
            id: "shared-demo-band".into(),
            name: "Demo mix & match".into(),
            tiers: [
                ("3.5", "0.15", "3000"),
                ("7", "0.20", "5500"),
                ("14", "0.30", "10000"),
                ("28", "0.50", "18000"),
                ("56", "0.75", "34000"),
            ]
            .into_iter()
            .map(|(nominal, tolerance, price)| TierConfig {
                nominal_grams: nominal.into(),
                tolerance_grams: tolerance.into(),
                price_minor: price.into(),
            })
            .collect(),
            overage_policy: "full_actual_at_tier_rate".into(),
            rounding: "half_up".into(),
            below_smallest: "proportional_at_smallest_tier".into(),
            tolerance_scope: "aggregate_pricing_group".into(),
        }],
        catalog: vec![
            CatalogItem {
                archived: false,
                sku: "bulk-a".into(),
                name: "Demo Bulk A".into(),
                kind: "measured".into(),
                declared_content_grams: None,
                pricing: CatalogPricing::WeightGroup {
                    group_id: "shared-demo-band".into(),
                },
            },
            CatalogItem {
                archived: false,
                sku: "bulk-b".into(),
                name: "Demo Bulk B".into(),
                kind: "measured".into(),
                declared_content_grams: None,
                pricing: CatalogPricing::WeightGroup {
                    group_id: "shared-demo-band".into(),
                },
            },
            CatalogItem {
                archived: false,
                sku: "prepack-mix".into(),
                name: "Demo 3.5 g Mix Pack".into(),
                kind: "prepacked".into(),
                declared_content_grams: Some("3.5".into()),
                pricing: CatalogPricing::WeightGroup {
                    group_id: "shared-demo-band".into(),
                },
            },
            CatalogItem {
                archived: false,
                sku: "prepack-fixed".into(),
                name: "Demo 3.5 g Fixed Pack".into(),
                kind: "prepacked".into(),
                declared_content_grams: Some("3.5".into()),
                pricing: CatalogPricing::FixedUnit {
                    unit_price_minor: "3500".into(),
                },
            },
        ],
    }
}
