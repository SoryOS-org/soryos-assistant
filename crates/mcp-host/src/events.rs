use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::ServerId;
use crate::sinks::HostSinks;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostEvent {
    pub meta: EventMeta,
    pub kind: HostEventKind,
}

/// Common metadata shared across all events so telemetry can correlate activity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMeta {
    pub server_id: ServerId,
    /// Milliseconds since Unix epoch.
    pub timestamp_ms: u128,
    /// Correlation identifier supplied by the app layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_index: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct EventContext {
    pub trace_id: Option<Uuid>,
    pub session_token: Option<String>,
    pub step_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HostEventKind {
    ServerStarted(ServerStartedEvent),
    ServerStopped(ServerStoppedEvent),
    ServerCrashed(ServerCrashedEvent),
    SchemasUpdated(SchemaSnapshotEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStartedEvent {
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStoppedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCrashedEvent {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaSnapshotEvent {
    pub tool_schemas: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<serde_json::Value>,
}

impl EventMeta {
    pub fn new(server_id: ServerId) -> Self {
        Self {
            server_id,
            timestamp_ms: current_epoch_ms(),
            trace_id: None,
            session_token: None,
            step_index: None,
        }
    }

    pub fn with_trace(mut self, trace_id: Uuid) -> Self {
        self.trace_id = Some(trace_id);
        self
    }

    pub fn with_session(mut self, token: String) -> Self {
        self.session_token = Some(token);
        self
    }

    pub fn with_step(mut self, step: u32) -> Self {
        self.step_index = Some(step);
        self
    }
}

impl EventContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_trace(trace_id: Option<Uuid>) -> Self {
        Self {
            trace_id,
            ..Self::default()
        }
    }
}

pub fn emit_server_started(
    sinks: &HostSinks,
    server_id: ServerId,
    pid: Option<u32>,
    context: EventContext,
    details: Option<serde_json::Value>,
) {
    let meta = apply_context(EventMeta::new(server_id), context);
    let event = HostEvent {
        meta,
        kind: HostEventKind::ServerStarted(ServerStartedEvent { pid, details }),
    };
    sinks.emit(event);
}

pub fn emit_server_stopped(
    sinks: &HostSinks,
    server_id: ServerId,
    exit_code: Option<i32>,
    context: EventContext,
    details: Option<serde_json::Value>,
) {
    let meta = apply_context(EventMeta::new(server_id), context);
    let event = HostEvent {
        meta,
        kind: HostEventKind::ServerStopped(ServerStoppedEvent { exit_code, details }),
    };
    sinks.emit(event);
}

pub fn emit_server_crashed(
    sinks: &HostSinks,
    server_id: ServerId,
    message: String,
    context: EventContext,
    details: Option<serde_json::Value>,
) {
    let meta = apply_context(EventMeta::new(server_id), context);
    let event = HostEvent {
        meta,
        kind: HostEventKind::ServerCrashed(ServerCrashedEvent { message, details }),
    };
    sinks.emit(event);
}

pub fn emit_schema_snapshot(
    sinks: &HostSinks,
    server_id: ServerId,
    tools: serde_json::Value,
    resources: Option<serde_json::Value>,
    context: EventContext,
) {
    let meta = apply_context(EventMeta::new(server_id), context);
    let event = HostEvent {
        meta,
        kind: HostEventKind::SchemasUpdated(SchemaSnapshotEvent {
            tool_schemas: tools,
            resources,
        }),
    };
    sinks.emit(event);
}

fn apply_context(mut meta: EventMeta, context: EventContext) -> EventMeta {
    if let Some(trace) = context.trace_id {
        meta = meta.with_trace(trace);
    }
    if let Some(token) = context.session_token {
        meta = meta.with_session(token);
    }
    if let Some(step) = context.step_index {
        meta = meta.with_step(step);
    }
    meta
}

fn current_epoch_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sinks::HostSinks;
    use std::sync::{Arc, Mutex};
    use tracing::Span;

    #[test]
    fn meta_generates_timestamp() {
        let meta = EventMeta::new("browser".to_string());
        assert_eq!(meta.server_id, "browser");
        assert!(meta.timestamp_ms > 0);
    }

    #[test]
    fn host_event_serializes() {
        let event = HostEvent {
            meta: EventMeta::new("browser".into()).with_trace(Uuid::nil()),
            kind: HostEventKind::ServerStarted(ServerStartedEvent {
                pid: Some(42),
                details: None,
            }),
        };

        let json = serde_json::to_string(&event).expect("serialize");
        let round_trip: HostEvent = serde_json::from_str(&json).expect("deserialize");
        match round_trip.kind {
            HostEventKind::ServerStarted(inner) => {
                assert_eq!(inner.pid, Some(42));
            }
            _ => panic!("expected server started event"),
        }
    }

    #[test]
    fn emitters_route_events() {
        let received: Arc<Mutex<Vec<HostEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = {
            let buffer = Arc::clone(&received);
            HostSinks::new(
                move |event| {
                    buffer.lock().unwrap().push(event);
                    Ok(())
                },
                Span::current(),
            )
        };

        emit_server_started(
            &sink,
            "browser".into(),
            Some(99),
            EventContext::new(),
            Some(serde_json::json!({"status": "ok"})),
        );

        let events = received.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0].kind {
            HostEventKind::ServerStarted(payload) => assert_eq!(payload.pid, Some(99)),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
