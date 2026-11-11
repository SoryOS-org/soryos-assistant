use thiserror::Error;

use crate::transport::TransportError;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("failed to serialize request: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to parse response: {0}")]
    Deserialize(#[source] serde_json::Error),
    #[error("rpc error {code}: {message}")]
    Rpc {
        code: i64,
        message: String,
        data: Option<serde_json::Value>,
    },
    #[error("unexpected response id (expected {expected:?}, got {received:?})")]
    MismatchedId {
        expected: serde_json::Value,
        received: serde_json::Value,
    },
    #[error("handshake failed: {0}")]
    Handshake(String),
}
