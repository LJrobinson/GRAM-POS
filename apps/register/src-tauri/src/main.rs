#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gram_register_service::{RegisterService, SalesReportInput, ServiceError};
use serde::Serialize;
use serde_json::Value;
use std::{
    io::Write,
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
    range: Option<SalesReportInput>,
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
                if kind == "sales_report_csv" {
                    "CSV"
                } else {
                    "JSON"
                },
                if kind == "sales_report_csv" {
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

fn generate_export(
    service: &RegisterService,
    kind: &str,
    range: Option<SalesReportInput>,
) -> Result<gram_register_service::ExportFile, CommandError> {
    match kind {
        "backup" => Ok(service.export_backup()?),
        "sales" => Ok(service.export_sales()?),
        "sales_report_csv" => {
            Ok(
                service.export_sales_report_csv(range.ok_or_else(|| CommandError {
                    code: "INVALID_INPUT".into(),
                    message: "Choose an explicit UTC report date range.".into(),
                })?)?,
            )
        }
        _ => Err(CommandError {
            code: "INVALID_EXPORT".into(),
            message: "Choose a backup, sales JSON, or sales report CSV export.".into(),
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
        .invoke_handler(tauri::generate_handler![register_command, save_export])
        .run(tauri::generate_context!())
        .expect("GRAM POS could not start; check local data-directory access and WebView2");
}

#[cfg(test)]
mod tests {
    use super::*;

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
            Some(SalesReportInput {
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
            serde_json::json!(["allow-register-command", "allow-save-export"])
        );
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
        assert_eq!(capability["local"], true);
        assert!(capability.get("remote").is_none());
    }
}
