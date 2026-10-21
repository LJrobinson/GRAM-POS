# Printable receipts and sales reports

Version 0.5.0 introduced read-only reporting and printing; 0.8.0 adds saved receipt identity and explicit IANA report timezones. It adds no tax engine, fiscal certification, inventory adjustments, live payments, or synchronization.

## Print an original receipt

Open a saved sale from Sales history or a report event, then choose **Print receipt**. Only that receipt is included in the print layout; the register, navigation, other dialogs and action buttons are excluded. Browser and native WebView printing use the user-triggered system print dialog through `window.print()`. Choose your printer or a supported PDF destination there. Canceling printing does not change the sale, cash session, or refund records. No silent printing or printer-driver integration is implemented.

Every product name, line quantity, declared package weight, pricing-group calculation, amount, currency, tender and change comes from the immutable saved receipt, not today's catalog. Renaming, repricing or archiving products does not rewrite receipts. A separate screen-only current-status section identifies an associated full refund and reference when known; it is excluded from the original receipt printout and does not change the original sale total. If refund status cannot be loaded, it is marked unavailable rather than assumed unrefunded. Receipts remain clearly marked demo/training, with no configured tax.

Printing follows the [official Tauri printing guidance](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html#method.print). Browser PDF/CSS validation is not physical-printer or native-device certification. Windows WebView2/system-dialog and physical-printer behavior require device testing; receipt paper sizes and printer margins remain user-selected.

## Reports: selected-zone event dates, not drawer balances

Open **Reports** and choose a timezone and date range. The start date at local midnight is included; end midnight is excluded. To report October 18, select start `2026-10-18` and end `2026-10-19`. Rust resolves both boundaries using IANA rules and returns UTC seconds, including DST and historical offsets. UTC remains the default for old date-only API calls. Dates use strict Gregorian `YYYY-MM-DD` syntax, including leap-year validation. Ambiguous/nonexistent midnights are rejected, not silently shifted. See [store settings](store-settings.md).

- Gross sales include saved sales whose original sale event timestamp falls in the range.
- Refunds include saved refund events whose own timestamp falls in the range, even when the original sale is outside the range.
- Net sales equal gross sales minus refunds. A refund-only range can have a negative net.
- Opening float, cash received, change, counted cash and drawer variances are not revenue.
- Historical sales without a cash session are included once as legacy sales. Session summaries are never added to receipt totals.
- Each currency code and precision is a separate group. There is no cross-currency grand total, exchange rate or precision conversion.

Reports use one consistent, validated snapshot of stored records and retain exact integer minor-unit amounts. They do not rewrite source records. Device clock errors can affect the recorded event date; a report does not reconstruct a corrected business date. Refresh to include activity recorded since the last report load.

## CSV is a report, not a backup

**Export report CSV** creates a fresh server-generated snapshot for the loaded range, timezone and configuration revision; changed settings reject the export. It may include activity saved since the on-screen report was generated. The fixed `gram-pos-sales-report.csv` UTF-8 file contains a header, separate `summary` rows per currency/precision, and `event` rows for sales and refunds. Empty explicit-zone/revision ranges include one metadata row identifying the zone and UTC bounds; legacy date-only requests remain header-only. Never add summary and event rows together. Gross/refund/net columns belong to summary rows; event amount and kind belong to event rows. Amount columns are integer minor units, with `minor_unit_exponent` indicating the decimal precision.

Fields are quoted and embedded quotes/newlines escaped. Potential spreadsheet formula prefixes in user-authored refund reasons are neutralized with an apostrophe. This defense applies to CSV only; saved reasons are unchanged. CSV retains full exact digits, but spreadsheets may auto-round long numbers or reinterpret dates. Import identifiers and exact minor-unit columns as **text** when preserving large values matters. Unicode is preserved.

Native exports use the OS destination picker and refuse to replace an existing file, even when the dialog offers overwrite. Canceling the picker saves nothing. Browser exports use the browser download workflow. Files are unencrypted and should be kept private.

CSV and sales JSON reports cannot restore your register. Continue using the portable backup and the unchanged [offline recovery procedure](backup.md), which restores to a new database file and never overwrites the live store.


## Find a saved receipt (0.11.0)

Sales history searches every sale returned by the local store, not just a recent page. Enter a whole or partial receipt number or sale ID. Matching is case-insensitive, trims surrounding whitespace, and treats punctuation literally, not as a pattern. Identifiers are never converted to numbers; leading zeroes and long IDs remain exact. Product names, prices and dates are not searched.

The matching count distinguishes no matches from an empty store. Clear search or Show all saved sales restores the complete list. Refresh keeps the typed query and reloads the local record. Leaving Sales history or restarting the app resets this temporary search; it does not alter a saved sale or draft.

Each row shows the sale ID and the timestamp in the current configured store timezone, including on phone-sized layouts. Times include the year and UTC offset where applicable, so repeated daylight-saving hours can be distinguished. A browser unable to render that timezone visibly falls back to UTC. Original receipt previews and printouts continue to use the immutable receipt snapshot, including its saved timezone, not the current history display setting.

Opening a result requests that row's exact sale ID. Changing the query, refreshing, or leaving history cancels publication of any pending receipt response; it cannot open over a newer view. Refund availability still comes from the cash ledger and selected original receipt. Searching does not write to the ledger, change refund state, reserve stock, or modify receipts.

All summaries are loaded in memory by the existing service API; this increment adds no server pagination or performance guarantee for very large histories.
