//! Preference snapshot + update commands for the panel allowlist view.

use std::collections::HashMap;

use tauri::{async_runtime, AppHandle, State};
use tracing::{debug, info, warn};

use crate::{
    catalog,
    config::{self, machine_state::MachineStateManager, AppConfig},
    intents::{application::InstalledApp, IntentState},
    machine,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct PreferencesSnapshot {
    pub personality: String,
    #[serde(default)]
    pub allowlist_overrides: HashMap<String, bool>,
    #[serde(default)]
    pub disabled_app_ids: Vec<String>,
    #[serde(default)]
    pub enabled_app_ids: Vec<String>,
    pub default_personality: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowlistUpdateInput {
    pub app_id: String,
    pub enabled: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesUpdatePayload {
    #[serde(default)]
    pub personality: Option<String>,
    #[serde(default)]
    pub allowlist: Vec<AllowlistUpdateInput>,
}

impl From<&config::preferences::PreferencesConfig> for PreferencesSnapshot {
    fn from(preferences: &config::preferences::PreferencesConfig) -> Self {
        let overrides = preferences.allowlist_overrides();
        let mut disabled_app_ids = Vec::new();
        let mut enabled_app_ids = Vec::new();

        for (app_id, enabled) in &overrides {
            if *enabled {
                enabled_app_ids.push(app_id.clone());
            } else {
                disabled_app_ids.push(app_id.clone());
            }
        }

        disabled_app_ids.sort();
        enabled_app_ids.sort();

        Self {
            personality: preferences.personality.clone(),
            allowlist_overrides: overrides,
            disabled_app_ids,
            enabled_app_ids,
            default_personality: config::preferences::PreferencesConfig::default().personality,
        }
    }
}

/// Return the active preferences view model for the panel UI.
#[tauri::command]
pub fn get_preferences_snapshot(
    app_config: State<'_, AppConfig>,
) -> Result<PreferencesSnapshot, String> {
    let preferences_handle = app_config.preferences();
    let snapshot = machine::snapshot_preferences(&preferences_handle)
        .ok_or_else(|| "failed to read preferences".to_string())?;
    Ok(PreferencesSnapshot::from(&snapshot))
}

/// Merge an update payload, persist it, and notify downstream consumers.
#[tauri::command]
pub async fn update_preferences(
    app_handle: AppHandle,
    app_config: State<'_, AppConfig>,
    machine_state: State<'_, MachineStateManager>,
    intent_state: State<'_, IntentState>,
    payload: PreferencesUpdatePayload,
) -> Result<PreferencesSnapshot, String> {
    info!("Applying preferences update");
    let preferences_handle = app_config.preferences();
    let mut current = machine::snapshot_preferences(&preferences_handle)
        .ok_or_else(|| "failed to read preferences".to_string())?;

    if let Some(personality) = payload.personality {
        let trimmed = personality.trim();
        if trimmed.is_empty() {
            current.personality = config::preferences::PreferencesConfig::default().personality;
        } else {
            current.personality = trimmed.to_string();
        }
    }

    let mut overrides = HashMap::with_capacity(payload.allowlist.len());
    for entry in payload.allowlist {
        let id = entry.app_id.trim();
        if id.is_empty() {
            continue;
        }

        overrides.insert(id.to_string(), entry.enabled);
    }
    current.set_allowlist_overrides(overrides);

    current
        .save_to_handle(&app_handle)
        .map_err(|err| err.to_string())?;
    info!("Preferences saved to disk");

    {
        let mut guard = preferences_handle
            .write()
            .map_err(|_| "failed to update preferences state".to_string())?;
        *guard = current.clone();
    }

    let machine_manager = machine_state.inner().clone();
    if let Err(err) = machine::sync_preferences(machine_manager.clone(), current.clone()).await {
        warn!("Failed to sync preferences to worker: {err}");
    }

    let catalog_state = intent_state.inner().clone();
    let mut latest_catalog: Option<Vec<InstalledApp>> = None;
    match async_runtime::spawn_blocking(move || catalog_state.catalog_snapshot()).await {
        Ok(Ok(snapshot)) => {
            latest_catalog = Some(snapshot.applications.clone());
            match catalog::persist_if_changed(&app_handle, &snapshot.applications) {
                Ok(outcome) => {
                    if outcome.changed {
                        catalog::emit_update_event(&app_handle, &outcome.snapshot);
                    }
                    debug!(
                        "Catalog snapshot reloaded after preferences update ({} apps, changed={})",
                        snapshot.applications.len(),
                        outcome.changed
                    );
                }
                Err(err) => {
                    warn!("Failed to persist catalog snapshot after preferences update: {err:?}");
                }
            }
        }
        Ok(Err(err)) => {
            warn!("Failed to load catalog snapshot after preferences update: {err}");
        }
        Err(err) => {
            warn!("Catalog snapshot task failed after preferences update: {err}");
        }
    }

    if let Some(applications) = latest_catalog {
        if let Err(err) =
            machine::sync_catalog(machine_manager, applications, current.clone()).await
        {
            warn!("Failed to sync application catalog after preferences update: {err}");
        }
    }

    info!("Preferences update applied");
    Ok(PreferencesSnapshot::from(&current))
}
