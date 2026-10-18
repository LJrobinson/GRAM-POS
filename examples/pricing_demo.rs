//! Run with `cargo run --example pricing_demo --offline`.
//! All amounts and policies below are illustrative, not merchant defaults.

use gram_pos_core::{
    price_cart, BelowSmallestPolicy, CartLine, Currency, LinePricing, Money, OveragePolicy,
    PriceSchedule, PricingError, PricingGroup, Quantity, Rounding, Tier, ToleranceScope, Weight,
};

fn illustrative_group(
    currency: Currency,
    policy: OveragePolicy,
) -> Result<PricingGroup, PricingError> {
    let offers = [
        ("3.5", "0.15", 3_000),
        ("7", "0.20", 5_500),
        ("14", "0.30", 10_000),
        ("28", "0.50", 18_000),
        ("56", "0.75", 34_000),
    ];
    let mut tiers = Vec::new();
    for (nominal, tolerance, minor_units) in offers {
        tiers.push(Tier {
            nominal_weight: Weight::from_grams_str(nominal)?,
            tolerance: Weight::from_grams_str(tolerance)?,
            price: Money::new(currency, minor_units),
        });
    }
    Ok(PricingGroup {
        id: "shared-price-band".into(),
        schedule: PriceSchedule {
            currency,
            tiers,
            overage_policy: policy,
            rounding: Rounding::HalfUp,
            tolerance_scope: ToleranceScope::AggregatePricingGroup,
            below_smallest: BelowSmallestPolicy::ProportionalAtSmallestTier,
        },
    })
}

fn weighed_line(id: &str, sku: &str, grams: &str) -> Result<CartLine, PricingError> {
    Ok(CartLine {
        id: id.into(),
        sku: sku.into(),
        quantity: Quantity::Measured {
            actual_weight: Weight::from_grams_str(grams)?,
        },
        pricing: LinePricing::WeightGroup {
            group_id: "shared-price-band".into(),
        },
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let currency = Currency::new("USD", 2)?;
    let group = illustrative_group(currency, OveragePolicy::FullActualAtTierRate)?;
    let lines = vec![
        weighed_line("sale-line-a", "bulk-sku-a", "3.62")?,
        weighed_line("sale-line-b", "bulk-sku-b", "3.58")?,
        CartLine {
            id: "sale-line-c".into(),
            sku: "sealed-package-sku".into(),
            quantity: Quantity::Prepacked {
                count: 2,
                declared_content: Weight::from_grams_str("3.5")?,
            },
            pricing: LinePricing::FixedUnit {
                unit_price: Money::new(currency, 3_500),
            },
        },
    ];
    let quote = price_cart(&lines, &[group])?;
    println!("Illustrative prices only. No tax, tender, inventory, or storage.");
    println!("Mix-and-match keeps each SKU and original quantity separate:");
    for line in &quote.lines {
        println!(
            "  {} / {}: {:?} -> {}",
            line.id, line.sku, line.quantity, line.amount
        );
    }
    let snapshot = &quote.groups[0];
    println!(
        "Group: actual {}, declared {}, eligible {}, nominal {}, billable {}, rule {:?}",
        snapshot.actual_measured_weight,
        snapshot.declared_package_weight,
        snapshot.eligible_weight,
        snapshot.selected_tier.nominal_weight,
        snapshot.billable_weight,
        snapshot.rule,
    );
    println!("Cart total: {}", quote.total);
    println!("The two fixed-price packages do not contribute to the weight group.");

    println!("\nSame 3.660 g measurement, two explicit overage policies:");
    for policy in [
        OveragePolicy::FullActualAtTierRate,
        OveragePolicy::ExcessBeyondToleranceAtTierRate,
    ] {
        let group = illustrative_group(currency, policy)?;
        let quote = price_cart(
            &[weighed_line("single-line", "bulk-sku-a", "3.660")?],
            &[group],
        )?;
        println!(
            "  {:?}: billable {}, amount {}",
            policy, quote.groups[0].billable_weight, quote.total
        );
    }
    println!("\nNominal 28 g is a configured offer, not a physical ounce conversion.");
    Ok(())
}
