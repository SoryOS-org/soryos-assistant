use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::client::{ResourceDescriptor, ServerInfo, ToolSchema};
use crate::config::{ConfigState, ServerConfig, ServerId};

#[derive(Clone)]
pub struct Registry {
    config: ConfigState,
    entries: Arc<RwLock<HashMap<ServerId, RegistryEntry>>>,
}

impl Registry {
    pub fn new(config: ConfigState) -> Self {
        Self {
            config,
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn upsert(
        &self,
        server_id: ServerId,
        info: ServerInfo,
        tools: Vec<ToolSchema>,
        resources: Vec<ResourceDescriptor>,
        trace_id: Option<Uuid>,
    ) {
        let config = self.lookup_config(&server_id);
        let mut guard = self.entries.write().await;
        guard.insert(
            server_id.clone(),
            RegistryEntry {
                server_id,
                info,
                config,
                tools,
                resources,
                trace_id,
                updated_at_ms: current_epoch_ms(),
            },
        );
    }

    pub async fn remove(&self, server_id: &ServerId) {
        let mut guard = self.entries.write().await;
        guard.remove(server_id);
    }

    pub async fn tool_schemas(&self, server_id: &ServerId) -> Option<Vec<ToolSchema>> {
        let guard = self.entries.read().await;
        guard.get(server_id).map(|entry| entry.tools.clone())
    }

    pub async fn entry(&self, server_id: &ServerId) -> Option<RegistryEntry> {
        let guard = self.entries.read().await;
        guard.get(server_id).cloned()
    }

    pub async fn snapshot(&self) -> Vec<RegistryEntry> {
        let guard = self.entries.read().await;
        guard.values().cloned().collect()
    }

    fn lookup_config(&self, server_id: &ServerId) -> Arc<ServerConfig> {
        let guard = self.config.read();
        guard
            .server(server_id)
            .map(|cfg| Arc::new(cfg.clone()))
            .unwrap_or_else(|| Arc::new(ServerConfig::default()))
    }
}

#[derive(Clone)]
pub struct RegistryEntry {
    pub server_id: ServerId,
    pub info: ServerInfo,
    pub config: Arc<ServerConfig>,
    pub tools: Vec<ToolSchema>,
    pub resources: Vec<ResourceDescriptor>,
    pub trace_id: Option<Uuid>,
    pub updated_at_ms: u128,
}

fn current_epoch_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crate::client::{ServerInfo, ToolSchema};
    use crate::config::HostConfig;

    use super::Registry;

    #[tokio::test]
    async fn upsert_and_snapshot() {
        let config = HostConfig::default().into_state();
        let registry = Registry::new(config);

        registry
            .upsert(
                "server".into(),
                ServerInfo {
                    name: "test".into(),
                    version: None,
                },
                vec![ToolSchema {
                    name: "tool".into(),
                    description: Some("desc".into()),
                    input_schema: serde_json::json!({"type": "object"}),
                }],
                vec![],
                None,
            )
            .await;

        let snapshot = registry.snapshot().await;
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].tools.len(), 1);
        assert_eq!(snapshot[0].tools[0].name, "tool");
    }
}
