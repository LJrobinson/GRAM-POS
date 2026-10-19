# Local export and recovery (prototype)

This release is an independent, local-only **DEMO** register. Exports do not add
sync, inventory, fiscal reporting, taxes, or production/compliance guarantees.

## Export safely

Use **Export sales JSON** for a versioned JSON report containing every full receipt,
including its original catalog, pricing policies, quantities, allocation
explanations, tender and change. Money, weights, timestamps and identifiers stay
exact strings; no spreadsheet floating-point conversion occurs.

Use **Download portable backup** for a versioned logical snapshot of the whole register:
current configuration and revision, any draft, all sales and original checkout
requests/idempotency keys, immutable sale snapshots and receipts, cash sessions,
closing counts, refunds, sale/session links, and all sequence high-water marks. Backup is not a sales report. Keep both if needed.

Exports use one SQLite read transaction, so another process cannot mix settings
from one moment with sales from another. A backup never clears the draft or
changes the live store. Invalid or incomplete stored records fail closed rather
than silently disappearing. The export is a snapshot when requested, not a live
connection: sales completed afterward require another backup.

In the native app, the OS Save dialog chooses the destination. Cancel writes
nothing. Choose a **new filename** each time: existing files are never replaced,
even if the system dialog offers an overwrite prompt. A write failure may leave
an incomplete new file; do not use it. Choose a different filename and retry.
Browser development mode uses the browser's download UI instead, subject to its
own destination and filename rules. No backup is silently uploaded anywhere.

Files are not encrypted or signed and contain business records. Store copies on
protected storage separate from the register device, verify that files open,
and practice recovery into a spare file. Do not email or upload them casually.
This is logical consistency validation, not proof against deliberate tampering.

## Offline recovery into a new database

There is deliberately no one-click live restore, import, merge, or overwrite.
Close the register and any development server first. Keep the old database and
backup untouched. From the repository root with Rust installed:

```sh
cargo run -p gram-register-service --bin register-restore -- /path/to/gram-pos-backup.json /path/to/new-recovered.sqlite3
```

The destination **must not exist**, including an empty file or symlink. The tool
accepts only the supported backup format/schema, validates records and their
relationships, recreates schema and immutability triggers, inserts all data in
one in-memory transaction, and then writes the complete SQLite image exclusively
to the new path. Existing files are never opened for writing. All original
checkout request bytes and keys are preserved, so retrying the same request
reconciles to the original receipt instead of creating a duplicate. The saved
sequence prevents reuse of previous receipt numbers.

A failed validation creates no destination. A failed filesystem write may leave
an incomplete new destination and prints an error; never use that file. The tool
does not automatically switch the running app to the recovered database.

Validate the recovered copy independently before adopting it. For example, use
a development server with `--db /path/to/new-recovered.sqlite3`, inspect settings,
draft and sales totals, and compare the latest receipt with the source backup.
Stop that server afterward. Native database placement is described in
[desktop setup](../apps/register/src-tauri/README.md). Switching a native install
to the verified file is a deliberate offline maintenance operation: preserve the
old file separately, keep all app/server processes closed, and never replace a
live SQLite file or copy it while writes are occurring. Consult a technical
operator if uncertain. Do not delete an old database to troubleshoot a failure.

## Older portable format v1 (accepted for recovery)

Milestone-3 exports are UTF-8 JSON with `format` and numeric `format_version: 1`.
`gram-pos-sales` has a `receipts` array ordered by ascending sale ID.
`gram-pos-backup` additionally has numeric `schema_version: 1` and:

- `settings`: null or `{revision, config_json}`. Revision is an integer string;
  config_json retains the exact stored JSON text.
- `draft_json`: null or the exact stored draft JSON text (including incomplete
  quantities still being edited).
- `sales`: ascending records with integer-string `id`, `idempotency_key`,
  `request_json`, integer-string `created_at_unix`, `snapshot_json`, and
  `receipt_json`. The JSON-suffixed fields intentionally contain JSON strings,
  not parsed objects: preserving their bytes preserves checkout retry semantics.
- `sales_sequence`: nonnegative integer string, at least the maximum sale ID.

Unknown format/schema versions are rejected, not guessed. The current CLI
accepts v1/schema-1 backups and restores their sales as legacy history into the
current schema, without attributing them to a new drawer. A technical
reimplementation must preserve original raw JSON, IDs, keys, current revision,
sequence high water, constraints and immutability triggers; recomputing receipts
from today's catalog or re-running checkouts is **not** recovery.


## Portable format v2 (current export)

Milestone-4 through milestone-8 exports use numeric `format_version: 2`. The `gram-pos-sales` report
retains `receipts` and also includes the cash ledger, so gross original receipts
are never silently presented as net-of-refund sales. It is a report, not a
recoverable backup.

`gram-pos-backup` uses numeric `schema_version: 2`, retaining all v1 fields and
adding:

- `cash_sessions`: opening rows `{id,idempotency_key,request_json,session_json}`.
- `cash_closes`: immutable `{session_id,request_json,session_json}` rows.
- `sale_sessions`: `{sale_id,session_id}` associations for tracked sales.
- `refunds`: `{id,sale_id,session_id,idempotency_key,request_json,receipt_json}`.
- `cash_sessions_sequence` and `refunds_sequence`: exact nonnegative strings.
- `legacy_sale_max_id`: the migration boundary separating pre-session sales
  from new sales that must have a session association.

Raw JSON fields preserve their original bytes, including request identities.
Recovery validates cross-record identities, currencies/precision, exact refund
amounts against original receipts, session totals and close counts, duplicate or
missing relationships, and sequence bounds before creating the destination.
Session/refund records and their immutable constraints are restored together.
No inventory return or payment-processor action occurs during recovery.

Schema-1 database migration is an explicit transaction and does not rewrite old
receipt or request bytes. Old sales stay unassigned, not cash movements in the
next opened drawer. Do not open a schema-2 database with an older app; retain a
pre-upgrade backup separately. New backups include the whole ledger and cannot
be imported by milestone-3 binaries.

## Version 0.6.0 quantity compatibility

Unit-product receipts preserve the `unit` quantity tag and exact count with no invented gram content. Optional catalog barcodes are preserved in configuration snapshots and backups. Older configurations without barcodes and existing measured/prepacked receipts remain valid; this feature does not change SQLite schema 2 or the supported v1/v2 portable formats. New unit records require 0.6.0 or newer, so do not downgrade an active store. Original request and receipt JSON bytes are retained, not regenerated from the new catalog.


## Version 0.7.0 catalog CSV compatibility

Catalog CSV is not a backup and cannot restore sales or pricing schedules. Import updates only catalog configuration under a revision check; existing receipt snapshots and raw history stay unchanged. SQLite schema 2 and supported portable v1/v2 formats are unchanged, with no migration. Existing backups remain valid. Newly permitted embedded CR/LF product names need 0.7.0 validation, so do not downgrade a store using those names. See [catalog CSV](catalog-csv.md).


## Durable carts and backup v3 (0.9.0)

Recognized schema-1/2 databases migrate transactionally to schema 3 on open. Existing sale/request/receipt bytes are unchanged. Current portable backups use format/schema version 3 and include the active draft identity, global draft revision, parked carts and persisted mutation keys in the same consistent snapshot as configuration and the immutable cash ledger. Successful checkout retires the active identity atomically with the receipt, so recovering a parked cart cannot duplicate its sale through a second checkout key.

The restore tool continues to accept valid v1 and v2 backups, including pre-settings golden backups. It does not reinterpret original measured, package or unit quantities against today's catalog. Restored draft lines may be unavailable or incompatible with current products; preview exposes these issues before ordinary checkout. Backups include retained completed/discarded draft lifecycle and operation records; discard is not secure erasure. Backups are unencrypted and now also contain optional cart labels, so labels should contain no customer or other personal information.

Parked carts are deliberately excluded from the separate sales JSON/report exports. Those files are not a substitute for a full backup and cannot restore unfinished carts. Back up before upgrading and do not open a schema-3 store with an older application. Restore into a new file only; do not replace the active database while the register is running. See [parked-cart recovery](parked-carts.md).
