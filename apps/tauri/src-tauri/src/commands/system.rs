//! Misc system metadata helpers exposed as Tauri commands.

use tauri::AppHandle;

#[derive(Debug, serde::Serialize)]
pub struct AboutMetadata {
    version: String,
    commit: String,
    build_timestamp: Option<String>,
    tauri: String,
    target: String,
}

/// Return build/runtime metadata for the About view.
#[tauri::command]
pub fn get_about_metadata(app_handle: AppHandle) -> AboutMetadata {
    let package_version = app_handle.package_info().version.to_string();
    let commit = option_env!("APP_BUILD_COMMIT")
        .unwrap_or("unknown")
        .to_string();
    let build_timestamp = option_env!("APP_BUILD_TIMESTAMP").map(|value| value.to_string());
    let target =
        tauri::utils::platform::target_triple().unwrap_or_else(|_| "unknown-target".to_string());
    let tauri_runtime = format!("Tauri {}", tauri::VERSION);

    AboutMetadata {
        version: package_version,
        commit,
        build_timestamp,
        tauri: tauri_runtime,
        target,
    }
}
