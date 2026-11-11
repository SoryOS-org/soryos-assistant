use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tracing::warn;

use crate::config::ServerId;
use crate::transport::Transport;

use super::error::ClientError;
use super::protocol::{
    InitializeParams, JsonRpcNotification, JsonRpcPayload, JsonRpcRequest, JsonRpcResponse,
    JSONRPC_VERSION, MCP_PROTOCOL_VERSION,
};
use super::types::{
    ClientInfo, InitializeResponse, ResourceDescriptor, ResourceListResponse, ToolListResponse,
    ToolSchema,
};

pub struct McpClient {
    #[allow(dead_code)]
    server_id: ServerId,
    transport: Arc<dyn Transport>,
    next_id: AtomicU64,
}

impl McpClient {
    pub fn new(server_id: ServerId, transport: Arc<dyn Transport>) -> Self {
        Self {
            server_id,
            transport,
            next_id: AtomicU64::new(1),
        }
    }

    fn next_request_id(&self) -> Value {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        Value::from(id)
    }

    pub async fn initialize(
        &self,
        client_info: ClientInfo,
    ) -> Result<InitializeResponse, ClientError> {
        let params = InitializeParams {
            protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            capabilities: json!({}),
            client: client_info,
        };
        let response = self
            .call_method("initialize", serde_json::to_value(params)?)
            .await?;
        serde_json::from_value(response).map_err(ClientError::Deserialize)
    }

    pub async fn list_tools(&self) -> Result<Vec<ToolSchema>, ClientError> {
        let response = self.call_method("tools/list", json!({})).await?;
        serde_json::from_value::<ToolListResponse>(response)
            .map(|payload| payload.tools)
            .map_err(ClientError::Deserialize)
    }

    pub async fn list_resources(&self) -> Result<Vec<ResourceDescriptor>, ClientError> {
        let response = self.call_method("resources/list", json!({})).await?;
        serde_json::from_value::<ResourceListResponse>(response)
            .map(|payload| payload.resources)
            .map_err(ClientError::Deserialize)
    }

    pub async fn ping(&self) -> Result<(), ClientError> {
        self.call_method("ping", json!({})).await?;
        Ok(())
    }

    pub async fn send_notification(&self, method: &str, params: Value) -> Result<(), ClientError> {
        let notification = JsonRpcNotification {
            jsonrpc: JSONRPC_VERSION,
            method,
            params: if params.is_null() { None } else { Some(params) },
        };
        let payload = serde_json::to_vec(&notification)?;
        self.transport.send(&payload).await?;
        Ok(())
    }

    pub async fn call_method(
        &self,
        method: &str,
        params: Value,
    ) -> Result<serde_json::Value, ClientError> {
        let id = self.next_request_id();
        let request = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION,
            id: id.clone(),
            method,
            params: if params.is_null() { None } else { Some(params) },
        };
        let payload = serde_json::to_vec(&request)?;
        self.transport.send(&payload).await?;

        loop {
            let bytes = self.transport.recv().await?;
            let response: JsonRpcResponse =
                serde_json::from_slice(&bytes).map_err(ClientError::Deserialize)?;

            if response.id != id {
                warn!(
                    expected_id = %id,
                    received_id = %response.id,
                    "ignoring stale MCP response id"
                );
                continue;
            }

            match response.payload {
                JsonRpcPayload::Result { result } => break Ok(result),
                JsonRpcPayload::Error(err) => {
                    break Err(ClientError::Rpc {
                        code: err.code,
                        message: err.message,
                        data: err.data,
                    })
                }
            }
        }
    }
}
