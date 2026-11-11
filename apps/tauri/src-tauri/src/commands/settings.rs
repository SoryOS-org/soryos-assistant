//! Configuration commands exposed to the panel's config editor.

use crate::config::prelude::{ConfigSummary, ConfigUpdateResponse};
use crate::{
    config::AppConfig,
    logging::{telemetry_level_from_state, update_runtime_log_filter},
    stt::EVENT_CONFIG_CHANGED,
};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

/// Return a read-only summary of current configuration.
#[tauri::command]
pub fn get_config_summary(app_config: State<'_, AppConfig>) -> Result<ConfigSummary, String> {
    Ok(app_config.summary())
}

/// Apply a JSON patch to the config and refresh log filters as needed.
#[tauri::command]
pub fn update_config(
    app_handle: AppHandle,
    app_config: State<'_, AppConfig>,
    patch: Value,
) -> Result<ConfigUpdateResponse, String> {
    let response = app_config
        .apply_stt_patch(&app_handle, &patch)
        .map_err(|err| err.to_string())?;

    if let Some(level) = telemetry_level_from_state(&app_config) {
        update_runtime_log_filter(level);
    }

    let _ = app_handle.emit(EVENT_CONFIG_CHANGED, ());

    Ok(response)
}
