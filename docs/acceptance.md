# Milestone 1 acceptance contract

This document records the prototype's explicit pricing behavior and independent
acceptance tests. It does not select a merchant's prices, promotions, legal
requirements, currency, or rounding rules. Prices in tests are fixtures only.

## Numeric and physical model

- Weight is an unsigned integer number of milligrams, with a maximum of
  1,000,000,000,000 mg for one weight and for an aggregated pricing group.
- Decimal grams are parsed exactly, with at most three fractional digits. The
  parser rejects excess precision rather than rounding. Negative values,
  exponent notation, floating-point special values, and unit suffixes are not
  accepted.
- Money is a nonnegative integer number of minor units. Intermediate charges
  are exact rational numbers. Currency code and minor-unit exponent must agree
  across a quote. The code validates currency syntax, not a live ISO registry.
- A zero configured price is a valid explicit free offer. A zero measured
  quantity, package count, declared package content, or nominal tier is invalid.
- Measured net weight stays measured net weight. A package count and declared
  content stay a count and declared content, not a claimed scale measurement.
  Price normalization never rewrites either representation.
- A retail 28 g size is exactly 28,000 mg. A physical avoirdupois ounce is
  28.349523125 g. General ounce conversion is deliberately outside this API:
  the exact physical-ounce decimal cannot be silently represented at 1 mg
  resolution, and labels such as `1 oz` are rejected. A later unit adapter
  needs an explicitly selected conversion precision and rounding policy.

## Nominal tiers and overfill allowances

Every schedule supplies its own ascending nominal tiers, each tier's overfill
allowance and price, rounding mode, below-smallest policy, and overage policy.
There are no merchant defaults and no automatic currency choice.

An allowance is one-sided and inclusive: `[nominal, nominal + tolerance]`.
Underweight quantities do not snap upward. Below the smallest nominal tier,
the caller must choose proportional pricing at that tier's rate or rejection.
Tolerance bands may neither overlap nor touch, because both endpoints are
inclusive. Tiers must be strictly increasing; the core does not silently sort
or repair an ambiguous configuration.

The following synthetic fixture is 3,500 minor units per 3,500 mg, with a 150 mg
overfill allowance. It makes every accepted milligram visible as one minor
unit, without requiring a rounding explanation:

| Actual mass | Full-actual charge | Excess-beyond-tolerance charge | Reason |
| --- | ---: | ---: | --- |
| 3,499 mg | 3,499 | 3,499 | Explicit proportional below-smallest policy |
| 3,500 mg | 3,500 | 3,500 | Exact nominal tier |
| 3,650 mg | 3,500 | 3,500 | Inclusive upper allowance boundary |
| 3,651 mg | 3,651 | 3,501 | First milligram beyond allowance |
| 3,660 mg | 3,660 | 3,510 | Full 3.660 g, or 3.500 g plus 0.010 g excess |

With the explicit rejection policy, 3,499 mg is an error instead. A tolerance
does not give permission to supply less than the nominal quantity.

For an aggregate above the selected tier's allowance:

- `FullActualAtTierRate` charges the actual aggregate mass at the selected
  tier's price-per-mass rate.
- `ExcessBeyondToleranceAtTierRate` charges the nominal price plus only the
  mass exceeding that tier's upper allowance boundary, at the same rate.

The highest nominal tier not exceeding aggregate mass is selected, except
for explicit below-smallest handling. The configured 7, 14, 28, and 56 g
thresholds are tested. Crossing a threshold applies that selected tier's rate
to the whole eligible group. This is not a bundle-composition or cheapest-price
optimizer.

### Revenue implications that must remain visible

Full-actual pricing creates an intentional jump immediately above the tolerance
boundary: the previously uncharged allowance becomes chargeable. Excess-only
pricing preserves that allowance after the boundary. A larger tier can also
lower or raise the cart total according to its configured rate. Therefore,
global price monotonicity is **not** a valid invariant for arbitrary schedules.
Merchant review and future configuration previews should display these effects
before a schedule is put into use.

## Grouping and package eligibility

The only implemented tolerance scope is explicitly named
`AggregatePricingGroup`. All lines assigned to the same group contribute their
eligible weight before one tier and one allowance are selected. Distinct SKUs
can mix when the caller assigns them to the same group. Unrelated groups never
combine, even if their schedules are identical.

Prepacked quantities have two deliberate options:

1. `FixedUnit`: count multiplied by the configured package price. Declared
   package content does not contribute to a weight promotion.
2. `WeightGroup`: explicit promotion opt-in; count multiplied by declared
   content contributes to that group. It is still declared, not measured, mass.

The caller must make this assignment. A measured quantity cannot silently use
a fixed package price. An unknown group is an error, not a free item or an
automatic fallback.

All supplied group configurations are validated, including unused ones, and
must use the quote's currency and exponent. Callers should supply the relevant
single-currency configuration set rather than an unrelated multicurrency catalog.

Applying tolerance once per group prevents additional allowances from being
created by splitting a visual cart row. Two 3.650 g rows totaling 7.300 g do
not become two independent nominal 3.500 g sales. Under the test's 7 g tier
and 150 mg group allowance, 7.300 g is outside the allowance. Per-portion
tolerances would require a separate future design with stable portion identity.

## Rounding and deterministic allocations

Each group's exact rational charge is rounded once using the explicitly chosen
`Floor`, `Ceil`, `HalfUp`, or `HalfEven` policy. Half-minor-unit cases distinguish
all four policies, including half-even ties in both directions.

The rounded group total is allocated proportionally to each line's eligible
weight. Every line receives its floor share, then remaining minor units go to
the largest fractional remainders. Equal remainders are resolved by stable,
case-sensitive line ID ordering, not cart order. The core preserves input line
order for display while allocation amounts remain independent of that order.

Examples:

- A two-minor-unit group split into equal weights with IDs `a`, `b`, `c`
  allocates `1, 1, 0`, even when input order is `c, a, b`.
- A seven-minor-unit group with weights `1, 2, 7` has exact shares
  `0.7, 1.4, 4.9` and allocates `1, 1, 5`.

Line allocation sums exactly to each group charge and the cart total. Splitting
a group line preserves the group total but can redistribute its allocated
minor units among new lines; it is not a guarantee of identical per-line
rounding after a split. Existing line IDs must remain stable for stable receipt
allocations.

## Independent automated coverage

`tests/pricing_acceptance.rs` exercises:

- Exact decimal parsing, upper weight bounds, malformed/negative inputs, and
  precision rejection.
- The 3,499/3,500/3,650/3,651 mg boundaries, 3.660 g overage policies, and
  explicit below-smallest rejection.
- Configured 7/14/28/56 g tiers, including quantity beyond the largest tier.
- Cross-SKU grouping, separate groups, fixed prepack exclusion, explicit
  prepack participation, and package-count multiplication.
- Aggregate-only tolerance, preserved physical/declared quantities, rounding,
  largest-remainder quotas, lexical ties, and cart/configuration order.
- Self-contained quote snapshots, selected rules, exact rational amounts,
  distinct measured-versus-declared contributions, remainder-unit explanations,
  and preservation of original configuration after later catalog edits.
- Zero-priced offers versus invalid zero quantities.
- Empty carts, duplicate IDs, unknown groups, unsupported quantity/pricing
  combinations, bad currency syntax, and currency/exponent mismatches.
- Empty, zero-nominal, duplicate, descending, overlapping, and touching tiers.
- Overflow in money parsing, fixed line multiplication, cart totals, expanded
  package weight, aggregate weight, and weight-based charges.
- Successful allocation at the maximum valid weight and maximum `u64`
  minor-unit amount, without intermediate overflow.
- A seeded 1,000-case randomized test with an independent price oracle,
  whole-versus-split totals, input permutations, conserved allocated totals,
  quota-respecting line amounts, and unchanged quantities. The fixed seed makes
  failures reproducible and requires no property-testing dependency.

Run the independent suite with `cargo test --test pricing_acceptance`. Run all
tests with `cargo test --all-targets` and the documentation tests with
`cargo test --doc`. `VALIDATION.md` records the actual final execution results;
the presence of a test or CI file alone is not proof of a successful run.

At independent QA handoff, all **39 acceptance tests and 4 module tests** passed
locally, including the 1,000-case invariant test. All-target Clippy with warnings
denied also passed. Consult the final validation record for compiler versions
and additional target checks.

## Adapted fixture provenance and limits

Two relevant pricing examples are adapted from CannabisMath's pinned
[`shared/test-fixtures/core-fixtures.json`](https://github.com/LJrobinson/CannabisMath/blob/f36238294fa5a8c9005b085fe2273a67a9ddf4aa/shared/test-fixtures/core-fixtures.json):

- `price per gram`: $35 for 3.5 g is $10 per g. The Rust test charges exactly
  1,000 cents for 1,000 mg under explicit proportional below-smallest pricing.
- `zero price per gram`: an explicitly zero-priced offer remains zero without
  relaxing positive-quantity validation.

These are focused mathematical adaptations, not a port of the fixture suite
or a claim of parity with CannabisMath or UnitMath. Potency, dose, cash-increment
rounding, and most upstream conversions are not implemented here. See
`PROVENANCE.md` for broader sources and the pending publication/license choice.

## Milestone boundary

This is a deterministic, in-memory quote engine. Its preserved quantities and
explanations support later adapters; they are not persistent inventory, sale
records, receipts, compliance decisions, payment authorization, or executable
refunds. No register UI, database, tax engine, device driver, platform installer,
or external publication is part of these acceptance results.


## Milestone 4: local cash ledger acceptance

- An explicit opening float and snapshotted currency/precision precede new cash sales.
- Sales and full refunds attach atomically to the active session; closed summaries cannot be amended.
- Cash sales contribute their original total (tender minus change), not gross tender.
- A refund references its immutable original receipt, exact amount and currency with a required reason. Catalog edits do not reprice it, another refund is rejected, and stock changes are never inferred.
- Refunds of earlier-session or legacy sales reduce only the current matching-currency drawer.
- Counted cash and exact signed variance are frozen on close. A stale reviewed movement count cannot close over concurrent activity.
- Request replay returns the original result after later configuration/session changes; changed payload reuse fails.
- Schema-1 receipts/requests remain byte-preserved legacy records. Portable v1 backups still restore, while v2 includes ledger and relationships.
- No claim of live payment processing, inventory, production tax/fiscal compliance, or physical cash verification is made.


## Milestone 5: saved receipts and event-date reports

- The receipt view takes only the immutable stored receipt for product names, physical/declared quantities, pricing-group details, currency and money. Current refund status is separate screen-only context.
- Printing is explicit, target-checked, read-only and limited by CSS to the selected saved receipt. Cancel/return from the print dialog is not treated as proof of printing and has no ledger side effect.
- A validated consistent ledger snapshot drives strict Gregorian UTC date reports: start midnight included, end midnight excluded. Sales and refunds use their own event timestamps, including refunds outside the original sale range.
- Exact summary groups use `(currency code, minor-unit exponent)`; gross sales, refunds and possibly negative net remain distinct. Legacy sales count once. Session float, cash received/change and closing balances never become sales revenue.
- Server-generated CSV uses fixed columns and filename, RFC-style quoting and CRLF rows, Unicode preservation, spreadsheet formula-prefix defense for refund reasons, and exact integer minor-unit text. Empty results produce the header only.
- The report/CSV additions do not migrate schema, mutate receipts, change idempotency, or alter the existing backup/recovery format. Reports are not recovery files.
- Rust report tests cover UTC boundaries and leap dates, refund-only periods, currency/precision groups, legacy/archive handling, totals beyond u64, malformed input, immutable recovery replay, corrupted records and CSV escaping/injection. Frontend/browser coverage and execution status are recorded in VALIDATION.md.

## Milestone 6: ordinary units and focused barcode lookup

- A unit line records only an integer count and exact fixed per-unit price; it neither manufactures declared grams nor joins a weight-pricing group. Zero/fractional/negative counts, unsupported quantity/pricing pairs and arithmetic overflow are rejected. Existing measured/prepacked vectors remain unchanged.
- Optional single barcodes are exact case-sensitive strings with leading zeros retained. Duplicate codes across active/archived products, surrounding whitespace, controls and overlong codes fail atomic configuration validation. Unknown and archived lookup errors do not add a line.
- Only the focused scanner field submits on Enter. Count-item scans add one item/package; measured scans request weight. Repeated deliberate scans are distinct, asynchronous lookups are serialized, and cancellation/navigation prevents late results from changing a different screen or cart context.
- Saved unit receipts, full-sale refunds and report totals use the original immutable money and quantity snapshots after product edits. Older configs without barcodes and measured/prepacked receipt/backup records remain valid; recovery preserves original raw records and retry identities.
- Browser tests capture actual unit/scanner desktop and phone states. Test discovery is not runtime execution, and successful browser/native packaging does not certify physical HID scanners, printer/scales, device synchronization or production use. Check VALIDATION.md for what actually ran.


## Milestone 7: local catalog CSV

- Select a UTF-8 CSV without applying; preview validates exact schema and all rows, displaying source row/line errors and complete before/after product values.
- Review mixed unit/prepack/measured creates and updates. Cancel changes nothing; explicit Apply writes the whole valid batch or nothing.
- Exact required SKU upserts; leading-zero barcodes and Unicode text survive reversible spreadsheet-safe export/reimport. Duplicate SKU/barcode conflicts (including archived/omitted products) and missing price groups reject the batch.
- Stale revision fails without overwrite. After lost response, authoritative refresh and a new review prevent duplicate creates. Failed refresh never silently discards later unsaved edits.
- Test malformed quoting, headers, booleans, types, controls, bounds, precision and overflow, and source positions after quoted multiline names.
- Existing receipt bytes, cash ledger, drafts and v1/v2 backups remain valid; catalog import does not edit pricing groups, stock or tax.
- Native selection exposes no caller path, uses safe regular-file opening and bounded reads, and grants no general filesystem access.
- Browser CI captures real invalid/valid desktop/mobile review screens, including before/after values. Local discovery is not browser execution; only promote screenshots after CI passes.
