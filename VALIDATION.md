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
