use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PreloadStatusPayload {
    pub stage: &'static str,
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TtsPipelineStage {
    Pending,
    Synthesizing,
    Playing,
    Completed,
    Error,
}

impl TtsPipelineStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            TtsPipelineStage::Pending => "pending",
            TtsPipelineStage::Synthesizing => "synthesizing",
            TtsPipelineStage::Playing => "playing",
            TtsPipelineStage::Completed => "completed",
            TtsPipelineStage::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TtsStatusEvent {
    pub utterance_id: String,
    pub stage: TtsPipelineStage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TtsErrorEvent {
    pub utterance_id: String,
    pub message: String,
}
