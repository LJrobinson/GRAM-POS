# GRAM POS

**GRAM POS** is a configurable, local-first point-of-sale project. The portable `gram-pos-core` Rust library powers a native register prototype. The portable pricing core is dependency-free. The first use case is deli and prepacked flower; the domain is deliberately generic.

Repository: https://github.com/LJrobinson/GRAM-POS.

## Milestone 3: your catalog, local exports

The register remains a **prototype / demo only**, now with a user-managed catalog instead of a mandatory seed catalog:

- Start an empty register with an explicit currency, or deliberately load illustrative demo data.
- Create and edit measured products and prepackages, fixed package prices, and shared weight-pricing groups with editable tiers and explicit policies.
- Archive or unarchive products without changing completed receipts. Archived products cannot be quoted or sold; saved drafts remain visible for correction.
- Save the whole configuration atomically. Duplicate identifiers, invalid quantities/prices, missing groups, and malformed policies are rejected by Rust. A revision check prevents one window silently overwriting another's configuration.
- Download exact, versioned sales JSON and a consistent logical backup of the local store. The backup includes catalog, settings, draft, immutable sales/receipts, and retry keys. See [backup and recovery](docs/backup.md) for formats, privacy, and safe recovery to a new file.

Changing a product name, price, archive state, group, or currency affects future quotes only. Existing receipt snapshots stay unchanged. Empty catalogs are valid; they cannot complete a sale until products are configured. This milestone adds no inventory, tax, live payments, compliance, or device synchronization.

## Milestone 2: demo register

A touch-friendly React interface calls the same Rust pricing engine through a local register service. Native Tauri packaging stores SQLite data in the application's per-user data directory. Registers operate independently; no hosted backend or device synchronization is required for the native app.

- Explicitly load a demo catalog before use; settings and receipts remain marked **DEMO / no tax configured**.
- Ring measured deli selections and integer-count prepackages in one cart.
- See Rust-computed mix-and-match pricing without duplicating price logic in JavaScript.
- Record a **demo cash sale** with tender, change, immutable receipt/config snapshots, and an atomic SQLite commit.
- Recover a saved draft, reopen local sales history, and retry uncertain checkout requests using the same idempotency key.
- Adjust supported pricing tiers, allowances, and overage settings; stale quotes cannot silently finalize at a different amount.

This is **not production-ready dispensary software**, a card payment processor, tax/compliance engine, or inventory/ERP system. No real electronic payment is taken. Demo cash recording is for development and evaluation, not permission to conduct regulated sales.

### Run the actual register locally for development

The browser development adapter uses the real Rust service and a chosen SQLite file. It is loopback-only and is not a hosted production backend.

Terminal 1, repository root:
```sh
cargo run -p gram-register-service --features dev-server --bin register-dev-server -- --db ./gram-demo.sqlite3 --port 4174
```
Terminal 2:
```sh
cd apps/register
npm ci
npm run dev
```
Open the displayed loopback Vite URL. Choose an empty catalog and currency, or **Load demo register**. Use a separate demo database for experiments; do not point this prototype at another application's database.

### Windows native development build

See [native setup and packaging](apps/register/src-tauri/README.md). The included Register workflow tests the service and frontend, then builds an **unsigned Windows development installer**. A successful artifact build is not a real-device usability or peripheral test. iPhone/iPad packaging is not implemented in this milestone.

Local data and exported files are not encrypted by this prototype. Protect the device and keep a separate copy of backups. Use the snapshot export instead of copying an active SQLite file. Recovery is an explicit offline operation to a new file, not an in-app overwrite. Migrations beyond the initial schema, refunds, inventory, and printer/scale drivers remain future work. Never replace or delete a merchant's data file to resolve an error.

## Milestone 1: portable pricing core


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

The implemented pricing schedule is a single selected tier rate, not a general promotion optimizer. It does not automatically compose bundles, stack discounts, choose the cheapest combination, or merge different price bands. Put different price bands into separate groups. When a larger tier is crossed, its configured rate is selected; merchant schedules can intentionally create price discontinuities. The configuration UI exposes these choices; test representative baskets after editing a schedule, especially near tier boundaries.

## Physical truth versus pricing

A deli quantity of 3.620 g stays 3.620 g even when a nominal 3.500 g price applies. Two packs declaring 3.500 g each remain **two packages with 7.000 g declared content**, not a claim of scale-measured mass. Configuring a retail size of 28.000 g does not redefine a physical ounce.

The API accepts explicit grams/milligrams; it does not silently convert the label “ounce” into a weight. Currency codes are syntax-checked, not validated against an online registry. Input precision beyond 1 mg is rejected rather than rounded. More general unit conversion and higher-precision adapters can be added without changing pricing semantics.

## Deliberately outside the pricing-core library (milestone 1)

The pure library does not implement UI, persistence, taxes, tender, refunds, inventory, devices, networking, customers, or compliance. Milestone 2 adds a separate demo UI/service with SQLite and cash-sale recording; all other production capabilities remain out of scope.

The intended application direction is independent Windows/tablet and Apple-device registers with local storage. This crate is isolated from those platform concerns. A successful WebAssembly compile does not provide browser storage, UI bindings, or hardware integration. Native iOS/iPadOS applications still require Apple tooling and platform testing.

## Project structure

- `src/`: reusable domain types, validation, pricing and allocation.
- `examples/pricing_demo.rs`: runnable concrete price calculations.
- `tests/`: independent acceptance and invariant tests.
- `docs/acceptance.md`: behavior coverage and design choices.
- `VALIDATION.md`: commands actually executed and their results.
- `PROVENANCE.md`: references, selected fixture adaptations, and pending license decision.
- `.github/workflows/rust.yml`: cross-platform core CI.
- `crates/register-service/`: local SQLite service and persistence tests.
- `apps/register/`: register frontend and native Tauri host.
- `.github/workflows/register.yml`: service/frontend checks and unsigned Windows development packaging.

## GitHub handoff

LJ selected the public GitHub repository `LJrobinson/GRAM-POS`. The license is still pending. The portable core and demo register are published; new milestones are reviewed through draft pull requests. Do not overwrite the existing UnitMath, CannabisMath, or MOBY repositories. The crate sets `publish = false`; a public package release is not part of this milestone.

Before publishing: choose a license and add complete notices, review the accepted commercial rules, and run the included CI on the intended platforms. See `PROVENANCE.md` for details. No license has been selected on the owner's behalf.
