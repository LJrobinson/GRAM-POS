use crate::{Currency, Money, PricingError, RationalAmount, Rounding, Weight};
use std::collections::BTreeMap;

/// A merchant-defined nominal offer. It is not a physical unit conversion.
/// The one-sided inclusive tolerance band is [nominal, nominal + tolerance].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tier {
    pub nominal_weight: Weight,
    pub tolerance: Weight,
    pub price: Money,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OveragePolicy {
    /// Outside tolerance, charge the entire eligible weight at the selected
    /// tier's exact price / nominal-weight rate.
    FullActualAtTierRate,
    /// Outside tolerance, charge nominal price plus only weight exceeding the
    /// upper tolerance bound at the same exact rate. Billable weight is then
    /// eligible weight minus the selected tier's tolerance.
    ExcessBeyondToleranceAtTierRate,
}

/// Scope is mandatory configuration, not an implied merchant preference.
/// Milestone 1 supports one tolerance allowance per aggregate pricing group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToleranceScope {
    AggregatePricingGroup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BelowSmallestPolicy {
    ProportionalAtSmallestTier,
    Reject,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriceSchedule {
    pub currency: Currency,
    /// Strictly ascending nominal weights; overlapping inclusive tolerance
    /// bands are invalid, including bands that touch at a single weight.
    pub tiers: Vec<Tier>,
    pub overage_policy: OveragePolicy,
    pub rounding: Rounding,
    pub tolerance_scope: ToleranceScope,
    pub below_smallest: BelowSmallestPolicy,
}

impl PriceSchedule {
    /// Validate an offer before displaying or saving it. `price_cart` also
    /// validates all supplied schedules, including unused ones.
    pub fn validate(&self) -> Result<(), PricingError> {
        if self.tiers.is_empty() {
            return Err(PricingError::EmptyTiers);
        }
        let mut previous: Option<&Tier> = None;
        for tier in &self.tiers {
            positive_weight(tier.nominal_weight, "nominal tier weight")?;
            if tier.price.currency != self.currency {
                return Err(PricingError::CurrencyMismatch);
            }
            tier.nominal_weight.checked_add(tier.tolerance)?;
            if let Some(before) = previous {
                if tier.nominal_weight <= before.nominal_weight {
                    return Err(PricingError::UnorderedTiers);
                }
                if before.nominal_weight.checked_add(before.tolerance)? >= tier.nominal_weight {
                    return Err(PricingError::OverlappingToleranceBands);
                }
            }
            previous = Some(tier);
        }
        Ok(())
    }
}

/// A group defines one common price schedule. Different price bands must use
/// different group IDs; there is no implicit cheapest-SKU selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PricingGroup {
    pub id: String,
    pub schedule: PriceSchedule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Quantity {
    Measured {
        actual_weight: Weight,
    },
    Prepacked {
        count: u64,
        declared_content: Weight,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinePricing {
    /// Explicit eligibility in the named common-price group. For a prepack,
    /// its declared content times package count contributes, not measured mass.
    WeightGroup { group_id: String },
    /// Excludes a prepack from weight offers and charges per physical package.
    /// Not valid for a measured-weight quantity.
    FixedUnit { unit_price: Money },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CartLine {
    /// Unique stable sale-line identity, also used to break allocation ties.
    pub id: String,
    pub sku: String,
    pub quantity: Quantity,
    pub pricing: LinePricing,
}

/// Explains which exact branch priced an aggregate group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PricingRule {
    WithinNominalTolerance,
    BelowSmallestProportional,
    FullActualAtTierRate,
    NominalPlusExcessBeyondTolerance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeightSource {
    ActualMeasured,
    DeclaredPackageContent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineExplanation {
    FixedUnit {
        unit_price: Money,
        count: u64,
    },
    GroupAllocation {
        group_id: String,
        contribution_weight: Weight,
        weight_source: WeightSource,
        group_eligible_weight: Weight,
        selected_nominal_weight: Weight,
        group_billable_weight: Weight,
        rule: PricingRule,
        /// Remainder of (group minor units * this line's contribution) divided
        /// by group eligible milligrams, before largest-remainder distribution.
        allocation_remainder: u64,
        received_remainder_unit: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PricedLine {
    pub id: String,
    pub sku: String,
    /// Original measured grams or original integer count + declared content.
    /// Pricing never normalizes or overwrites these recorded sale facts.
    pub quantity: Quantity,
    pub amount: Money,
    pub explanation: LineExplanation,
}

/// A self-contained quote snapshot, independent of subsequent catalog edits.
/// For opted-in prepackages, eligible weight includes declared content; it is
/// deliberately NOT presented as actual measured physical weight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupSnapshot {
    pub group_id: String,
    pub actual_measured_weight: Weight,
    pub declared_package_weight: Weight,
    pub eligible_weight: Weight,
    pub selected_tier: Tier,
    pub billable_weight: Weight,
    pub rule: PricingRule,
    pub exact_amount: RationalAmount,
    pub amount: Money,
    pub schedule: PriceSchedule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PricedCart {
    pub currency: Currency,
    /// Same order and separate SKU lines as the input.
    pub lines: Vec<PricedLine>,
    /// Active group snapshots sorted by group ID for stable output.
    pub groups: Vec<GroupSnapshot>,
    pub total: Money,
}

#[derive(Clone, Copy)]
struct Contribution {
    line_index: usize,
    weight: Weight,
    source: WeightSource,
}

/// Price a cart as a pure computation. No storage, tax, tender, I/O, or global
/// state is involved. Validation or arithmetic errors return no partial quote.
///
/// Group weights are aggregated before tier selection and rounding. Select the
/// highest nominal tier not greater than eligible weight; below the smallest,
/// use the explicitly configured policy. Tolerance applies only to overfill.
///
/// Each group's rounded amount is allocated proportionally by contribution
/// weight using largest remainder. Equal remainders go to lexicographically
/// smallest line IDs (Rust string byte order), independent of input order.
/// Zero-price offers and zero rounded allocations are valid; quantities are not.
pub fn price_cart(lines: &[CartLine], groups: &[PricingGroup]) -> Result<PricedCart, PricingError> {
    if lines.is_empty() {
        return Err(PricingError::EmptyCart);
    }
    let mut currency = None;
    let mut configured_groups = BTreeMap::new();
    for group in groups {
        validate_id(&group.id, "pricing group id")?;
        if configured_groups.insert(group.id.as_str(), group).is_some() {
            return Err(PricingError::DuplicateGroupId(group.id.clone()));
        }
        group.schedule.validate()?;
        set_currency(&mut currency, group.schedule.currency)?;
    }

    let mut seen_ids = BTreeMap::new();
    let mut contributions: BTreeMap<&str, Vec<Contribution>> = BTreeMap::new();
    let mut priced_lines: Vec<Option<PricedLine>> = vec![None; lines.len()];
    for (index, line) in lines.iter().enumerate() {
        validate_id(&line.id, "cart line id")?;
        validate_id(&line.sku, "SKU")?;
        if seen_ids.insert(line.id.as_str(), ()).is_some() {
            return Err(PricingError::DuplicateLineId(line.id.clone()));
        }
        let (weight, source) = match line.quantity {
            Quantity::Measured { actual_weight } => {
                positive_weight(actual_weight, "actual measured weight")?;
                (actual_weight, WeightSource::ActualMeasured)
            }
            Quantity::Prepacked {
                count,
                declared_content,
            } => {
                if count == 0 {
                    return Err(PricingError::NonPositiveInput("package count"));
                }
                positive_weight(declared_content, "declared package content")?;
                (
                    declared_content.checked_mul(count)?,
                    WeightSource::DeclaredPackageContent,
                )
            }
        };
        match &line.pricing {
            LinePricing::WeightGroup { group_id } => {
                validate_id(group_id, "line pricing group id")?;
                if !configured_groups.contains_key(group_id.as_str()) {
                    return Err(PricingError::UnknownGroup(group_id.clone()));
                }
                contributions
                    .entry(group_id.as_str())
                    .or_default()
                    .push(Contribution {
                        line_index: index,
                        weight,
                        source,
                    });
            }
            LinePricing::FixedUnit { unit_price } => {
                let count = match line.quantity {
                    Quantity::Prepacked { count, .. } => count,
                    Quantity::Measured { .. } => {
                        return Err(PricingError::UnsupportedQuantityPricing)
                    }
                };
                set_currency(&mut currency, unit_price.currency)?;
                let minor_units = unit_price
                    .minor_units
                    .checked_mul(count)
                    .ok_or(PricingError::Overflow("pricing fixed package count"))?;
                priced_lines[index] = Some(PricedLine {
                    id: line.id.clone(),
                    sku: line.sku.clone(),
                    quantity: line.quantity.clone(),
                    amount: Money::new(unit_price.currency, minor_units),
                    explanation: LineExplanation::FixedUnit {
                        unit_price: *unit_price,
                        count,
                    },
                });
            }
        }
    }

    let mut snapshots = Vec::with_capacity(contributions.len());
    for (group_id, members) in contributions {
        let group = configured_groups
            .get(group_id)
            .ok_or_else(|| PricingError::UnknownGroup(group_id.to_owned()))?;
        let snapshot = quote_group(group, &members)?;
        let allocations = allocate(
            &members,
            lines,
            snapshot.amount.minor_units,
            snapshot.eligible_weight,
        )?;
        for (member, minor_units, remainder, received_remainder_unit) in allocations {
            let line = &lines[member.line_index];
            priced_lines[member.line_index] = Some(PricedLine {
                id: line.id.clone(),
                sku: line.sku.clone(),
                quantity: line.quantity.clone(),
                amount: Money::new(snapshot.amount.currency, minor_units),
                explanation: LineExplanation::GroupAllocation {
                    group_id: group_id.to_owned(),
                    contribution_weight: member.weight,
                    weight_source: member.source,
                    group_eligible_weight: snapshot.eligible_weight,
                    selected_nominal_weight: snapshot.selected_tier.nominal_weight,
                    group_billable_weight: snapshot.billable_weight,
                    rule: snapshot.rule,
                    allocation_remainder: remainder,
                    received_remainder_unit,
                },
            });
        }
        snapshots.push(snapshot);
    }

    let currency = currency.ok_or(PricingError::EmptyCart)?;
    let mut total = 0_u64;
    let mut result_lines = Vec::with_capacity(lines.len());
    for line in priced_lines {
        let line = line.ok_or(PricingError::UnsupportedQuantityPricing)?;
        total = total
            .checked_add(line.amount.minor_units)
            .ok_or(PricingError::Overflow("summing cart amount"))?;
        result_lines.push(line);
    }
    Ok(PricedCart {
        currency,
        lines: result_lines,
        groups: snapshots,
        total: Money::new(currency, total),
    })
}

fn quote_group(
    group: &PricingGroup,
    members: &[Contribution],
) -> Result<GroupSnapshot, PricingError> {
    let mut measured = Weight::ZERO;
    let mut declared = Weight::ZERO;
    for member in members {
        match member.source {
            WeightSource::ActualMeasured => measured = measured.checked_add(member.weight)?,
            WeightSource::DeclaredPackageContent => {
                declared = declared.checked_add(member.weight)?
            }
        }
    }
    let eligible = measured.checked_add(declared)?;
    let schedule = &group.schedule;
    let tier_index = schedule
        .tiers
        .iter()
        .rposition(|tier| tier.nominal_weight <= eligible);
    let (tier, billable, rule) = if let Some(index) = tier_index {
        let tier = &schedule.tiers[index];
        let upper = tier.nominal_weight.checked_add(tier.tolerance)?;
        if eligible <= upper {
            (
                tier,
                tier.nominal_weight,
                PricingRule::WithinNominalTolerance,
            )
        } else {
            match schedule.overage_policy {
                OveragePolicy::FullActualAtTierRate => {
                    (tier, eligible, PricingRule::FullActualAtTierRate)
                }
                OveragePolicy::ExcessBeyondToleranceAtTierRate => {
                    // This branch guarantees eligible > nominal + tolerance.
                    let mg = eligible
                        .milligrams()
                        .checked_sub(tier.tolerance.milligrams())
                        .ok_or(PricingError::Overflow("subtracting tolerance"))?;
                    (
                        tier,
                        Weight::from_milligrams(mg)?,
                        PricingRule::NominalPlusExcessBeyondTolerance,
                    )
                }
            }
        }
    } else {
        match schedule.below_smallest {
            BelowSmallestPolicy::Reject => {
                return Err(PricingError::BelowSmallestTier {
                    group_id: group.id.clone(),
                })
            }
            BelowSmallestPolicy::ProportionalAtSmallestTier => (
                &schedule.tiers[0],
                eligible,
                PricingRule::BelowSmallestProportional,
            ),
        }
    };
    let numerator = u128::from(tier.price.minor_units)
        .checked_mul(u128::from(billable.milligrams()))
        .ok_or(PricingError::Overflow("multiplying exact tier price"))?;
    let exact_amount = RationalAmount {
        numerator,
        denominator: tier.nominal_weight.milligrams(),
    };
    let amount = Money::new(schedule.currency, exact_amount.round(schedule.rounding)?);
    Ok(GroupSnapshot {
        group_id: group.id.clone(),
        actual_measured_weight: measured,
        declared_package_weight: declared,
        eligible_weight: eligible,
        selected_tier: tier.clone(),
        billable_weight: billable,
        rule,
        exact_amount,
        amount,
        schedule: schedule.clone(),
    })
}

type Allocation = (Contribution, u64, u64, bool);

fn allocate(
    members: &[Contribution],
    lines: &[CartLine],
    amount: u64,
    total_weight: Weight,
) -> Result<Vec<Allocation>, PricingError> {
    let denominator = u128::from(total_weight.milligrams());
    if denominator == 0 {
        return Err(PricingError::NonPositiveInput("allocation weight"));
    }
    let mut allocations = Vec::with_capacity(members.len());
    let mut floor_sum = 0_u64;
    for member in members {
        let numerator = u128::from(amount)
            .checked_mul(u128::from(member.weight.milligrams()))
            .ok_or(PricingError::Overflow("allocating group amount"))?;
        let floor = u64::try_from(numerator / denominator)
            .map_err(|_| PricingError::Overflow("converting line allocation"))?;
        let remainder = u64::try_from(numerator % denominator)
            .map_err(|_| PricingError::Overflow("converting allocation remainder"))?;
        floor_sum = floor_sum
            .checked_add(floor)
            .ok_or(PricingError::Overflow("summing allocation floors"))?;
        allocations.push((*member, floor, remainder, false));
    }
    let leftover = amount
        .checked_sub(floor_sum)
        .ok_or(PricingError::Overflow("reconciling allocation"))?;
    allocations.sort_by(|left, right| {
        right.2.cmp(&left.2).then_with(|| {
            lines[left.0.line_index]
                .id
                .cmp(&lines[right.0.line_index].id)
        })
    });
    let leftover = usize::try_from(leftover)
        .map_err(|_| PricingError::Overflow("distributing remainder units"))?;
    if leftover > allocations.len() {
        return Err(PricingError::Overflow(
            "reconciling allocation remainder count",
        ));
    }
    for allocation in allocations.iter_mut().take(leftover) {
        allocation.1 = allocation
            .1
            .checked_add(1)
            .ok_or(PricingError::Overflow("adding remainder unit"))?;
        allocation.3 = true;
    }
    Ok(allocations)
}

fn positive_weight(weight: Weight, field: &'static str) -> Result<(), PricingError> {
    if weight == Weight::ZERO {
        return Err(PricingError::NonPositiveInput(field));
    }
    Ok(())
}

fn validate_id(value: &str, field: &'static str) -> Result<(), PricingError> {
    if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
        return Err(PricingError::InvalidIdentifier(field));
    }
    Ok(())
}

fn set_currency(current: &mut Option<Currency>, candidate: Currency) -> Result<(), PricingError> {
    match *current {
        Some(existing) if existing != candidate => Err(PricingError::CurrencyMismatch),
        _ => {
            *current = Some(candidate);
            Ok(())
        }
    }
}
