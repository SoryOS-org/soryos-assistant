use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use thiserror::Error;
use tokio::sync::{watch, Mutex, RwLock};
use tokio::task::JoinSet;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::client::{ClientError, ClientInfo, McpClient, ResourceDescriptor, ToolSchema};
use crate::config::{ConfigState, HostConfig, ServerConfig, ServerId, TransportConfig};
use crate::events::{
    emit_schema_snapshot, emit_server_crashed, emit_server_started, emit_server_stopped,
    EventContext,
};
use crate::registry::Registry;
use crate::router::{Router, RouterError};
use crate::sinks::HostSinks;
use crate::transport::{build_transport, Transport, TransportError};

const CLIENT_NAME: &str = "sprite-mcp-host";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[async_trait]
pub trait TransportFactory: Send + Sync {
    async fn build(
        &self,
        server_id: &str,
        config: &TransportConfig,
    ) -> Result<Arc<dyn Transport>, TransportError>;
}

#[derive(Default)]
struct DefaultTransportFactory;

type ServerPartition = (Vec<(ServerId, ServerConfig)>, Vec<(ServerId, ServerConfig)>);

#[async_trait]
impl TransportFactory for DefaultTransportFactory {
    async fn build(
        &self,
        server_id: &str,
        config: &TransportConfig,
    ) -> Result<Arc<dyn Transport>, TransportError> {
        build_transport(server_id, config).await
    }
}

struct ServerRuntime {
    _client: Arc<McpClient>,
    transport: Arc<dyn Transport>,
}

/// Correlation information supplied during tool invocations.
#[derive(Debug, Clone, Default)]
pub struct ToolCallContext {
    pub trace_id: Option<Uuid>,
    pub session_token: Option<String>,
    pub step_index: Option<u32>,
}

/// Handle for interacting with the in-process MCP host. Server launches are scheduled
/// asynchronously; once `start` returns the host is live and emits events as individual
/// servers start, fail, or stop.
#[derive(Clone)]
pub struct HostHandle {
    config: ConfigState,
    registry: Registry,
    router: Router,
    sinks: HostSinks,
    transport_factory: Arc<dyn TransportFactory>,
    servers: Arc<RwLock<HashMap<ServerId, ServerRuntime>>>,
    join_set: Arc<Mutex<JoinSet<()>>>,
    shutdown_tx: watch::Sender<bool>,
    launch_epoch: Arc<AtomicU64>,
}

impl HostHandle {
    pub async fn start(config: HostConfig, sinks: HostSinks) -> Result<Self, HostError> {
        Self::start_with_factory(config, sinks, Arc::new(DefaultTransportFactory)).await
    }

    pub async fn start_with_factory(
        config: HostConfig,
        sinks: HostSinks,
        factory: Arc<dyn TransportFactory>,
    ) -> Result<Self, HostError> {
        let config_state = ConfigState::new(config);
        let registry = Registry::new(config_state.clone());
        let router = Router::new(config_state.clone(), registry.clone(), sinks.clone());
        let (shutdown_tx, _shutdown_rx) = watch::channel(false);

        let host = Self {
            config: config_state,
            registry,
            router,
            sinks,
            transport_factory: factory,
            servers: Arc::new(RwLock::new(HashMap::new())),
            join_set: Arc::new(Mutex::new(JoinSet::new())),
            shutdown_tx,
            launch_epoch: Arc::new(AtomicU64::new(0)),
        };

        {
            let guard = host.config.read();
            info!(server_count = guard.servers.len(), "Bootstrapping MCP host");
        }

        host.bootstrap_servers().await?;
        info!("MCP host bootstrap scheduled");
        Ok(host)
    }

    pub fn registry(&self) -> Registry {
        self.registry.clone()
    }

    pub fn router(&self) -> Router {
        self.router.clone()
    }

    pub async fn call_tool_with_context(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Value,
        context: ToolCallContext,
    ) -> Result<Value, RouterError> {
        debug!(
            server_id = %server_id,
            tool = %tool_name,
            session_token = ?context.session_token,
            step = ?context.step_index,
            "Dispatching MCP tool call"
        );

        let result = self
            .router
            .call_tool(server_id, tool_name, arguments, context.trace_id)
            .await?;

        debug!(
            server_id = %server_id,
            tool = %tool_name,
            session_token = ?context.session_token,
            step = ?context.step_index,
            "MCP tool call completed"
        );

        Ok(result)
    }

    pub fn config(&self) -> ConfigState {
        self.config.clone()
    }

    pub async fn update_config(&self, new_config: HostConfig) -> Result<(), HostError> {
        self.apply_new_config(new_config).await
    }

    pub async fn restart(&self, new_config: HostConfig) -> Result<(), HostError> {
        self.apply_new_config(new_config).await
    }

    pub async fn shutdown(&self) -> Result<(), HostError> {
        let _ = self.shutdown_tx.send(true);
        self.stop_servers().await
    }

    async fn apply_new_config(&self, new_config: HostConfig) -> Result<(), HostError> {
        self.invalidate_pending_launches();
        self.stop_servers().await?;
        self.config.update(new_config);
        self.bootstrap_servers().await
    }

    async fn bootstrap_servers(&self) -> Result<(), HostError> {
        let (server_entries, disabled_entries): ServerPartition = {
            let guard = self.config.read();
            guard
                .servers
                .iter()
                .map(|(id, cfg)| (id.clone(), cfg.clone()))
                .partition(|(_, cfg)| cfg.enabled)
        };

        if !disabled_entries.is_empty() {
            for (server_id, _) in &disabled_entries {
                info!(server_id = %server_id, "Skipping disabled MCP server");
            }
        }

        if server_entries.is_empty() {
            if disabled_entries.is_empty() {
                info!("No MCP servers configured");
            } else {
                info!("All configured MCP servers are disabled");
            }
            return Ok(());
        }

        let epoch = self.bump_launch_epoch();
        info!(
            server_count = server_entries.len(),
            epoch, "Scheduling MCP servers for launch"
        );

        let mut join_set = self.join_set.lock().await;
        for (server_id, server_config) in server_entries {
            let host = self.clone();
            join_set.spawn(async move {
                host.run_server_task(server_id, server_config, epoch).await;
            });
        }
        Ok(())
    }

    async fn run_server_task(self, server_id: ServerId, server_config: ServerConfig, epoch: u64) {
        if self.shutdown_requested() {
            debug!(server_id = %server_id, "Skipping server launch; shutdown in progress");
            return;
        }

        if !self.epoch_matches(epoch) {
            debug!(server_id = %server_id, epoch, "Skipping server launch; epoch mismatch");
            return;
        }

        if let Err(err) = self
            .launch_server(server_id.clone(), server_config, epoch)
            .await
        {
            warn!(server_id = %server_id, error = %err, "Server launch failed");
        }
    }

    async fn launch_server(
        &self,
        server_id: ServerId,
        server_config: ServerConfig,
        epoch: u64,
    ) -> Result<(), HostError> {
        info!(server_id = %server_id, transport = %transport_label(&server_config.transport), "Starting MCP server");

        if self.shutdown_requested() || !self.epoch_matches(epoch) {
            debug!(server_id = %server_id, epoch, "Launch aborted prior to transport creation");
            return Ok(());
        }

        let transport = self
            .transport_factory
            .build(&server_id, &server_config.transport)
            .await?;
        debug!(server_id = %server_id, "Transport constructed successfully");
        let client = Arc::new(McpClient::new(server_id.clone(), Arc::clone(&transport)));

        let client_info = ClientInfo {
            name: CLIENT_NAME.to_string(),
            version: Some(CLIENT_VERSION.to_string()),
        };

        let trace_id: Option<Uuid> = None;
        info!(server_id = %server_id, "Sending initialize request to MCP server");
        let initialize = match client.initialize(client_info.clone()).await {
            Ok(value) => value,
            Err(err) => {
                emit_server_crashed(
                    &self.sinks,
                    server_id.clone(),
                    format!("handshake failed: {err}"),
                    EventContext::from_trace(trace_id),
                    None,
                );
                return Err(err.into());
            }
        };
        info!(
            server_id = %server_id,
            server_name = %initialize.server_info.name,
            server_version = ?initialize.server_info.version,
            "Initialize handshake completed"
        );

        debug!(server_id = %server_id, "Sending notifications/initialized to MCP server");
        if let Err(err) = client
            .send_notification("notifications/initialized", json!({}))
            .await
        {
            emit_server_crashed(
                &self.sinks,
                server_id.clone(),
                format!("initialized notification failed: {err}"),
                EventContext::from_trace(trace_id),
                None,
            );
            return Err(err.into());
        }

        if self.shutdown_requested() || !self.epoch_matches(epoch) {
            debug!(server_id = %server_id, epoch, "Aborting server registration due to shutdown or epoch change");
            if let Err(err) = transport.shutdown().await {
                warn!(server_id = %server_id, "Transport shutdown during abort failed: {err}");
            }
            return Ok(());
        }

        emit_server_started(
            &self.sinks,
            server_id.clone(),
            None,
            EventContext::from_trace(trace_id),
            Some(json!({
                "transport": transport_label(&server_config.transport),
                "server_info": {
                    "name": initialize.server_info.name,
                    "version": initialize.server_info.version,
                }
            })),
        );
        info!(server_id = %server_id, "MCP server handshake complete");

        debug!(server_id = %server_id, "Requesting tool list from MCP server");
        let tools: Vec<ToolSchema> = client.list_tools().await?;
        debug!(server_id = %server_id, tool_count = tools.len(), "Tool list received");
        let resources = match client.list_resources().await {
            Ok(res) => res,
            Err(err) => {
                warn!(
                    server_id = %server_id,
                    "Failed to list resources: {err}"
                );
                Vec::<ResourceDescriptor>::new()
            }
        };
        debug!(server_id = %server_id, resource_count = resources.len(), "Resource list processed");

        self.registry
            .upsert(
                server_id.clone(),
                initialize.server_info,
                tools.clone(),
                resources.clone(),
                trace_id,
            )
            .await;

        let tools_value = serde_json::to_value(&tools)?;
        let resources_value = serde_json::to_value(&resources)?;

        emit_schema_snapshot(
            &self.sinks,
            server_id.clone(),
            tools_value,
            Some(resources_value),
            EventContext::from_trace(trace_id),
        );

        debug!(server_id = %server_id, "Registering MCP client with router");
        self.router
            .register_client(server_id.clone(), Arc::clone(&client))
            .await;

        {
            let mut guard = self.servers.write().await;
            guard.insert(
                server_id.clone(),
                ServerRuntime {
                    _client: client,
                    transport,
                },
            );
        }

        info!(server_id = %server_id, "Server initialised");
        Ok(())
    }

    async fn stop_servers(&self) -> Result<(), HostError> {
        self.invalidate_pending_launches();

        let servers = {
            let mut guard = self.servers.write().await;
            guard.drain().collect::<Vec<_>>()
        };

        let server_count = servers.len();
        let mut first_err: Option<HostError> = None;

        if server_count == 0 {
            info!("No MCP servers running when stop requested");
        } else {
            info!(count = server_count, "Stopping MCP servers");
        }

        for (server_id, runtime) in servers {
            if let Err(err) = runtime.transport.shutdown().await {
                warn!(server_id = %server_id, "Transport shutdown failed: {err}");
                if first_err.is_none() {
                    first_err = Some(err.into());
                }
            }

            self.router.remove_client(&server_id).await;
            self.registry.remove(&server_id).await;
            emit_server_stopped(
                &self.sinks,
                server_id.clone(),
                None,
                EventContext::new(),
                None,
            );
        }

        {
            let mut join_set = self.join_set.lock().await;
            while join_set.join_next().await.is_some() {}
        }

        if let Some(err) = first_err {
            Err(err)
        } else {
            if server_count == 0 {
                info!("No MCP servers were running");
            } else {
                info!("All MCP servers stopped");
            }
            Ok(())
        }
    }
}

fn transport_label(config: &TransportConfig) -> &'static str {
    match config {
        TransportConfig::Stdio { .. } => "stdio",
        TransportConfig::Ws { .. } => "ws",
        TransportConfig::Http { .. } => "http",
    }
}

impl HostHandle {
    fn shutdown_requested(&self) -> bool {
        *self.shutdown_tx.borrow()
    }

    fn invalidate_pending_launches(&self) {
        self.launch_epoch.fetch_add(1, Ordering::SeqCst);
    }

    fn bump_launch_epoch(&self) -> u64 {
        self.launch_epoch.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn epoch_matches(&self, epoch: u64) -> bool {
        self.launch_epoch.load(Ordering::SeqCst) == epoch
    }
}

#[derive(Debug, Error)]
pub enum HostError {
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("client error: {0}")]
    Client(#[from] ClientError),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PolicyConfig;
    use crate::events::HostEvent;
    use crate::transport::Transport;
    use anyhow::Error;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tracing::Span;

    struct MockTransport {
        sent: Mutex<Vec<Vec<u8>>>,
        responses: Mutex<VecDeque<Vec<u8>>>,
    }

    impl MockTransport {
        fn with_sequence(sequence: Vec<serde_json::Value>) -> Self {
            let responses = sequence
                .into_iter()
                .map(|value| serde_json::to_vec(&value).unwrap())
                .collect::<VecDeque<_>>();
            Self {
                sent: Mutex::new(Vec::new()),
                responses: Mutex::new(responses),
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
                .pop_front()
                .ok_or(TransportError::NotInitialised)
        }

        async fn shutdown(&self) -> Result<(), TransportError> {
            Ok(())
        }
    }

    struct MockFactory {
        transports: Mutex<HashMap<String, Arc<dyn Transport>>>,
    }

    impl MockFactory {
        fn new(map: HashMap<String, Arc<dyn Transport>>) -> Self {
            Self {
                transports: Mutex::new(map),
            }
        }
    }

    #[async_trait]
    impl TransportFactory for MockFactory {
        async fn build(
            &self,
            server_id: &str,
            _config: &TransportConfig,
        ) -> Result<Arc<dyn Transport>, TransportError> {
            self.transports
                .lock()
                .await
                .remove(server_id)
                .ok_or(TransportError::NotInitialised)
        }
    }

    fn handshake_sequence() -> Vec<serde_json::Value> {
        vec![
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "protocolVersion": "1.0",
                    "serverInfo": { "name": "mock", "version": "1.0" },
                    "capabilities": {}
                }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "result": {
                    "tools": [{
                        "name": "echo",
                        "inputSchema": { "type": "object" }
                    }]
                }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "result": { "resources": [] }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "result": { "echo": true }
            }),
        ]
    }

    fn test_config() -> HostConfig {
        let mut config = HostConfig::default();
        config.servers.insert(
            "mock".into(),
            ServerConfig {
                policy: {
                    let mut policy = PolicyConfig::default();
                    policy.allowed_tools.insert("echo".into());
                    policy
                },
                transport: TransportConfig::Stdio {
                    command: "mock".into(),
                    args: vec![],
                    env: Default::default(),
                    current_dir: None,
                    timeouts: Default::default(),
                    heartbeat: None,
                },
                ..Default::default()
            },
        );
        config
    }

    #[tokio::test]
    async fn start_and_shutdown_host() {
        let transport = Arc::new(MockTransport::with_sequence(handshake_sequence()));
        let factory = Arc::new(MockFactory::new(HashMap::from([(
            "mock".into(),
            transport as Arc<dyn Transport>,
        )])));

        let sinks = HostSinks::new(|_event: HostEvent| Ok::<(), Error>(()), Span::current());

        let host = HostHandle::start_with_factory(test_config(), sinks, factory)
            .await
            .expect("host start");

        let router = host.router();
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                match router
                    .call_tool("mock", "echo", json!({"value": 1}), None)
                    .await
                {
                    Ok(value) => break Ok(value),
                    Err(crate::router::RouterError::ClientUnavailable(_))
                    | Err(crate::router::RouterError::ServerNotRegistered(_)) => {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                    Err(err) => break Err(err),
                }
            }
        })
        .await
        .expect("tool availability timeout")
        .expect("call tool");
        assert_eq!(result, json!({"echo": true}));

        host.shutdown().await.expect("shutdown host");
    }
}
