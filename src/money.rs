use crate::PricingError;
use std::fmt;

/// Currency code plus explicitly chosen decimal exponent for its minor unit.
/// Code syntax is validated, not membership in a live currency registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Currency {
    code: [u8; 3],
    minor_unit_exponent: u8,
}

impl Currency {
    pub fn new(code: &str, minor_unit_exponent: u8) -> Result<Self, PricingError> {
        if code.len() != 3
            || !code.bytes().all(|b| b.is_ascii_uppercase())
            || minor_unit_exponent > 9
        {
            return Err(PricingError::InvalidCurrency);
        }
        let bytes = code.as_bytes();
        Ok(Self {
            code: [bytes[0], bytes[1], bytes[2]],
            minor_unit_exponent,
        })
    }

    pub fn code(&self) -> &str {
        // Construction guarantees three ASCII bytes; the representation is private.
        std::str::from_utf8(&self.code).expect("validated ASCII currency code")
    }

    pub const fn minor_unit_exponent(self) -> u8 {
        self.minor_unit_exponent
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code())
    }
}

/// A nonnegative amount in integer minor units. Zero-price offers and rounded
/// zero allocations are valid; sale quantities must still be positive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Money {
    pub currency: Currency,
    pub minor_units: u64,
}

impl Money {
    pub const fn new(currency: Currency, minor_units: u64) -> Self {
        Self {
            currency,
            minor_units,
        }
    }

    pub fn from_minor_str(currency: Currency, input: &str) -> Result<Self, PricingError> {
        if input.is_empty() || !input.bytes().all(|b| b.is_ascii_digit()) {
            return Err(PricingError::MalformedAmount);
        }
        let minor_units = input
            .parse()
            .map_err(|_| PricingError::Overflow("parsing minor units"))?;
        Ok(Self::new(currency, minor_units))
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let exponent = self.currency.minor_unit_exponent;
        if exponent == 0 {
            return write!(f, "{} {}", self.currency, self.minor_units);
        }
        let scale = 10_u64.pow(exponent as u32);
        write!(
            f,
            "{} {}.{:0width$}",
            self.currency,
            self.minor_units / scale,
            self.minor_units % scale,
            width = exponent as usize
        )
    }
}

/// Explicit rounding of a nonnegative exact amount to one currency minor unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounding {
    Floor,
    Ceil,
    HalfUp,
    HalfEven,
}

/// Unrounded minor-unit amount. The exact rational is kept in quote snapshots;
/// it need not be reduced. A zero denominator is rejected rather than panicking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RationalAmount {
    pub numerator: u128,
    pub denominator: u64,
}

impl RationalAmount {
    pub fn round(self, rounding: Rounding) -> Result<u64, PricingError> {
        if self.denominator == 0 {
            return Err(PricingError::NonPositiveInput(
                "rational amount denominator",
            ));
        }
        let divisor = u128::from(self.denominator);
        let quotient = self.numerator / divisor;
        let remainder = self.numerator % divisor;
        let twice_remainder = remainder
            .checked_mul(2)
            .ok_or(PricingError::Overflow("rounding amount"))?;
        let increment = match rounding {
            Rounding::Floor => false,
            Rounding::Ceil => remainder > 0,
            Rounding::HalfUp => twice_remainder >= divisor,
            Rounding::HalfEven => {
                twice_remainder > divisor || (twice_remainder == divisor && quotient % 2 == 1)
            }
        };
        let rounded = quotient
            .checked_add(u128::from(increment))
            .ok_or(PricingError::Overflow("rounding amount"))?;
        u64::try_from(rounded).map_err(|_| PricingError::Overflow("converting rounded minor units"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_rounding_distinguishes_ties() {
        let amount = RationalAmount {
            numerator: 5,
            denominator: 2,
        };
        assert_eq!(amount.round(Rounding::Floor).unwrap(), 2);
        assert_eq!(amount.round(Rounding::Ceil).unwrap(), 3);
        assert_eq!(amount.round(Rounding::HalfUp).unwrap(), 3);
        assert_eq!(amount.round(Rounding::HalfEven).unwrap(), 2);
        assert_eq!(
            RationalAmount {
                numerator: 7,
                denominator: 2
            }
            .round(Rounding::HalfEven)
            .unwrap(),
            4
        );
    }

    #[test]
    fn bad_denominators_and_overflow_fail() {
        assert!(RationalAmount {
            numerator: 1,
            denominator: 0
        }
        .round(Rounding::Floor)
        .is_err());
        assert!(RationalAmount {
            numerator: u128::MAX,
            denominator: 1
        }
        .round(Rounding::Floor)
        .is_err());
    }
}
