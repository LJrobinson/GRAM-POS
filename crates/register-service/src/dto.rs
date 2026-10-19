use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrencyDto {
    pub code: String,
    pub minor_unit_exponent: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierConfig {
    pub nominal_grams: String,
    pub tolerance_grams: String,
    pub price_minor: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupConfig {
    pub id: String,
    pub name: String,
    pub tiers: Vec<TierConfig>,
    pub overage_policy: String,
    pub rounding: String,
    pub below_smallest: String,
    pub tolerance_scope: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatalogPricing {
    WeightGroup { group_id: String },
    FixedUnit { unit_price_minor: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogItem {
    #[serde(default)]
    pub archived: bool,
    pub sku: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub barcode: Option<String>,
    pub kind: String,
    pub declared_content_grams: Option<String>,
    pub pricing: CatalogPricing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarcodeLookup {
    pub item: CatalogItem,
    pub config_revision: String,
}

/// Optional branding for new receipts. Legacy snapshots remain absent on serialization.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoreSettings {
    pub display_name: String,
    pub receipt_header: String,
    pub receipt_contact: String,
    pub receipt_footer: String,
    pub timezone: String,
}

impl Default for StoreSettings {
    fn default() -> Self {
        Self {
            display_name: "GRAM POS".into(),
            receipt_header: String::new(),
            receipt_contact: String::new(),
            receipt_footer: String::new(),
            timezone: "UTC".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store_settings: Option<StoreSettings>,
    pub schema_version: u32,
    pub demo: bool,
    pub tax_mode: String,
    pub currency: CurrencyDto,
    pub groups: Vec<GroupConfig>,
    pub catalog: Vec<CatalogItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InputQuantity {
    Measured { grams: String },
    Prepacked { count: String },
    Unit { count: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CartInputLine {
    pub id: String,
    pub sku: String,
    pub quantity: InputQuantity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CartInput {
    pub lines: Vec<CartInputLine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckoutInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_draft_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_draft_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_currency: Option<CurrencyDto>,
    pub idempotency_key: String,
    pub lines: Vec<CartInputLine>,
    pub tender_minor: String,
    pub expected_total_minor: String,
    pub expected_config_revision: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Explanation {
    FixedUnit {
        unit_price_minor: String,
        count: String,
    },
    GroupAllocation {
        group_id: String,
        contribution_grams: String,
        weight_source: String,
        group_eligible_grams: String,
        selected_nominal_grams: String,
        group_billable_grams: String,
        rule: String,
        allocation_remainder: String,
        received_remainder_unit: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteLine {
    pub id: String,
    pub sku: String,
    pub name: String,
    pub quantity: InputQuantity,
    pub declared_content_grams: Option<String>,
    pub amount_minor: String,
    pub explanation: Explanation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteGroup {
    pub group_id: String,
    pub actual_measured_grams: String,
    pub declared_package_grams: String,
    pub eligible_grams: String,
    pub selected_nominal_grams: String,
    pub tolerance_grams: String,
    pub billable_grams: String,
    pub rule: String,
    pub amount_minor: String,
    pub exact_numerator: String,
    pub exact_denominator: String,
    pub schedule: GroupConfig,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    pub config_revision: String,
    pub currency: CurrencyDto,
    pub lines: Vec<QuoteLine>,
    pub groups: Vec<QuoteGroup>,
    pub total_minor: String,
    pub tax_mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub sale_id: String,
    pub receipt_number: String,
    pub created_at_unix: String,
    pub quote: Quote,
    pub tender_minor: String,
    pub change_minor: String,
    pub payment_method: String,
    pub demo: bool,
    /// Full policy and catalog used for this sale, unaffected by later edits.
    pub config_snapshot: Config,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleSummary {
    pub sale_id: String,
    pub receipt_number: String,
    pub created_at_unix: String,
    pub total_minor: String,
    pub tender_minor: String,
    pub change_minor: String,
    pub currency: CurrencyDto,
    pub line_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub configured: bool,
    pub config_revision: String,
    pub config: Option<Config>,
    pub catalog: Vec<CatalogItem>,
    pub draft: CartInput,
    pub draft_revision: String,
    pub active_draft: Option<ActiveDraft>,
    pub parked_drafts: Vec<ParkedDraftSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenCashSessionInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_currency: Option<CurrencyDto>,
    pub idempotency_key: String,
    pub opening_float_minor: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloseCashSessionInput {
    pub session_id: String,
    pub counted_minor: String,
    pub expected_movement_count: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefundInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_currency: Option<CurrencyDto>,
    pub idempotency_key: String,
    pub sale_id: String,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefundReceipt {
    pub refund_id: String,
    pub refund_number: String,
    pub created_at_unix: String,
    pub sale_id: String,
    pub original_receipt_number: String,
    pub session_id: String,
    pub reason: String,
    pub amount_minor: String,
    pub currency: CurrencyDto,
    pub payment_method: String,
}
/// A cash-only drawer adjustment, never a sale or a refund.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CashMovementKind {
    PaidIn,
    PaidOut,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CashMovementInput {
    pub idempotency_key: String,
    pub kind: CashMovementKind,
    pub amount_minor: String,
    pub reason: String,
    pub expected_session_id: String,
    pub expected_currency: CurrencyDto,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CashMovement {
    pub movement_id: String,
    pub created_at_unix: String,
    pub session_id: String,
    pub kind: CashMovementKind,
    pub amount_minor: String,
    pub reason: String,
    pub currency: CurrencyDto,
}
fn zero_minor() -> String {
    "0".into()
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CashSession {
    pub session_id: String,
    pub opened_at_unix: String,
    pub currency: CurrencyDto,
    pub opening_float_minor: String,
    pub sales_total_minor: String,
    pub refunds_total_minor: String,
    #[serde(default = "zero_minor")]
    pub paid_in_total_minor: String,
    #[serde(default = "zero_minor")]
    pub paid_out_total_minor: String,
    pub expected_cash_minor: String,
    pub movement_count: String,
    pub closed_at_unix: Option<String>,
    pub counted_minor: Option<String>,
    pub variance_minor: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaleSession {
    pub sale_id: String,
    pub session_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CashLedger {
    pub active_session: Option<CashSession>,
    pub sessions: Vec<CashSession>,
    pub refunds: Vec<RefundReceipt>,
    pub cash_movements: Vec<CashMovement>,
    pub sale_sessions: Vec<SaleSession>,
}

/// UTC calendar dates: start midnight inclusive, end midnight exclusive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SalesReportInput {
    pub start_date: String,
    pub end_date: String,
}

/// Calendar dates in the explicitly selected IANA zone. Omitted zone means UTC.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SalesReportRequest {
    pub start_date: String,
    pub end_date: String,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub expected_config_revision: Option<String>,
}

impl From<SalesReportInput> for SalesReportRequest {
    fn from(input: SalesReportInput) -> Self {
        Self {
            start_date: input.start_date,
            end_date: input.end_date,
            timezone: None,
            expected_config_revision: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesReport {
    pub start_date: String,
    pub end_date: String,
    pub timezone: String,
    /// Resolved inclusive/exclusive UTC seconds, exact decimal strings.
    pub start_at_unix: String,
    pub end_at_unix: String,
    pub config_revision: String,
    pub groups: Vec<SalesReportGroup>,
    pub events: Vec<SalesReportEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesReportGroup {
    pub currency: CurrencyDto,
    pub gross_sales_minor: String,
    pub refunds_minor: String,
    pub net_sales_minor: String,
    pub sale_count: String,
    pub refund_count: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesReportEventKind {
    Sale,
    Refund,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesReportEvent {
    /// ISO 8601 local timestamp with numeric offset, including historical seconds.
    pub created_at_local: String,
    pub utc_offset_seconds: String,
    pub kind: SalesReportEventKind,
    /// Scoped by kind: sale ID for a sale, refund ID for a refund.
    pub event_id: String,
    pub sale_id: String,
    pub created_at_unix: String,
    pub currency: CurrencyDto,
    /// Nonnegative saved total; refunds are deducted only in grouped net totals.
    pub total_minor: String,
    /// Legacy sales have no session. Refunds always have their own session.
    pub session_id: Option<String>,
    pub reason: Option<String>,
}

/// A global revision fences every cart transition, including successful checkout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveDraft {
    pub id: String,
    pub revision: String,
    pub issues: Vec<DraftIssue>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParkedDraftSummary {
    pub id: String,
    pub label: Option<String>,
    pub parked_at_unix: String,
    pub line_count: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftIssue {
    /// Empty only for an issue affecting the complete pooled-price quote.
    pub line_id: String,
    pub code: String,
    pub message: String,
    pub blocking: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParkedDraftPreview {
    pub draft_id: String,
    pub label: Option<String>,
    pub lines: Vec<CartInputLine>,
    pub config_revision: String,
    pub currency: CurrencyDto,
    pub quote: Option<Quote>,
    pub issues: Vec<DraftIssue>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveActiveDraftInput {
    pub idempotency_key: String,
    pub expected_draft_revision: String,
    pub lines: Vec<CartInputLine>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParkDraftInput {
    pub idempotency_key: String,
    pub expected_draft_revision: String,
    #[serde(default)]
    pub label: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeParkedDraftInput {
    pub idempotency_key: String,
    pub expected_draft_revision: String,
    pub draft_id: String,
    pub expected_config_revision: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscardParkedDraftInput {
    pub idempotency_key: String,
    pub expected_draft_revision: String,
    pub draft_id: String,
}
