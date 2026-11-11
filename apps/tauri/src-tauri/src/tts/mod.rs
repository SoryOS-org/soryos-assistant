pub mod events;
mod normalize;
mod piper;
mod playback;
pub mod preload;
mod service;

#[allow(unused_imports)]
pub use events::{TtsErrorEvent, TtsPipelineStage, TtsStatusEvent};
pub use normalize::normalize_for_tts;
#[allow(unused_imports)]
pub use service::{emit_error, TtsRequest, TtsService};

pub const EVENT_TTS_PRELOAD_STATUS: &str = "tts_preload_status";
pub const EVENT_TTS_STATUS: &str = "assistant://tts-status";
pub const EVENT_TTS_ERROR: &str = "assistant://tts-error";
