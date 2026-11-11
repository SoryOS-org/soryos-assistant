use serde_json::Value;

use super::super::mcp;
use super::super::types::{PlannerHistoryAction, PlannerHistoryEntry};

impl PlannerHistoryEntry {
    pub(crate) fn from_execution(
        action: &mcp::McpAction,
        result: &mcp::McpActionResult,
        response_override: Option<Value>,
    ) -> Self {
        let raw_arguments = action.arguments_or_default();
        let arguments = if matches!(raw_arguments, Value::Object(ref map) if map.is_empty()) {
            None
        } else {
            Some(raw_arguments)
        };

        Self {
            step_index: result.step_index,
            action: PlannerHistoryAction {
                server_id: action.server_id.clone(),
                tool_name: action.tool_name.clone(),
                arguments,
            },
            status: planner_status_label(&result.status).to_string(),
            mcp_response: response_override.or_else(|| {
                result
                    .output
                    .clone()
                    .or_else(|| result.error.as_ref().map(|err| Value::String(err.clone())))
            }),
            error: result.error.clone(),
            latency_ms: result.latency_ms,
        }
    }
}

pub(super) fn planner_status_label(status: &mcp::McpActionStatus) -> &'static str {
    match status {
        mcp::McpActionStatus::Completed => "completed",
        mcp::McpActionStatus::TransportError => "transport_error",
        mcp::McpActionStatus::Skipped => "skipped",
        mcp::McpActionStatus::Timeout => "timeout",
        mcp::McpActionStatus::Cancelled => "cancelled",
        mcp::McpActionStatus::Retry => "retry",
    }
}
