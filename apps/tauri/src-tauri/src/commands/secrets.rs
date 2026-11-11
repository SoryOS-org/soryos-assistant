use crate::secret_store::{self, SecretKey};
use crate::state_sync;
use crate::stt::EVENT_CONFIG_CHANGED;
use tauri::{AppHandle, Emitter, Manager};
use tracing::info;

#[derive(serde::Serialize)]
pub struct SecretsStatus {
    #[serde(rename = "picovoice")]
    pub picovoice: SecretFieldStatus,
    #[serde(rename = "worker_auth")]
    pub worker_auth: SecretFieldStatus,
}

#[derive(serde::Serialize)]
pub struct SecretFieldStatus {
    pub has_key: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

fn status_for(key: SecretKey) -> SecretFieldStatus {
    let secret = match key {
        SecretKey::Picovoice => secret_store::load_picovoice_key(),
        SecretKey::WorkerAuth => secret_store::load_worker_auth_token(),
    };
    let has_key = secret.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
    let hint = secret
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(secret_store::masked_hint);

    SecretFieldStatus { has_key, hint }
}

fn update_picovoice_runtime(app_handle: &AppHandle, value: Option<String>) {
    if let Some(stt_service) = app_handle.try_state::<crate::stt::SttService>() {
        stt_service.update_picovoice_key(value);
    }
    let _ = app_handle.emit(EVENT_CONFIG_CHANGED, ());
}

fn store_secret(
    app_handle: &AppHandle,
    key: SecretKey,
    value: &str,
    on_update: fn(&AppHandle, Option<String>),
) -> Result<SecretFieldStatus, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("Access key cannot be empty".to_string());
    }

    info!("Saving {:?} to secure storage", key);
    match key {
        SecretKey::Picovoice => secret_store::store_picovoice_key(trimmed),
        SecretKey::WorkerAuth => secret_store::store_worker_auth_token(trimmed),
    }
    .map_err(|err| err.to_string())?;
    on_update(app_handle, Some(trimmed.to_string()));
    Ok(SecretFieldStatus {
        has_key: true,
        hint: Some(secret_store::masked_hint(trimmed)),
    })
}

fn clear_secret(
    app_handle: &AppHandle,
    key: SecretKey,
    on_update: fn(&AppHandle, Option<String>),
) -> Result<SecretFieldStatus, String> {
    info!("Clearing {:?} from secure storage", key);
    match key {
        SecretKey::Picovoice => secret_store::clear_picovoice_key(),
        SecretKey::WorkerAuth => secret_store::clear_worker_auth_token(),
    }
    .map_err(|err| err.to_string())?;
    on_update(app_handle, None);
    Ok(SecretFieldStatus {
        has_key: false,
        hint: None,
    })
}

fn no_op(_app_handle: &AppHandle, _value: Option<String>) {}

#[tauri::command]
pub fn get_secrets_status() -> SecretsStatus {
    SecretsStatus {
        picovoice: status_for(SecretKey::Picovoice),
        worker_auth: status_for(SecretKey::WorkerAuth),
    }
}

#[tauri::command]
pub fn set_picovoice_key(
    app_handle: AppHandle,
    value: String,
) -> Result<SecretFieldStatus, String> {
    let status = store_secret(
        &app_handle,
        SecretKey::Picovoice,
        &value,
        update_picovoice_runtime,
    )?;
    info!("Picovoice key saved");
    Ok(status)
}

#[tauri::command]
pub fn clear_picovoice_key(app_handle: AppHandle) -> Result<SecretFieldStatus, String> {
    let status = clear_secret(&app_handle, SecretKey::Picovoice, update_picovoice_runtime)?;
    info!("Picovoice key cleared");
    Ok(status)
}

#[tauri::command]
pub fn set_worker_auth_token(
    app_handle: AppHandle,
    value: String,
) -> Result<SecretFieldStatus, String> {
    let status = store_secret(&app_handle, SecretKey::WorkerAuth, &value, no_op)?;
    if let Some(machine_state) =
        app_handle.try_state::<crate::config::machine_state::MachineStateManager>()
    {
        tracing::debug!("Triggering worker sync after auth token set");
        let app_handle_clone = app_handle.clone();
        let machine_state = machine_state.inner().clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = state_sync::sync_worker_state(&app_handle_clone, &machine_state).await
            {
                tracing::warn!("Failed to sync worker state after auth token set: {err}");
            }
        });
    } else {
        tracing::warn!("MachineStateManager not available to sync worker state after auth");
    }
    info!("Worker auth token saved");
    Ok(status)
}

#[tauri::command]
pub fn clear_worker_auth_token(app_handle: AppHandle) -> Result<SecretFieldStatus, String> {
    let status = clear_secret(&app_handle, SecretKey::WorkerAuth, no_op)?;
    info!("Worker auth token cleared");
    Ok(status)
}
