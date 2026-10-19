# Cash sessions, refunds and paid-in/out (training prototype)

These records model manually handled cash. GRAM does not move money, open physical cash drawers, verify physical cash, process card refunds, return stock, or provide production accounting/fiscal certification. Every example below is synthetic training data.

## Start and sell

Open a cash session with the counted starting float. All amounts use the configured currency and precision. Each new sale is attached to the open session atomically with its original receipt and draft clearing. Expected drawer cash is:

**opening float + cash sale totals − full cash refund totals + paid-in totals − paid-out totals**

Cash sales use the price actually due, not the full amount tendered; change is already excluded. Tender and change still appear separately on the original sale receipt. Paid-ins and paid-outs change drawer cash, not revenue. For example, a 50.00 float, 12.00 sale, 12.00 refund, 20.00 paid-in and 15.00 paid-out leave 55.00 expected cash and zero net sales revenue.

Only one session may be open. Currency/precision changes are rejected until it closes. A request prepared for a different session cannot silently land in a newly opened drawer. Pricing edits within the same currency remain allowed and existing stale-quote protections still apply.

## Record cash added or removed (0.10.0)

1. Open **Cash sessions** and choose **Paid in** for cash added or **Paid out** for cash removed.
2. Enter the exact positive amount and a short reason. Reasons are required, bounded to 500 UTF-8 bytes and cannot contain control characters. Use a nonpersonal description such as “Extra change float”; do not enter customer, staff or payment-account details.
3. Review the direction, amount, currency, session and reason. Confirm only when the record matches the manually handled cash. **Cancel** before confirmation creates nothing.
4. Review the new permanent movement in cash history. Its own identity, recorded time, reason, kind, amount and session distinguish it from a sale or refund.

Zero, negative, excess-precision and overflowing amounts are rejected. Currency and minor-unit precision must exactly match the open drawer; no conversion or rounding is guessed. No receipt, tax, stock, sale or refund is fabricated. The existing sales reports continue to derive revenue solely from original sales and refunds. A separate cash-movement section in sales JSON is labeled as drawer activity, not revenue.

### Conservative paid-out limit

This prototype rejects a paid-out larger than the current **expected** drawer balance, including changes committed by other windows before confirmation. It does not claim to know the physical cash available and does not permit an override or a negative paid-out balance. If the drawer is already negative because of a legitimate refund, a new paid-out is blocked. This check does not change existing full-refund behavior. Investigate the discrepancy rather than entering a fictitious sale or paid-in to bypass it. The limit is a training safety policy, not a financial, accounting or cash-handling recommendation.

There is no edit/delete/reversal command for a committed cash movement. A genuinely later addition or removal is a separate reviewed record with its own reason; never alter original history to make a drawer reconcile.

## Refund a receipt

Find the original receipt in Sales, review its original total, enter a reason, and confirm the full cash refund. The refund records the original sale/receipt identity, exact total and currency, reason, time and current session. It does not recalculate from the current catalog, edit the original sale, or infer inventory movement. A second refund with another key is rejected; retrying the original completed request returns the same refund.

A sale from an earlier session can be refunded in the current session of the same currency and precision. The old session stays closed and unchanged. Receipts from before cash-session tracking are explicitly legacy/unassigned: they do not increase a new drawer's cash, but their full refund is a cash outflow in the currently open matching-currency session.

## Count and close

Review every component of expected cash, count physical cash, and enter the actual amount. Variance = counted − expected. Positive means over; negative means short. Closing snapshots the sales, refunds, paid-ins, paid-outs, expected cash, count, variance and time permanently. If another connection posts any cash event after review, the movement-count guard rejects the stale close so the totals can be refreshed and reviewed again. A paid-in and equal paid-out still change that guard even though their net amount is zero.

An exact retry of a completed close returns the same summary. A changed count cannot revise a closed session. Newly submitted movements cannot mutate a closed session, but retrying a previously completed movement returns that original record without adding another. Never use close/open to edit history. There are no session-delete/reopen, partial-refund or automatic reconciliation-adjustment commands.

## Interrupted requests and other windows

Keep the same saved request and key after a lost response. Do not record a new paid-in/out, refund or session merely because a response was interrupted. The UI stores pending mutation data locally before sending it and supports reconciliation after reload. Backend replay checks precede current-session checks, so a completed action can still be recovered after its session closes. Reusing the same key for different details is a conflict, not an edit.

Same-origin windows coordinate their pending writes using the register's existing Web Lock. A cash action cannot replace another window's unresolved journal. Local storage errors or invalid recovery data block sending rather than losing the request identity. Do not manually clear browser storage while an outcome is uncertain. Browser pending storage is recovery context, not a database backup; SQLite remains authoritative across processes.

## Upgrade, recovery and time

Version 0.10.0 migrates recognized schema-1/2/3 databases to schema 4 without rewriting historical receipt, request or closed-session JSON. Older sessions have zero paid-in/out totals. Portable v4 backups include immutable movements, recovery keys, sequences and source-event watermarks together with existing parked-cart state; v1/v2/v3 backups remain recoverable into a new database. Keep the original database and backup, never restore over a live file, and do not reopen an upgraded store with an older app. See [backup instructions](backup.md).

Ledger times are displayed in the configured store timezone, which is shown alongside the records. Recorded Unix timestamps reflect the local computer clock and are display metadata, not a trusted ordering source. A clock correction can make a later event show an earlier time. Session identities, immutable relationships, transaction serialization, source-event watermarks and closing counts determine ledger consistency. Valid backups are not rejected just because the wall clock moved backwards.
