# Milestone 3 validation

Validated locally on 2026-10-18 in Linux x86_64, against the milestone-2 main source at `b61735349657c16480e0cc92204144c5dceec30d`. The existing source export was compared with the published milestone-2 text files before changes; no divergent source was found.

## Executed successfully for milestone 3

- Rust stable 1.99.1: `cargo test --workspace --all-targets --all-features --locked`: 68 tests passed (43 pricing core, 18 register persistence/catalog, 7 export/recovery).
- `cargo test --workspace --doc --locked`: passed, no documentation test cases.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.

Coverage includes blank setup, product creation and exact custom pricing above JavaScript safe integer range, archive/unarchive with saved drafts, duplicate IDs, malformed fields and schedules, atomic rejected settings, competing editor revisions, restart persistence, frozen receipt names/prices, retry after catalog changes, coherent backup round trips, raw legacy snapshot fidelity, duplicate/corrupt restore rows, missing receipts, preserved sale sequence, and refusal to overwrite any existing recovery destination.

- Frontend `npm run typecheck`, `npm test` (18 tests), and `npm run build`: passed.
- Native `cargo fmt --manifest-path apps/register/src-tauri/Cargo.toml -- --check`: passed.
- Native `cargo metadata --manifest-path apps/register/src-tauri/Cargo.toml --locked --no-deps --format-version 1`: passed. Dialog dependency resolution adds 14 packages without upgrading unrelated dependencies.

## Verification limits

- Native `cargo check --locked` attempted but blocked before application compilation: Linux GObject/GTK development pkg-config libraries are unavailable. Native application clippy/tests and installer build have not passed locally.
- Browser E2E launch attempted but blocked: downloaded Chromium archive was unusable; installed Chromium could not create required sockets, and the supported escalation failed during environment setup. No browser runtime result or new screenshot is claimed locally. Three E2E scenarios and catalog/export screenshot capture are included for the existing CI job.
- The blank-onboarding browser scenario uses a mocked transport; catalog editing and existing checkout scenarios use the real Rust service. Rust integration tests cover the actual empty setup and restore behavior.
Windows packaging, native save-dialog interaction, iPad/iPhone, peripherals, production tax/payments/compliance, and independent security certification are not established by these Linux checks. This remains a prototype/demo only.

---

# Milestone 1 validation

Validated on 2026-10-18 in a Linux x86_64 build environment, after renaming the crate to `gram-pos-core`.

## Executed successfully

- Rust 1.99.1: `cargo test --all-targets --offline`: 4 module tests + 39 integration/acceptance tests passed; 0 failed.
- Rust 1.70.0 (declared minimum): `cargo +1.70.0 test --all-targets --offline`: the same 43 tests passed.
- The integration suite includes 1,000 deterministic seeded cases checking an independent price oracle, total conservation, cart splitting, input permutation, and preserved quantities. These are scenarios within one test, not 1,000 additional Rust test functions.
- `cargo test --doc --offline`: succeeded; currently 0 documentation test cases.
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets --offline -- -D warnings`: passed.
- `cargo check --lib --target wasm32-unknown-unknown --offline`: passed (compile check only, not browser execution).
- `cargo run --example pricing_demo --offline`: passed and printed the outputs below.

## Demonstrated behavior

Illustrative prices, USD with two decimal places, explicit half-up rounding:

- Two deli lines of 3.620 g and 3.580 g qualify together for a configured 7.000 g / $55 tier with 0.300 g allowance. Original measurements remain intact. Group total is $55; line allocations are $27.65 and $27.35.
- Two fixed-price packages each declaring 3.500 g at $35 each contribute $70, but do not qualify for the deli promotion. Cart total is $125.
- At a $30 per 3.500 g tier with 0.150 g allowance, 3.660 g produces $31.37 under the full-actual policy, or $30.09 under the nominal-plus-excess policy.

## Not validated or claimed

No Windows/macOS/iPhone/iPad application builds, hardware drivers, database persistence, taxes, payments, network synchronization, production deployment, or legal compliance testing. The included GitHub workflow has not been executed as part of this local validation. No performance benchmark or security certification is claimed.

The pricing model deliberately implements only one explicitly selected tolerance scope: once per aggregate pricing group. That scope, the below-smallest behavior, and tier-rate semantics are described in `docs/acceptance.md`; test success is not a substitute for merchant review of those policies.

Public repository creation/publication, if completed after this validation, is reported independently with its verified URL. A license has not yet been selected by the owner.
