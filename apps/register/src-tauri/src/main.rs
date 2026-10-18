#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gram_register_service::{RegisterService, ServiceError};
use serde::Serialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tauri::Manager;

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

// This is the only application IPC entry point. It accepts service operations,
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
        .invoke_handler(tauri::generate_handler![register_command])
        .run(tauri::generate_context!())
        .expect("GRAM POS could not start; check local data-directory access and WebView2");
}

#[cfg(test)]
mod tests {
    use super::*;

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
            serde_json::json!(["allow-register-command"])
        );
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
        assert_eq!(capability["local"], true);
        assert!(capability.get("remote").is_none());
    }
}
