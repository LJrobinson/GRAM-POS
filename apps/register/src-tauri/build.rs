fn main() {
    // App commands otherwise bypass capability filtering by default. Generate
    // the narrow allow-register-command permission and opt in only main.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["register_command", "save_export"]),
    ))
    .expect("failed to build GRAM POS desktop assets and permissions");
}
