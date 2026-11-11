use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttReadyEvent {
    pub provider: String,
    pub model: String,
    pub lang: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttStartedEvent {
    pub trigger: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttFinalEvent {
    pub text: String,
    pub ts_start_ms: u64,
    pub ts_end_ms: u64,
    pub lang: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttErrorEvent {
    pub stage: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionEvent {
    pub text: String,
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDirtyEvent {
    pub path: String,
    pub kind: String,
}
