# GRAM POS desktop host

GRAM POS is a local development register, not a production payment or
compliance product. Windows is the first packaged target. There is no shipped
iPad application, sync, card processing, inventory ledger, or fiscal integration.

## Develop on Windows

Install the official [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/):
current stable Rust (MSVC), Microsoft C++ Build Tools, WebView2, and Node.js 22.
From `apps/register`:

```sh
npm ci
npm run desktop:dev
```

The Tauri CLI starts Vite on `127.0.0.1:5173`, then opens the native register.
This development server is not used by the installed app. The installed app
embeds its frontend and talks directly to Rust through bounded IPC commands; it does
not start an HTTP server. Browser preview alone is not the native application.

## Build the unsigned Windows prototype

On Windows, from `apps/register`:

```sh
npm ci
npm run desktop:build -- -- --locked
```

The build runs the frontend build first. Output:

- `src-tauri/target/release/gram-pos-register.exe`
- `src-tauri/target/release/bundle/nsis/*-setup.exe`

The NSIS installer installs for the current user. It includes the offline
WebView2 installer so installation does not depend on downloading that runtime;
this adds roughly 127 MB. Dependency downloads are still required on the build
machine. No signing certificate is configured: these are **unsigned development
artifacts**, not signed releases. Windows may flag the unsigned executable or
installer. Do not change OS security settings to bypass a warning; review and
sign a release before production deployment.

The `Register` GitHub Actions workflow validates service behavior on Linux and
Windows and creates a Windows artifact only after those checks and frontend
checks pass. It does not publish a release, sign code, or deploy anything. A
workflow file alone is not evidence that the build or installer has been tested.

## Local data and boundary

The host opens `register.sqlite3` in Tauri's app-data directory for the stable
identifier `com.grampos.register`. On Windows this is ordinarily
`%APPDATA%\com.grampos.register\register.sqlite3`. It creates schema only: demo
configuration requires an explicit first-run empty-catalog or demo setup action.
Closing/reopening the app uses the same database. Never delete the database to
fix a startup problem; check directory permissions and preserve a backup first.

SQLite lives behind a mutex and bounded service commands. Frontend code cannot
select a database path, run SQL, access arbitrary files, launch processes, open
the shell, or call an HTTP plugin. Only the local `main` window receives the
`allow-register-command`, `allow-save-export`, and `allow-select-catalog-csv` permissions. External navigation and remote frame
content are blocked; release CSP allows IPC but no network connections.
SQLite is not encrypted, and this prototype does not supply login, roles,
automatic backups, or tamper-proof storage. Manual JSON exports use a native
Save dialog, refuse existing destination files, and need no frontend filesystem
permissions. See [backup and offline recovery](../../../docs/backup.md). Protect the operating-system account
and disk; do not use this prototype as the sole record for a real business.

Desktop dependencies and lockfile are isolated from the dependency-free pricing
core. Linux native host compilation additionally requires GTK/WebKit system
libraries; pure service tests do not.

```sh
cargo fmt --manifest-path apps/register/src-tauri/Cargo.toml -- --check
cargo test --manifest-path crates/register-service/Cargo.toml --locked
# On a Windows machine with prerequisites and frontend dist already built:
cargo test --manifest-path apps/register/src-tauri/Cargo.toml --locked
```

Installer, capability, and CSP configuration follows the official
[Windows packaging](https://v2.tauri.app/distribute/windows-installer/),
[Tauri capabilities](https://v2.tauri.app/security/capabilities/), and
[CSP](https://v2.tauri.app/security/csp/) documentation. The simple icon artwork
is original project geometry, with editable source in `icons/icon.svg`.


Version 0.4 added local training cash sessions and full-sale refunds. In the current
0.10.0 app, recognized schema-1/2/3 databases migrate transactionally to schema 4 without
rewriting old receipts. Keep a pre-upgrade backup and do not reopen the upgraded
file with an older app. The existing bounded IPC command handles the ledger;
no new filesystem/network permissions or payment integration are added.

## Milestone 5 receipt printing and CSV

The saved-receipt view uses a user-triggered `window.print()` call, following Tauri's printing guidance. Scoped print CSS includes only the selected original receipt, not the rest of the register. Current refund status is a separate screen-only section, not part of the original printed sale. No printer plugin, silent printing, new native print command, or filesystem grants are added. Closing or canceling the system print dialog does not change stored records. Browser PDF checks are not a physical-printer or native-device test.

`save_export` also accepts the fixed `sales_report_csv` kind with an explicit `range` containing UTC `start_date` and exclusive `end_date`. Rust validates the dates and creates the bytes; web content cannot pass arbitrary bytes or a destination path. The OS picker selects a new CSV filename, existing files are never replaced, and cancellation writes nothing. Backup/JSON export and offline recovery semantics are unchanged. See [receipt/report details](../../../docs/receipts-and-reports.md).

## Milestone 6 keyboard scanner input

Version 0.6.0 accepts barcode text only through the focused register field. Keyboard-style HID scanners use ordinary text entry and Enter; there is no native scanner driver, global keyboard hook, camera/device permission or expanded capability. `lookup_barcode` uses the existing bounded register-command dispatcher. Unit products have integer item counts and fixed unit prices, not declared grams. Browser/CI coverage does not certify a physical scanner or native-device timing.

## Milestone 7 local catalog CSV

Version 0.7.0 adds `select_catalog_csv` and only its `allow-select-catalog-csv` capability. The command accepts no caller-provided path or content: the native picker selects one local regular `.csv` file, read with a 1 MiB cap and strict UTF-8 decoding. Selection returns `{filename, content}` or `null` on cancel and never imports. Preview and explicit atomic Apply use the existing service dispatcher with revision checks. No broad filesystem grant, shell access, or scanner driver is added.

The fixed `catalog_csv` and `catalog_template` export kinds use `save_export`; output bytes come only from Rust and existing destination files are never overwritten. See [format and operator instructions](../../../docs/catalog-csv.md) for reversible `text:` encoding, SKU upserts, error recovery, and spreadsheet limits. Existing backups and receipts remain valid; new multiline product names require version 0.7.0 validation.

Safe opening rejects substituted final-component links and special files: Unix uses `O_NOFOLLOW | O_NONBLOCK` (the existing transitive libc package is now also a target-Unix direct dependency); Windows opens the reparse point itself and rejects its handle attributes before reading. Both paths validate the opened handle and retain the byte cap. Unix helper tests include replacement with a symlink and FIFO. The Windows symlink test reports a skip when the runner lacks symlink privilege; full native validation remains a Windows CI/manual responsibility.


## Parked carts (0.9.0)

Park, preview, resume and discard use the existing bounded `register_command` dispatcher, the same local SQLite file and the same native export picker. No filesystem permission, network listener or plugin capability is added. The service checks durable draft identity and revision, preserves original quantity kinds, and consumes the active identity atomically with checkout. Native dispatch regressions cover the parked-cart lifecycle and backup contents; a compiled test does not certify native window behavior or physical peripherals. See [the parked-cart guide](../../../docs/parked-carts.md).

The frontend requires Web Locks for safe same-origin recovery-key coordination. It checks the capability at runtime and refuses cart writes if unavailable; it does not substitute a non-atomic browser-storage lock. The Windows package bundles WebView2 installation support, but native UI/runtime capability still needs actual platform verification beyond Rust dispatcher tests.

The existing Windows origin is deliberately unchanged: Tauri documents the default as `http://<scheme>.localhost` ([configuration reference](https://v2.tauri.app/reference/config/#windowconfig)); its maintainer confirms `http://tauri.localhost` is treated as a secure desktop origin ([upstream discussion](https://github.com/tauri-apps/tauri/issues/12331)). This matches the [W3C secure-context rules for `.localhost`](https://www.w3.org/TR/secure-contexts/#is-origin-trustworthy). Changing `useHttpsScheme` between releases changes browser storage location, so the upgrade must not silently switch schemes and orphan existing pending checkout keys. Browser CI also probes the exact Windows origin with Chromium, but this remains engine/origin evidence, not a running Tauri/WebView2 UI certification.


## Cash paid-in/out (0.10.0)

The existing bounded `register_command` dispatcher accepts reviewed cash-movement operations; no new native capability, arbitrary path, shell, hardware or live-payment permission is added. Positive paid-ins/out belong to an open matching-currency session and are immutable, independently identified, idempotent records. They do not create a receipt or change revenue. The service checks paid-out limits and closing revisions inside its serialized transaction.

Portable backup/schema v4 includes movements and their recovery/audit data with parked carts. Sales JSON format v3 separates movements from sales/refunds. Offline recovery still accepts v1/v2/v3 backups into a new destination, never overwriting the current database. See [cash workflows](../../../docs/cash-sessions.md) and [backup details](../../../docs/backup.md).

Native dispatcher tests cover the movement lifecycle, retry after close/restore, stale closing reviews, overdraw and rejection of extra authority fields. Running their portable extracted harness is not packaged Tauri/WebView2 execution. Real native UI, physical drawers, scanners, scales and printers still require separate testing.
