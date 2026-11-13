//! Command handlers for intent dispatch, catalog refresh, and worker round-trips.

use crate::{
    brain,
    catalog,
    config::{machine_state::MachineStateManager, AppConfig},
    intents::{
        IntentBatchResult, IntentCatalogSnapshot, IntentRequest, IntentResponse, IntentState,
    },
    AssistantCommandResponse,
};
use mcp_host::HostHandle;
use tauri::{async_runtime, AppHandle, State};
use tracing::{debug, warn};

/// Dispatch a single intent request through the launcher.
#[tauri::command]
pub async fn handle_intent(
    state: State<'_, IntentState>,
    request: IntentRequest,
) -> Result<IntentResponse, String> {
    let state = state.inner().clone();
    async_runtime::spawn_blocking(move || state.process_intent(request))
        .await
        .map_err(|err| format!("intent dispatcher task failed: {err}"))?
        .map_err(|err| err.to_string())
}

/// Dispatch a batch of intents sequentially, preserving order.
#[tauri::command]
pub async fn handle_intents_batch(
    state: State<'_, IntentState>,
    requests: Vec<IntentRequest>,
) -> Result<IntentBatchResult, String> {
    let state = state.inner().clone();
    async_runtime::spawn_blocking(move || state.process_intents(requests))
        .await
        .map_err(|err| format!("intent batch task failed: {err}"))
}

/// Refresh the installed application catalog and emit UI events if it changed.
#[tauri::command]
pub async fn refresh_application_catalog(
    app_handle: AppHandle,
    state: State<'_, IntentState>,
) -> Result<IntentCatalogSnapshot, String> {
    let state = state.inner().clone();
    debug!("Refreshing application catalog via command");
    let snapshot = async_runtime::spawn_blocking(move || state.refresh_catalog())
        .await
        .map_err(|err| format!("catalog refresh task failed: {err}"))?
        .map_err(|err| err.to_string())?;

    match catalog::persist_if_changed(&app_handle, &snapshot.applications) {
        Ok(outcome) => {
            if outcome.changed {
                catalog::emit_update_event(&app_handle, &outcome.snapshot);
            }
            debug!(
                "Catalog refresh completed ({} apps, changed={})",
                snapshot.applications.len(),
                outcome.changed
            );
        }
        Err(err) => {
            warn!("Failed to persist refreshed catalog snapshot: {err:?}");
            debug!(
                "Catalog refresh returning without persistence ({} apps)",
                snapshot.applications.len()
            );
        }
    }

    Ok(snapshot)
}

/// Return the cached application catalog snapshot without refreshing.
#[tauri::command]
pub async fn get_application_catalog(
    state: State<'_, IntentState>,
) -> Result<IntentCatalogSnapshot, String> {
    let state = state.inner().clone();
    async_runtime::spawn_blocking(move || state.catalog_snapshot())
        .await
        .map_err(|err| format!("catalog snapshot task failed: {err}"))?
        .map_err(|err| err.to_string())
}

/// Invoke the single Codex brain for a user request (v1 + v2 UIs).
#[tauri::command]
pub async fn process_user_request(
    app_handle: AppHandle,
    app_config: State<'_, AppConfig>,
    state: State<'_, IntentState>,
    machine_state: State<'_, MachineStateManager>,
    host_handle: State<'_, HostHandle>,
    user_query: String,
) -> Result<AssistantCommandResponse, String> {
    let brain_cfg = app_config.brain();
    let outcome = brain::run_chat_flow(
        app_handle.clone(),
        brain_cfg,
        state.inner().clone(),
        machine_state.inner().clone(),
        host_handle.inner().clone(),
        user_query.clone(),
    )
    .await?;

    Ok(outcome.response)
}
