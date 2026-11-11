use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::AssistantCommandResponse;

#[derive(Debug)]
pub struct ChatWorkerOutcome {
    pub response: AssistantCommandResponse,
    pub planner_intent: Option<PlannerLaunch>,
    pub planner_context: Option<String>,
    pub session_token: String,
    pub user_query: String,
}

#[derive(Debug, Clone)]
pub struct PlannerLaunch {
    pub server_id: String,
    pub instruction: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct PlannerExecutionOutcome {
    pub plan: Option<mcp::McpPlan>,
    pub results: Vec<mcp::McpActionResult>,
    pub planner_context: Option<String>,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PlannerResponse {
    pub mcp_plan: mcp::McpPlan,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PlannerHistoryEntry {
    pub step_index: u32,
    pub action: PlannerHistoryAction,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_response: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PlannerHistoryAction {
    pub server_id: String,
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

pub mod mcp {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    /// Planned MCP tool invocation emitted by the worker planner.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct McpAction {
        pub server_id: String,
        pub tool_name: String,
        #[serde(default)]
        pub arguments: Value,
        #[serde(default)]
        pub confirmation_required: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub timeout_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub step_index: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub status_note: Option<String>,
    }

    impl McpAction {
        pub fn arguments_or_default(&self) -> Value {
            if self.arguments.is_null() {
                Value::Object(Default::default())
            } else {
                self.arguments.clone()
            }
        }
    }

    /// Aggregate planning instructions supplied with a worker reply.
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct McpPlan {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub session_token: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub actions: Vec<McpAction>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub reply: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub planner_context: Option<String>,
        #[serde(default)]
        pub reply_speak: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub summary: Option<String>,
    }

    /// Possible execution outcomes sent back to the worker.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum McpActionStatus {
        Completed,
        TransportError,
        Skipped,
        Timeout,
        Cancelled,
        Retry,
    }

    /// Structure used when reporting MCP execution results.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct McpActionResult {
        pub step_index: u32,
        pub status: McpActionStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub output: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub latency_ms: Option<u64>,
    }

    impl McpActionResult {
        pub fn completed(step_index: u32, output: Value, latency_ms: Option<u64>) -> Self {
            Self {
                step_index,
                status: McpActionStatus::Completed,
                output: Some(output),
                error: None,
                latency_ms,
            }
        }

        pub fn transport_error(step_index: u32, message: impl Into<String>) -> Self {
            Self {
                step_index,
                status: McpActionStatus::TransportError,
                output: None,
                error: Some(message.into()),
                latency_ms: None,
            }
        }
    }

    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "snake_case")]
    pub enum McpProgressStatus {
        Queued,
        Running,
        Completed,
        Crashed,
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct McpProgressEvent {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub session_token: Option<String>,
        pub step_index: u32,
        pub server_id: String,
        pub tool_name: String,
        pub status: McpProgressStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub status_note: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub error: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub latency_ms: Option<u64>,
    }
}
