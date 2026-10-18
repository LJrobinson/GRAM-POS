# Cash sessions and refunds (training prototype)

These records model manually handled cash. GRAM does not move money, verify physical cash, process card refunds, return stock, or provide production accounting/fiscal certification.

## Start and sell

Open a cash session with the counted starting float. All amounts use the configured currency and precision. Each new sale is attached to the open session atomically with its original receipt and draft clearing. Expected cash is starting float + sale totals − full refund totals, not the sum of tendered amounts. Cash tender and change still appear separately on the sale receipt.

Only one session may be open. Currency/precision changes are rejected until it closes. A request that was prepared for a different session cannot silently land in a newly opened drawer. Pricing edits within the same currency remain allowed and existing stale-quote protections still apply.

## Refund a receipt

Find the original receipt in Sales, review its original total, enter a reason, and confirm the full cash refund. The refund records the original sale/receipt identity, exact total and currency, reason, time and current session. It does not recalculate from the current catalog, edit the original sale, or infer inventory movement. A second refund with another key is rejected; retrying the original completed request returns the same refund.

A sale from an earlier session can be refunded in the current session of the same currency and precision. The old session stays closed and unchanged. Receipts from before cash-session tracking are explicitly legacy/unassigned: they do not increase a new drawer's cash, but their full refund is a cash outflow in the currently open matching-currency session.

## Count and close

Review the expected cash, count physical cash, and enter the actual amount. Variance = counted − expected. Positive means over; negative means short. Closing snapshots the totals, count, variance and time permanently. If another connection posts a sale/refund after the review, the movement-count guard rejects the stale close so the totals can be reviewed again. An exact retry of a completed close returns the same summary; a changed count cannot revise a closed session.

Never use close/open as a way to edit history. There are no session-delete/reopen, partial-refund, pay-in/pay-out, or reconciliation-adjustment commands in this prototype.

## Interrupted requests

Keep the same request and key after a lost response. Do not issue another refund or new session merely because a response was interrupted. The frontend saves pending mutation data locally before sending it and supports reconciliation after reload. Backend replay checks run before current-session checks, so an original completed sale/refund can still be recovered after its session closes. Local browser pending-request storage is not itself a database backup.

## Upgrade and recovery

Schema migration adds cash-session/refund records and separate sale/session relationships without rewriting old receipt or checkout-request JSON. Legacy sales remain outside every drawer until an actual refund is explicitly recorded. Portable v2 backups include the ledger; v1 backups remain importable into a new database as legacy history. Keep the original database and backup; never replace a live file. See [backup instructions](backup.md).

Recorded timestamps reflect the local computer clock and are display metadata, not a trusted ordering source. A clock correction can make a later event show an earlier time. Session identities, immutable relationships, transaction serialization and closing counts determine ledger consistency; backups do not reject otherwise valid records just because the wall clock moved backwards.
