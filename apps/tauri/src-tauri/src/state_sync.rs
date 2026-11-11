use std::sync::{Arc, RwLock};

use tauri::{AppHandle, Manager, State};
use tracing::{debug, warn};

use crate::config::machine_state::MachineStateManager;
use crate::config::preferences::PreferencesConfig;
use crate::config::AppConfig;
use crate::intents::IntentState;
use crate::{catalog, machine};

/// Snapshot preferences without holding the lock across await points.
pub fn snapshot_preferences(handle: &Arc<RwLock<PreferencesConfig>>) -> Option<PreferencesConfig> {
    handle.read().map(|guard| guard.clone()).ok()
}

/// Warm the catalog, emit updates, and sync preferences + catalog to the worker.
pub fn warm_and_sync_catalog(
    app: &tauri::App,
    machine_state: &MachineStateManager,
    prefs_snapshot: Option<PreferencesConfig>,
) {
    let state: State<'_, IntentState> = app.state();
    if let Ok(Some(cached_snapshot)) = catalog::load_snapshot(app.handle()) {
        debug!(
            "Hydrating intent catalog from cache ({} apps)",
            cached_snapshot.applications().len()
        );
        state.hydrate_catalog(cached_snapshot.applications().to_vec());
    }

    let catalog_snapshot = match state.refresh_catalog() {
        Ok(snapshot) => {
            match catalog::persist_if_changed(app.handle(), &snapshot.applications) {
                Ok(outcome) => {
                    if outcome.changed {
                        catalog::emit_update_event(app.handle(), &outcome.snapshot);
                    }
                }
                Err(err) => warn!("Failed to persist warmed catalog snapshot: {err:?}"),
            }
            Some(snapshot)
        }
        Err(err) => {
            warn!("Failed to warm application catalog: {err}");
            None
        }
    };

    if let Some(prefs) = prefs_snapshot {
        if let Err(err) = tauri::async_runtime::block_on(machine::sync_preferences(
            machine_state.clone(),
            prefs.clone(),
        )) {
            warn!("Failed to sync preferences to worker: {err}");
        }

        if let Some(snapshot) = catalog_snapshot {
            if let Err(err) = tauri::async_runtime::block_on(machine::sync_catalog(
                machine_state.clone(),
                snapshot.applications.clone(),
                prefs,
            )) {
                warn!("Failed to sync initial app catalog to worker: {err}");
            }
        }
    }
}

/// Sync preferences, catalog, and MCP config to the worker after auth is available.
pub async fn sync_worker_state(
    app_handle: &AppHandle,
    machine_state: &MachineStateManager,
) -> Result<(), String> {
    debug!("Syncing worker state after auth update");
    let app_config = app_handle
        .try_state::<AppConfig>()
        .ok_or_else(|| "AppConfig state not available".to_string())?;
    let prefs_handle = app_config.preferences();
    let prefs_snapshot = snapshot_preferences(&prefs_handle)
        .ok_or_else(|| "Failed to read preferences for worker sync".to_string())?;

    if let Err(err) = machine::sync_preferences(machine_state.clone(), prefs_snapshot.clone()).await
    {
        warn!("Failed to sync preferences to worker: {err}");
    }

    if let Some(state) = app_handle.try_state::<IntentState>() {
        let catalog_snapshot = state
            .inner()
            .catalog_snapshot()
            .map_err(|err| format!("Failed to read catalog snapshot: {err}"))?;
        if let Err(err) = machine::sync_catalog(
            machine_state.clone(),
            catalog_snapshot.applications.clone(),
            prefs_snapshot,
        )
        .await
        {
            warn!("Failed to sync app catalog to worker: {err}");
        }
    } else {
        warn!("IntentState not available for worker sync");
    }

    let mcp_handle = app_config.mcp();
    let mcp_value = match mcp_handle.read() {
        Ok(config) => match serde_json::to_value(&*config) {
            Ok(value) => Some(value),
            Err(_) => {
                warn!("Failed to serialize MCP config for worker sync");
                None
            }
        },
        Err(err) => {
            warn!("Failed to read MCP config for worker sync: {err}");
            None
        }
    };

    if let Some(value) = mcp_value {
        if let Err(err) = machine::sync_mcp_config(machine_state.clone(), value).await {
            warn!("Failed to sync MCP configuration to worker: {err}");
        }
    }

    debug!("Worker state sync completed");
    Ok(())
}
