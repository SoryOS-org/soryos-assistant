// Re-export event types for convenience
pub use events::*;

pub mod audio_pipeline;
pub mod download;
pub mod events;
pub mod hybrid_service;
pub mod model_downloader;
pub mod preload;
mod service;
pub mod sidecar;
pub mod threading;

pub const EVENT_STT_RESULT: &str = "assistant://stt-result";
pub const EVENT_STT_READY: &str = "assistant://stt-ready";
pub const EVENT_STT_STARTED: &str = "assistant://stt-started";
pub const EVENT_STT_FINAL: &str = "assistant://stt-final";
pub const EVENT_STT_ERROR: &str = "assistant://stt-error";
pub const EVENT_CONFIG_CHANGED: &str = "config_changed";
pub const EVENT_CONFIG_DIRTY: &str = "config_dirty";
pub const EVENT_PRELOAD_STATUS: &str = "stt_preload_status";

pub use crate::config::stt::{SttConfig, SttProvider, WakeWordProvider};
pub(crate) use service::SttCommand;
#[allow(unused_imports)]
pub use service::{PttState, SttService};
