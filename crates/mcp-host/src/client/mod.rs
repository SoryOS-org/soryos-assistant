mod core;
mod error;
mod protocol;
mod types;

pub use core::McpClient;
pub use error::ClientError;
pub use types::{ClientInfo, InitializeResponse, ResourceDescriptor, ServerInfo, ToolSchema};

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    use super::{ClientInfo, McpClient};
    use crate::transport::{Transport, TransportError};
    use serde_json::{json, Value};

    struct MockTransport {
        sent: Mutex<Vec<Vec<u8>>>,
        responses: Mutex<Vec<Vec<u8>>>,
    }

    impl MockTransport {
        fn with_response(responses: Vec<Value>) -> Self {
            let payloads = responses
                .into_iter()
                .map(|value| serde_json::to_vec(&value).expect("serialize response"))
                .collect();
            Self {
                sent: Mutex::new(Vec::new()),
                responses: Mutex::new(payloads),
            }
        }
    }

    #[async_trait]
    impl Transport for MockTransport {
        async fn send(&self, payload: &[u8]) -> Result<(), TransportError> {
            self.sent.lock().await.push(payload.to_vec());
            Ok(())
        }

        async fn recv(&self) -> Result<Vec<u8>, TransportError> {
            self.responses
                .lock()
                .await
                .pop()
                .ok_or(TransportError::NotInitialised)
        }

        async fn shutdown(&self) -> Result<(), TransportError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn initialize_parses_response() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "protocolVersion": "1.0",
                "serverInfo": { "name": "test", "version": "0.1.0" },
                "capabilities": { "tools": true }
            }
        });

        let transport = Arc::new(MockTransport::with_response(vec![response]));
        let client = McpClient::new("server".into(), transport);
        let result = client
            .initialize(ClientInfo {
                name: "sprite".into(),
                version: Some("1.0.0".into()),
            })
            .await
            .expect("initialize");

        assert_eq!(result.server_info.name, "test");
        assert_eq!(result.server_info.version.as_deref(), Some("0.1.0"));
        assert_eq!(result.protocol_version.as_deref(), Some("1.0"));
    }

    #[tokio::test]
    async fn ignores_stale_ids_and_uses_matching_response() {
        let stale = json!({
            "jsonrpc": "2.0",
            "id": 999,
            "result": {}
        });
        let result = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "ok": true
            }
        });
        let transport = Arc::new(MockTransport::with_response(vec![result, stale]));
        let client = McpClient::new("server".into(), transport);

        let response = client
            .call_method("ping", Value::Null)
            .await
            .expect("expected successful response");

        assert_eq!(response, json!({"ok": true}));
    }
}
