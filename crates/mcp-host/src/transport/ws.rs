use std::collections::BTreeMap;
use std::sync::Arc;

use crate::config::{HeartbeatConfig, TransportTimeouts};

use super::{Transport, TransportError};

/// Placeholder WebSocket transport integration.
///
/// The actual implementation will establish an async WebSocket connection, wrap it
/// in a `Transport` trait object, and manage heartbeats. For now, calling this
/// function surfaces an `Unimplemented` error so callers know the feature is not
/// ready.
pub async fn connect(
    server_id: &str,
    url: &str,
    _headers: &BTreeMap<String, String>,
    _timeouts: &TransportTimeouts,
    _heartbeat: &Option<HeartbeatConfig>,
) -> Result<Arc<dyn Transport>, TransportError> {
    let _ = (server_id, url);
    Err(TransportError::Unimplemented("websocket"))
}
