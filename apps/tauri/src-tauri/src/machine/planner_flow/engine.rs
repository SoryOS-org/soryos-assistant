use std::time::{Duration, Instant};

use mcp_host::HostHandle;
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;
use tracing::{debug, error, info, info_span, warn};

use crate::config::{machine_state::MachineStateManager, WorkerBridgeConfig};
use crate::machine::{EVENT_WORKER_AUTH_FAILED, EVENT_WORKER_AUTH_OK};
use crate::mcp_log::{self, McpDirection};
use crate::metrics::MetricsHandle;
use crate::worker_log;

use super::super::chunking::{
    truncate_mcp_output, ChunkStore, INTERNAL_CHUNK_FETCH_TOOL, INTERNAL_CHUNK_SERVER_ID,
    INTERNAL_TEXT_FIND_TOOL,
};
use super::super::mcp;
use super::super::sync::current_timestamp;
use super::super::types::{PlannerExecutionOutcome, PlannerHistoryEntry, PlannerResponse};
use super::super::worker_client::{self, backoff_delay, build_http_client, post_json_with_retry};
use super::ui::broadcast_reply_if_new;

const MAX_PLANNER_ITERATIONS: u32 = 20;

#[allow(clippy::too_many_arguments)]
pub(super) async fn orchestrate_planner(
    app_handle: &AppHandle,
    host_handle: &HostHandle,
    config: &WorkerBridgeConfig,
    machine_state: &MachineStateManager,
    session_token: &str,
    user_query: &str,
    planner_server_id: &str,
    planner_instruction: &str,
    planner_context: Option<String>,
    trace_id: Option<String>,
    metrics: Option<MetricsHandle>,
) -> PlannerExecutionOutcome {
    let client = match build_http_client() {
        Ok(client) => client,
        Err(err) => {
            error!("failed to build planner HTTP client: {err}");
            return PlannerExecutionOutcome {
                plan: None,
                results: Vec::new(),
                planner_context: planner_context.clone(),
                ok: false,
                error: Some(format!("failed to build planner HTTP client: {err}")),
            };
        }
    };
    let planner_endpoint = worker_client::planner_endpoint(config);
    debug!("Requesting MCP planner at {planner_endpoint}");

    let retry_policy = config.retry_policy();
    let session_token = session_token.to_string();
    let mut history_entries: Vec<PlannerHistoryEntry> = Vec::new();
    let mut recorded_actions: Vec<mcp::McpAction> = Vec::new();
    let mut results: Vec<mcp::McpActionResult> = Vec::new();
    let mut reply_updates: Vec<String> = Vec::new();
    let mut next_step_index: u32 = 0;
    let mut final_plan: Option<mcp::McpPlan> = None;
    let mut chunk_store = ChunkStore::default();
    let mut last_broadcast_reply: Option<String> = None;
    let mut last_reply_speak = false;
    let mut active_context = planner_context;
    let mut had_error = false;
    let mut last_error: Option<String> = None;

    for iteration in 0..MAX_PLANNER_ITERATIONS {
        let mut planner_body = serde_json::json!({
            "query": user_query,
            "machine_id": machine_state.machine_id(),
            "instruction": planner_instruction,
            "mcp_server": planner_server_id,
        });

        if let Some(summary) = active_context.as_deref() {
            planner_body["conversation_memory"] = serde_json::json!({
                "type": "summary",
                "summary": summary,
                "lastUpdated": current_timestamp(),
            });
        }

        planner_body["session_token"] = serde_json::json!(session_token);
        if !history_entries.is_empty() {
            planner_body["history"] =
                serde_json::to_value(&history_entries).unwrap_or(serde_json::Value::Null);
        }

        worker_log::log_planner_request(Some(&session_token), planner_body.clone());

        let planner_reply = match post_json_with_retry::<PlannerResponse>(
            &client,
            &planner_endpoint,
            &planner_body,
            config,
            "Planner request payload",
        )
        .await
        {
            Ok(payload) => payload,
            Err(err) => {
                warn!("Planner request ultimately failed: {err}");
                emit_auth_failure(app_handle, &planner_endpoint, &err);
                let fallback_reply = "I couldn't reach the planner service. Want me to try again?";
                broadcast_reply_if_new(
                    app_handle,
                    metrics.as_ref(),
                    trace_id.as_deref(),
                    fallback_reply,
                    true,
                    None,
                    &mut last_broadcast_reply,
                );
                reply_updates.push(fallback_reply.to_string());
                last_reply_speak = true;
                final_plan = Some(mcp::McpPlan {
                    session_token: Some(session_token.clone()),
                    actions: recorded_actions.clone(),
                    reply: reply_updates.last().cloned(),
                    reply_speak: last_reply_speak,
                    summary: None,
                    planner_context: active_context.clone(),
                });
                had_error = true;
                last_error = Some(format!("planner request failed: {err}"));
                break;
            }
        };
        emit_auth_success(app_handle, &planner_endpoint);

        if let Ok(value) = serde_json::to_value(&planner_reply) {
            worker_log::log_planner_response(Some(&session_token), value);
        }

        sleep(Duration::from_secs(1)).await;

        let mut plan = planner_reply.mcp_plan;
        plan.session_token = Some(session_token.clone());

        let actions_empty = plan.actions.is_empty();

        if let Some(reply) = plan.reply.clone().filter(|r| !r.trim().is_empty()) {
            let speak = if actions_empty {
                true
            } else {
                plan.reply_speak
            };
            broadcast_reply_if_new(
                app_handle,
                metrics.as_ref(),
                trace_id.as_deref(),
                reply.trim(),
                speak,
                plan.planner_context.as_deref(),
                &mut last_broadcast_reply,
            );
            reply_updates.push(reply);
            last_reply_speak = speak;
        }

        if let Some(context) = plan
            .planner_context
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            active_context = Some(context.to_string());
        }

        if actions_empty {
            info!("Planner returned empty action list; finishing orchestration");
            final_plan = Some(mcp::McpPlan {
                session_token: Some(session_token.clone()),
                actions: recorded_actions.clone(),
                reply: reply_updates.last().cloned(),
                reply_speak: last_reply_speak,
                summary: None,
                planner_context: active_context.clone(),
            });
            break;
        }

        let mut should_replan = false;

        for mut action in plan.actions.into_iter() {
            let step_index = ensure_step_index(&mut next_step_index, action.step_index);
            action.step_index = Some(step_index);

            if let Ok(value) = serde_json::to_value(&action) {
                worker_log::log_mcp_command(Some(&session_token), Some(step_index), value);
            }

            emit_progress_event(
                app_handle,
                &mcp::McpProgressEvent {
                    session_token: Some(session_token.clone()),
                    step_index,
                    server_id: action.server_id.clone(),
                    tool_name: action.tool_name.clone(),
                    status: mcp::McpProgressStatus::Queued,
                    status_note: action.status_note.clone(),
                    error: None,
                    latency_ms: None,
                },
            );

            emit_progress_event(
                app_handle,
                &mcp::McpProgressEvent {
                    session_token: Some(session_token.clone()),
                    step_index,
                    server_id: action.server_id.clone(),
                    tool_name: action.tool_name.clone(),
                    status: mcp::McpProgressStatus::Running,
                    status_note: action.status_note.clone(),
                    error: None,
                    latency_ms: None,
                },
            );

            let mut attempt: u32 = 0;
            let mut action_result;

            loop {
                attempt += 1;
                let started_at = Instant::now();
                let context = mcp_host::host::ToolCallContext {
                    trace_id: None,
                    session_token: Some(session_token.clone()),
                    step_index: Some(step_index),
                };
                let arguments = action.arguments_or_default();

                mcp_log::log_tool_event(
                    Some(&session_token),
                    Some(step_index),
                    &action.server_id,
                    &action.tool_name,
                    McpDirection::Request,
                    arguments.clone(),
                );

                let call_future = async {
                    if action.server_id == INTERNAL_CHUNK_SERVER_ID {
                        match action.tool_name.as_str() {
                            INTERNAL_CHUNK_FETCH_TOOL => chunk_store.fetch_chunk(arguments.clone()),
                            INTERNAL_TEXT_FIND_TOOL => chunk_store.find_text(arguments.clone()),
                            other => Err(format!("Unsupported sprite_internal tool '{}'", other)),
                        }
                    } else {
                        host_handle
                            .call_tool_with_context(
                                &action.server_id,
                                &action.tool_name,
                                arguments.clone(),
                                context.clone(),
                            )
                            .await
                            .map_err(|err| err.to_string())
                    }
                };
                let call_result: Result<Value, String> = info_span!(
                    "mcp_action",
                    server = %action.server_id,
                    tool = %action.tool_name,
                    step_index
                )
                .in_scope(|| call_future)
                .await;

                info!(
                    step = step_index,
                    server = %action.server_id,
                    tool = %action.tool_name,
                    attempt,
                    "Planner action attempt completed"
                );

                match call_result {
                    Ok(output) => {
                        let latency_ms = started_at.elapsed().as_millis() as u64;
                        if let Some(error_msg) = extract_tool_error(&output) {
                            let mut failure =
                                mcp::McpActionResult::transport_error(step_index, error_msg);
                            failure.latency_ms = Some(latency_ms);
                            failure.output = Some(output);
                            action_result = failure;
                            mcp_log::log_tool_event(
                                Some(&session_token),
                                Some(step_index),
                                &action.server_id,
                                &action.tool_name,
                                McpDirection::Response,
                                tool_response_payload(&action_result),
                            );
                            break;
                        }
                        action_result =
                            mcp::McpActionResult::completed(step_index, output, Some(latency_ms));
                        mcp_log::log_tool_event(
                            Some(&session_token),
                            Some(step_index),
                            &action.server_id,
                            &action.tool_name,
                            McpDirection::Response,
                            tool_response_payload(&action_result),
                        );
                        info!(
                            step = step_index,
                            server = %action.server_id,
                            tool = %action.tool_name,
                            latency_ms,
                            "Planner action succeeded"
                        );
                        break;
                    }
                    Err(err) => {
                        let latency_ms = started_at.elapsed().as_millis() as u64;
                        warn!(
                            server = %action.server_id,
                            tool = %action.tool_name,
                            step = step_index,
                            attempt,
                            "MCP tool call failed: {err}"
                        );
                        let mut failure =
                            mcp::McpActionResult::transport_error(step_index, err.to_string());
                        failure.latency_ms = Some(latency_ms);
                        action_result = failure;
                        mcp_log::log_tool_event(
                            Some(&session_token),
                            Some(step_index),
                            &action.server_id,
                            &action.tool_name,
                            McpDirection::Response,
                            tool_response_payload(&action_result),
                        );
                        if attempt >= retry_policy.max_attempts() {
                            break;
                        }

                        let delay = backoff_delay(retry_policy, attempt);
                        sleep(delay).await;
                        continue;
                    }
                }
            }

            let is_success = matches!(action_result.status, mcp::McpActionStatus::Completed);

            emit_progress_event(
                app_handle,
                &mcp::McpProgressEvent {
                    session_token: Some(session_token.clone()),
                    step_index,
                    server_id: action.server_id.clone(),
                    tool_name: action.tool_name.clone(),
                    status: if is_success {
                        mcp::McpProgressStatus::Completed
                    } else {
                        mcp::McpProgressStatus::Crashed
                    },
                    status_note: action.status_note.clone(),
                    error: action_result.error.clone(),
                    latency_ms: action_result.latency_ms,
                },
            );

            if let Ok(mut value) = serde_json::to_value(&action_result) {
                truncate_mcp_output(&mut value);
                worker_log::log_mcp_result(Some(&session_token), Some(step_index), value);
            }

            let chunked_response = action_result.output.as_ref().and_then(|output| {
                chunk_store.chunk_large_fields(&session_token, step_index, output)
            });
            history_entries.push(PlannerHistoryEntry::from_execution(
                &action,
                &action_result,
                chunked_response,
            ));
            recorded_actions.push(action.clone());
            results.push(action_result.clone());

            if !is_success {
                had_error = true;
                last_error = action_result
                    .error
                    .clone()
                    .or_else(|| Some("planner action failed".to_string()));
                should_replan = true;
                break;
            }
        }

        if should_replan {
            continue;
        }

        debug!(
            iteration,
            "Planner iteration completed; requesting next actions"
        );
    }

    if final_plan.is_none() && (!recorded_actions.is_empty() || !reply_updates.is_empty()) {
        final_plan = Some(mcp::McpPlan {
            session_token: Some(session_token.clone()),
            actions: recorded_actions.clone(),
            reply: reply_updates.last().cloned(),
            reply_speak: last_reply_speak,
            summary: None,
            planner_context: active_context.clone(),
        });
    }

    chunk_store.clear();

    PlannerExecutionOutcome {
        plan: final_plan,
        results,
        planner_context: active_context,
        ok: !had_error,
        error: last_error,
    }
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

pub(super) fn ensure_step_index(counter: &mut u32, provided: Option<u32>) -> u32 {
    if let Some(step) = provided {
        if step >= *counter {
            *counter = step.saturating_add(1);
            return step;
        }

        let assigned = *counter;
        *counter = counter.saturating_add(1);
        return assigned;
    }

    let current = *counter;
    *counter = counter.saturating_add(1);
    current
}

fn emit_progress_event(app_handle: &AppHandle, event: &mcp::McpProgressEvent) {
    if let Err(err) = app_handle.emit("assistant://mcp-progress", event) {
        warn!("Failed to emit MCP progress event: {err}");
    }
}

fn extract_tool_error(value: &Value) -> Option<String> {
    let is_error = value
        .get("isError")
        .and_then(|flag| flag.as_bool())
        .unwrap_or(false);
    if !is_error {
        return None;
    }

    if let Some(content) = value.get("content").and_then(|c| c.as_array()) {
        for entry in content {
            if let Some(text) = entry
                .get("text")
                .and_then(|t| t.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                return Some(text.to_string());
            }
        }
    }

    value
        .get("error")
        .and_then(|err| err.as_str())
        .map(|s| s.to_string())
        .or_else(|| Some("Tool returned an error response".to_string()))
}

fn tool_response_payload(result: &mcp::McpActionResult) -> Value {
    let mut map = Map::new();
    if let Some(output) = result.output.clone() {
        map.insert("output".to_string(), output);
    }
    if let Some(error) = result.error.clone() {
        map.insert("error".to_string(), Value::String(error));
    }
    if let Some(latency) = result.latency_ms {
        map.insert("latency_ms".to_string(), Value::Number(latency.into()));
    }
    Value::Object(map)
}
