//! Exact, industry-neutral cart pricing, without I/O or external dependencies.
//!
//! Weight is physical mass at one-milligram resolution, not a package label or
//! an ounce conversion. Money is integer minor units. Merchants must configure
//! every rounding, tolerance, under-minimum, and overage decision explicitly.
//! No configuration implements a `Default` that silently chooses revenue policy.
//!
//! A [`Quantity::Measured`] preserves actual net mass. A [`Quantity::Prepacked`]
//! preserves an integer package count and declared content, never claiming that
//! declared content is a measured actual mass. Prepackages join a weight group
//! only through an explicit [`LinePricing::WeightGroup`] assignment.

#![forbid(unsafe_code)]

mod error;
mod money;
mod pricing;
mod weight;

pub use error::PricingError;
pub use money::{Currency, Money, RationalAmount, Rounding};
pub use pricing::*;
pub use weight::{Weight, MAX_WEIGHT_MILLIGRAMS};
