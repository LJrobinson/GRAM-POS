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
    pub kind: String,
    pub declared_content_grams: Option<String>,
    pub pricing: CatalogPricing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CashSession {
    pub session_id: String,
    pub opened_at_unix: String,
    pub currency: CurrencyDto,
    pub opening_float_minor: String,
    pub sales_total_minor: String,
    pub refunds_total_minor: String,
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
    pub sale_sessions: Vec<SaleSession>,
}

/// UTC calendar dates: start midnight inclusive, end midnight exclusive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SalesReportInput {
    pub start_date: String,
    pub end_date: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesReport {
    pub start_date: String,
    pub end_date: String,
    pub timezone: String,
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
