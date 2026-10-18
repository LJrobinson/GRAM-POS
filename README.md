# GRAM POS — pricing core, milestone 1

**GRAM POS** starts with the portable `gram-pos-core` Rust library. A portable, dependency-free Rust pricing library for LJ Robinson's configurable, open-source POS project. The first use case is deli and prepacked flower; the domain is deliberately generic.

This is a **pricing-core prototype**, not an installable register, production dispensary system, compliance engine, payment processor, inventory system, or ERP. It does not take payments or record completed sales to a database. Repository: https://github.com/LJrobinson/GRAM-POS. This is the initial pricing-core milestone.

## Run locally

Install Rust from https://rustup.rs/ (Rust 1.70 or newer) and open a terminal in this directory:

```sh
cargo test --all-targets
cargo test --doc
cargo run --example pricing_demo
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

The crate has no third-party dependencies. Test execution is local. A proposed GitHub Actions workflow is included for Windows, macOS, Linux, and a WebAssembly compile check; its presence is not evidence that those remote jobs ran.

## What this milestone models

- Exact weights in integer milligrams; exact currency amounts in integer minor units, with rational intermediate calculations.
- Measured deli quantities separate from prepacked unit counts and declared package contents.
- Configurable nominal weight tiers and one-sided overfill allowances, inclusive at the upper edge.
- Both explicitly selected merchant overage policies: full actual-weight charge at the selected tier's rate, or nominal tier price plus only the mass beyond its tolerance.
- Cart-wide mix-and-match through explicit common pricing groups, retaining original line quantities.
- Fixed-unit prepack pricing by default only when the caller explicitly selects it; weight promotion participation requires an explicit group assignment.
- Deterministic allocation of rounded group charges to lines, preserving the exact group total.
- Calculation explanations and configuration snapshots suitable for later receipt/audit integration.

## Merchant choices, not hidden defaults

Callers must explicitly supply the rounding policy, overage policy, tier allowances, below-minimum behavior, and supported tolerance scope. Demo prices and choices are test data, not recommended commercial settings. There is no automatically selected currency.

Tolerance is currently applied **once to the total eligible weight of a pricing group**, not once per visual cart row. This is an explicit prototype policy, not a claim that LJ has chosen it as the sole future workflow. It avoids multiplying allowances by splitting lines. Separate per-portion allowances require a deliberate future design with stable portion identities.

The implemented pricing schedule is a single selected tier rate, not a general promotion optimizer. It does not automatically compose bundles, stack discounts, choose the cheapest combination, or merge different price bands. Put different price bands into separate groups. When a larger tier is crossed, its configured rate is selected; merchant schedules can intentionally create price discontinuities. Setup previews/warnings belong in the future configuration UI.

## Physical truth versus pricing

A deli quantity of 3.620 g stays 3.620 g even when a nominal 3.500 g price applies. Two packs declaring 3.500 g each remain **two packages with 7.000 g declared content**, not a claim of scale-measured mass. Configuring a retail size of 28.000 g does not redefine a physical ounce.

The API accepts explicit grams/milligrams; it does not silently convert the label “ounce” into a weight. Currency codes are syntax-checked, not validated against an online registry. Input precision beyond 1 mg is rejected rather than rounded. More general unit conversion and higher-precision adapters can be added without changing pricing semantics.

## Deliberately outside milestone 1

Register UI, installation packages, SQLite storage, taxes, cash/card tender, discounts beyond configured quantity schedules, finalized-sale persistence, returns/refund execution, inventory balances or lot allocation, device drivers, network sync, customer records, and jurisdiction-specific compliance.

The intended application direction is independent Windows/tablet and Apple-device registers with local storage. This crate is isolated from those platform concerns. A successful WebAssembly compile does not provide browser storage, UI bindings, or hardware integration. Native iOS/iPadOS applications still require Apple tooling and platform testing.

## Project structure

- `src/`: reusable domain types, validation, pricing and allocation.
- `examples/pricing_demo.rs`: runnable concrete price calculations.
- `tests/`: independent acceptance and invariant tests.
- `docs/acceptance.md`: behavior coverage and design choices.
- `VALIDATION.md`: commands actually executed and their results.
- `PROVENANCE.md`: references, selected fixture adaptations, and pending license decision.
- `.github/workflows/rust.yml`: proposed CI after a repository is selected.

## GitHub handoff

LJ selected the public GitHub repository `LJrobinson/GRAM-POS`. The license is still pending. This source is prepared for its initial commit. Do not overwrite the existing UnitMath, CannabisMath, or MOBY repositories. The crate sets `publish = false`; a public package release is not part of this milestone.

Before publishing: choose a license and add complete notices, review the accepted commercial rules, and run the included CI on the intended platforms. See `PROVENANCE.md` for details. No license has been selected on the owner's behalf.
