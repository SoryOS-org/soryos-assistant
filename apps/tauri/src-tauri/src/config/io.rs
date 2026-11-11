use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use tauri::{App, AppHandle};
use tracing::{debug, info};

use super::paths;

pub fn load_or_create<T, FParse, FSerialize, FDefault>(
    _app: &App,
    filename: &str,
    parse: FParse,
    serialize: FSerialize,
    default: FDefault,
) -> Result<T>
where
    FParse: Fn(&str) -> Result<T>,
    FSerialize: Fn(&T) -> Result<String>,
    FDefault: Fn() -> T,
{
    let path = paths::config_path(filename);
    debug!(path = %path.display(), "loading config");

    if path.exists() {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        match parse(&content) {
            Ok(config) => return Ok(config),
            Err(err) => {
                tracing::warn!(
                    "Failed to parse config {}: {err}; falling back to defaults",
                    path.display()
                );
            }
        }
    } else if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    info!(path = %path.display(), "config missing; writing defaults");
    let config = default();
    save_to_path(&path, &config, serialize)?;
    Ok(config)
}

pub fn read_config_string(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Config file {} is empty", path.display()));
    }
    Ok(trimmed.to_string())
}

pub fn save_to_handle<T, FSerialize>(
    _app_handle: &AppHandle,
    filename: &str,
    config: &T,
    serialize: FSerialize,
) -> Result<()>
where
    FSerialize: Fn(&T) -> Result<String>,
{
    let path = paths::config_path(filename);
    debug!(path = %path.display(), "saving config");
    save_to_path(&path, config, serialize)
}

fn save_to_path<T, FSerialize>(path: &Path, config: &T, serialize: FSerialize) -> Result<()>
where
    FSerialize: Fn(&T) -> Result<String>,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let content = serialize(config)?;
    fs::write(path, content)
        .with_context(|| format!("Failed to write config file: {}", path.display()))?;
    Ok(())
}
