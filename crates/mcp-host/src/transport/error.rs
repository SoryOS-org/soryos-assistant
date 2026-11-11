use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("unsupported transport: {0}")]
    Unsupported(String),
    #[error("missing command for stdio transport")]
    MissingCommand,
    #[error("command `{command}` for stdio transport could not be resolved on PATH")]
    CommandNotFound { command: String },
    #[error("failed to spawn transport process: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("transport I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("transport timeout during {phase} after {duration:?}")]
    Timeout {
        phase: &'static str,
        duration: Duration,
    },
    #[error("transport process for {server_id} exited unexpectedly")]
    ProcessExited { server_id: String },
    #[error("{transport} transport requires enabling the corresponding feature flag")]
    FeatureDisabled { transport: &'static str },
    #[error("{0} transport support not implemented yet")]
    Unimplemented(&'static str),
    #[error("transport not initialised")]
    NotInitialised,
}

impl TransportError {
    pub fn timeout(phase: &'static str, duration: Duration) -> Self {
        Self::Timeout { phase, duration }
    }

    pub fn process_exited(server_id: impl Into<String>) -> Self {
        Self::ProcessExited {
            server_id: server_id.into(),
        }
    }

    pub fn feature_disabled(transport: &'static str) -> Self {
        Self::FeatureDisabled { transport }
    }
}
