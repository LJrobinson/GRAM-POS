use std::fmt;

/// Invalid inputs and checked-arithmetic failures never yield a partial quote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PricingError {
    MalformedWeight,
    WeightOutOfRange,
    MalformedAmount,
    InvalidCurrency,
    EmptyCart,
    EmptyTiers,
    NonPositiveInput(&'static str),
    InvalidIdentifier(&'static str),
    DuplicateLineId(String),
    DuplicateGroupId(String),
    UnknownGroup(String),
    CurrencyMismatch,
    UnorderedTiers,
    OverlappingToleranceBands,
    UnsupportedQuantityPricing,
    BelowSmallestTier { group_id: String },
    Overflow(&'static str),
}

impl fmt::Display for PricingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedWeight => write!(f, "weight must be unsigned decimal grams with at most three fractional digits"),
            Self::WeightOutOfRange => write!(f, "weight exceeds the documented milligram limit"),
            Self::MalformedAmount => write!(f, "minor-unit amount must contain only unsigned decimal digits"),
            Self::InvalidCurrency => write!(f, "currency requires three uppercase ASCII letters and a minor-unit exponent from 0 through 9"),
            Self::EmptyCart => write!(f, "cart must contain at least one line"),
            Self::EmptyTiers => write!(f, "price schedule must contain at least one tier"),
            Self::NonPositiveInput(field) => write!(f, "{field} must be positive"),
            Self::InvalidIdentifier(field) => write!(f, "{field} must be nonempty, trimmed, and free of control characters"),
            Self::DuplicateLineId(id) => write!(f, "duplicate cart line id: {id}"),
            Self::DuplicateGroupId(id) => write!(f, "duplicate pricing group id: {id}"),
            Self::UnknownGroup(id) => write!(f, "unknown pricing group: {id}"),
            Self::CurrencyMismatch => write!(f, "all prices in a cart must have the same currency and minor-unit exponent"),
            Self::UnorderedTiers => write!(f, "nominal tier weights must be strictly increasing"),
            Self::OverlappingToleranceBands => write!(f, "inclusive nominal-to-upper-tolerance bands must not overlap or touch"),
            Self::UnsupportedQuantityPricing => write!(f, "quantity and pricing are incompatible: units require fixed unit pricing, measured quantities require weight group pricing"),
            Self::BelowSmallestTier { group_id } => write!(f, "pricing group {group_id} is below its smallest allowed nominal tier"),
            Self::Overflow(operation) => write!(f, "arithmetic overflow while {operation}"),
        }
    }
}

impl std::error::Error for PricingError {}
