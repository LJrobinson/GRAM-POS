//! Independent acceptance tests. Prices below are synthetic fixtures, never merchant defaults.

use gram_pos_core::*;
use std::collections::BTreeMap;

fn weight(mg: u64) -> Weight {
    Weight::from_milligrams(mg).unwrap()
}

fn usd() -> Currency {
    Currency::new("USD", 2).unwrap()
}

fn money(minor_units: u64) -> Money {
    Money::new(usd(), minor_units)
}

fn schedule(tiers: &[(u64, u64, u64)], policy: OveragePolicy) -> PriceSchedule {
    PriceSchedule {
        currency: usd(),
        tiers: tiers
            .iter()
            .map(|&(nominal, tolerance, minor)| Tier {
                nominal_weight: weight(nominal),
                tolerance: weight(tolerance),
                price: money(minor),
            })
            .collect(),
        overage_policy: policy,
        rounding: Rounding::HalfUp,
        tolerance_scope: ToleranceScope::AggregatePricingGroup,
        below_smallest: BelowSmallestPolicy::ProportionalAtSmallestTier,
    }
}

fn group(id: &str, tiers: &[(u64, u64, u64)], policy: OveragePolicy) -> PricingGroup {
    PricingGroup {
        id: id.into(),
        schedule: schedule(tiers, policy),
    }
}

fn measured(id: &str, sku: &str, mg: u64, group_id: &str) -> CartLine {
    CartLine {
        id: id.into(),
        sku: sku.into(),
        quantity: Quantity::Measured {
            actual_weight: weight(mg),
        },
        pricing: LinePricing::WeightGroup {
            group_id: group_id.into(),
        },
    }
}

fn prepack(id: &str, count: u64, content_mg: u64, pricing: LinePricing) -> CartLine {
    CartLine {
        id: id.into(),
        sku: "sealed-pack".into(),
        quantity: Quantity::Prepacked {
            count,
            declared_content: weight(content_mg),
        },
        pricing,
    }
}

fn single_total(mg: u64, schedule: PriceSchedule) -> u64 {
    price_cart(
        &[measured("line", "deli", mg, "g")],
        &[PricingGroup {
            id: "g".into(),
            schedule,
        }],
    )
    .unwrap()
    .total
    .minor_units
}

fn amounts_by_id(cart: &PricedCart) -> BTreeMap<String, u64> {
    cart.lines
        .iter()
        .map(|line| (line.id.clone(), line.amount.minor_units))
        .collect()
}

#[test]
fn exact_decimal_grams_have_no_float_conversion() {
    for (text, mg) in [("0", 0), ("3.5", 3500), ("3.650", 3650), ("0.001", 1)] {
        assert_eq!(Weight::from_grams_str(text).unwrap().milligrams(), mg);
    }
    for text in ["NaN", "inf", "-1", "1e3", "3.6501", "", "abc"] {
        assert!(Weight::from_grams_str(text).is_err(), "accepted {text:?}");
    }
}

#[test]
fn adapted_cannabismath_price_per_gram_fixture() {
    // Pinned upstream fixture: $35 / 3.5 g = $10 / g.
    // This tests proportional pricing, not parity with the upstream app.
    assert_eq!(
        single_total(
            1000,
            schedule(&[(3500, 0, 3500)], OveragePolicy::FullActualAtTierRate)
        ),
        1000
    );
}

#[test]
fn adapted_cannabismath_zero_price_fixture_allows_explicit_free_offer() {
    assert_eq!(
        single_total(
            1000,
            schedule(&[(3500, 0, 0)], OveragePolicy::FullActualAtTierRate)
        ),
        0
    );
    let result = price_cart(
        &[prepack(
            "free",
            2,
            3500,
            LinePricing::FixedUnit {
                unit_price: money(0),
            },
        )],
        &[],
    )
    .unwrap();
    assert_eq!(result.total.minor_units, 0);
}

#[test]
fn ounce_labels_and_submilligram_precision_are_never_silently_reinterpreted() {
    assert_eq!(Weight::from_grams_str("28.000").unwrap(), weight(28000));
    // A physical avoirdupois ounce is 28.349523125 g, not a retail 28 g label.
    // Neither conversion nor an implicit precision policy is part of this API.
    assert!(Weight::from_grams_str("28.349523125").is_err());
    for value in ["1 oz", "1 ounce", "28g", "28 g"] {
        assert!(Weight::from_grams_str(value).is_err());
    }
    assert_ne!(Weight::from_grams_str("28.350").unwrap(), weight(28000));
}

#[test]
fn weight_limit_and_decimal_overflow_are_checked() {
    assert!(Weight::from_milligrams(1_000_000_000_000).is_ok());
    assert!(Weight::from_milligrams(1_000_000_000_001).is_err());
    assert!(Weight::from_milligrams(u64::MAX).is_err());
    assert!(Weight::from_grams_str("18446744073709551616").is_err());
}

#[test]
fn tolerance_is_overfill_only_and_upper_boundary_is_inclusive() {
    for (mg, expected) in [(3499, 3499), (3500, 3500), (3650, 3500), (3651, 3651)] {
        assert_eq!(
            single_total(
                mg,
                schedule(&[(3500, 150, 3500)], OveragePolicy::FullActualAtTierRate)
            ),
            expected,
            "actual mg = {mg}"
        );
    }
}

#[test]
fn excess_policy_charges_only_weight_beyond_upper_tolerance_boundary() {
    for (mg, expected) in [
        (3499, 3499),
        (3500, 3500),
        (3650, 3500),
        (3651, 3501),
        (3660, 3510),
    ] {
        assert_eq!(
            single_total(
                mg,
                schedule(
                    &[(3500, 150, 3500)],
                    OveragePolicy::ExcessBeyondToleranceAtTierRate
                )
            ),
            expected,
            "actual mg = {mg}"
        );
    }
}

#[test]
fn full_actual_policy_bills_all_3660_mg_not_only_excess() {
    assert_eq!(
        single_total(
            3660,
            schedule(&[(3500, 150, 3500)], OveragePolicy::FullActualAtTierRate)
        ),
        3660
    );
}

#[test]
fn below_smallest_rejection_is_explicit_and_does_not_snap_underweight() {
    let mut g = group(
        "g",
        &[(3500, 150, 3500)],
        OveragePolicy::FullActualAtTierRate,
    );
    g.schedule.below_smallest = BelowSmallestPolicy::Reject;
    assert!(price_cart(
        &[measured("a", "deli", 3499, "g")],
        std::slice::from_ref(&g)
    )
    .is_err());
    assert!(price_cart(&[measured("a", "deli", 3500, "g")], &[g]).is_ok());
}

#[test]
fn all_requested_tiers_are_configuration_not_invented_prices() {
    let tiers = [
        (7000, 0, 7000),
        (14000, 0, 12600),
        (28000, 0, 22400),
        (56000, 0, 39200),
    ];
    for (mg, expected) in [
        (7000, 7000),
        (13999, 13999),
        (14000, 12600),
        (28000, 22400),
        (56000, 39200),
        (112000, 78400),
    ] {
        assert_eq!(
            single_total(mg, schedule(&tiers, OveragePolicy::FullActualAtTierRate)),
            expected,
            "actual mg = {mg}"
        );
    }
}

#[test]
fn mix_match_aggregates_different_skus_before_selecting_tier() {
    let g = group(
        "mix",
        &[(7000, 0, 700), (14000, 0, 1200)],
        OveragePolicy::FullActualAtTierRate,
    );
    let result = price_cart(
        &[
            measured("a", "apple", 7000, "mix"),
            measured("b", "pear", 7000, "mix"),
        ],
        &[g],
    )
    .unwrap();
    assert_eq!(result.total.minor_units, 1200);
    assert_eq!(
        amounts_by_id(&result),
        BTreeMap::from([("a".into(), 600), ("b".into(), 600)])
    );
}

#[test]
fn ineligible_fixed_prepack_does_not_contribute_to_group_tiers() {
    let g = group(
        "mix",
        &[(7000, 0, 700), (14000, 0, 1200)],
        OveragePolicy::FullActualAtTierRate,
    );
    let result = price_cart(
        &[
            measured("a", "loose", 7000, "mix"),
            prepack(
                "b",
                1,
                7000,
                LinePricing::FixedUnit {
                    unit_price: money(900),
                },
            ),
        ],
        &[g],
    )
    .unwrap();
    assert_eq!(result.total.minor_units, 1600);
    assert_eq!(amounts_by_id(&result)["a"], 700);
    assert_eq!(amounts_by_id(&result)["b"], 900);
}

#[test]
fn prepack_count_multiplies_fixed_unit_price_without_weight_discount() {
    let result = price_cart(
        &[prepack(
            "a",
            3,
            3500,
            LinePricing::FixedUnit {
                unit_price: money(499),
            },
        )],
        &[],
    )
    .unwrap();
    assert_eq!(result.total.minor_units, 1497);
}

#[test]
fn prepack_weight_participates_only_on_explicit_group_assignment() {
    let g = group(
        "mix",
        &[(7000, 0, 700), (14000, 0, 1200)],
        OveragePolicy::FullActualAtTierRate,
    );
    let result = price_cart(
        &[
            measured("a", "loose", 7000, "mix"),
            prepack(
                "b",
                2,
                3500,
                LinePricing::WeightGroup {
                    group_id: "mix".into(),
                },
            ),
        ],
        &[g],
    )
    .unwrap();
    assert_eq!(result.total.minor_units, 1200);
}

#[test]
fn unrelated_groups_do_not_combine_even_with_identical_schedules() {
    let g = group(
        "a",
        &[(7000, 0, 700), (14000, 0, 1200)],
        OveragePolicy::FullActualAtTierRate,
    );
    let mut other = g.clone();
    other.id = "b".into();
    let result = price_cart(
        &[
            measured("one", "x", 7000, "a"),
            measured("two", "x", 7000, "b"),
        ],
        &[g, other],
    )
    .unwrap();
    assert_eq!(result.total.minor_units, 1400);
}

#[test]
fn tolerance_is_applied_once_per_group_not_once_per_line() {
    let g = group(
        "g",
        &[(3500, 150, 3500), (7000, 150, 7000)],
        OveragePolicy::FullActualAtTierRate,
    );
    let split = price_cart(
        &[measured("a", "x", 3650, "g"), measured("b", "y", 3650, "g")],
        std::slice::from_ref(&g),
    )
    .unwrap();
    let combined = price_cart(&[measured("z", "x", 7300, "g")], &[g]).unwrap();
    assert_eq!(split.total.minor_units, 7300);
    assert_eq!(split.total, combined.total);
}

#[test]
fn largest_remainder_breaks_equal_ties_by_stable_line_id() {
    let g = group("g", &[(3, 0, 2)], OveragePolicy::FullActualAtTierRate);
    let cart = vec![
        measured("c", "x", 1, "g"),
        measured("a", "x", 1, "g"),
        measured("b", "x", 1, "g"),
    ];
    let result = price_cart(&cart, std::slice::from_ref(&g)).unwrap();
    assert_eq!(result.total.minor_units, 2);
    assert_eq!(
        amounts_by_id(&result),
        BTreeMap::from([("a".into(), 1), ("b".into(), 1), ("c".into(), 0)])
    );
    assert_eq!(
        result
            .lines
            .iter()
            .map(|l| l.id.as_str())
            .collect::<Vec<_>>(),
        vec!["c", "a", "b"]
    );
    let reversed = cart.into_iter().rev().collect::<Vec<_>>();
    assert_eq!(
        amounts_by_id(&price_cart(&reversed, &[g]).unwrap()),
        amounts_by_id(&result)
    );
}

#[test]
fn largest_remainder_allocates_to_largest_fraction_not_largest_line() {
    let g = group("g", &[(10, 0, 7)], OveragePolicy::FullActualAtTierRate);
    let result = price_cart(
        &[
            measured("a", "x", 1, "g"),
            measured("b", "x", 2, "g"),
            measured("c", "x", 7, "g"),
        ],
        &[g],
    )
    .unwrap();
    // Exact shares are 0.7, 1.4, 4.9. The two remaining cents go to c then a.
    assert_eq!(
        amounts_by_id(&result),
        BTreeMap::from([("a".into(), 1), ("b".into(), 1), ("c".into(), 5)])
    );
}

#[test]
fn rounding_mode_is_explicit_at_half_minor_units() {
    for (rounding, expected) in [
        (Rounding::Floor, 2),
        (Rounding::Ceil, 3),
        (Rounding::HalfUp, 3),
        (Rounding::HalfEven, 2),
    ] {
        let mut s = schedule(&[(2, 0, 1)], OveragePolicy::FullActualAtTierRate);
        s.rounding = rounding;
        assert_eq!(single_total(5, s), expected);
    }
    let mut s = schedule(&[(2, 0, 1)], OveragePolicy::FullActualAtTierRate);
    s.rounding = Rounding::HalfEven;
    assert_eq!(single_total(7, s), 4);
}

#[test]
fn repricing_preserves_original_actual_and_declared_weights() {
    let cart = vec![
        measured("actual", "x", 3650, "g"),
        prepack(
            "sealed",
            2,
            3500,
            LinePricing::FixedUnit {
                unit_price: money(100),
            },
        ),
    ];
    let before = cart.clone();
    let g = group(
        "g",
        &[(3500, 150, 3500)],
        OveragePolicy::FullActualAtTierRate,
    );
    let result = price_cart(&cart, &[g]).unwrap();
    for ((input, original), output) in cart.iter().zip(&before).zip(&result.lines) {
        assert_eq!(input.quantity, original.quantity);
        assert_eq!(output.quantity, original.quantity);
    }
    assert_eq!(result.lines[0].amount.minor_units, 3500);
}

#[test]
fn nominal_quote_snapshot_preserves_actual_mass_and_original_configuration() {
    let mut g = group(
        "g",
        &[(3500, 150, 3500)],
        OveragePolicy::FullActualAtTierRate,
    );
    let original_schedule = g.schedule.clone();
    let result = price_cart(
        &[measured("a", "loose", 3650, "g")],
        std::slice::from_ref(&g),
    )
    .unwrap();
    let snapshot = &result.groups[0];
    assert_eq!(snapshot.group_id, "g");
    assert_eq!(snapshot.actual_measured_weight, weight(3650));
    assert_eq!(snapshot.declared_package_weight, Weight::ZERO);
    assert_eq!(snapshot.eligible_weight, weight(3650));
    assert_eq!(snapshot.selected_tier.nominal_weight, weight(3500));
    assert_eq!(snapshot.billable_weight, weight(3500));
    assert_eq!(snapshot.rule, PricingRule::WithinNominalTolerance);
    assert_eq!(
        snapshot.exact_amount,
        RationalAmount {
            numerator: 3500 * 3500,
            denominator: 3500
        }
    );
    assert_eq!(snapshot.amount, money(3500));
    g.schedule.tiers[0].price = money(9999);
    g.schedule.tiers[0].tolerance = weight(0);
    assert_eq!(snapshot.schedule, original_schedule);
    assert_eq!(snapshot.selected_tier, original_schedule.tiers[0]);
}

#[test]
fn mixed_source_snapshot_does_not_describe_declared_packages_as_measured_mass() {
    let g = group(
        "g",
        &[(3500, 150, 3500), (7000, 200, 6000)],
        OveragePolicy::ExcessBeyondToleranceAtTierRate,
    );
    let result = price_cart(
        &[
            measured("a", "loose", 3600, "g"),
            prepack(
                "b",
                2,
                3500,
                LinePricing::WeightGroup {
                    group_id: "g".into(),
                },
            ),
        ],
        &[g],
    )
    .unwrap();
    let snapshot = &result.groups[0];
    assert_eq!(snapshot.actual_measured_weight, weight(3600));
    assert_eq!(snapshot.declared_package_weight, weight(7000));
    assert_eq!(snapshot.eligible_weight, weight(10600));
    assert_eq!(snapshot.selected_tier.nominal_weight, weight(7000));
    assert_eq!(snapshot.billable_weight, weight(10400));
    assert_eq!(snapshot.rule, PricingRule::NominalPlusExcessBeyondTolerance);
    assert_eq!(
        snapshot.exact_amount,
        RationalAmount {
            numerator: 6000 * 10400,
            denominator: 7000
        }
    );
    assert_eq!(snapshot.amount, money(8914));
    for (index, (expected_source, expected_contribution)) in [
        (WeightSource::ActualMeasured, 3600),
        (WeightSource::DeclaredPackageContent, 7000),
    ]
    .into_iter()
    .enumerate()
    {
        match &result.lines[index].explanation {
            LineExplanation::GroupAllocation {
                group_id,
                contribution_weight,
                weight_source,
                group_eligible_weight,
                selected_nominal_weight,
                group_billable_weight,
                rule,
                ..
            } => {
                assert_eq!(group_id, "g");
                assert_eq!(*contribution_weight, weight(expected_contribution));
                assert_eq!(*weight_source, expected_source);
                assert_eq!(*group_eligible_weight, weight(10600));
                assert_eq!(*selected_nominal_weight, weight(7000));
                assert_eq!(*group_billable_weight, weight(10400));
                assert_eq!(*rule, PricingRule::NominalPlusExcessBeyondTolerance);
            }
            other => panic!("wrong explanation: {other:?}"),
        }
    }
}

#[test]
fn line_explanation_records_deterministic_remainder_unit_distribution() {
    let result = price_cart(
        &[
            measured("c", "x", 1, "g"),
            measured("a", "x", 1, "g"),
            measured("b", "x", 1, "g"),
        ],
        &[group(
            "g",
            &[(3, 0, 2)],
            OveragePolicy::FullActualAtTierRate,
        )],
    )
    .unwrap();
    for line in result.lines {
        match line.explanation {
            LineExplanation::GroupAllocation {
                allocation_remainder,
                received_remainder_unit,
                ..
            } => {
                assert_eq!(allocation_remainder, 2);
                assert_eq!(received_remainder_unit, line.id != "c");
            }
            other => panic!("wrong explanation: {other:?}"),
        }
    }
}

#[test]
fn fixed_unit_explanation_preserves_price_and_count() {
    let result = price_cart(
        &[prepack(
            "a",
            3,
            3500,
            LinePricing::FixedUnit {
                unit_price: money(499),
            },
        )],
        &[],
    )
    .unwrap();
    assert_eq!(
        result.lines[0].explanation,
        LineExplanation::FixedUnit {
            unit_price: money(499),
            count: 3
        }
    );
    assert!(result.groups.is_empty());
}

#[test]
fn rejects_zero_measurements_counts_and_declared_contents() {
    let g = group(
        "g",
        &[(3500, 150, 3500)],
        OveragePolicy::FullActualAtTierRate,
    );
    assert!(price_cart(&[measured("a", "x", 0, "g")], &[g]).is_err());
    for (count, content) in [(0, 3500), (1, 0)] {
        assert!(price_cart(
            &[prepack(
                "a",
                count,
                content,
                LinePricing::FixedUnit {
                    unit_price: money(100)
                }
            )],
            &[]
        )
        .is_err());
    }
}

#[test]
fn duplicate_line_and_group_ids_are_rejected() {
    let g = group(
        "g",
        &[(3500, 150, 3500)],
        OveragePolicy::FullActualAtTierRate,
    );
    assert!(price_cart(
        &[
            measured("same", "x", 3500, "g"),
            measured("same", "y", 3500, "g")
        ],
        std::slice::from_ref(&g)
    )
    .is_err());
    assert!(price_cart(&[measured("a", "x", 3500, "g")], &[g.clone(), g]).is_err());
}

#[test]
fn missing_pricing_group_is_not_silently_fixed_or_free() {
    assert!(price_cart(&[measured("a", "x", 3500, "missing")], &[]).is_err());
}

#[test]
fn invalid_or_overlapping_tier_bands_are_rejected() {
    for tiers in [
        vec![],
        vec![(0, 0, 100)],
        vec![(3500, 0, 100), (3500, 0, 200)],
        vec![(7000, 0, 200), (3500, 0, 100)],
        vec![(3500, 3500, 100), (7000, 0, 200)],
        vec![(3500, 3501, 100), (7000, 0, 200)],
    ] {
        assert!(
            price_cart(
                &[measured("a", "x", 7000, "g")],
                &[group("g", &tiers, OveragePolicy::FullActualAtTierRate)]
            )
            .is_err(),
            "accepted tiers: {tiers:?}"
        );
    }
}

#[test]
fn empty_cart_and_invalid_identifiers_are_rejected() {
    assert!(matches!(price_cart(&[], &[]), Err(PricingError::EmptyCart)));
    let g = group(
        "g",
        &[(3500, 150, 3500)],
        OveragePolicy::FullActualAtTierRate,
    );
    for bad_id in ["", " leading", "trailing ", "tab\tinside", "line\nbreak"] {
        assert!(price_cart(
            &[measured(bad_id, "x", 3500, "g")],
            std::slice::from_ref(&g)
        )
        .is_err());
    }
}

#[test]
fn measured_quantity_cannot_silently_use_a_fixed_package_price() {
    let mut line = measured("a", "x", 3500, "unused");
    line.pricing = LinePricing::FixedUnit {
        unit_price: money(100),
    };
    assert!(matches!(
        price_cart(&[line], &[]),
        Err(PricingError::UnsupportedQuantityPricing)
    ));
}

#[test]
fn invalid_currency_syntax_and_exponents_are_rejected() {
    for code in ["usd", "US", "USDD", "U$D", "", "€UR"] {
        assert!(Currency::new(code, 2).is_err());
    }
    assert!(Currency::new("USD", 10).is_err());
}

#[test]
fn money_parsing_never_accepts_negative_fractional_or_overflowed_minor_units() {
    assert_eq!(Money::from_minor_str(usd(), "3500").unwrap(), money(3500));
    for value in [
        "-1",
        "+1",
        "1.0",
        "1e3",
        "NaN",
        " 1",
        "18446744073709551616",
    ] {
        assert!(Money::from_minor_str(usd(), value).is_err());
    }
}

#[test]
fn schedule_order_does_not_change_price_or_allocation() {
    let first = group("first", &[(3, 0, 2)], OveragePolicy::FullActualAtTierRate);
    let second = group("second", &[(3, 0, 4)], OveragePolicy::FullActualAtTierRate);
    let cart = [
        measured("a", "x", 1, "first"),
        measured("b", "x", 2, "first"),
        measured("c", "x", 3, "second"),
    ];
    let a = price_cart(&cart, &[first.clone(), second.clone()]).unwrap();
    let b = price_cart(&cart, &[second, first]).unwrap();
    assert_eq!(a, b);
}

#[test]
fn mixed_currency_and_scale_are_rejected_not_converted() {
    for other_currency in [
        Currency::new("EUR", 2).unwrap(),
        Currency::new("USD", 3).unwrap(),
    ] {
        let a = prepack(
            "a",
            1,
            1,
            LinePricing::FixedUnit {
                unit_price: money(100),
            },
        );
        let b = prepack(
            "b",
            1,
            1,
            LinePricing::FixedUnit {
                unit_price: Money::new(other_currency, 100),
            },
        );
        assert!(price_cart(&[a, b], &[]).is_err());
        let mut g = group(
            "g",
            &[(3500, 150, 3500)],
            OveragePolicy::FullActualAtTierRate,
        );
        g.schedule.tiers[0].price.currency = other_currency;
        assert!(price_cart(&[measured("a", "x", 3500, "g")], &[g]).is_err());
    }
}

#[test]
fn fixed_line_and_cart_total_overflow_return_errors() {
    assert!(price_cart(
        &[prepack(
            "a",
            2,
            1,
            LinePricing::FixedUnit {
                unit_price: money(u64::MAX)
            }
        )],
        &[]
    )
    .is_err());
    assert!(price_cart(
        &[
            prepack(
                "a",
                1,
                1,
                LinePricing::FixedUnit {
                    unit_price: money(u64::MAX)
                }
            ),
            prepack(
                "b",
                1,
                1,
                LinePricing::FixedUnit {
                    unit_price: money(1)
                }
            ),
        ],
        &[]
    )
    .is_err());
}

#[test]
fn weight_expansion_and_weight_price_overflow_return_errors() {
    let g = group("g", &[(1, 0, 1)], OveragePolicy::FullActualAtTierRate);
    assert!(price_cart(
        &[prepack(
            "a",
            u64::MAX,
            1_000_000_000_000,
            LinePricing::WeightGroup {
                group_id: "g".into()
            }
        )],
        &[g]
    )
    .is_err());
    let expensive = group(
        "g",
        &[(1, 0, u64::MAX)],
        OveragePolicy::FullActualAtTierRate,
    );
    assert!(price_cart(&[measured("a", "x", 2, "g")], &[expensive]).is_err());
}

#[test]
fn aggregate_weight_limit_is_checked() {
    let g = group("g", &[(1, 0, 1)], OveragePolicy::FullActualAtTierRate);
    assert!(price_cart(
        &[
            measured("a", "x", 1_000_000_000_000, "g"),
            measured("b", "x", 1, "g")
        ],
        &[g]
    )
    .is_err());
}

#[test]
fn maximum_valid_weight_and_money_succeed_without_intermediate_overflow() {
    let result = price_cart(
        &[
            measured("a", "x", 999_999_999_999, "g"),
            measured("b", "x", 1, "g"),
        ],
        &[group(
            "g",
            &[(1_000_000_000_000, 0, u64::MAX)],
            OveragePolicy::FullActualAtTierRate,
        )],
    )
    .unwrap();
    assert_eq!(result.total.minor_units, u64::MAX);
    assert_eq!(
        result
            .lines
            .iter()
            .map(|line| u128::from(line.amount.minor_units))
            .sum::<u128>(),
        u128::from(u64::MAX)
    );
}

#[test]
fn deterministic_randomized_splits_order_and_allocation_invariants() {
    let mut state = 0x1d3f_84a9_5c27_be61_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for case in 0..1000 {
        let count = 2 + (next() % 10) as usize;
        let weights = (0..count).map(|_| 1 + next() % 7000).collect::<Vec<_>>();
        let total_weight = weights.iter().sum::<u64>();
        let price = 1 + next() % 10000;
        let policy = if next() & 1 == 0 {
            OveragePolicy::FullActualAtTierRate
        } else {
            OveragePolicy::ExcessBeyondToleranceAtTierRate
        };
        let g = group(
            "g",
            &[
                (3500, 150, price),
                (14000, 250, price * 3),
                (28000, 500, price * 5),
            ],
            policy,
        );
        let cart = weights
            .iter()
            .enumerate()
            .map(|(i, &mg)| measured(&format!("line-{i:02}"), "mix", mg, "g"))
            .collect::<Vec<_>>();
        let result = price_cart(&cart, std::slice::from_ref(&g)).unwrap();
        let (nominal, tolerance, tier_price) = if total_weight >= 28000 {
            (28000, 500, price * 5)
        } else if total_weight >= 14000 {
            (14000, 250, price * 3)
        } else {
            (3500, 150, price)
        };
        let billed_weight = if total_weight < nominal {
            total_weight
        } else if total_weight <= nominal + tolerance {
            nominal
        } else if matches!(policy, OveragePolicy::ExcessBeyondToleranceAtTierRate) {
            total_weight - tolerance
        } else {
            total_weight
        };
        let expected_total = (u128::from(billed_weight) * u128::from(tier_price) * 2
            + u128::from(nominal))
            / (2 * u128::from(nominal));
        assert_eq!(
            u128::from(result.total.minor_units),
            expected_total,
            "independent price oracle disagrees for case {case}"
        );
        let combined = price_cart(
            &[measured("one", "mix", total_weight, "g")],
            std::slice::from_ref(&g),
        )
        .unwrap();
        assert_eq!(
            result.total, combined.total,
            "split changed total for case {case}"
        );
        assert_eq!(
            result
                .lines
                .iter()
                .map(|l| l.amount.minor_units)
                .sum::<u64>(),
            result.total.minor_units,
            "allocation failed conservation for case {case}"
        );
        for (line, &mg) in result.lines.iter().zip(&weights) {
            let numerator = u128::from(result.total.minor_units) * u128::from(mg);
            let floor = numerator / u128::from(total_weight);
            let allocated = u128::from(line.amount.minor_units);
            assert!(
                allocated == floor || allocated == floor + 1,
                "allocation is not quota-respecting for case {case}"
            );
        }
        let mut permuted = cart.clone();
        for i in (1..permuted.len()).rev() {
            let j = (next() % (i as u64 + 1)) as usize;
            permuted.swap(i, j);
        }
        let reordered = price_cart(&permuted, &[g]).unwrap();
        assert_eq!(
            reordered.total, result.total,
            "order changed total for case {case}"
        );
        assert_eq!(
            amounts_by_id(&reordered),
            amounts_by_id(&result),
            "order changed allocation for case {case}"
        );
        for (original, repriced) in cart.iter().zip(&result.lines) {
            assert_eq!(
                original.quantity, repriced.quantity,
                "quantity mutated for case {case}"
            );
        }
    }
}

fn unit(id: &str, count: u64, minor_units: u64) -> CartLine {
    CartLine {
        id: id.into(),
        sku: "individual-item".into(),
        quantity: Quantity::Unit { count },
        pricing: LinePricing::FixedUnit {
            unit_price: money(minor_units),
        },
    }
}

#[test]
fn units_price_exact_integer_counts_without_any_weight() {
    let line = unit("unit", 3, 249);
    let result = price_cart(std::slice::from_ref(&line), &[]).unwrap();
    assert_eq!(result.total, money(747));
    assert!(result.groups.is_empty());
    assert_eq!(result.lines[0].quantity, Quantity::Unit { count: 3 });
    assert_eq!(result.lines[0].quantity, line.quantity);
    assert_eq!(
        result.lines[0].explanation,
        LineExplanation::FixedUnit {
            unit_price: money(249),
            count: 3,
        }
    );
}

#[test]
fn unit_counts_are_positive_and_support_full_u64_range() {
    assert_eq!(
        price_cart(&[unit("zero", 0, 100)], &[]),
        Err(PricingError::NonPositiveInput("unit count"))
    );
    let largest_count = price_cart(&[unit("largest", u64::MAX, 1)], &[]).unwrap();
    assert_eq!(largest_count.total.minor_units, u64::MAX);
    assert_eq!(
        largest_count.lines[0].quantity,
        Quantity::Unit { count: u64::MAX }
    );
    // Free items still preserve their exact count without weight overflow.
    let free = price_cart(&[unit("free", u64::MAX, 0)], &[]).unwrap();
    assert_eq!(free.total.minor_units, 0);
    assert!(free.groups.is_empty());
}

#[test]
fn unit_count_multiplication_and_money_sum_are_checked() {
    for (count, price) in [(u64::MAX, 2), (2, u64::MAX)] {
        assert_eq!(
            price_cart(&[unit("overflow", count, price)], &[]),
            Err(PricingError::Overflow("pricing fixed unit count"))
        );
    }
    assert_eq!(
        price_cart(&[unit("maximum", 1, u64::MAX), unit("extra", 1, 1)], &[]),
        Err(PricingError::Overflow("summing cart amount"))
    );
}

#[test]
fn units_reject_weight_group_pricing_even_as_the_only_group_member() {
    let g = group("g", &[(1000, 0, 100)], OveragePolicy::FullActualAtTierRate);
    let mut invalid = unit("unit", 1, 100);
    invalid.pricing = LinePricing::WeightGroup {
        group_id: "g".into(),
    };
    assert_eq!(
        price_cart(std::slice::from_ref(&invalid), std::slice::from_ref(&g)),
        Err(PricingError::UnsupportedQuantityPricing)
    );
    assert_eq!(
        price_cart(&[measured("measured", "bulk", 1000, "g"), invalid], &[g]),
        Err(PricingError::UnsupportedQuantityPricing)
    );
}

#[test]
fn unit_lines_do_not_affect_weight_group_totals_or_allocations() {
    let g = group("g", &[(1000, 0, 100)], OveragePolicy::FullActualAtTierRate);
    let weighted = vec![
        measured("a", "bulk-a", 600, "g"),
        prepack(
            "b",
            2,
            200,
            LinePricing::WeightGroup {
                group_id: "g".into(),
            },
        ),
    ];
    let baseline = price_cart(&weighted, std::slice::from_ref(&g)).unwrap();
    let mut mixed = weighted;
    mixed.push(unit("c", 4, 250));
    let result = price_cart(&mixed, &[g]).unwrap();
    assert_eq!(result.groups, baseline.groups);
    assert_eq!(&result.lines[..2], baseline.lines.as_slice());
    assert_eq!(result.total, money(baseline.total.minor_units + 1000));
    assert_eq!(result.groups[0].actual_measured_weight, weight(600));
    assert_eq!(result.groups[0].declared_package_weight, weight(400));
}

#[test]
fn units_still_require_valid_identifiers_and_consistent_currency() {
    let mut invalid = unit("", 1, 100);
    assert_eq!(
        price_cart(std::slice::from_ref(&invalid), &[]),
        Err(PricingError::InvalidIdentifier("cart line id"))
    );
    invalid.id = "id".into();
    invalid.sku = " ".into();
    assert_eq!(
        price_cart(&[invalid], &[]),
        Err(PricingError::InvalidIdentifier("SKU"))
    );
    let a = unit("a", 1, 100);
    let mut b = unit("b", 1, 100);
    b.pricing = LinePricing::FixedUnit {
        unit_price: Money::new(Currency::new("EUR", 2).unwrap(), 100),
    };
    assert_eq!(
        price_cart(&[a, b], &[]),
        Err(PricingError::CurrencyMismatch)
    );
}
