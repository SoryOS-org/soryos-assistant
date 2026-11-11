use std::collections::HashMap;

use once_cell::sync::Lazy;
use tauri::{AppHandle, Manager};
use tracing::{debug, warn};

use super::{UiEventService, UiVisualState};

static FLOW_STATE: Lazy<std::sync::Mutex<HashMap<StateFlow, UiVisualState>>> =
    Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

/// High-level source categories for visual state signals.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StateFlow {
    Stt,
    Tts,
    Planner,
    Worker,
}

/// Unified state signal dispatched through the central router.
#[derive(Debug, Clone)]
pub struct StateSignal {
    pub state: UiVisualState,
    pub flow: StateFlow,
}

impl StateSignal {
    pub fn new(state: UiVisualState, flow: StateFlow) -> Self {
        Self { state, flow }
    }
}

/// Central entry point for emitting sprite UI state (and later, other sinks).
pub fn emit(app_handle: &AppHandle, signal: StateSignal) {
    debug!(flow = ?signal.flow, state = ?signal.state, "state router dispatch");

    let mut flows = match FLOW_STATE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("state router mutex poisoned; recovering previous state");
            poisoned.into_inner()
        }
    };

    match signal.state {
        UiVisualState::Idle | UiVisualState::Error => {
            flows.remove(&signal.flow);
        }
        _ => {
            flows.insert(signal.flow.clone(), signal.state);
        }
    }

    let resolved_state = resolve(flows.values());

    let open_flows: Vec<StateFlow> = flows.keys().cloned().collect();
    debug!(?open_flows, resolved_state = ?resolved_state, "state router resolved state");

    if let Some(service) = app_handle.try_state::<UiEventService>() {
        service.sender().emit_state(resolved_state);
    }
}

/// Remove a flow without emitting a specific state (useful when closing on success).
pub fn clear_flow(app_handle: &AppHandle, flow: StateFlow) {
    let mut flows = match FLOW_STATE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("state router mutex poisoned during clear_flow; recovering");
            poisoned.into_inner()
        }
    };
    flows.remove(&flow);

    let resolved_state = resolve(flows.values());
    let open_flows: Vec<StateFlow> = flows.keys().cloned().collect();
    debug!(flow = ?flow, ?open_flows, resolved_state = ?resolved_state, "state router cleared flow");

    if let Some(service) = app_handle.try_state::<UiEventService>() {
        service.sender().emit_state(resolved_state);
    }
}

/// Clear a flow without emitting a resolved state (use when handing off to another flow to avoid Idle).
pub fn clear_flow_silent(flow: StateFlow) {
    let mut flows = match FLOW_STATE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("state router mutex poisoned during silent clear; recovering");
            poisoned.into_inner()
        }
    };
    flows.remove(&flow);
    let open_flows: Vec<StateFlow> = flows.keys().cloned().collect();
    debug!(flow = ?flow, ?open_flows, "state router cleared flow silently");
}

pub fn is_busy_for_stt() -> bool {
    let flows = match FLOW_STATE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("state router mutex poisoned during busy check; recovering");
            poisoned.into_inner()
        }
    };

    flows.iter().any(|(flow, state)| {
        *flow != StateFlow::Stt
            && matches!(
                state,
                UiVisualState::Thinking | UiVisualState::Speaking | UiVisualState::Planning
            )
    })
}

fn resolve<'a>(states: impl Iterator<Item = &'a UiVisualState>) -> UiVisualState {
    // Collect once to allow multiple passes without Clone bound.
    let collected: Vec<UiVisualState> = states.copied().collect();

    if collected.iter().any(|s| matches!(s, UiVisualState::Error)) {
        return UiVisualState::Error;
    }

    collected
        .into_iter()
        .max_by_key(|state| priority(*state))
        .unwrap_or(UiVisualState::Idle)
}

fn priority(state: UiVisualState) -> u8 {
    match state {
        UiVisualState::Error => 5,
        UiVisualState::Speaking => 4,
        UiVisualState::Planning => 3,
        UiVisualState::Thinking => 2,
        UiVisualState::Listening => 1,
        UiVisualState::Idle => 0,
    }
}
