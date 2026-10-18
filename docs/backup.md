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
requests/idempotency keys, immutable sale snapshots and receipts, and the sales
sequence high-water mark. Backup is not a sales report. Keep both if needed.

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

## Portable format v1

Both exports are UTF-8 JSON with `format` and numeric `format_version: 1`.
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

Unknown format/schema versions are rejected, not guessed or automatically
migrated. The included CLI is the supported v1 recovery mechanism. A technical
reimplementation must preserve original raw JSON, IDs, keys, current revision,
sequence high water, constraints and immutability triggers; recomputing receipts
from today's catalog or re-running checkouts is **not** recovery.
