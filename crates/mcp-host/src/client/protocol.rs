use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::ClientInfo;

pub(crate) const JSONRPC_VERSION: &str = "2.0";
pub(crate) const MCP_PROTOCOL_VERSION: &str = "1.0";

#[derive(Debug, Serialize)]
pub(crate) struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub(crate) protocol_version: String,
    #[serde(rename = "capabilities")]
    pub(crate) capabilities: Value,
    #[serde(rename = "clientInfo")]
    pub(crate) client: ClientInfo,
}

#[derive(Debug, Serialize)]
pub(crate) struct JsonRpcRequest<'a> {
    pub(crate) jsonrpc: &'static str,
    pub(crate) id: Value,
    pub(crate) method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub(crate) struct JsonRpcNotification<'a> {
    pub(crate) jsonrpc: &'static str,
    pub(crate) method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) params: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcResponse {
    #[allow(dead_code)]
    pub(crate) jsonrpc: String,
    pub(crate) id: Value,
    #[serde(flatten)]
    pub(crate) payload: JsonRpcPayload,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum JsonRpcPayload {
    Result { result: Value },
    Error(JsonRpcError),
}

#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcError {
    pub(crate) code: i64,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) data: Option<Value>,
}
