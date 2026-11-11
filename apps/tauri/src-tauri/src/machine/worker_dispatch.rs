use std::sync::{Arc, RwLock};

use mcp_host::HostHandle;
use tauri::{async_runtime, AppHandle, Emitter};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::{self, machine_state::MachineStateManager, WorkerBridgeConfig};
use crate::intents::{IntentRequest, IntentState};
use crate::metrics::MetricsHandle;
use crate::ui::{
    state_router::{self, StateFlow, StateSignal},
    UiVisualState,
};
use crate::{worker_log, AssistantCommandResponse, AssistantIntent, SpeechPlan};

use super::types::{ChatWorkerOutcome, PlannerLaunch};
use super::worker_client::{self, build_http_client, post_json_with_retry};
use crate::machine::{EVENT_WORKER_AUTH_FAILED, EVENT_WORKER_AUTH_OK};

#[allow(clippy::too_many_arguments)]
pub async fn run_worker_flow(
    app_handle: AppHandle,
    config: WorkerBridgeConfig,
    state: IntentState,
    machine_state: MachineStateManager,
    _host_handle: HostHandle,
    _preferences: Arc<RwLock<config::preferences::PreferencesConfig>>,
    user_query: String,
    _trace_id: Option<String>,
    _metrics: Option<MetricsHandle>,
) -> Result<ChatWorkerOutcome, String> {
    let endpoint = worker_client::chat_endpoint(&config);
    debug!(
        "Dispatching transcript to worker endpoint {} ({} chars)",
        endpoint,
        user_query.len()
    );

    let client = build_http_client()?;
    state_router::emit(
        &app_handle,
        StateSignal::new(UiVisualState::Thinking, StateFlow::Worker),
    );

    let session_token = new_session_token();
    let request_body = build_chat_request(&user_query, &session_token, &machine_state);

    worker_log::log_chat_request(Some(&session_token), &user_query, request_body.clone());

    let worker_reply: crate::WorkerReply = match post_json_with_retry(
        &client,
        &endpoint,
        &request_body,
        &config,
        "Worker request payload",
    )
    .await
    {
        Ok(reply) => reply,
        Err(err) => {
            emit_auth_failure(&app_handle, &endpoint, &err);
            state_router::emit(
                &app_handle,
                StateSignal::new(UiVisualState::Error, StateFlow::Worker),
            );
            return Err(err);
        }
    };
    emit_auth_success(&app_handle, &endpoint);
    if let Ok(value) = serde_json::to_value(&worker_reply) {
        worker_log::log_worker_response(Some(&session_token), value);
    }

    let crate::WorkerReply {
        reply,
        summary: _,
        planner_context,
        intents,
        reply_speak: _,
    } = worker_reply;
    info!("Worker response received ({} intents)", intents.len());
    debug!("Worker reply text: {}", reply);

    let (launch_requests, planner_request) = split_worker_intents(&intents);

    log_launch_requests(&state, &launch_requests);
    let dispatch_result = dispatch_launch_requests(state, launch_requests).await?;

    let speech_plan = SpeechPlan::with_initial(reply.clone());
    let planner_intent = planner_request
        .clone()
        .map(|(server_id, instruction)| PlannerLaunch {
            server_id,
            instruction,
        });
    let reply_streamed = false;

    let outcome = ChatWorkerOutcome {
        response: AssistantCommandResponse {
            reply,
            speech: speech_plan,
            intents,
            mcp_plan: None,
            mcp_action_results: Vec::new(),
            mcp_session_token: Some(session_token.clone()),
            dispatch_result,
            reply_streamed,
        },
        planner_intent,
        planner_context,
        session_token,
        user_query,
    };
    state_router::emit(
        &app_handle,
        StateSignal::new(UiVisualState::Idle, StateFlow::Worker),
    );
    Ok(outcome)
}

fn emit_auth_failure(app_handle: &AppHandle, endpoint: &str, error: &str) {
    if !worker_client::is_auth_error_message(error) {
        return;
    }
    #[derive(Clone, serde::Serialize)]
    struct WorkerAuthFailureEvent<'a> {
        endpoint: &'a str,
        status: Option<u16>,
        message: &'a str,
    }
    let payload = WorkerAuthFailureEvent {
        endpoint,
        status: worker_client::auth_error_status(error),
        message: error,
    };
    let _ = app_handle.emit(EVENT_WORKER_AUTH_FAILED, payload);
}

fn emit_auth_success(app_handle: &AppHandle, endpoint: &str) {
    #[derive(Clone, serde::Serialize)]
    struct WorkerAuthOkEvent<'a> {
        endpoint: &'a str,
    }
    let _ = app_handle.emit(EVENT_WORKER_AUTH_OK, WorkerAuthOkEvent { endpoint });
}

fn new_session_token() -> String {
    Uuid::new_v4().to_string()
}

fn build_chat_request(
    user_query: &str,
    session_token: &str,
    machine_state: &MachineStateManager,
) -> serde_json::Value {
    serde_json::json!({
        "query": user_query,
        "machine_id": machine_state.machine_id(),
        "session_token": session_token,
    })
}

fn split_worker_intents(
    intents: &[AssistantIntent],
) -> (Vec<IntentRequest>, Option<(String, String)>) {
    let mut launch_requests = Vec::new();
    let mut planner_request = None;

    for intent in intents {
        match intent {
            AssistantIntent::OpenApp { app_name } => {
                let cleaned = app_name.trim().trim_end_matches('.').trim();
                info!(
                    "Opening app: \"{}\" (cleaned from \"{}\")",
                    cleaned, app_name
                );
                if cleaned.is_empty() {
                    warn!("App name is empty after cleaning, skipping");
                } else {
                    launch_requests.push(IntentRequest::LaunchApplication {
                        query: cleaned.to_string(),
                        strict: false,
                        refresh_catalog: false,
                        focus: false,
                    });
                }
            }
            AssistantIntent::OpenAppNotFound { .. } => {
                debug!("Worker reported open_app_not_found intent");
            }
            AssistantIntent::McpPlan {
                mcp_server,
                instruction,
            } => {
                planner_request = Some((mcp_server.clone(), instruction.clone()));
            }
            AssistantIntent::Converse => {}
        }
    }

    (launch_requests, planner_request)
}

fn log_launch_requests(state: &IntentState, launch_requests: &[IntentRequest]) {
    if launch_requests.is_empty() {
        return;
    }

    info!("Launch requests: {}", launch_requests.len());
    for request in launch_requests {
        debug!("Launch request: {:?}", request);
    }

    match state.catalog_snapshot() {
        Ok(catalog) => {
            debug!("Available apps in catalog: {}", catalog.applications.len());
            for app in &catalog.applications {
                if app.name.to_lowercase().contains("chrome")
                    || app.name.to_lowercase().contains("google")
                {
                    debug!("Catalog candidate: {} (ID: {:?})", app.name, app.app_id);
                }
            }
        }
        Err(err) => {
            warn!("Failed to get catalog snapshot: {err}");
        }
    }
}

async fn dispatch_launch_requests(
    state: IntentState,
    launch_requests: Vec<IntentRequest>,
) -> Result<Option<crate::intents::IntentBatchResult>, String> {
    if launch_requests.is_empty() {
        return Ok(None);
    }

    let state_clone = state.clone();
    let result =
        async_runtime::spawn_blocking(move || state_clone.process_intents(launch_requests))
            .await
            .map_err(|err| format!("intent batch task failed: {err}"))?;
    info!("Intent dispatch result: {:?}", result);
    Ok(Some(result))
}
