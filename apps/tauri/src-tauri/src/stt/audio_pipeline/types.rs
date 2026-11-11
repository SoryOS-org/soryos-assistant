use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioState {
    Idle = 0,
    WakeWordRecording = 1,
    PttRecording = 2,
    Processing = 3,
    Error = 4,
}

impl AudioState {
    pub(super) fn from_code(code: u8) -> Self {
        match code {
            0 => AudioState::Idle,
            1 => AudioState::WakeWordRecording,
            2 => AudioState::PttRecording,
            3 => AudioState::Processing,
            4 => AudioState::Error,
            _ => AudioState::Error,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AudioPipelineConfig {
    pub model_path: PathBuf,
    pub language: String,
    pub vad_aggressiveness: u8,
    pub min_transcript_chars: usize,
    pub start_grace_ms: u64,
    pub max_segment_ms: u64,
    pub activation_ms: u64,
    pub post_speech_silence_ms: u64,
    pub whisper_threads: Option<u32>,
}

impl AudioPipelineConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model_path: PathBuf,
        language: String,
        vad_aggressiveness: u8,
        min_transcript_chars: usize,
        start_grace_ms: u64,
        max_segment_ms: u64,
        activation_ms: u64,
        post_speech_silence_ms: u64,
        whisper_threads: Option<u32>,
    ) -> Self {
        Self {
            model_path,
            language,
            vad_aggressiveness,
            min_transcript_chars,
            start_grace_ms,
            max_segment_ms,
            activation_ms,
            post_speech_silence_ms,
            whisper_threads,
        }
    }
}
