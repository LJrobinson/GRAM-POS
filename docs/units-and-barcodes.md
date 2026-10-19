# Unit products and keyboard barcode lookup

Version 0.6.0 adds count-based merchandise without inventing a weight. This remains a local training prototype, not a stock system or production/fiscal certification.

## Three distinct quantities

- **Measured** records the entered actual gram weight and uses an explicit weight-pricing group.
- **Prepacked** records a positive integer package count and merchant-declared gram content per package. Existing fixed-package and opted-in weight-group policies are unchanged.
- **Unit / each** records a positive integer item count and a fixed price per item. It has no declared content and cannot contribute to a weight-pricing group. Totals use checked integer money arithmetic; invalid counts and overflow fail rather than rounding or wrapping.

Saved receipt snapshots retain the original quantity kind, count, product details and price. Later catalog edits do not rewrite sales or full-sale refund amounts. Reports continue to aggregate recorded money, never inferred stock movements.

## One optional barcode per product

Configure the barcode in the product editor. It is a string, not a number: `00123` differs from `123`, and letter case is significant. No fuzzy search, numeric conversion or automatic trimming is performed. Empty configured codes, surrounding whitespace, control characters and codes longer than 128 UTF-8 bytes are rejected. Leave the optional field unset to remove a code.

Codes are unique across the whole catalog, including archived products. Archiving does not silently free a code for reassignment; explicitly remove or change it before assigning it elsewhere. This prevents an old label from ambiguously selecting another item. Lookup only sells active products and reports archived/unknown codes explicitly.

## Focused keyboard workflow

Click the dedicated barcode field before scanning. A keyboard-style HID scanner must enter the code as text and finish with Enter, just as a user can type and press Enter. No global keyboard hook, camera or device API is used. Scanning into another field behaves like ordinary typing there; GRAM does not intercept it.

A successful unit/prepack scan adds one item/package. A measured-product scan opens the normal measured-weight prompt rather than inventing a weight. Deliberate repeated scans are separate actions. Pending lookups are serialized, and canceled or superseded contexts must not append stale items after navigation. Lookup itself is read-only; it never completes a sale or changes stock. Recheck the cart before checkout. Cash checkout keeps its existing idempotent retry workflow.

## Compatibility and boundaries

Missing barcode fields in older configurations mean no barcode. Existing measured/prepacked quantity tags and saved receipt/request bytes are preserved. The SQLite schema and supported portable-backup versions remain unchanged; supported older stores still follow the existing explicit migration path. New `unit` quantity records require version 0.6.0 or newer. Back up before upgrading and do not reopen an upgraded store or restore its new records with an older application.

A keyboard scanner is not certified by an automated browser test. Real hardware timing, keyboard layout, scanner suffix configuration and native-device behavior still require operator testing. No camera scanning, barcode printing, multi-code aliases, scale/printer drivers, stock counts, device syncing, taxes or live payments are added.
