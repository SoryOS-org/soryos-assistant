use std::collections::BTreeMap;
use std::sync::Arc;

use crate::config::{HeartbeatConfig, TransportTimeouts};

use super::{Transport, TransportError};

/// Placeholder HTTP transport integration.
///
/// Future work will wrap an HTTP/JSON-RPC client with timeout + heartbeat support.
/// Until then, we return an `Unimplemented` error when selected in configuration.
pub async fn connect(
    server_id: &str,
    url: &str,
    _headers: &BTreeMap<String, String>,
    _timeouts: &TransportTimeouts,
    _heartbeat: &Option<HeartbeatConfig>,
) -> Result<Arc<dyn Transport>, TransportError> {
    let _ = (server_id, url);
    Err(TransportError::Unimplemented("http"))
}
