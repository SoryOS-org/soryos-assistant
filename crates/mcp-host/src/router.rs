use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;
use tokio::sync::RwLock;
use tracing::debug;
use uuid::Uuid;

use crate::client::{ClientError, McpClient};
use crate::config::{ConfigState, PolicyConfig, ServerConfig, ServerId};
use crate::registry::Registry;
use crate::sinks::HostSinks;

#[derive(Clone)]
pub struct Router {
    config: ConfigState,
    registry: Registry,
    _sinks: HostSinks,
    clients: Arc<RwLock<HashMap<ServerId, Arc<McpClient>>>>,
}

impl Router {
    pub fn new(config: ConfigState, registry: Registry, sinks: HostSinks) -> Self {
        Self {
            config,
            registry,
            _sinks: sinks,
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_client(&self, server_id: ServerId, client: Arc<McpClient>) {
        let mut guard = self.clients.write().await;
        guard.insert(server_id, client);
    }

    pub async fn remove_client(&self, server_id: &str) {
        let mut guard = self.clients.write().await;
        guard.remove(server_id);
    }

    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        trace_id: Option<Uuid>,
    ) -> Result<serde_json::Value, RouterError> {
        let server_key = server_id.to_string();
        let entry = self
            .registry
            .entry(&server_key)
            .await
            .ok_or_else(|| RouterError::ServerNotRegistered(server_key.clone()))?;

        self.ensure_tool_allowed(server_id, tool_name, &entry.config)?;
        if !entry.tools.iter().any(|tool| tool.name == tool_name) {
            debug!(
                server_id = %server_id,
                tool = %tool_name,
                "Tool not advertised in registry; proceeding due to policy allowance"
            );
        }

        let client = {
            let guard = self.clients.read().await;
            guard
                .get(&server_key)
                .cloned()
                .ok_or_else(|| RouterError::ClientUnavailable(server_key.clone()))?
        };

        debug!(
            server_id = %server_id,
            tool = %tool_name,
            trace = ?trace_id,
            "Dispatching MCP tool call"
        );

        let payload = serde_json::json!({
            "name": tool_name,
            "arguments": arguments,
        });

        let global_policy = self.config.policy_snapshot();
        let timeout = call_timeout(&entry.config.policy, &global_policy);
        let result = if let Some(duration) = timeout {
            match tokio::time::timeout(duration, client.call_method("tools/call", payload)).await {
                Ok(res) => res,
                Err(_) => return Err(RouterError::Timeout { duration }),
            }
        } else {
            client.call_method("tools/call", payload).await
        }?;

        Ok(result)
    }

    fn ensure_tool_allowed(
        &self,
        server_id: &str,
        tool_name: &str,
        config: &ServerConfig,
    ) -> Result<(), RouterError> {
        let server_policy = &config.policy;
        if !server_policy.allowed_tools.is_empty()
            && !server_policy.allowed_tools.contains(tool_name)
        {
            return Err(RouterError::ToolNotAllowed {
                server: server_id.to_string(),
                tool: tool_name.to_string(),
            });
        }

        let global_policy = self.config.policy_snapshot();
        if server_policy.allowed_tools.is_empty()
            && !global_policy.allowed_tools.is_empty()
            && !global_policy.allowed_tools.contains(tool_name)
        {
            return Err(RouterError::ToolNotAllowed {
                server: server_id.to_string(),
                tool: tool_name.to_string(),
            });
        }

        Ok(())
    }
}

fn call_timeout(server: &PolicyConfig, global: &PolicyConfig) -> Option<Duration> {
    let ms = if server.call_timeout_ms > 0 {
        server.call_timeout_ms
    } else {
        global.call_timeout_ms
    };

    if ms == 0 {
        None
    } else {
        Some(Duration::from_millis(ms))
    }
}

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("server {0} not registered")]
    ServerNotRegistered(String),
    #[error("no active client registered for server {0}")]
    ClientUnavailable(String),
    #[error("tool {tool} is not allowed for server {server}")]
    ToolNotAllowed { server: String, tool: String },
    #[error("tool call timed out after {duration:?}")]
    Timeout { duration: Duration },
    #[error("client error: {0}")]
    Client(#[from] ClientError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{McpClient, ServerInfo, ToolSchema};
    use crate::config::{HostConfig, PolicyConfig, ServerConfig, TransportConfig};
    use crate::registry::Registry;
    use crate::transport::{Transport, TransportError};
    use anyhow::Error;
    use serde_json::json;
    use tokio::sync::Mutex;
    use tracing::Span;

    struct MockTransport {
        sent: Mutex<Vec<Vec<u8>>>,
        responses: Mutex<Vec<Vec<u8>>>,
    }

    impl MockTransport {
        fn with_response(response: serde_json::Value) -> Self {
            Self {
                sent: Mutex::new(Vec::new()),
                responses: Mutex::new(vec![serde_json::to_vec(&response).unwrap()]),
            }
        }
    }

    #[async_trait::async_trait]
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
    async fn rejects_unknown_server() {
        let mut host_config = HostConfig::default();
        host_config.servers.insert(
            "server".into(),
            ServerConfig {
                policy: {
                    let mut policy = PolicyConfig::default();
                    policy.allowed_tools.insert("tool".into());
                    policy
                },
                transport: TransportConfig::Stdio {
                    command: "fake".into(),
                    args: vec![],
                    env: Default::default(),
                    current_dir: None,
                    timeouts: Default::default(),
                    heartbeat: None,
                },
                ..Default::default()
            },
        );
        let config_state = host_config.into_state();
        let registry = Registry::new(config_state.clone());
        let sinks = HostSinks::new(|_| Ok::<(), Error>(()), Span::current());
        let router = Router::new(config_state, registry, sinks);

        let err = router
            .call_tool("missing", "tool", json!({}), None)
            .await
            .expect_err("should fail");
        assert!(matches!(err, RouterError::ServerNotRegistered(_)));
    }

    #[tokio::test]
    async fn call_tool_succeeds_for_allowed_tool() {
        let mut host_config = HostConfig::default();
        host_config.servers.insert(
            "server".into(),
            ServerConfig {
                policy: {
                    let mut policy = PolicyConfig::default();
                    policy.allowed_tools.insert("tool".into());
                    policy.call_timeout_ms = 50;
                    policy
                },
                transport: TransportConfig::Stdio {
                    command: "fake".into(),
                    args: vec![],
                    env: Default::default(),
                    current_dir: None,
                    timeouts: Default::default(),
                    heartbeat: None,
                },
                ..Default::default()
            },
        );

        let config_state = host_config.into_state();
        let registry = Registry::new(config_state.clone());
        let sinks = HostSinks::new(|_| Ok::<(), Error>(()), Span::current());
        let router = Router::new(config_state.clone(), registry.clone(), sinks);

        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": { "value": 42 }
        });
        let client = Arc::new(McpClient::new(
            "server".into(),
            Arc::new(MockTransport::with_response(response)),
        ));
        router.register_client("server".into(), client).await;

        registry
            .upsert(
                "server".into(),
                ServerInfo {
                    name: "Test".into(),
                    version: Some("0.1.0".into()),
                },
                vec![ToolSchema {
                    name: "tool".into(),
                    description: None,
                    input_schema: json!({}),
                }],
                vec![],
                None,
            )
            .await;

        let result = router
            .call_tool("server", "tool", json!({"foo": "bar"}), None)
            .await
            .expect("tool call succeeds");
        assert_eq!(result, json!({ "value": 42 }));
    }

    #[tokio::test]
    async fn rejects_tool_not_allowed_by_policy() {
        let mut host_config = HostConfig::default();
        host_config.servers.insert(
            "server".into(),
            ServerConfig {
                policy: {
                    let mut policy = PolicyConfig::default();
                    policy.allowed_tools.insert("allowed".into());
                    policy
                },
                ..Default::default()
            },
        );

        let config_state = host_config.into_state();
        let registry = Registry::new(config_state.clone());
        let sinks = HostSinks::new(|_| Ok::<(), Error>(()), Span::current());
        let router = Router::new(config_state.clone(), registry.clone(), sinks);

        registry
            .upsert(
                "server".into(),
                ServerInfo {
                    name: "Test".into(),
                    version: None,
                },
                vec![ToolSchema {
                    name: "allowed".into(),
                    description: None,
                    input_schema: json!({}),
                }],
                vec![],
                None,
            )
            .await;

        let err = router
            .call_tool("server", "blocked", json!({}), None)
            .await
            .expect_err("should reject tool");
        assert!(matches!(
            err,
            RouterError::ToolNotAllowed { server, tool } if server == "server" && tool == "blocked"
        ));
    }

    #[tokio::test]
    async fn fails_without_registered_client() {
        let mut host_config = HostConfig::default();
        host_config.servers.insert(
            "server".into(),
            ServerConfig {
                policy: {
                    let mut policy = PolicyConfig::default();
                    policy.allowed_tools.insert("tool".into());
                    policy
                },
                ..Default::default()
            },
        );

        let config_state = host_config.into_state();
        let registry = Registry::new(config_state.clone());
        let sinks = HostSinks::new(|_| Ok::<(), Error>(()), Span::current());
        let router = Router::new(config_state.clone(), registry.clone(), sinks);

        registry
            .upsert(
                "server".into(),
                ServerInfo {
                    name: "Test".into(),
                    version: None,
                },
                vec![ToolSchema {
                    name: "tool".into(),
                    description: None,
                    input_schema: json!({}),
                }],
                vec![],
                None,
            )
            .await;

        let err = router
            .call_tool("server", "tool", json!({}), None)
            .await
            .expect_err("client should be required");
        assert!(matches!(err, RouterError::ClientUnavailable(server) if server == "server"));
    }
}
