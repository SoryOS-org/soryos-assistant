use serde::Serialize;
use serde_json::Value;

use super::time::monotonic_now_ms;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricsPhase {
    Stt,
    Llm,
    Act,
    Tts,
    E2e,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetricsEventKind {
    TurnStart,
    VadStop,
    SttDone,
    LlmReq,
    LlmResp,
    TtsInferStart,
    TtsAudioReady,
    TtsPlayStart,
    TurnSummary,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnTrigger {
    Wakeword,
    Ptt,
}

impl TurnTrigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            TurnTrigger::Wakeword => "wakeword",
            TurnTrigger::Ptt => "ptt",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsEvent {
    pub trace_id: String,
    pub phase: MetricsPhase,
    pub event: MetricsEventKind,
    pub t_mono_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub err: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

impl MetricsEvent {
    pub fn new(trace_id: impl Into<String>, phase: MetricsPhase, event: MetricsEventKind) -> Self {
        Self {
            trace_id: trace_id.into(),
            phase,
            event,
            t_mono_ms: monotonic_now_ms(),
            duration_ms: None,
            ok: None,
            err: None,
            metadata: None,
        }
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn with_ok(mut self, ok: bool) -> Self {
        self.ok = Some(ok);
        self
    }

    pub fn with_err(mut self, err: impl Into<String>) -> Self {
        self.err = Some(err.into());
        self
    }

    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Statistics about metrics recording health.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct MetricsStats {
    pub write_errors: u64,
    pub dropped_events: u64,
}
