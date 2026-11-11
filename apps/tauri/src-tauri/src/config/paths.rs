use std::path::PathBuf;

use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};

use anyhow::{Context, Result};

/// Resolve the application's logs directory under the AppData container.
/// Centralized here so runtime paths are not hardcoded across modules.
pub fn logs_dir_from_handle(app_handle: &AppHandle) -> PathBuf {
    // Keep for future use by modules needing a central logs dir
    match app_handle.path().resolve("logs", BaseDirectory::AppData) {
        Ok(path) => {
            let _ = std::fs::create_dir_all(&path);
            path
        }
        Err(_) => logs_dir_default(),
    }
}

/// Resolve the default logs directory using OS conventions without requiring a Tauri handle.
pub fn logs_dir_default() -> PathBuf {
    let base = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.sprite.validation")
        .join("logs");
    let _ = std::fs::create_dir_all(&base);
    base
}

/// Resolve the application's metrics directory under the AppData container.
pub fn metrics_dir_from_handle(app_handle: &AppHandle) -> PathBuf {
    match app_handle.path().resolve("metrics", BaseDirectory::AppData) {
        Ok(path) => {
            let _ = std::fs::create_dir_all(&path);
            path
        }
        Err(_) => metrics_dir_default(),
    }
}

/// Resolve the default metrics directory using OS conventions without requiring a Tauri handle.
pub fn metrics_dir_default() -> PathBuf {
    let base = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.sprite.validation")
        .join("metrics");
    let _ = std::fs::create_dir_all(&base);
    base
}

/// Resolve the application's catalog directory under the AppData container.
pub fn catalog_dir_from_handle(app_handle: &AppHandle) -> PathBuf {
    match app_handle.path().resolve("catalog", BaseDirectory::AppData) {
        Ok(path) => {
            let _ = std::fs::create_dir_all(&path);
            path
        }
        Err(_) => catalog_dir_default(),
    }
}

/// Resolve the default catalog directory without an app handle.
pub fn catalog_dir_default() -> PathBuf {
    let base = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.sprite.validation")
        .join("catalog");
    let _ = std::fs::create_dir_all(&base);
    base
}

pub fn dev_config_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".dev")
        .join("config")
        .join(filename)
}

pub fn prod_config_path(filename: &str) -> PathBuf {
    prod_storage_root().join("config").join(filename)
}

pub fn config_path(filename: &str) -> PathBuf {
    if cfg!(debug_assertions) {
        dev_config_path(filename)
    } else {
        prod_config_path(filename)
    }
}

pub fn prod_storage_root() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.sprite.validation")
}

pub fn prod_models_dir_os() -> PathBuf {
    prod_storage_root().join("models").join("whisper")
}

pub fn default_models_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/whisper")
    } else {
        prod_models_dir_os()
    }
}

pub fn models_dir_from_handle(app_handle: &AppHandle) -> Result<PathBuf> {
    let dir = if cfg!(debug_assertions) {
        default_models_dir()
    } else {
        app_handle
            .path()
            .resolve("models/whisper", BaseDirectory::AppData)
            .context("Failed to resolve models directory under AppData")?
    };

    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create models directory: {}", dir.display()))?;

    Ok(dir)
}

pub fn default_piper_assets_dir() -> PathBuf {
    if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/piper")
    } else {
        prod_storage_root().join("tools").join("piper")
    }
}

pub fn piper_assets_dir_from_handle(app_handle: &AppHandle) -> Result<PathBuf> {
    let dir = if cfg!(debug_assertions) {
        default_piper_assets_dir()
    } else {
        app_handle
            .path()
            .resolve("tools/piper", BaseDirectory::AppData)
            .context("Failed to resolve piper tools directory under AppData")?
    };

    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create Piper tools directory: {}", dir.display()))?;

    Ok(dir)
}
