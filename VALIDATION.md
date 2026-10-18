# Milestone 5 validation

Implemented from merged main `82d3e2ae773a591d4c253764be81215f5687d9df` on 2026-10-18. All 68 tracked local blobs were verified byte-for-byte against that Git tree before editing; no divergent user files were overwritten. App version is 0.5.0. The SQLite schema and backup/recovery formats are unchanged.

## Executed locally

- Rust stable 1.99.1 `cargo test --workspace --all-targets --all-features --locked`: 96 tests passed (43 core + 53 service).
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace --doc --locked`: passed, no documentation test cases.
- `cargo run --example pricing_demo --locked`: passed.
- Native host formatting and locked no-dependency metadata: passed.
- Frontend `npm run typecheck`: passed.
- Frontend `npm test`: 41 tests passed (including 10 receipt and 10 report tests).
- Frontend `npm run build`: passed for app version 0.5.0.
- `npx playwright test --list`: all 5 browser scenarios discovered; discovery is not runtime execution.

New backend coverage checks strict Gregorian/UTC dates and start-inclusive/end-exclusive boundaries, refunds dated outside the original-sale range, currency-plus-precision separation, negative net, exact totals beyond u64, legacy sales exactly once, renamed/archived catalog history, unchanged backup/replay, corrupt-ledger rejection, Unicode/quote/newline CSV escaping, formula-prefix defense and header-only empty exports. Independent read-only code review found no backend semantic blockers.

## Runtime evidence boundary

No milestone-5 browser runtime, screenshots, native Windows runtime or physical printing pass is claimed locally. Browser socket execution and native GTK/GObject dependencies remain unavailable in this environment; no repeat escalation was attempted. CI runs the real Rust/browser scenarios and Windows native checks/unsigned packaging. The new browser scenario verifies immutable receipt contents after catalog changes, explicit print-call cancellation without ledger mutation, print-only CSS/DOM and Chromium PDF output, report totals/date filtering/CSV content, and desktop/mobile screenshots. Test discovery or successful packaging is not physical printer, native save-dialog, production-payment/tax/compliance, or security certification.

## Verified milestone-5 browser CI

At commit `5309fc2c6695999115c5a9b5e348b1a51205f0e9`, [Register browser job](https://github.com/LJrobinson/GRAM-POS/actions/runs/39177654986/job/117823723881) passed all five scenarios. Original PNGs and the print PDF were downloaded from artifact `12154867772`, matched to that exact head, and visually inspected. No generated mockup, cropping or pixel editing was substituted. The single-page PDF text and rendered pixels contain only original receipt `DEMO-00000002`, the exact $45.75 total, $50.00 tender, $4.25 change and original quantities; navigation, other receipts and current refund metadata are excluded. The separate large-amount print capture retains every digit with wrapping and no overflow.

The report capture shows $178.00 gross sales, $170.75 refunds and $7.25 net; downloaded CSV was parsed and agrees exactly, with an apostrophe-neutralized formula-leading Unicode refund reason. Mobile reports use an intentionally horizontally scrolling activity table while the page and totals fit the viewport. README now includes unmodified report/receipt captures and the actual sample print PDF; older register/catalog/cash-session captures remain labeled milestone 4.

At this head, Rust core CI passed Linux/macOS/Windows, Rust 1.70 and Wasm checks. Register frontend and service checks passed on Linux and Windows. Native Windows checking/unsigned packaging was still running when this screenshot update was prepared; the final PR-head workflow and artifact must be checked separately. Browser PDF generation does not prove native OS-dialog or physical-printer operation. Current behavior/limits are documented in [receipts and reports](docs/receipts-and-reports.md).

---

# Milestone 4 validation

Implemented from main `ed648bed085606483d528381b22093b619442dcd` on 2026-10-18. Every tracked local source/artifact was verified against the published Git tree before editing; no divergent files were overwritten.

## Executed locally

- Frontend `npm run typecheck`: passed.
- Frontend `npm test`: 21 tests passed, including exact signed drawer variance, currency/precision matching and unchanged saved mutation keys on reload.
- Frontend `npm run build`: passed for app version 0.4.0.
- Native host formatting and locked no-dependency metadata: passed.
- Rust stable 1.99.1 `cargo test --workspace --all-targets --all-features --locked`: 84 tests passed (43 core, 18 persistence/catalog, 10 cash ledger, 7 existing export/recovery, 6 ledger export/recovery).
- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --doc --locked`: passed, no documentation test cases.
- `cargo run --example pricing_demo --locked`: passed.
- `npx playwright test --list`: discovered all 4 browser scenarios; discovery is not runtime execution.

Ledger coverage includes exact refunds after catalog edits, changed-key double-refund rejection, concurrent refund attempts, stale/retried closes, sale-vs-close serialization, rollback when association insertion fails, immutable SQL records, restart, currency/precision/session guards, very large and negative drawer totals, schema-1 migration/legacy replay, malformed migration rollback, v1 restore, v2 raw-byte roundtrips and sequence retention, corrupt links/totals/currencies, and clock rollback. Review identified that wall-clock ordering must not control backup validity; regression now verifies nonmonotonic timestamps and future-dated legacy receipts remain exportable/recoverable while structural session order is enforced.

## Runtime evidence boundary

No milestone-4 browser screenshots or native Windows runtime pass is claimed locally. The previously verified environment lacks native GTK/GObject dependencies and browser socket execution; no repeat escalation was attempted. The Register CI workflow runs real-Rust browser E2E and native Windows checks/unsigned packaging. New browser coverage captures full-refund review, cash-session closing review, and mobile session history/open state; it exercises lost-response/reload recovery for opening, refund and closing, plus repeated refund clicks. The original mixed-cart checkout/catalog/export checks remain. The blank-onboarding case uses mocked transport; ledger cases use the real Rust adapter.

## Verified milestone-4 browser CI

At commit `53332e4cf3cd2edb1cc7b20a2376cecfca189c85`, [Register browser job](https://github.com/LJrobinson/GRAM-POS/actions/runs/39170235816/job/117801860359) passed all four scenarios. Its original screenshots were downloaded and visually inspected for README; no generated mockup or pixel edits were substituted. The initial run exposed unnamed icon-only mobile navigation; explicit accessible labels corrected it, and the same mobile flow now passes.

Core CI at this commit passed Linux/macOS/Windows, Rust 1.70 and Wasm checks. Frontend and service checks passed on Linux and Windows. Native Windows packaging was still running when this screenshot documentation was prepared; the final PR-head workflow result must be checked separately. Successful unsigned installer builds do not establish real-device usability, peripherals, production payments/tax/compliance, or independent security certification.

---

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
