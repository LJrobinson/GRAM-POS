#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gram_register_service::{RegisterService, SalesReportRequest, ServiceError};
use serde::Serialize;
use serde_json::Value;
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

type SharedRegister = Arc<Mutex<RegisterService>>;

#[derive(Debug, Serialize)]
struct CommandError {
    code: String,
    message: String,
}

impl From<ServiceError> for CommandError {
    fn from(error: ServiceError) -> Self {
        Self {
            code: error.code,
            message: error.message,
        }
    }
}

fn unavailable() -> CommandError {
    CommandError {
        code: "register_unavailable".to_owned(),
        message: "The local register is unavailable. Restart GRAM POS before continuing."
            .to_owned(),
    }
}

fn dispatch(
    service: &SharedRegister,
    command: &str,
    payload: Value,
) -> Result<Value, CommandError> {
    service
        .lock()
        .map_err(|_| unavailable())?
        .handle(command, payload)
        .map_err(CommandError::from)
}

// This is the register service IPC entry point. It accepts service operations,
// never SQL, a filesystem path, a URL, a process command, or a shell script.
#[tauri::command]
async fn register_command(
    state: tauri::State<'_, SharedRegister>,
    command: String,
    payload: Value,
) -> Result<Value, CommandError> {
    let service = Arc::clone(state.inner());
    // SQLite waits and disk operations must not block the native UI thread.
    tauri::async_runtime::spawn_blocking(move || dispatch(&service, &command, payload))
        .await
        .map_err(|_| unavailable())?
}

// Only this bounded command can write an export. The webview supplies neither
// bytes nor a path; the service generates bytes and the OS picker chooses a path.
#[tauri::command]
async fn save_export(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedRegister>,
    kind: String,
    range: Option<SalesReportRequest>,
) -> Result<Option<String>, CommandError> {
    let service = Arc::clone(state.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let export = {
            let service = service.lock().map_err(|_| unavailable())?;
            generate_export(&service, &kind, range)?
        };
        let selected = app
            .dialog()
            .file()
            .add_filter(
                if matches!(
                    kind.as_str(),
                    "sales_report_csv" | "catalog_csv" | "catalog_template"
                ) {
                    "CSV"
                } else {
                    "JSON"
                },
                if matches!(
                    kind.as_str(),
                    "sales_report_csv" | "catalog_csv" | "catalog_template"
                ) {
                    &["csv"]
                } else {
                    &["json"]
                },
            )
            .set_file_name(&export.filename)
            .blocking_save_file();
        let Some(selected) = selected else {
            return Ok(None);
        };
        let path = selected.into_path().map_err(|_| CommandError {
            code: "EXPORT_PATH".into(),
            message: "Select a local destination file.".into(),
        })?;
        write_new_export(&path, export.content.as_bytes())?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|_| unavailable())?
}

#[derive(Debug, Serialize)]
struct SelectedCatalogCsv {
    filename: String,
    content: String,
}

// There is deliberately no caller path argument or general filesystem grant.
// Selection reads only the chosen local regular CSV, never applies its content.
#[tauri::command]
async fn select_catalog_csv(
    app: tauri::AppHandle,
) -> Result<Option<SelectedCatalogCsv>, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let selected = app
            .dialog()
            .file()
            .add_filter("Catalog CSV", &["csv"])
            .blocking_pick_file();
        let Some(selected) = selected else {
            return Ok(None);
        };
        let path = selected
            .into_path()
            .map_err(|_| csv_read_error("Select a local CSV file."))?;
        read_selected_catalog_csv(&path).map(Some)
    })
    .await
    .map_err(|_| unavailable())?
}

fn csv_read_error(message: &str) -> CommandError {
    CommandError {
        code: "CATALOG_CSV_FILE".into(),
        message: message.into(),
    }
}

// No-follow is enforced at open, not merely by a racy preflight metadata check.
// Nonblocking Unix opens also prevent a substituted FIFO from hanging a worker.
fn open_selected_csv_handle(path: &std::path::Path) -> Result<std::fs::File, CommandError> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT: inspect the link itself, never its target.
        options.custom_flags(0x0020_0000);
    }
    let file = options
        .open(path)
        .map_err(|_| csv_read_error("Cannot safely open the selected regular CSV file."))?;
    let metadata = file
        .metadata()
        .map_err(|_| csv_read_error("Cannot inspect the selected CSV file."))?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT includes symlinks and other redirects.
        if metadata.file_attributes() & 0x0000_0400 != 0 {
            return Err(csv_read_error(
                "Select a regular local CSV file, not a link or reparse point.",
            ));
        }
    }
    if !metadata.is_file() {
        return Err(csv_read_error(
            "Select a regular local CSV file, not a device or pipe.",
        ));
    }
    Ok(file)
}

fn read_selected_catalog_csv(path: &std::path::Path) -> Result<SelectedCatalogCsv, CommandError> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
    {
        return Err(csv_read_error("Select a .csv file encoded as UTF-8."));
    }
    // Inspect before opening to reject pipes/devices without blocking. Recheck
    // the opened handle, and bound reads even if the selected file grows.
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| csv_read_error("Cannot inspect the selected CSV file."))?;
    if !metadata.is_file() {
        return Err(csv_read_error(
            "Select a regular local CSV file, not a link or device.",
        ));
    }
    let file = open_selected_csv_handle(path)?;
    let metadata = file
        .metadata()
        .map_err(|_| csv_read_error("Cannot inspect the selected CSV file."))?;
    let limit = gram_register_service::MAX_CATALOG_CSV_BYTES;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(csv_read_error(
            "Select a regular CSV file no larger than 1 MiB.",
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| csv_read_error("Cannot read the selected CSV file."))?;
    if bytes.len() > limit {
        return Err(csv_read_error("CSV files must be no larger than 1 MiB."));
    }
    let content = String::from_utf8(bytes)
        .map_err(|_| csv_read_error("CSV must be valid UTF-8; resave it as UTF-8 CSV."))?;
    Ok(SelectedCatalogCsv {
        filename: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        content,
    })
}

fn generate_export(
    service: &RegisterService,
    kind: &str,
    range: Option<SalesReportRequest>,
) -> Result<gram_register_service::ExportFile, CommandError> {
    match kind {
        "backup" => Ok(service.export_backup()?),
        "sales" => Ok(service.export_sales()?),
        "catalog_csv" => Ok(service.export_catalog_csv()?),
        "catalog_template" => Ok(service.catalog_csv_template()?),
        "sales_report_csv" => {
            Ok(
                service.export_sales_report_csv_with_timezone(range.ok_or_else(|| CommandError {
                    code: "INVALID_INPUT".into(),
                    message: "Choose an explicit report date range and timezone.".into(),
                })?)?,
            )
        }
        _ => Err(CommandError {
            code: "INVALID_EXPORT".into(),
            message: "Choose a backup, sales JSON, sales report CSV, catalog CSV, or catalog template export.".into(),
        }),
    }
}

fn write_new_export(path: &std::path::Path, content: &[u8]) -> Result<(), CommandError> {
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)
        .map_err(|_| CommandError {code:"EXPORT_DESTINATION".into(),message:"Choose a new filename in a writable folder. Existing files are never replaced, even if the dialog offers to overwrite.".into()})?;
    file.write_all(content).and_then(|_| file.sync_all()).map_err(|_| CommandError {
        code:"EXPORT_WRITE".into(), message:"The export could not be completely saved. Do not use that incomplete file; retry with a new filename.".into()
    })
}

fn is_local_navigation(url: &tauri::Url, development: bool) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    // Tauri serves bundled assets through a custom protocol. On Windows this
    // maps to the reserved tauri.localhost origin, not an HTTP server.
    let bundled = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (matches!(url.scheme(), "http" | "https")
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none());
    let vite = development
        && url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port() == Some(5173);
    bundled || vite
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let service = RegisterService::open(directory.join("register.sqlite3"))
                .map_err(|error| std::io::Error::other(error.message))?;
            app.manage(Arc::new(Mutex::new(service)));

            // Delay window creation until storage is ready, and reject external
            // navigation. Demo initialization remains an explicit UI action.
            let window = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "main")
                .ok_or_else(|| std::io::Error::other("Main register window is not configured"))?;
            tauri::WebviewWindowBuilder::from_config(app, window)?
                .on_navigation(|url| is_local_navigation(url, cfg!(debug_assertions)))
                .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            register_command,
            save_export,
            select_catalog_csv
        ])
        .run(tauri::generate_context!())
        .expect("GRAM POS could not start; check local data-directory access and WebView2");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csv_test_path(suffix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "gram-native-csv-{}-{}.{suffix}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn selected_csv_is_bounded_utf8_and_selection_does_not_apply() {
        let path = csv_test_path("csv");
        let content = "sku,name\r\n00123,Crème\r\n";
        std::fs::write(&path, content).unwrap();
        let selected = read_selected_catalog_csv(&path).unwrap();
        assert_eq!(selected.content, content);
        assert_eq!(
            selected.filename,
            path.file_name().unwrap().to_string_lossy()
        );
        // Reading returns text only; no register/database handle is accepted.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert_eq!(
            read_selected_catalog_csv(&path).unwrap_err().code,
            "CATALOG_CSV_FILE"
        );
        std::fs::write(
            &path,
            vec![b'a'; gram_register_service::MAX_CATALOG_CSV_BYTES + 1],
        )
        .unwrap();
        assert_eq!(
            read_selected_catalog_csv(&path).unwrap_err().code,
            "CATALOG_CSV_FILE"
        );
        std::fs::remove_file(path).unwrap();
        let path = csv_test_path("json");
        std::fs::write(&path, "{}").unwrap();
        assert!(read_selected_catalog_csv(&path).is_err());
        std::fs::remove_file(path).unwrap();
        let path = csv_test_path("csv");
        std::fs::create_dir(&path).unwrap();
        assert!(read_selected_catalog_csv(&path).is_err());
        std::fs::remove_dir(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn safe_open_rejects_replaced_links_and_pipes_without_following_or_blocking() {
        use std::os::unix::fs::symlink;
        let target = csv_test_path("csv");
        let selected = csv_test_path("csv");
        std::fs::write(&target, "do not follow").unwrap();
        std::fs::write(&selected, "original").unwrap();
        assert!(std::fs::symlink_metadata(&selected).unwrap().is_file());
        // Simulate replacement between preflight metadata and open.
        std::fs::remove_file(&selected).unwrap();
        symlink(&target, &selected).unwrap();
        assert!(open_selected_csv_handle(&selected).is_err());
        std::fs::remove_file(&selected).unwrap();
        let c_path = std::ffi::CString::new(selected.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: c_path is a valid, owned NUL-terminated path; no aliasing.
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        assert!(open_selected_csv_handle(&selected).is_err());
        std::fs::remove_file(selected).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "do not follow");
        std::fs::remove_file(target).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn safe_open_rejects_windows_reparse_points_when_symlinks_are_supported() {
        let target = csv_test_path("csv");
        let selected = csv_test_path("csv");
        std::fs::write(&target, "do not follow").unwrap();
        match std::os::windows::fs::symlink_file(&target, &selected) {
            Ok(()) => {
                assert!(open_selected_csv_handle(&selected).is_err());
                std::fs::remove_file(selected).unwrap();
            }
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("Windows symlink test skipped: runner lacks symlink privilege.");
            }
            Err(error) => panic!("Could not create test symlink: {error}"),
        }
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "do not follow");
        std::fs::remove_file(target).unwrap();
    }

    #[test]
    fn catalog_exports_are_service_generated_and_fixed_kind() {
        let mut service = RegisterService::in_memory().unwrap();
        service.initialize_demo().unwrap();
        for kind in ["catalog_csv", "catalog_template"] {
            let file = generate_export(&service, kind, None).unwrap();
            assert!(file.filename.ends_with(".csv"));
            assert!(file.content.contains("unit_price_minor"));
            assert_eq!(file.media_type, "text/csv;charset=utf-8");
        }
        assert!(generate_export(&service, "../catalog.csv", None).is_err());
    }

    #[test]
    fn keyboard_barcode_uses_existing_bounded_dispatch() {
        let service = Arc::new(Mutex::new(RegisterService::in_memory().unwrap()));
        let state = dispatch(&service, "initialize_demo", serde_json::json!({})).unwrap();
        let mut config = state["config"].clone();
        config["catalog"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "sku": "NATIVE-UNIT", "name": "Accessory", "kind": "unit",
                "declared_content_grams": null, "barcode": "00123",
                "pricing": { "kind": "fixed_unit", "unit_price_minor": "250" }
            }));
        let saved = dispatch(
            &service,
            "put_config",
            serde_json::json!({
                "config": config, "expected_config_revision": state["config_revision"]
            }),
        )
        .unwrap();
        let found = dispatch(
            &service,
            "lookup_barcode",
            serde_json::json!({"barcode":"00123"}),
        )
        .unwrap();
        assert_eq!(found["item"]["sku"], "NATIVE-UNIT");
        assert_eq!(found["config_revision"], saved["config_revision"]);
        assert_eq!(
            dispatch(
                &service,
                "lookup_barcode",
                serde_json::json!({"barcode":"123"})
            )
            .unwrap_err()
            .code,
            "UNKNOWN_BARCODE"
        );
        assert_eq!(
            dispatch(
                &service,
                "lookup_barcode",
                serde_json::json!({"barcode":"00123", "path":"ignored"})
            )
            .unwrap_err()
            .code,
            "INVALID_INPUT"
        );
        let quote = dispatch(
            &service,
            "quote_cart",
            serde_json::json!({"lines":[{
                "id":"unit-1", "sku":"NATIVE-UNIT", "quantity":{"kind":"unit","count":"2"}
            }]}),
        )
        .unwrap();
        assert_eq!(quote["total_minor"], "500");
        assert_eq!(quote["groups"], serde_json::json!([]));
        assert!(quote["lines"][0]["declared_content_grams"].is_null());
    }

    #[test]
    fn report_exports_require_fixed_kind_and_validated_dates() {
        let service = RegisterService::in_memory().unwrap();
        assert_eq!(
            generate_export(&service, "other.csv", None)
                .unwrap_err()
                .code,
            "INVALID_EXPORT"
        );
        assert_eq!(
            generate_export(&service, "sales_report_csv", None)
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
        let export = generate_export(
            &service,
            "sales_report_csv",
            Some(SalesReportRequest {
                timezone: None,
                expected_config_revision: None,
                start_date: "2026-01-01".into(),
                end_date: "2026-01-02".into(),
            }),
        )
        .unwrap();
        assert_eq!(export.filename, "gram-pos-sales-report.csv");
        assert_eq!(export.media_type, "text/csv;charset=utf-8");
        assert!(export.content.starts_with("\"record_type\""));
    }

    #[test]
    fn report_exports_freeze_timezone_and_reject_changed_configuration() {
        let mut service = RegisterService::in_memory().unwrap();
        let state = service.initialize_demo().unwrap();
        let range = SalesReportRequest {
            start_date: "2026-03-08".into(),
            end_date: "2026-03-09".into(),
            timezone: Some("America/Los_Angeles".into()),
            expected_config_revision: Some(state.config_revision.clone()),
        };
        let report = service.sales_report_with_timezone(range.clone()).unwrap();
        assert_eq!(
            report.end_at_unix.parse::<i64>().unwrap()
                - report.start_at_unix.parse::<i64>().unwrap(),
            23 * 3600
        );
        let export = generate_export(&service, "sales_report_csv", Some(range.clone())).unwrap();
        assert!(export.content.contains("America/Los_Angeles"));
        assert!(export.content.contains(&report.start_at_unix));
        assert!(export.content.contains("metadata"));
        service
            .handle(
                "put_store_settings",
                serde_json::json!({
                    "store_settings": {
                        "display_name": "Later store", "receipt_header": "", "receipt_contact": "",
                        "receipt_footer": "", "timezone": "Asia/Tokyo"
                    },
                    "expected_config_revision": state.config_revision
                }),
            )
            .unwrap();
        assert_eq!(
            generate_export(&service, "sales_report_csv", Some(range))
                .unwrap_err()
                .code,
            "STALE_CONFIG"
        );
        // Already generated bytes are the labeled snapshot passed to the picker.
        assert!(export.content.contains("America/Los_Angeles"));
        assert!(!export.content.contains("Asia/Tokyo"));
    }

    #[test]
    fn exports_never_replace_existing_files() {
        let path = std::env::temp_dir().join(format!(
            "gram-native-export-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_new_export(&path, b"original").unwrap();
        assert!(write_new_export(&path, b"replacement").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn production_navigation_accepts_only_bundled_origins() {
        for url in [
            "tauri://localhost/index.html",
            "http://tauri.localhost/index.html",
            "https://tauri.localhost/index.html",
        ] {
            assert!(is_local_navigation(&url.parse().unwrap(), false), "{url}");
        }
        for url in [
            "https://example.com",
            "http://127.0.0.1:5173",
            "http://localhost:5173",
            "file:///etc/passwd",
            "https://tauri.localhost.example.com",
            "http://tauri.localhost:5173",
            "https://user@tauri.localhost",
            "tauri://not-localhost",
        ] {
            assert!(!is_local_navigation(&url.parse().unwrap(), false), "{url}");
        }
    }

    #[test]
    fn development_navigation_allows_only_the_configured_vite_origin() {
        assert!(is_local_navigation(
            &"http://127.0.0.1:5173/".parse().unwrap(),
            true
        ));
        for url in [
            "http://127.0.0.1:4173",
            "http://localhost:5173",
            "https://127.0.0.1:5173",
            "https://example.com",
        ] {
            assert!(!is_local_navigation(&url.parse().unwrap(), true), "{url}");
        }
    }

    #[test]
    fn main_window_has_only_register_permission() {
        let capability: Value =
            serde_json::from_str(include_str!("../capabilities/register.json")).unwrap();
        assert_eq!(
            capability["permissions"],
            serde_json::json!([
                "allow-register-command",
                "allow-save-export",
                "allow-select-catalog-csv"
            ])
        );
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
        assert_eq!(capability["local"], true);
        assert!(capability.get("remote").is_none());
    }

    #[test]
    fn parked_cart_lifecycle_uses_bounded_dispatch_and_atomic_checkout() {
        let service = Arc::new(Mutex::new(RegisterService::in_memory().unwrap()));
        let initial = dispatch(&service, "initialize_demo", serde_json::json!({})).unwrap();
        let lines = serde_json::json!([
            {"id":"native-weight", "sku":"bulk-a", "quantity":{"kind":"measured","grams":"3.501"}},
            {"id":"native-pack", "sku":"prepack-fixed", "quantity":{"kind":"prepacked","count":"2"}}
        ]);
        let saved = dispatch(&service, "save_active_draft", serde_json::json!({
            "idempotency_key":"native-save", "expected_draft_revision":initial["draft_revision"], "lines":lines
        })).unwrap();
        let identity = saved["active_draft"]["id"].clone();
        assert!(identity.is_string());
        let park_request = serde_json::json!({
            "idempotency_key":"native-park", "expected_draft_revision":saved["draft_revision"], "label":"Counter 2"
        });
        let parked = dispatch(&service, "park_draft", park_request.clone()).unwrap();
        assert_eq!(parked["draft"]["lines"], serde_json::json!([]));
        assert_eq!(parked["parked_drafts"][0]["id"], identity);
        assert_eq!(
            dispatch(&service, "park_draft", park_request).unwrap(),
            parked
        );
        let preview = dispatch(
            &service,
            "preview_parked_draft",
            serde_json::json!({"draft_id":identity}),
        )
        .unwrap();
        assert_eq!(preview["lines"], lines);
        assert!(preview["quote"]["total_minor"].is_string());
        assert_eq!(
            dispatch(&service, "list_sales", serde_json::json!({})).unwrap()["sales"],
            serde_json::json!([])
        );
        assert!(
            dispatch(&service, "get_cash_ledger", serde_json::json!({})).unwrap()["active_session"]
                .is_null()
        );
        let export = generate_export(&service.lock().unwrap(), "backup", None).unwrap();
        let backup: Value = serde_json::from_str(&export.content).unwrap();
        assert_eq!(backup["format_version"], 3);
        assert_eq!(backup["schema_version"], 3);
        assert!(export.content.contains("Counter 2"));
        let resumed = dispatch(&service,"resume_parked_draft",serde_json::json!({
            "idempotency_key":"native-resume", "expected_draft_revision":parked["draft_revision"],
            "expected_config_revision":preview["config_revision"], "draft_id":identity
        })).unwrap();
        assert_eq!(resumed["active_draft"]["id"], identity);
        assert_eq!(resumed["draft"]["lines"], lines);
        let session = dispatch(&service,"open_cash_session",serde_json::json!({
            "idempotency_key":"native-open","opening_float_minor":"0", "expected_currency":preview["currency"]
        })).unwrap();
        let checkout = serde_json::json!({
            "idempotency_key":"native-sale", "lines":lines,
            "expected_draft_id":identity,"expected_draft_revision":resumed["draft_revision"],
            "expected_session_id":session["session_id"],"expected_currency":preview["currency"],
            "expected_config_revision":preview["config_revision"],
            "expected_total_minor":preview["quote"]["total_minor"], "tender_minor":preview["quote"]["total_minor"]
        });
        let receipt = dispatch(&service, "complete_cash_sale", checkout.clone()).unwrap();
        assert_eq!(
            dispatch(&service, "complete_cash_sale", checkout.clone()).unwrap(),
            receipt
        );
        let mut duplicate = checkout;
        duplicate["idempotency_key"] = serde_json::json!("native-second-sale-key");
        assert!(dispatch(&service, "complete_cash_sale", duplicate).is_err());
        let after = dispatch(&service, "get_state", serde_json::json!({})).unwrap();
        assert!(after["active_draft"].is_null());
        assert_eq!(after["draft"]["lines"], serde_json::json!([]));
        assert_eq!(after["parked_drafts"], serde_json::json!([]));
        assert_eq!(
            dispatch(&service, "list_sales", serde_json::json!({})).unwrap()["sales"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            dispatch(&service, "get_cash_ledger", serde_json::json!({})).unwrap()["active_session"]
                ["movement_count"],
            "1"
        );
    }

    #[test]
    fn draft_dispatch_rejects_extra_authority_and_stale_mutations() {
        let service = Arc::new(Mutex::new(RegisterService::in_memory().unwrap()));
        let initial = dispatch(&service, "initialize_demo", serde_json::json!({})).unwrap();
        assert_eq!(dispatch(&service,"save_active_draft",serde_json::json!({
            "idempotency_key":"native-extra", "expected_draft_revision":initial["draft_revision"],
            "lines":[], "path":"arbitrary.sqlite3"
        })).unwrap_err().code,"INVALID_INPUT");
        let saved = dispatch(&service,"save_active_draft",serde_json::json!({
            "idempotency_key":"native-current", "expected_draft_revision":initial["draft_revision"],
            "lines":[{"id":"native-line","sku":"bulk-a","quantity":{"kind":"measured","grams":"3.5"}}]
        })).unwrap();
        assert_eq!(dispatch(&service,"save_active_draft",serde_json::json!({
            "idempotency_key":"native-stale", "expected_draft_revision":initial["draft_revision"], "lines":[]
        })).unwrap_err().code,"STALE_DRAFT");
        assert_eq!(
            dispatch(&service, "get_state", serde_json::json!({})).unwrap(),
            saved
        );
        assert_eq!(
            dispatch(&service, "save_draft", serde_json::json!({"lines":[]}))
                .unwrap_err()
                .code,
            "GUARDED_DRAFT_REQUIRED"
        );
    }
}
