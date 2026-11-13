//! Brain entry point — ONE engine only.
//!
//! The legacy worker chat/planner engine was removed during the brain merge:
//! the local Codex app-server is the single brain for both UIs (v1 sprite
//! window and v2 panel) and the voice pipeline. Support modules that Codex
//! does not implement (MCP router, open_app executor, chunking, machine
//! state) live in `crate::machine` and are exploited by the Codex engine
//! through its dynamic tool bridge (`brain/codex.rs`).

use mcp_host::HostHandle;
use tauri::AppHandle;
use tracing::info;

use crate::config::machine_state::MachineStateManager;
use crate::config::{BrainConfig, BrainProvider};
use crate::intents::IntentState;
use crate::machine::ChatWorkerOutcome;

pub mod codex;

/// Run one user request through the single Codex brain.
///
/// The `provider` field is kept in the config schema for forward/backward
/// compatibility, but there is no second engine anymore: any value routes to
/// the Codex engine.
pub async fn run_chat_flow(
    app_handle: AppHandle,
    brain_cfg: BrainConfig,
    state: IntentState,
    machine_state: MachineStateManager,
    host_handle: HostHandle,
    user_query: String,
) -> Result<ChatWorkerOutcome, String> {
    match brain_cfg.provider {
        BrainProvider::Codex => {}
        BrainProvider::Worker => {
            info!("Legacy worker brain retired; routing through the single Codex engine");
        }
    }
    codex::run_codex_flow(
        &app_handle,
        &brain_cfg.codex,
        &state,
        &machine_state,
        &host_handle,
        user_query,
    )
    .await
}
