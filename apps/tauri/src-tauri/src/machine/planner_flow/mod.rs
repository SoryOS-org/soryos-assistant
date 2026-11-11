use mcp_host::HostHandle;
use tauri::{AppHandle, Manager};

use crate::config::{machine_state::MachineStateManager, WorkerBridgeConfig};
use crate::metrics::{MetricsHandle, TurnTrigger};
use crate::ui::{
    state_router::{self, StateFlow, StateSignal},
    UiVisualState,
};

use super::types::PlannerLaunch;

mod engine;
mod history;
mod ui;

#[cfg(test)]
mod tests;

#[allow(clippy::too_many_arguments)]
pub async fn run_planner_flow(
    app_handle: AppHandle,
    config: WorkerBridgeConfig,
    machine_state: MachineStateManager,
    host_handle: HostHandle,
    session_token: String,
    user_query: String,
    planner: PlannerLaunch,
    planner_context: Option<String>,
    trace_id: Option<String>,
    metrics: Option<MetricsHandle>,
) {
    let metrics_handle = metrics.or_else(|| {
        app_handle
            .try_state::<MetricsHandle>()
            .map(|m| m.inner().clone())
    });
    let mut planner_trace_id = trace_id;
    let mut synthetic_turn: Option<String> = None;

    if planner_trace_id.is_none() {
        if let Some(handle) = metrics_handle.as_ref() {
            let synthetic_id = handle.begin_turn(TurnTrigger::Ptt);
            planner_trace_id = Some(synthetic_id.clone());
            synthetic_turn = Some(synthetic_id);
        }
    }

    state_router::emit(
        &app_handle,
        StateSignal::new(UiVisualState::Planning, StateFlow::Planner),
    );

    let outcome = engine::orchestrate_planner(
        &app_handle,
        &host_handle,
        &config,
        &machine_state,
        &session_token,
        &user_query,
        &planner.server_id,
        &planner.instruction,
        planner_context,
        planner_trace_id.clone(),
        metrics_handle.clone(),
    )
    .await;
    let planner_state = if outcome.ok {
        UiVisualState::Idle
    } else {
        UiVisualState::Error
    };
    state_router::emit(
        &app_handle,
        StateSignal::new(planner_state, StateFlow::Planner),
    );

    if let (Some(handle), Some(turn_id)) = (metrics_handle.as_ref(), synthetic_turn.as_ref()) {
        handle.finalize_turn(turn_id, outcome.ok, outcome.error.as_deref());
    }
}
