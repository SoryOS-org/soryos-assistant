pub mod error;
#[cfg(feature = "http")]
pub mod http;
pub mod stdio;
#[cfg(feature = "ws")]
pub mod ws;

use async_trait::async_trait;
use std::sync::Arc;

pub use error::TransportError;
pub use stdio::StdioTransport;

use crate::config::TransportConfig;

#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, payload: &[u8]) -> Result<(), TransportError>;
    async fn recv(&self) -> Result<Vec<u8>, TransportError>;
    async fn shutdown(&self) -> Result<(), TransportError>;
}

/// Helper to instantiate a transport based on configuration.
pub async fn build_transport(
    server_id: &str,
    config: &TransportConfig,
) -> Result<Arc<dyn Transport>, TransportError> {
    match config {
        TransportConfig::Stdio { .. } => {
            let transport = StdioTransport::spawn(server_id.to_string(), config).await?;
            Ok(Arc::new(transport))
        }
        TransportConfig::Ws { .. } => build_ws_transport(server_id, config).await,
        TransportConfig::Http { .. } => build_http_transport(server_id, config).await,
    }
}

#[cfg(feature = "ws")]
async fn build_ws_transport(
    server_id: &str,
    config: &TransportConfig,
) -> Result<Arc<dyn Transport>, TransportError> {
    if let TransportConfig::Ws {
        url,
        headers,
        timeouts,
        heartbeat,
    } = config
    {
        ws::connect(server_id, url, headers, timeouts, heartbeat).await
    } else {
        Err(TransportError::Unsupported(
            "expected ws transport configuration".into(),
        ))
    }
}

#[cfg(not(feature = "ws"))]
async fn build_ws_transport(
    _server_id: &str,
    _config: &TransportConfig,
) -> Result<Arc<dyn Transport>, TransportError> {
    Err(TransportError::feature_disabled("ws"))
}

#[cfg(feature = "http")]
async fn build_http_transport(
    server_id: &str,
    config: &TransportConfig,
) -> Result<Arc<dyn Transport>, TransportError> {
    if let TransportConfig::Http {
        url,
        headers,
        timeouts,
        heartbeat,
    } = config
    {
        http::connect(server_id, url, headers, timeouts, heartbeat).await
    } else {
        Err(TransportError::Unsupported(
            "expected http transport configuration".into(),
        ))
    }
}

#[cfg(not(feature = "http"))]
async fn build_http_transport(
    _server_id: &str,
    _config: &TransportConfig,
) -> Result<Arc<dyn Transport>, TransportError> {
    Err(TransportError::feature_disabled("http"))
}
