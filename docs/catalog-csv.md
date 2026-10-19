# Local catalog CSV (0.7.0)

Catalog CSV updates products, not sales, inventory, pricing schedules, or taxes. It is **not a backup**. Use a portable JSON backup before a large edit. Files stay on this device: the browser preview sends selected UTF-8 text only to the loopback Rust service; the packaged app uses local IPC. No spreadsheet cell is executed.

## Operator workflow

1. Save or discard unfinished configuration edits. Export the saved catalog, or download the blank template for a new catalog.
2. Edit a copy. Keep the header and format marker, retain each product's SKU, and use existing pricing-group IDs. The template contains headers only, not automatically imported demo products.
3. Select the CSV. Selection does not import anything. Choose **Preview CSV**, then review create/update/unchanged rows and any row-level errors.
4. Fix all errors in the source and select/preview it again. Cancel discards the staged import without changing saved data.
5. Explicitly choose **Apply catalog changes** only after review. The entire payload is rechecked against the same configuration revision in one write transaction. Either every valid change is saved together or nothing is saved.

The screen invalidates its preview when the selected file or saved configuration changes. Unsaved configuration edits must not be overwritten. Another window's edits cause a stale-revision rejection at Apply; refresh and preview again rather than overwrite those changes. A preview is not a reservation.

### Lost responses and retries

A disconnected response does not prove Apply failed. Refresh the saved catalog and preview the exact same file again before applying. SKU is required and identifies every create, so re-previewing a previously committed file produces updates/unchanged rows, never another generated product identity. A retry with the old revision is rejected after a successful change. An unchanged import does not advance revision. There is no automatic retry, generated-ID allocation, or background import.

## Versioned, reversible spreadsheet text

The exact header, in this order, is:

```csv
format,sku,name,barcode,kind,archived,declared_content_grams,pricing_kind,group_id,unit_price_minor
```

Every data record has `gram_catalog_v1` in `format`. Nonempty `sku`, `name`, `barcode`, and `group_id` cells **must start with `text:`**. Exactly one prefix is removed on import. Export always adds exactly one prefix. This explicit encoding makes text cells inert in spreadsheets, including values beginning `=`, `+`, `-`, `@`, apostrophes, numeric-looking codes, and strings that already start `text:`. It is not an Excel formula or a lossy apostrophe convention.

| Original value | CSV cell contents before CSV quoting |
| --- | --- |
| `00123456789012345678` | `text:00123456789012345678` |
| `=1+1` | `text:=1+1` |
| `'Special` | `text:'Special` |
| `text:Tea` | `text:text:Tea` |
| Empty optional barcode/group | Empty cell |

Required SKU/name cannot be empty. For a literal barcode beginning `text:`, write two prefixes. Never strip prefixes from an exported spreadsheet before reimporting it. CSV quoting protects commas, double quotes, and embedded line breaks; it does **not** itself prevent spreadsheet formulas or numeric coercion. UTF-8, including Unicode names, is preserved. Product names may contain embedded CR/LF; identifiers may not. Other controls and surrounding whitespace are rejected.

Spreadsheets can round long numeric cells, remove leading zeros, convert dates, and normalize line endings. Use an explicit UTF-8 import and set columns to **Text**; keep the `text:` prefixes. Prices and weights are exact strings to GRAM, but a spreadsheet may already have rounded them before saving. Review them against the original export. Do not rely on CSV double quotes alone to protect a barcode. Byte-for-byte reimport guarantees apply to the GRAM export itself, not arbitrary spreadsheet rewrites.

### Example unit row

```csv
format,sku,name,barcode,kind,archived,declared_content_grams,pricing_kind,group_id,unit_price_minor
gram_catalog_v1,text:UNIT-001,"text:Tea, café",text:000123,unit,false,,fixed_unit,,425
```

This represents a unit product priced at 425 currency minor units, not a quantity of 425 and not 425 major currency units. The saved register's currency and precision are never changed by import.

## Fields and identity rules

- `sku`: stable, exact, case-sensitive product ID. Matching SKU updates that product; a new SKU creates a product. No matching by name, position, barcode, case folding, or trimmed value. To change a SKU is to create another identity; archive the old product explicitly if appropriate. There is no separate generated ID column.
- `name`: required display name. Unicode, quotes, commas and embedded CR/LF survive direct export/reimport. SKU/name/barcode/group IDs remain bounded by catalog validation (128 UTF-8 bytes).
- `barcode`: optional; empty clears it. Nonempty codes must be unique across **active and archived** products and the resulting batch. Leading zeros are significant. A barcode does not select the update target.
- `kind`: exactly `unit`, `prepacked`, or `measured`.
- `archived`: exactly `true` or `false`; both must be explicit. Omitted products are never deleted or archived.
- `declared_content_grams`: positive decimal grams, at most three decimal places, required only for `prepacked`. No exponent notation, signs, floating-point arithmetic, or guessed weight. Empty for `unit` and `measured`.
- `pricing_kind`: `fixed_unit` for units; `weight_group` for measured goods; either for prepacked goods.
- `group_id`: required only for `weight_group`, references an already-saved exact group ID. No group is created or changed, and no price tiers, tolerance, rounding, or overage policies are inferred.
- `unit_price_minor`: nonnegative whole currency minor units, required only for `fixed_unit`. No decimal currency, signs, exponent notation or precision loss. Empty for group pricing. Integer overflow is rejected.

No quantity/count, inventory, tax, or schedule columns are accepted. Sales counts continue to use the existing exact whole-item/whole-package parser, not imported stock levels. Every row supplies the full product state; blank optional fields clear that field, not “leave unchanged.” Duplicate SKUs are errors, not last-row-wins. Duplicate barcode conflicts include retained products absent from the file.

## Bounds and errors

Files are limited to **1 MiB (1,048,576 UTF-8 bytes)** and **1,000 product records**; the resulting catalog must also contain at most 1,000 products. Exact headers, column counts, quoting, record format, types, required and mutually exclusive fields are validated. Malformed CSV, unknown/duplicate headers, invalid UTF-8, unsupported controls, missing groups, duplicate identities and overflow cannot be applied. A UTF-8 BOM is supported at the file start.

Preview reports logical data-row numbers beginning at 1 and physical source-line numbers beginning at 1; the header/global errors use row 0. Quoted multiline values do not cause later errors to point to the wrong physical line. All issues must be resolved before Apply. A failed preview never changes configuration, draft, receipts, cash sessions, or backups.

## Compatibility and native boundary

SQLite schema 2 and portable backup formats v1/v2 are unchanged. Existing receipts retain their original catalog/pricing snapshot and bytes. Import changes only saved catalog configuration and its revision; it never rewrites history. Older backups remain valid. Newly permitted multiline names require 0.7.0 for validation; do not downgrade a store containing them.

The native `select_catalog_csv` command has no path argument. Its OS picker selects one regular local `.csv` file, which is read with a byte cap and strict UTF-8 decoding, without a general filesystem permission. The final path component is opened with no-follow/nonblocking flags on Unix; Windows opens and rejects reparse points before reading. The opened handle is checked to reject non-regular files. Selection returns text only and never applies it. Native export uses service-generated bytes and the existing Save dialog; it refuses to replace an existing file. No shell, network plugin, or broad filesystem grant is introduced.

The development HTTP adapter remains loopback-only. Its bounded JSON envelope allows escaping overhead while the CSV payload itself retains the 1 MiB limit. This adapter is not a production server.

## Verification evidence

Service tests exercise preview immutability, atomic rejection, revision races, retry behavior, exact codes, reversible injection-safe export, archived barcode conflicts, malformed rows, and receipt preservation. Browser tests exercise the actual Rust service and capture import review/error/mobile screens in CI. Newly added screenshot references should be published only after the real browser job produces and verifies those files; existing README screenshots are from earlier milestones, not fabricated CSV workflow captures. Native file-dialog behavior requires native-platform validation; service/browser tests do not certify Windows dialogs or hardware.
