use crate::PricingError;
use std::fmt;

/// One million kilograms. Applies to each weight and every aggregated weight.
pub const MAX_WEIGHT_MILLIGRAMS: u64 = 1_000_000_000_000;

/// Exact physical mass in milligrams. Zero is useful for tolerance/accumulators;
/// sale quantities and nominal tier sizes must be positive and are validated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Weight(u64);

impl Weight {
    pub const ZERO: Self = Self(0);

    pub fn from_milligrams(milligrams: u64) -> Result<Self, PricingError> {
        if milligrams > MAX_WEIGHT_MILLIGRAMS {
            return Err(PricingError::WeightOutOfRange);
        }
        Ok(Self(milligrams))
    }

    /// Parse grams without floating point or implicit rounding. Accepted forms
    /// include `"3"`, `"3.5"`, and `"3.625"`. Signs, whitespace, exponents, omitted
    /// integer/fractional parts, and more than three fractional digits fail.
    pub fn from_grams_str(input: &str) -> Result<Self, PricingError> {
        let mut pieces = input.split('.');
        let whole = pieces.next().ok_or(PricingError::MalformedWeight)?;
        let fraction = pieces.next();
        if pieces.next().is_some() || whole.is_empty() || !whole.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(PricingError::MalformedWeight);
        }
        let whole: u64 = whole.parse().map_err(|_| PricingError::WeightOutOfRange)?;
        let fractional_mg = match fraction {
            None => 0,
            Some(part) => {
                if part.is_empty() || part.len() > 3 || !part.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(PricingError::MalformedWeight);
                }
                let value: u64 = part.parse().map_err(|_| PricingError::MalformedWeight)?;
                value * 10_u64.pow(3 - part.len() as u32)
            }
        };
        let mg = whole
            .checked_mul(1_000)
            .and_then(|w| w.checked_add(fractional_mg))
            .ok_or(PricingError::WeightOutOfRange)?;
        Self::from_milligrams(mg)
    }

    pub const fn milligrams(self) -> u64 {
        self.0
    }

    pub fn checked_add(self, other: Self) -> Result<Self, PricingError> {
        let mg = self
            .0
            .checked_add(other.0)
            .ok_or(PricingError::Overflow("adding weights"))?;
        Self::from_milligrams(mg)
    }

    pub fn checked_mul(self, count: u64) -> Result<Self, PricingError> {
        let mg = self
            .0
            .checked_mul(count)
            .ok_or(PricingError::Overflow("multiplying package content"))?;
        Self::from_milligrams(mg)
    }
}

impl fmt::Display for Weight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:03} g", self.0 / 1_000, self.0 % 1_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exactly_without_rounding() {
        assert_eq!(Weight::from_grams_str("3.625").unwrap().milligrams(), 3_625);
        assert_eq!(Weight::from_grams_str("0.001").unwrap().milligrams(), 1);
        assert_eq!(Weight::from_grams_str("3").unwrap().to_string(), "3.000 g");
        for input in [
            "", "-1", "+1", " 1", "1 ", ".1", "1.", "1e3", "1.2345", "1.2.3", "NaN", "inf",
        ] {
            assert!(Weight::from_grams_str(input).is_err(), "{input}");
        }
    }

    #[test]
    fn bounds_are_checked() {
        assert!(Weight::from_milligrams(MAX_WEIGHT_MILLIGRAMS).is_ok());
        assert!(Weight::from_milligrams(MAX_WEIGHT_MILLIGRAMS + 1).is_err());
        assert!(Weight::from_grams_str("1000000000.001").is_err());
        assert!(Weight::from_milligrams(MAX_WEIGHT_MILLIGRAMS)
            .unwrap()
            .checked_mul(2)
            .is_err());
    }
}
