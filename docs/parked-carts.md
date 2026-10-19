# Parked carts

GRAM 0.9.0 can keep several unfinished carts in its local SQLite database. This is a bounded cash-register workflow, not an order-management, inventory, customer-account or payment system. The register remains a training prototype with no configured tax/compliance workflow.

## Operator workflow

1. Add products as usual. **Park cart** is available only for a nonempty cart.
2. Optionally enter a short cart label (up to 80 Unicode characters), such as “Counter 2”. Do not put customer names, phone numbers, health information or other personal details in labels. Review and choose **Park this cart**. Closing the dialog without confirmation leaves the current cart unchanged.
3. The saved cart appears under **Parked carts**, and the register is ready for another cart. Parking makes no sale, cash movement or stock reservation.
4. Preview a parked cart before resuming it. Original quantities are shown alongside current product information, current prices and any warnings. A saved amount is never a guaranteed held price.
5. Resume only into an empty current cart. If another cart is active, park it first or deliberately clear it. No automatic replacement or merging is performed.
6. Resolve any unavailable/type-changed lines, review the current quote, and complete the normal cash checkout. An open cash session and the usual quote/session checks remain required. Successfully completing checkout consumes that draft identity in the same transaction as the sale and receipt.
7. To abandon a parked cart, choose its discard action and explicitly confirm. Cancelling the confirmation leaves it saved. Discard removes the cart from resumable work, not from retained recovery records or existing backups.

## Quantities and catalog changes

The original quantity tag and text are retained: measured grams (up to milligram precision), whole package count or whole unit count. For example, `12.345 g` cannot become 12 units, and a unit count cannot acquire a package weight. Nothing is silently rounded, retyped or dropped on resume.

Preview reads the current catalog and configuration. Missing or archived products, changed product types and invalid quantities produce line-specific warnings. Compatible product or price changes are visible too. A complete current quote appears only when all lines can be priced; a partial total is not presented as a full-cart total. Resume retains problematic lines for deliberate correction/removal rather than pretending they disappeared. A configuration revision check prevents resuming against a preview that became stale.

## Persistence and recovery

The active cart and parked carts are local to this register database. They survive application restart and are included in portable backups. They are not synced to other devices and do not expire automatically. Completed/discarded identities and operation records are retained to prevent replay; discard is not a secure-erasure feature. Browser development mode uses the Rust development service's database, not a separate browser-only cart store.

Guarded draft mutations use a global draft revision and a unique operation key. The UI stores a pending request before sending it and locks conflicting actions until it can reconcile the result. If a response is lost, retry the saved action rather than entering a replacement cart or guessing whether it succeeded. Retrying an identical key cannot repeat the transition; the server returns its current authoritative state. Reusing a key for different content is rejected.

A delayed scanner lookup cannot add a product after entering the parked-cart workflow. Pending autosaves are serialized and flushed before parking or checkout. A stale client cannot silently overwrite a more recent active cart. Checkout still uses its separate persistent payment-attempt key; a lost checkout response must be recovered before any draft edits.

The UI coordinates recovery-key writes between same-origin windows using the browser’s Web Locks API and refuses to replace another pending request. It requires a compatible browser/WebView secure context; if that capability is unavailable, cart writes fail closed instead of risking recovery-data loss. Initial read-only loading does not require this lock; after a blocked write the recovery dialog keeps the local lines available rather than silently abandoning them. Web Locks is a secure-context feature; see [Mozilla’s API reference](https://developer.mozilla.org/en-US/docs/Web/API/Navigator/locks). SQLite revision checks remain the authority across separate processes.

If another window changes the register, reload/review the authoritative cart after a conflict. Do not manually clear browser storage while an operation is uncertain: the pending request is how the UI identifies and reconciles it. Clearing browser storage does not erase the SQLite carts, but it can remove useful recovery context.

## Backup and reporting boundaries

Schema 3 and portable backup v3 retain active/parked identities and draft operation history consistently with receipts and ledger records. The offline recovery tool also accepts older v1/v2 backups. Original historical request, receipt and snapshot JSON are preserved. Restore always creates a new database; stop the app before switching databases and keep the source backup. See [backup and recovery](backup.md).

Parked carts are not completed sales, receivables or revenue. They do not appear as sales in sales history, sales JSON, sales reports, refund totals or expected drawer cash. Ordinary checkout creates the sale exactly once; parking itself does not collect money, reserve stock or hold catalog prices.

## Validation boundary

Automated service tests exercise restart, revision conflicts, idempotent retry, preview changes, atomic checkout consumption and backup migration/restore. Browser tests drive the real Rust service and include desktop/phone screenshot scenarios. Native dispatcher tests cover the existing IPC route without expanding capabilities. A passing browser suite does not certify a physical scanner, scale, cash drawer, printer, native file picker or production suitability.
