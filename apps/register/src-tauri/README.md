# GRAM POS desktop host

Milestone 3 is a local development register, not a production payment or
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
`allow-register-command` and `allow-save-export` permissions. External navigation and remote frame
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


Version 0.4 adds local training cash sessions and full-sale refunds. On first
open, recognized schema-1 databases migrate transactionally to schema 2 without
rewriting old receipts. Keep a pre-upgrade backup and do not reopen the upgraded
file with an older app. The existing bounded IPC command handles the ledger;
no new filesystem/network permissions or payment integration are added.
