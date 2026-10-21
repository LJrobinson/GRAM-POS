# Milestone 11 local verification (2026-10-21)

Source baseline: merged main `439f19a94bdf063489994abf262dd6310fc2e32b`, tree `95eaeac8079c5a7d21e0109cd23743add3efdf93`. Restored into a separate work folder without overwriting the older export; all baseline files were checked against Git blob hashes.

- `npm test`: **137 passed**, including six focused history search/timezone/request-guard tests.
- `npm run typecheck`: **passed**.
- `npm run build`: **passed**.
- Independent focused review: no actionable findings in search semantics, exact receipt identity, request invalidation or phone timestamp CSS; six focused history tests rerun successfully. This was code review, not browser execution.
- Local Rust/service/native and real-browser E2E: **not run**. No Rust toolchain survived the workspace restore; official Rust download was policy-blocked. No mirror or bypass was attempted. Existing GitHub workflows must verify the exact published commit before this increment is called ready.
- New browser specification: **6 scenarios discovered** and standalone strict TypeScript check passed (discovery is not execution). Covers an older receipt beyond 100 newer real sales, desktop/phone search and read-only records, receipt/refund identity after catalog changes, refresh/reload, and delayed-response cancellation.
- Browser screenshot capture is included in the new scenarios; no new milestone-11 screenshot or browser-pass claim is made before that run completes.

The feature is read-only and uses unchanged service commands/schema. Existing prior milestone evidence below is historical and does not prove this increment passed.

# Milestone 10 validation

Implemented from merged main `6407721177674a16c155e4c3ad91b1ebc5a54d0a` on 2026-10-19. Before editing, all **142 tracked local blobs**, including binary assets, matched remote tree `f3bd0ba072f26afcfe34b50403f47462a6e2e297`; an immutable baseline was retained. App version is **0.10.0**, SQLite/portable backup version is **4**, and separate sales JSON is version **3**. Legacy backup v1/v2/v3 recovery and historical receipt/request/session bytes remain supported. Pricing/config schemas are unchanged.

## Scope and invariants

Explicit reviewed/confirmed paid-in and paid-out entries during an open cash session, positive exact amounts, bounded nonpersonal reasons, independent immutable identities and retry keys, restart/restore reconciliation, exact drawer conservation, and closing revision protection. The conservative paid-out rule rejects an amount above current expected drawer cash. It does not infer physical cash or change refund behavior. No receipt, sale, refund, revenue, tax, stock, sync, authorization framework, live financial action or hardware activation is fabricated.

Session totals and closing views distinguish sales, refunds, paid-ins and paid-outs. Record times use the configured store timezone. Transaction-ordered sale/refund watermarks let backup replay validate available cash without relying on timestamps; clock rollback remains supported. Cross-window cash journals share the existing register writer Web Lock, use exact compare-before-write/remove checks, and reject malformed recovery records. A stale window cannot resurrect an action whose journal another window completed or discarded.

## Local verification

- `cargo test --workspace --all-targets --all-features --locked`: **177 tests passed**, comprising the prior 154 plus 17 cash-movement contract/race tests and 6 movement export/replay tests.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` and `cargo fmt --all -- --check`: passed.
- Workspace doctests (no cases) and the pricing-demo example: passed.
- Frontend typecheck, **131 pure tests** (103 prior plus 28 cash/recovery/guard regressions), and production build: passed.
- Strict standalone TypeScript checking of the new browser specification and guarded-fixture helper: passed. Playwright discovery: **47 scenarios**, including the 37 prior scenarios and 10 cash-movement scenarios. Discovery is not browser execution.
- Native host formatting and locked no-dependency Cargo metadata: passed. Two new dispatcher scenarios also passed in a portable extracted Rust harness against the real service, without Tauri system dependencies. The native suite now contains 14 Windows test cases; its final Windows execution and packaging remain CI-pending.
- All pre-settings golden fixture files remain byte-identical. Valid v2/v3 closed-session snapshots without new total fields retain their exact raw JSON through restore.

Tests cover zero/negative/overflow/excess-precision inputs, UTF-8 reason limits and controls, mandatory fields and unknown authority, currency/precision/session mismatches, wide integer aggregates, explicit overdraw rejection including negative-refund balances, one-time retries after restart/close/restore, exact-matching and conflicting concurrent keys, competing paid-outs, stale-close races, immutable constraints, sequence watermarks and malformed replay. Four independent-connection races also passed 20 repetitions each. Ordinary parked-cart tests remain in the full suite.

A final parallel test run exposed a pre-existing timestamp-only temporary-path collision in the parked-cart test helper. The helper now adds a process-local atomic counter; no assertion, production behavior or historical fixture was weakened. The full final run passed after this isolation correction. An old pure ledger fixture was updated to include the session/currency guards already emitted by the prior UI; separate new tests explicitly reject missing or malformed recovery fields.

## Independent review

Read-only review found no blocking correctness issue in transaction/replay ordering, immutable closes, currency isolation, cash/revenue separation or cross-window recovery. It independently passed 39 focused Rust tests, all 131 frontend tests and typecheck. An additional harness completed **108 backup/ledger/sales-export roundtrips across 12 sessions in USD, JPY and KWD**, and verified that an omitted nonzero closed paid-out total fails validation before creating a restore destination. Independent Node checks covered lost-response reload, exact retries, stale-tab ownership, corrupt journals and amount/reason boundaries.

## Browser, screenshots and native boundary

Ten new real-Rust browser scenarios cover amount/reason review, Cancel without writes, conservation with a sale and refund, repeated clicks during a held real response, both paid-in/out lost-response reload recovery, closed-session rejection, another window invalidating a close, input/overdraw rejection, malformed-journal startup, and two-window recovery ownership. Network interception only delays or loses actual service responses; it does not fabricate financial results. History asserts Tokyo timestamps while the browser timezone is Honolulu.

For source head **`f3a798f2a1b03e8bd1839af806743a71040a421a`**, both the [PR browser job](https://github.com/LJrobinson/GRAM-POS/actions/runs/39211604267/job/117926524649) and [push browser job](https://github.com/LJrobinson/GRAM-POS/actions/runs/39211593331/job/117926508317) passed **47 of 47 scenarios**. The PR checked out merge `654865e1eca45689dc42a0deba0adf6828da26b2` against unchanged base `6407721177674a16c155e4c3ad91b1ebc5a54d0a`; the push job tested the source head directly.

Original PR artifact **12168096741**, `gram-pos-register-browser-review`, is **17,465,895 bytes** with ZIP SHA-256 **`08704837cf85d762c7f617bb2153bc082982c152c683e8ea06578e47a7722158`**. It was downloaded, matched to the source head and hash-verified. Eight original PNGs were copied byte-for-byte into `docs/screenshots/`: desktop **1440×1000** and phone **390×844** pairs for `cash-paid-in-review`, `cash-paid-out-review`, `cash-movement-closing`, and `cash-movement-history`. No generated image, crop, resize or pixel edit was substituted.

Visual inspection confirms the exact $12.50 paid-in review, $10.00 paid-out review, and full **50 + 25 − 5 + 12.50 − 10 = 72.50** drawer equation with zero closing variance. Records visibly retain distinct movement/session IDs, nonpersonal reasons and explicit Asia/Tokyo timestamps despite the Honolulu browser timezone. The captures fit horizontally. Phone dialogs scroll vertically: confirmation is visible, while Edit/Cancel may be below the captured view; phone history shows the first movement with later records below. All content is synthetic Cedar Market training data, not real customer records or financial activity.

At screenshot preparation, this source head had **18 of 20 PR/push checks successful**. The two Windows package jobs were still compiling native clippy: [PR Windows job](https://github.com/LJrobinson/GRAM-POS/actions/runs/39211604267/job/117926524802), [push Windows job](https://github.com/LJrobinson/GRAM-POS/actions/runs/39211593331/job/117926508292). These pending runs do not establish Windows success. Final PR-head native tests, no-skip evidence and packaging must be verified separately after the evidence-only update.

Local Chromium/socket and GTK/GObject restrictions remain. No blocked browser launch, permission escalation or native build workaround was attempted. The extracted dispatcher harness is not Tauri/WebView2 runtime execution. Passing browser CI or unsigned packaging does not certify physical phone/tablet support, native dialogs, scanners, printers, scales, cash drawers, production payments, taxes or compliance. Final PR-head CI and artifact provenance must be recorded separately after publication; user approval is still required to merge.

---

# Milestone 9 validation

Implemented from merged main `b80f472199dcc103a0fcaa4d58b45a45afe2c957` on 2026-10-19. Before editing, all **125 tracked local blobs**, including binary assets, matched the remote Git tree. App version is 0.9.0; SQLite schema 3 and portable backup v3 add durable active/parked identities and mutation recovery records while accepting older v1/v2 backups. Pricing/config schemas and immutable historical receipt bytes are unchanged.

## Scope

Explicit Park/list/preview/resume/confirmed discard, optional nonpersonal labels, exact original quantity types/text, current-price/catalog warnings, active-cart overwrite prevention, revision-checked serialized saves, idempotent recovery across lost responses/restart, and atomic one-time consumption with ordinary cash checkout. No inventory reservation, customer records, live payment, sync, taxes, compliance or license choice is added. Parked carts do not count as completed sales or cash activity.

## Local verification

- `cargo test --workspace --all-targets --all-features --locked`: **154 tests passed**, comprising the 132 prior tests plus 22 parked-draft regressions.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` and `cargo fmt --all -- --check`: passed.
- Workspace doctests (no cases) and the pricing-demo example: passed.
- Native host formatting and locked no-dependency Cargo metadata: passed. Two new dispatcher scenarios also passed in an extracted portable Rust harness against the real service, without Tauri system dependencies. This does **not** count as native Tauri runtime or WebView2 execution.
- Frontend typecheck, **103 pure tests** (69 prior plus 34 draft/writer regressions, including the CI corrections below), and production build: passed.
- Strict standalone TypeScript checking of the new browser specifications and guarded-fixture helper: passed.
- The original golden pre-settings backup and receipt fixtures are byte-identical. Their whole-file backup regression explicitly changes only the versioned envelope/new empty workflow section; historical stored JSON bytes are still checked exactly.

## Independent review and CI corrections

Separate read-only review covered transactions, cross-window state, frontend journals and backup consistency. Fixes address legacy nonempty draft identity initialization, recovery error-code mismatches and checkout-first boot recovery, storage-write failures retaining unsent local work, per-line invalid-quantity warnings, canonical guard revisions, ordinary queued scans vs autosave, and cross-window recovery-key overwrite/removal. Backup validation replays ordered guarded events and verifies coverage from an explicit guarded-start revision, including the first empty save. Immediate pre-removal journal checks protect conflict adoption after awaited reads.

Initial browser CI at `331eb535` found a real frontend integration defect missed by the pure fixtures and initial review: Rust JSON object-key order differs from locally constructed JavaScript objects. Stringified cart comparison wrongly treated a successful save as a competing-window change. Cart equality now compares each typed field and exact quantity text, retaining array order, IDs, SKUs and quantity kinds. Exact recovery-journal byte comparisons are unchanged. A new regression failed before the correction and passed afterward; independent verification also replayed the actual failed-CI request/response bytes through both controller versions.

Recovery remains visible until its retry is acknowledged, and Add is disabled while another cart save is in flight. Browser fixture helpers now assert the real HTTP 400 domain-error contract rather than requiring HTTP 200 for intentionally rejected commands. Recovery tests await the successful response and cleared journal before asserting the same two requests, identical payloads and unchanged revision. No retry-count, quantity, duplicate-sale or overwrite assertion was removed. Final read-only review reported no remaining blocking finding; it independently passed all 103 frontend tests and typecheck.

## Verified published browser evidence

For source head **`85eb436b8359ebc6d3fbc129d0d74a0679ed78d5`**, both the [PR browser job](https://github.com/LJrobinson/GRAM-POS/actions/runs/39206584813/job/117911033610) and [push browser job](https://github.com/LJrobinson/GRAM-POS/actions/runs/39206581046/job/117910945749) passed **37 of 37 scenarios**. The PR job checked out merge `48aac34` against unchanged base `b80f472`; the push job tested the source head directly. The suite includes 15 parked-cart scenarios and an exact-Windows-origin Chromium capability probe, alongside the 21 prior scenarios.

Original PR artifact **12166281568**, `gram-pos-register-browser-review`, has ZIP SHA-256 **`9e047d5cc06472e146747ea19a83e00e5a94a4634837dbfb55e8b162be1d7730`**. It was downloaded, matched to that source head, hash-verified and inspected. Six original PNGs were copied byte-for-byte into `docs/screenshots/`: desktop/phone pairs for `parked-carts-preview`, `parked-cart-price-warning` and `parked-cart-product-review`. No generated image, crop, resize or pixel edit was substituted.

Visual checks confirm separate **3.621 g measured**, **2 packages** and **3 units**, a $30.87 current quote, a changed-price $33.12 quote with review gate, and missing/archived/changed-type warnings without a usable total. Desktop captures are 1440×1000 and phone captures are 390×844. All fit horizontally. Long warning content is vertically scrollable; the product-review phone capture prioritizes the warnings, with confirmation/actions below the captured view. Cedar Market data is synthetic training content, not customer details or business revenue.

The earlier milestone-9 Windows [PR package job](https://github.com/LJrobinson/GRAM-POS/actions/runs/39204868276/job/117905844623) and [push package job](https://github.com/LJrobinson/GRAM-POS/actions/runs/39204854316/job/117905820261), at `331eb535`, passed **12 native dispatcher tests**, native clippy and unsigned NSIS packaging with offline WebView2. Their `--show-output` logs contain no reparse-point test skip. Those results do not establish success for a later head. At screenshot preparation time, Windows tests/packaging for `85eb436` were still running; the final PR-head checks must be verified separately after this documentation update.

## WebView origin and evidence boundary

The Windows Tauri origin and `useHttpsScheme` setting are unchanged, preserving existing localStorage checkout keys. Same-origin draft and checkout journals use one Web Lock and fail closed when the capability is unavailable; there is no fake localStorage-lock fallback. Official origin documentation and upstream secure-context confirmation are linked in [native-host notes](apps/register/src-tauri/README.md). The passing Chromium probe explicitly verifies `http://tauri.localhost` is a secure context with available Web Locks and serializes two pages, but is **not** packaged Tauri/WebView2 runtime certification.

Local Chromium/socket and GTK/GObject limitations remain; no denied escalation was repeated. Runtime browser evidence above comes from GitHub CI against Rust/SQLite, not mocked screenshots. Browser success and unsigned packaging do not prove physical phone/tablet support, native OS-dialog usability, scanner timing, printers/scales, production payments/tax/compliance or independent security certification. Earlier README captures retain their original milestone labels.

---

# Milestone 6 validation

Implemented from merged main `d9792d757a9118534581ea0ed1979235bbe6d3bc` on 2026-10-19. All 85 tracked local blobs, including binary screenshots, were verified against the recursive remote Git tree before edits; no divergent files were overwritten. App version is 0.6.0. SQLite schema 2, configuration schema 1 and accepted portable backup formats are unchanged.

## Scope and regression coverage

True unit products use positive integer counts and checked fixed-unit money, without declared grams or weight-group eligibility. One optional exact barcode per catalog item preserves leading zeros and is unique across active and archived products. Existing receipt/request bytes and legacy backup/migration paths remain intact. Rust coverage includes unit overflow, incompatible quantities/pricing, unit-plus-measured carts, barcode UTF-8 limits/control characters/duplicates/exact lookup, old missing fields, draft/backup/retry round trips, immutable unit refunds and event-date report totals after product edits and archive.

Frontend coverage checks count/receipt labels and serialized scanner work, deliberate repeated submissions, exact identity, error/retry, measured prompts, stale revisions and cancellation. Real-Rust browser tests include delayed responses on navigation/cancel/cart clear, unknown/archived codes, focused keyboard input, unit checkout/refund snapshots and desktop/phone capture paths.

## Local verification

- Rust stable 1.99.1 `cargo test --workspace --all-targets --all-features --locked`: **111 tests passed** (49 core, 62 service), including the old migration and backup suites.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- Workspace documentation tests and the pricing-demo example: passed (no doctest cases).
- Native host formatting and locked no-dependency Cargo metadata: passed. Native Rust runtime tests/build were not executed locally.
- Frontend `npm run typecheck`, `npm test` (**52 tests**) and `npm run build`: passed for version 0.6.0.
- Strict standalone TypeScript compilation of the new browser spec: passed.
- `npx playwright test --list`: **9 scenarios discovered**, including 4 new unit/barcode scenarios; not runtime execution.
- Independent read-only review found a delayed-scan/cart-clear race; the fix now invalidates pending and queued work synchronously on Clear, navigation, manual selection and checkout. Two unit regressions and controlled delayed-response E2E coverage were added. Ordinary queued repeated scans remain valid. No outstanding concrete review finding remained at freeze.

## Runtime evidence boundary

No milestone-6 browser runtime, new screenshots, native Windows runtime or physical scanner pass is claimed locally. Chromium socket execution and native GTK/GObject dependencies remain blocked in this environment; denied escalation was not repeated. Existing README screenshots retain their original milestone-4/5 labels. Playwright discovery only proves test discovery, not execution. CI must run the real Rust/browser suite and native Windows checks/unsigned package for the published commit before merge. Actual keyboard scanner configuration/timing and native peripheral use remain unverified even if browser CI passes.


## Published PR-head browser evidence

[Real Rust register browser job 117843819615](https://github.com/LJrobinson/GRAM-POS/actions/runs/39184353224/job/117843819615) passed all **9 scenarios** for PR head `36b2428cb61530fec3d7e8950f5da989421737d8` (PR merge checkout `d98f862`). Original artifact `12157368288` has SHA-256 `0a7c55aba519e1b47f6d10aa8bea43cf6ac7800a1fac031632e3fcba4fbc3bb7`; the downloaded ZIP matched this digest.

All six published unit/scanner/editor PNGs were visually inspected. Desktop and phone scanner captures show two separate one-unit lines at $4.25, total $8.50, without invented mass. Focused editor captures preserve barcode `001234567890`, fixed per-unit price and absence of a declared-weight field. Receipt captures show three units, $12.75 total, $20.00 tender and $7.25 change. The mobile receipt scrolls within its modal; original quantities and money remain readable without horizontal overflow. README uses unmodified original Playwright viewport/element/page captures, not reconstructed UI.

An earlier full-page screenshot showed a blank mobile fixed modal despite passing functional assertions. The revised test captures the viewport and asserts receipt lines, total, visible intersection and no horizontal overflow after resizing. These assertions and visual inspection passed without application changes, establishing a capture problem rather than evidence of an app rendering defect. Extremely large drawer balances in scanner screenshots are deliberate shared exact-arithmetic fixtures, not real business activity.

At this capture head, core CI including Rust 1.70/Wasm and platform checks passed; frontend and Linux/Windows service checks passed. Native Windows formatting, clippy and tests passed, with unsigned installer packaging still in progress when the evidence update was prepared. The final documentation head must pass its own CI and publish its matching installer artifact. Browser text-entry tests do not certify a physical scanner or native-device peripheral behavior.

---

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


## Milestone 7 local validation (0.7.0)

Based on merged main `1db7e8ad831c0bc93f1acdbb5802fd742b224eb9`: all 97 tracked local blobs matched the remote recursive tree before editing. No baseline files were silently replaced.

Passed locally with Rust stable 1.99.1:

- `cargo +stable fmt --all -- --check`
- `cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings`
- `cargo +stable test --workspace --all-targets --all-features --locked`: 120 tests including nine catalog CSV integration tests.
- Workspace doc tests.
- `npm run typecheck`, `npm test`: 62 frontend tests, and `npm run build`.
- `npm run test:e2e -- --list`: 16 tests discovered, including seven new catalog CSV browser scenarios. Discovery does not execute browser tests.
- Native `cargo fmt --manifest-path apps/register/src-tauri/Cargo.toml -- --check` and full `cargo metadata --manifest-path apps/register/src-tauri/Cargo.toml --locked --offline --format-version 1`.
- Two extracted, unchanged native file-reader helper tests compiled and ran separately on Unix, covering safe link/FIFO rejection, byte bounds, strict UTF-8, extension and regular-file checks. This is not compilation or execution of the full Tauri host.

The catalog tests cover read-only preview; atomic invalid-batch rejection; separate-connection revision race and reopen/retry; duplicate/archived barcode conflicts; exact maximum money/weight and leading zeros; malformed quoting/headers/types/control input/limits; Unicode/commas/newlines and injection-safe reversible text export; full configuration roundtrip; and unchanged receipt/sales/ledger/draft history. The UI regressions cover late/cancelled results, unsaved edits, review gating, exact before/after display, and recovery-refresh data-loss prevention.

Two read-only reviews identified and fixed a native file check/open race and unsaved-edit recovery refresh path before freezing. No additional blocking findings remained.

Not run locally: real Chromium browser execution/screenshots, complete native Tauri compile/tests or Windows installer. The browser executable and Linux GTK/WebKit runtime are unavailable; no denied installation/escalation was retried. The existing CI jobs must validate the exact published commit before merge. New browser tests attach actual invalid/valid desktop/phone CSV review captures; no new screenshot is claimed or fabricated here. The Windows reparse-point regression can explicitly skip if the runner lacks symlink privilege, so manual native-picker review remains useful even after CI passes.


### Milestone 7 verified browser CI evidence

At head `946979bfe204339ef6d4234920a5917aee702708`, both the PR and push browser jobs passed all **16 scenarios**, including all seven catalog CSV scenarios. The committed original CSV PNGs come from PR run [39187501879](https://github.com/LJrobinson/GRAM-POS/actions/runs/39187501879), browser job [117853222635](https://github.com/LJrobinson/GRAM-POS/actions/runs/39187501879/job/117853222635), artifact `12158324168` (`gram-pos-register-browser-review`). The downloaded original ZIP SHA256 was verified as `b62d73b479348bd30eacc32b5a153bdb7fdde5216833fc345361f2788cf1be91`.

The four unmodified CSV screenshots were visually inspected: desktop valid preview shows the create/update summary and saved/imported product details; desktop invalid preview shows exact row/line price and duplicate-barcode errors with disabled Apply; phone captures show wrapped review/errors without horizontal page overflow. Screenshots are viewport captures, so lower details and actions can require vertical scrolling. Large price and formula-shaped text fixtures are intentional exact-arithmetic/injection tests, not real transactions.

The narrow CI change `-- --show-output` on native tests makes the Windows reparse-point test's privilege-skip message observable in the final-head logs. It does not elevate permissions or claim a physical file-picker test. At preparation of this evidence update, both original-head Windows packaging jobs remained running. The final evidence/workflow commit must receive its own complete CI run before any all-green or installer claim.

## Milestone 8 local validation (0.8.0)

Baseline: merged main `5fd88ae5253000aa939aedb13d33b7fb5fc98ef7`. All 108 tracked local Git blob hashes matched the complete recursive remote tree before edits; no preexisting differences were overwritten.

Passed locally with Rust stable 1.99.1:

- Workspace `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- `cargo test --workspace --all-targets --all-features --locked`: **132 tests**, including six store-settings and six timezone integration tests.
- Workspace documentation tests and the pricing demo example.
- `npm run typecheck`, `npm test`: **69 frontend tests**, and `npm run build`.
- `npm run test:e2e -- --list`: **21 scenarios discovered**, including five new actual-service settings/timezone scenarios. Discovery is not browser execution.
- Native rustfmt check and complete `cargo metadata --manifest-path apps/register/src-tauri/Cargo.toml --locked --offline --format-version 1`.

Backend coverage includes genuine pre-change config/receipt/backup fixtures with byte-preserving legacy roundtrips; independent-connection stale settings rejection; unchanged currency/catalog/pricing/draft/drawer; checkout quote invalidation; invalid controls/zone/backup rejection; new branding retained through later edits, restart, refund and backup export/restore. Report coverage includes UTC compatibility, Los Angeles 23/25-hour DST days, inclusive/exclusive exact instants, refunds on their own local event date, per-event offsets, Monrovia's historical second-precision offset, leap/month and pre-epoch date bounds, invalid zones and ambiguous/nonexistent/skipped midnight rejection, exact totals and stale/empty CSV metadata.

Frontend coverage includes legacy GRAM/UTC formatting, saved receipt branding/timezone independent of current settings, repeated DST-hour offsets, unsupported browser-zone fallback, signed historical UTC boundary rendering, revision/zone matching and stale-request guards. New browser scenarios exercise read-only preview/cancel, explicit save, immutable branded/legacy receipts and refund printout, stale save, DST/report metadata, unknown zones, delayed report response invalidation, export-time setting changes, and confirmed/cancelled legacy settings reload. They attach actual desktop/phone settings, receipt and report captures when executed by CI.

An independent read-only review found two UI edge cases before freeze: confirmed reload on unchanged legacy settings did not reset the unsaved editor, and negative UTC report boundaries were rejected by the display formatter. Both were fixed with explicit editor reset and a signed boundary formatter; unit/browser regression coverage was added, and review confirmed no remaining blockers. Full applicable local checks were rerun after fixes.

Not run locally: actual Chromium browser execution/screenshots, full Tauri host compilation/tests, Windows packaging, core Rust 1.70 and WebAssembly target checks. Only stable 1.99.1 and the host target are installed here; browser sockets and native GTK/WebKit are known unavailable. No denied installation/escalation was repeated. Existing CI must validate the exact published commit on real browser/Windows, minimum Rust and WebAssembly. The new native export timezone/stale-revision test is included for Windows CI, not claimed as locally executed. New README screenshot evidence remains pending that run; no fabricated image is supplied.

The portable core remains unchanged and dependency-free. New timezone dependencies are confined to the register service; workspace lock stays format 3 for Rust 1.70 compatibility, and both workspace/native dependency locks are updated. No broader filesystem permissions, live payments, taxes, inventory or sync were added.

### Milestone 8 first CI correction

At head `9d2a303953e8de6c3f60098088e206d3a5824ebb`, all ten core PR/push jobs (including Rust 1.70 and wasm), both frontend jobs and both Ubuntu services passed. Both Windows service jobs exposed a checkout-only difference in the golden backup fixture: Git converted its LF bytes to CRLF before the byte-preservation assertion. The assertion was retained, and `.gitattributes` now disables text conversion for the three exact-byte compatibility fixtures. This does not change backup serialization or relax receipt immutability. Browser/native jobs were skipped by that dependency failure, not executed successfully. The corrected head requires its own full CI.

### Milestone 8 verified real-browser evidence

At head `08bbd98f4d362e6432044617f8f4eeb5808d9ca4`, both Windows service jobs passed after the fixture-checkout correction, and both real-browser jobs passed. The six new original PNGs come from PR run [39194908384](https://github.com/LJrobinson/GRAM-POS/actions/runs/39194908384), browser job [117875732987](https://github.com/LJrobinson/GRAM-POS/actions/runs/39194908384/job/117875732987), artifact `12161750878` (`gram-pos-register-browser-review`). Original ZIP SHA256 was verified as `a104a8dcd0f0e414b8dcb8029c5cbc511dd4924641a9dc447d8f69de1470ad2a`.

All six images were visually inspected before copying unmodified to `docs/screenshots`: desktop settings capture includes actual editable fields, explicit actions and preview; phone focuses on wrapped identity preview and retained warnings. Both branded receipt captures display Cedar Market, saved contact/footer, explicit Los Angeles GMT-07 label and exact $8.50 sale/$1.50 change after current branding was changed. Desktop reporting shows zone, local dates, resolved UTC bounds, separate refund total and exact large-number fixture totals; phone shows date/zone controls without horizontal overflow, with totals below the fold. Large values are deliberate shared regression fixtures, not business revenue.

The same run's original `receipt-print.pdf` was rendered and visually inspected: one page, legacy GRAM/UTC identity, selected sale only, correct lines/totals and training notice. It is not a branded-receipt PDF or a physical-printer/native-dialog test; no such verification is claimed. The existing milestone-5 PDF remains separately labeled in the README rather than being silently replaced.

At preparation of this evidence commit, Windows packaging was still running on the source head. The final evidence/docs head must receive its own complete PR/push checks and generated installer artifacts before an all-green or ready-to-merge claim.
