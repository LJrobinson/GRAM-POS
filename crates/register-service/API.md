# GRAM register service: milestone 6

`gram-register-service` is a transport-neutral, local SQLite service. It delegates every selling-price calculation and allocation to `gram-pos-core`. Its cash workflow records a manual cash tender; it does not process a payment or verify that physical cash changed hands.

This release is explicitly **demo only**. Fixtures and editable pricing policies are illustrative, not merchant defaults. No tax is calculated. There is no production tax engine, payment processor, inventory movement, partial refunds, user/role system, synchronization, fiscal certification, or printer integration. Do not use demo receipts for live trade.

## Rust interface

```rust
use gram_register_service::RegisterService;
use serde_json::json;
let mut register = RegisterService::open("my-selected-demo.sqlite")?;
let state = register.handle("get_state", json!({}))?;
```

- `open(path)` initializes a fresh recognized schema but does not seed settings/catalog.
- `handle(&mut self, command: &str, payload: Value) -> Result<Value, ServiceError>` is used by the desktop host and optional development HTTP adapter.
- `ServiceError` serializes as `{ "code": "...", "message": "..." }`. Storage error messages omit database paths.
- Public typed DTOs and methods are available for tests/native callers. The JSON contract lives in `src/dto.rs`.
- A `Mutex<RegisterService>` is sufficient in a desktop command host. SQLite transactions also protect duplicate checkout across independent connections.

## Exact JSON numbers

Money (minor units), package and unit counts, config revisions, sale IDs, Unix timestamps, allocation remainders, and rational numerators/denominators are decimal **strings**. Weight is a decimal grams **string** with at most three fractional digits, parsed directly by the core into integer milligrams. No floating-point value participates in a quote, checkout, or saved receipt. Small enum versions, currency exponents, and line counts are JSON numbers.

```json
{
  "lines": [
    {"id":"line-a", "sku":"bulk-a", "quantity":{"kind":"measured", "grams":"3.620"}},
    {"id":"line-b", "sku":"prepack-fixed", "quantity":{"kind":"prepacked", "count":"2"}}
  ]
}
```

Use stable, unique line IDs. Catalog SKU determines kind, declared package content, and pricing eligibility; callers cannot inject a price or package label into a sale line. Actual measured grams, declared content, package count, selected nominal tier, billable grams, and allocated price remain separate in snapshots.

## Commands

| Command | Payload | Result |
|---|---|---|
| `get_state` | `{}` | `{configured, config_revision, config, catalog, draft:{lines:[]}}` |
| `initialize_demo` | `{}` | State; fails `ALREADY_CONFIGURED` rather than overwrite settings |
| `initialize_empty` | `{currency:{code,minor_unit_exponent}}` | Empty configured State; never overwrites settings |
| `put_config` | `{config: Config,expected_config_revision:"1"}` | State, after complete validation and revision increment; stale revision fails `STALE_CONFIG` |
| `export_sales` | `{}` | `{filename,media_type,content}` with versioned full receipt JSON |
| `export_backup` | `{}` | `{filename,media_type,content}` with consistent logical backup JSON |
| `lookup_barcode` | `{barcode:"001234"}` | `{item: CatalogItem,config_revision:"1"}`; exact active match, otherwise `UNKNOWN_BARCODE` / `ARCHIVED_PRODUCT` |
| `quote_cart` | `{lines: CartInputLine[]}` | `Quote` from the Rust core |
| `save_draft` | `{lines: CartInputLine[]}` | `{saved:true}` |
| `complete_cash_sale` | Checkout below | `Receipt` |
| `list_sales` | `{}` | `{sales: SaleSummary[]}`, newest first |
| `get_receipt` | `{sale_id:"1"}` | Immutable `Receipt` |
| `get_sale_by_key` | `{idempotency_key:"..."}` | `{receipt: Receipt|null}` |

```json
{
  "idempotency_key":"checkout-generated-once-before-submission",
  "lines":[{"id":"line-a","sku":"bulk-a","quantity":{"kind":"measured","grams":"3.620"}}],
  "tender_minor":"5000",
  "expected_total_minor":"3000",
  "expected_config_revision":"1",
  "expected_session_id":"1",
  "expected_currency":{"code":"USD","minor_unit_exponent":2}
}
```

Generate and persist one idempotency key **before** submitting a checkout. Retrying the same complete payload returns its original receipt, even after later policy edits. Reusing a key for a changed payload returns `IDEMPOTENCY_CONFLICT`. Never generate a new key solely because a request timed out or its result was lost.

Finalization begins a SQLite immediate transaction, checks for replay, loads the current settings, recalculates the entire cart with the core, checks expected revision and total, validates tender, writes the immutable sale and receipt, clears the draft, and commits. Any pre-commit failure rolls all those writes back. Tender and change are separate exact integer strings, not sale line adjustments. Stored prices and receipt policy/catalog snapshots do not change when current settings change.

Errors `INVALID_INPUT`, `INVALID_CONFIG`, `NOT_CONFIGURED`, `INSUFFICIENT_TENDER`, `STALE_QUOTE`, and `PRICE_ERROR` insert no new sale in that attempt. An interrupted transport or `STORAGE_ERROR` should be reconciled by retrying **the exact same key and payload**, or querying `get_sale_by_key`. A key conflict indicates an already-used key; retrieve its original receipt rather than assuming no sale exists.

Drafts can contain incomplete quantity text for interrupted typing, but must have valid bounded IDs and supported DTO shapes. Draft recovery is not a quote. Saving configuration can make a recovered draft require correction. Checkout clears the saved draft atomically; host/frontends must serialize autosave and checkout, and not submit delayed old autosaves after a successful sale.

## Unit products and exact barcodes

A true unit product has `kind:"unit"`, `declared_content_grams:null` (or omitted), and `pricing:{kind:"fixed_unit",unit_price_minor:"125"}`. Its cart quantity is `{kind:"unit",count:"2"}`. Count is a positive u64 decimal string; multiplication and cart totals are checked for overflow. Unlike a prepack, it has no declared weight and cannot be assigned to a weight group, even when it is the group's only product. Unit lines use the existing fixed-unit explanation and never contribute to group eligible grams or tolerance. Measured and prepacked rules remain unchanged.

Every catalog kind may optionally have one `barcode` string. Missing/null means no barcode; serialization omits absent values, so legacy configurations, receipts, and backups retain their shape. Barcodes are exact case-sensitive identifiers, never numbers: leading zeros are significant. Valid barcodes have 1–128 UTF-8 bytes, no surrounding whitespace, and no Unicode control characters. No trimming, numeric coercion, case folding, prefix matching, or check-digit normalization occurs. Duplicates are rejected across both active and archived products. Lookup never falls back to SKU or name, never mutates a draft, and rejects archived products rather than silently selling them. The lookup response includes the revision of the same configuration read, allowing clients to reject stale scan results. Invalid barcode payloads return `INVALID_INPUT`; an unconfigured register returns `NOT_CONFIGURED`.

These are additive DTO changes, not a database migration. SQLite stays schema version 2 and configuration stays schema version 1. Saved unit drafts, receipt snapshots, and logical backups preserve unit counts and optional barcodes; historical receipts are not repriced after catalog edits.

## Settings

All pricing choices are required. An empty catalog is valid. Products have an `archived` boolean (absent legacy values mean false); archived products remain in configuration but return `ARCHIVED_PRODUCT` if quoted. Drafts are never silently deleted when products are archived. Duplicate SKUs and group IDs, unknown group references, malformed prices, and invalid tier schedules fail atomically. There are no implicit revenue-policy defaults. `put_config` validates the entire configuration before replacing the prior revision. The command requires the revision the editor loaded; its comparison and write share an immediate transaction. Typed `put_config_checked` provides the same guard; unguarded `put_config` is a low-level trusted Rust setup/testing API, not exposed as an unguarded IPC command. Only `schema_version:1`, `demo:true`, and `tax_mode:"demo_no_tax"` are currently accepted.

```json
{
  "schema_version":1,
  "demo":true,
  "tax_mode":"demo_no_tax",
  "currency":{"code":"USD","minor_unit_exponent":2},
  "groups":[{
    "id":"shared-demo-band",
    "name":"Demo mix & match",
    "tiers":[{"nominal_grams":"3.5","tolerance_grams":"0.15","price_minor":"3000"}],
    "overage_policy":"full_actual_at_tier_rate",
    "rounding":"half_up",
    "below_smallest":"proportional_at_smallest_tier",
    "tolerance_scope":"aggregate_pricing_group"
  }],
  "catalog":[{
    "archived":false,
    "sku":"bulk-a",
    "name":"Demo Bulk A",
    "kind":"measured",
    "declared_content_grams":null,
    "pricing":{"kind":"weight_group","group_id":"shared-demo-band"}
  }]
}
```

Supported overage policies: `full_actual_at_tier_rate`, `excess_beyond_tolerance_at_tier_rate`. Supported rounding: `floor`, `ceil`, `half_up`, `half_even`. Supported below-smallest behavior: `proportional_at_smallest_tier`, `reject`. Tolerance is explicitly aggregate-group only. A fixed prepack uses `{"kind":"fixed_unit","unit_price_minor":"3500"}` and must have positive `declared_content_grams`. A prepack joins a weight group only when its catalog pricing explicitly names that group.

## Opt-in development HTTP adapter

The adapter exists only under the `dev-server` Cargo feature. The desktop app does not need a listening server.

```sh
cargo run -p gram-register-service --features dev-server --bin register-dev-server -- \
  --db /tmp/gram-explicit-demo.sqlite --port 4174
```

It binds only `127.0.0.1`, requires an explicit database path, and accepts `POST /api/command` with `Content-Type: application/json`, JSON `{command,payload}`, and at most 1 MiB. Responses are `{ok:true,data}` or `{ok:false,error:{code,message}}`. GET `/health` is the only read endpoint. This is a development adapter, not a production network service.

Host headers must be `127.0.0.1:<backend port>` or `localhost:<backend port>`. Origin headers, when present, must name loopback dev frontend port 5173/4173 or the selected backend port. Other origins are rejected, not reflected in CORS responses. A local Vite `/api` proxy should use `changeOrigin:true`. Missing Origin is supported for the local proxy/CLI, while peer and Host are still checked. No wildcard CORS, remote bind option, credentials, or real payment API are provided.

## Local storage guarantees and limits

SQLite bundled with `rusqlite` provides transactional local storage with foreign keys, full synchronization, an application ID, and schema version 2 with an explicit migration from version 1. Unsupported future schemas and nonempty foreign files are rejected. Sale and receipt update/delete triggers protect immutable records through normal SQL operations; this is not tamper-resistant accounting against a user with arbitrary file access. Keep the database on a local filesystem, not a network share. Exports are unencrypted logical snapshots, not a synchronization protocol or tamper-resistant accounting archive. See [backup and recovery](../../docs/backup.md) for the versioned format, recovery tool, and restrictions. Older app versions must not reopen upgraded files.

Tests include fresh-file explicit setup, close/reopen, replay after policy edits, conflicting replay, concurrent connections, underpayment/malformed tender no writes, stale quote rejection, injected receipt-write rollback, immutable rows, invalid settings preserving prior settings, future-schema rejection, policy/core agreement, pooled declared-vs-measured weight, and integers beyond JavaScript's safe-number range.

```sh
cargo test -p gram-register-service --all-features
cargo clippy -p gram-register-service --all-features --all-targets -- -D warnings
```


## Cash ledger commands (v0.4)

All new cash activity requires an open session. Receipts remain unchanged; sale/session association is a separate immutable relation. Old schema-1 sales have no session and never contribute to a newly opened drawer.

- `get_cash_ledger {}` returns `{active_session, sessions, refunds, sale_sessions}`. Active session is null when closed. `sale_sessions` contains `{sale_id,session_id}` mappings.
- `open_cash_session {idempotency_key, opening_float_minor, expected_currency}` returns the created session. Persist this key and exact payload before sending; replay returns that same session, even after it closes.
- `close_cash_session {session_id, counted_minor, expected_movement_count}` returns a permanent closed summary. The movement count from the reviewed ledger guards against a racing sale/refund. A completed close can be replayed with the same count; it cannot be edited.
- `refund_cash_sale {idempotency_key, sale_id, reason, expected_session_id, expected_currency}` records the original full receipt total in the matching current session, with no catalog repricing or stock movement. A second refund for the same sale is rejected.
- `get_refund_by_key {idempotency_key}` returns `{refund: RefundReceipt|null}` for uncertain-response recovery.

`CashSession` contains `session_id`, `opened_at_unix`, `currency`, `opening_float_minor`, `sales_total_minor`, `refunds_total_minor`, `expected_cash_minor`, `movement_count`, nullable `closed_at_unix`, nullable `counted_minor`, and nullable `variance_minor`. IDs, counts, timestamps and money are exact strings. Expected cash and variance may be signed. Expected cash = opening float + sales − refunds; variance = counted − expected.

`RefundReceipt` contains `refund_id`, `refund_number`, `created_at_unix`, `sale_id`, `original_receipt_number`, `session_id`, `reason`, `amount_minor`, `currency`, and `payment_method: "cash"`. Refund amounts are nonnegative original totals, deducted once in cash reconciliation. Original sale receipts are never removed or rewritten.

Session currency and minor-unit precision are frozen at opening. Configuration changes to currency/precision while open, and refunds of receipts in a different currency/precision, are rejected. Refunds of old-session/legacy receipts affect only the currently open matching-currency session. Session close and all cash mutations use immediate SQLite transactions. The ledger is local logical consistency, not tamper resistance against arbitrary database access.

See [cash-session workflow](../../docs/cash-sessions.md) for operator and retry guidance.

New frontend cash-sale/refund requests bind `expected_session_id` and `expected_currency`; opening requests bind `expected_currency`. The optional JSON fields preserve deserialization and exact replay of old checkout requests, not a recommendation to omit guards in new clients. An uncompleted legacy pending checkout requires renewed review; completed legacy keys can still retrieve the original receipt.

## Read-only sales reports and CSV (v0.5)

- `sales_report {start_date, end_date}` returns a `SalesReport` below.
- `export_sales_report_csv {start_date, end_date}` returns `{filename,media_type,content}` with a server-generated `gram-pos-sales-report.csv` filename and `text/csv;charset=utf-8` media type. Native file saving uses the fixed export kind `sales_report_csv`; clients cannot supply CSV bytes, a filename, or a filesystem path through this report command.
- Both commands accept only two strings, strict Gregorian `YYYY-MM-DD` dates in years `0001`–`9999`. The start midnight is **inclusive** and the end midnight is **exclusive**, always in **UTC**, not the device's local timezone. To report February 29, 2024, use `{"start_date":"2024-02-29","end_date":"2024-03-01"}`. Impossible dates, invalid leap days, timestamps, equal/reversed dates, unknown fields and alternative timezone inputs fail with `INVALID_INPUT`.

```json
{
  "start_date": "2024-02-29",
  "end_date": "2024-03-01",
  "timezone": "UTC",
  "groups": [{
    "currency": {"code":"USD", "minor_unit_exponent":2},
    "gross_sales_minor": "7000",
    "refunds_minor": "3500",
    "net_sales_minor": "3500",
    "sale_count": "2",
    "refund_count": "1"
  }],
  "events": [{
    "kind": "refund",
    "event_id": "1",
    "sale_id": "1",
    "created_at_unix": "1709164800",
    "currency": {"code":"USD", "minor_unit_exponent":2},
    "total_minor": "3500",
    "session_id": "2",
    "reason": "Returned next day"
  }]
}
```

The example event is illustrative; a full report includes every event contributing to its groups. Groups are sorted by currency code and precision. Events are sorted by numeric Unix time, then sale before refund for equal times, then numeric event ID. `event_id` is scoped by `kind`: sale ID for a sale, refund ID for a refund. `reason` is null for sales; `session_id` is null for legacy sales. IDs, timestamps, amounts and both counts are exact decimal strings. A refund event's `total_minor` is its nonnegative saved payout. Group net is gross sales minus refunds and may be negative in a refund-only period.

Reports use one consistent, validated saved-ledger snapshot. A sale is selected by its saved sale time; a refund is independently selected by its own saved refund time, even when its original sale falls outside the date range. Legacy sales are included once without inventing sessions. Changed or archived catalog items do not reprice original events. Each currency **and minor-unit precision** is a separate group: there is no mixed-currency grand total or conversion. Opening float, cash counts, cash-session totals, tender and change are never revenue. An empty result has `groups:[]` and `events:[]`, including an unconfigured register. A malformed/incomplete ledger fails closed rather than silently omitting history.

CSV is UTF-8, with one rectangular 18-column schema: `record_type,start_date,end_date,timezone,currency_code,minor_unit_exponent,gross_sales_minor,refunds_minor,net_sales_minor,sale_count,refund_count,event_kind,event_id,sale_id,created_at_unix,total_minor,session_id,reason`. `summary` rows hold each currency/precision aggregate; `event` rows hold the matching immutable sale/refund details. Columns not applicable to a row are empty. Do not add summary rows and event rows together. An empty report produces only the header. All cells are RFC 4180 quoted, embedded quotes are doubled, and records end with CRLF. Unicode is preserved. Spreadsheet formula-leading refund text (`=`, `+`, `-`, `@`, including after whitespace) and leading tab/newline controls are prefixed with an apostrophe. Numeric cells contain only validated exact decimal representations, never formula expressions.

CSV preserves the full integer digits, but spreadsheet auto-detection can round long numbers. Import money, count, ID and Unix-time columns as **text** when exact large values must be preserved. JSON reports retain these values as strings. Reporting/exporting does not change the draft, catalog, receipt snapshots, idempotency keys, drawer state or versioned backup/recovery format. CSV is a report, not a recoverable backup.
