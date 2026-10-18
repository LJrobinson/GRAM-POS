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
