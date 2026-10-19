# Store settings and local-timezone reporting

Version 0.8.0 remains a local demo/training prototype, not a production POS, tax engine, fiscal receipt system or payment processor.

## Configure a receipt identity

Open **Configuration → Store settings**. Enter a display name, optional receipt header/contact/footer and an IANA timezone such as `America/Los_Angeles`, `Europe/London` or `UTC`. These are single-line plain-text fields, not HTML, logos or tax/legal identifiers. Limits count Unicode scalar values: name 120, header 500, contact 240, footer 500. The display name cannot be blank. Control characters, line/paragraph separators and directional override/isolate characters are rejected. Text is escaped rather than interpreted as markup.

Choose **Preview receipt** before **Save store settings**. Editing after preview requires a new preview. **Cancel changes** discards only unsaved settings. Saving compares the loaded configuration revision within the same SQLite transaction as the write. A stale editor cannot overwrite another window's changes. If a response is lost or saving fails, keep the draft and reload the saved configuration before deciding what to retry; a repeated old revision cannot silently overwrite new settings.

The dedicated settings operation cannot change currency, pricing rules, catalog, cart or cash records. It increments the configuration revision, so checkout must refresh its quote. The UI conservatively disables configuration saves while the cart is nonempty. The demo/training warning is always present and cannot be customized away.

## Immutable receipts and backups

Each new sale captures the complete saved configuration, including explicit store settings, in its immutable receipt. Later branding/timezone edits, refunds, restarts and backup export/restore cannot replace that identity. The receipt's timestamp is formatted using its saved timezone with an offset label; device timezone is not selected implicitly. Where a browser lacks that timezone, the receipt clearly falls back to UTC instead of inventing an offset.

Legacy configurations and receipts have no `store_settings` field. No read, print, export or restore inserts defaults into those historical snapshots. Old receipt printing retains the original **GRAM** identity and UTC time. Before the first explicit settings save, new sales also keep the legacy absence. The settings editor offers **GRAM POS**, empty optional text and **UTC** as its proposed defaults; viewing them is not a write.

SQLite stays schema version 2, configuration schema version 1 and backup format version 2 (with legacy version-1 recovery retained). Settings are part of existing configuration snapshots, not a new table or migration. The original logical backup fields and embedded receipt JSON remain recoverable. Files remain unencrypted. Use the [offline recovery procedure](backup.md), not sales CSV, to restore to a new database.

## Select local business days

Reports initially offer the saved store timezone, or UTC when absent. Select another **Report timezone** to inspect the same immutable records under another calendar without saving settings. Dates use strict Gregorian `YYYY-MM-DD` years 0001–9999. Start midnight is inclusive; end midnight is exclusive. To include one calendar day, choose the following date as the end.

The Rust service uses `chrono` and `chrono-tz` with bundled IANA data, never a browser/device timezone or a fixed UTC offset. For `America/Los_Angeles`, 2026-03-08 through 2026-03-09 is 23 hours and 2026-11-01 through 2026-11-02 is 25 hours. Historical second-level offsets are preserved. The resolved UTC seconds and zone appear in report JSON/UI and CSV, and each event has its own numeric offset.

A local midnight can be ambiguous or nonexistent, including a skipped date such as `Pacific/Apia` 2011-12-30. Either case is rejected with an explicit error; the operator must choose a different boundary. There is no silent forward shift, choice of the earlier/later offset or substitution of a 24-hour interval. Unknown zone names and malformed dates are rejected. Browser validation is only a convenience; Rust is authoritative.

Gross sales use the original sale event's local date. Refunds use the refund event's own local date, independently of the original sale. Net remains gross minus refunds; a refund-only day can be negative. Money and stored Unix timestamps never change when settings or report timezone changes.

## Consistency and CSV

A loaded report records the configuration revision from the same consistent ledger snapshot. The UI discards late results after date/zone edits or context changes. Exports freeze the loaded range/zone/revision and ask Rust to reject stale settings, even when another process changed them. A refreshed export can include newly recorded events if configuration is unchanged; it is not a frozen copy of the earlier on-screen activity.

Native saving generates bytes before showing its destination picker; a settings change while the picker is open cannot rewrite that already-generated, labeled snapshot. Canceling saves nothing and existing destination files are never overwritten. Browser download responses are discarded when their selection is no longer current.

CSV has 23 named columns. It includes zone, UTC bounds and configuration revision on summary/event rows, with local timestamps and numeric offset seconds on event rows. Empty explicit-timezone/revision requests include a `metadata` row with no invented totals; old date-only empty requests remain header-only. Import exact integer values as text in spreadsheets. See the [service API](../crates/register-service/API.md) for the complete schema.

## Dependency boundary

Timezone dependencies are restricted to the register service. The portable pricing core remains dependency-free with Rust 1.70 as its declared minimum. `chrono` 0.4.45 and `chrono-tz` 0.10.4 are locked; their documented minimum compilers are 1.61 and 1.65 respectively. Future IANA updates require a reviewed dependency/lockfile update and rerunning boundary tests. Receipt JSON and stored event instants are not migrated when timezone data changes.
