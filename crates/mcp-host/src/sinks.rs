use std::sync::Arc;

use anyhow::Result;
use tracing::{warn, Span};

use crate::events::HostEvent;

#[derive(Clone)]
pub struct HostSinks {
    event_callback: Arc<dyn Fn(HostEvent) -> Result<()> + Send + Sync>,
    log_span: Span,
}

impl HostSinks {
    pub fn new<F>(event_callback: F, log_span: Span) -> Self
    where
        F: Fn(HostEvent) -> Result<()> + Send + Sync + 'static,
    {
        Self {
            event_callback: Arc::new(event_callback),
            log_span,
        }
    }

    pub fn emit(&self, event: HostEvent) {
        let callback = Arc::clone(&self.event_callback);
        let span = self.log_span.clone();
        let _guard = span.enter();
        if let Err(err) = callback(event) {
            warn!("Host event callback failed: {err:?}");
        }
    }

    pub fn span(&self) -> &Span {
        &self.log_span
    }
}
