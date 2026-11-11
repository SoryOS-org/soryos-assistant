// Centralized code-owned defaults for config values that should not be user-editable.
// Edit here to change internal defaults without hunting through serde annotations.

// STT
pub const STT_MODEL_FILENAME: &str = "ggml-base.en.bin"; // code-only: bundled Whisper model filename (resolved to assets dir)
pub const STT_DEFAULT_MODEL_ID: &str = "base.en"; // hydrates config: model.id (selection persisted; resolves to path internally)
pub const STT_DEFAULT_LANGUAGE: &str = "en"; // hydrates config: model.language.default
pub const STT_WAKEWORD_KEYWORD: &str = "jarvis"; // hydrates config: wakeword.keyword (Picovoice sidecar)
pub const STT_WAKEWORD_SENSITIVITY: f32 = 0.5; // hydrates config: wakeword.sensitivity (Picovoice sidecar)
pub const STT_DEFAULT_VAD_AGGRESSIVENESS: u8 = 2; // hydrates config: vad.aggressiveness (WebRTC VAD level)
pub const STT_DEFAULT_VAD_START_GRACE_MS: u64 = 3000; // hydrates config: vad.start_grace_ms (post-wakeword grace)
pub const STT_DEFAULT_VAD_MAX_SEGMENT_MS: u64 = 12000; // hydrates config: vad.max_segment_ms (max capture length)
pub const STT_DEFAULT_VAD_ACTIVATION_MS: u64 = 240; // hydrates config: vad.activation_ms (speech activation window)
pub const STT_DEFAULT_VAD_POST_SPEECH_MS: u64 = 1500; // hydrates config: vad.post_speech_silence_ms (end-of-utterance delay)
pub const STT_DEFAULT_PTT_HOTKEY: &str = "Ctrl+Space"; // hydrates config: hotkey.push_to_talk (panel hotkey)
pub const STT_DEFAULT_MODE_PRECEDENCE: &str = "ptt"; // hydrates config: hotkey.mode_precedence (ptt vs wake)
pub const STT_DEFAULT_TELEMETRY_LEVEL: &str = "errors"; // hydrates config: telemetry.level (log filter)
pub const STT_RETRY_MAX: u32 = 1; // code-only: worker.retry.max (skip serialized)
pub const STT_RETRY_BASE_MS: u64 = 400; // code-only: worker.retry.base_ms (skip serialized)
pub const STT_SIDECAR_STARTUP_TIMEOUT_MS: u64 = 5000; // code-only: sidecar.startup_timeout_ms
pub const STT_SIDECAR_COMMUNICATION_TIMEOUT_MS: u64 = 10000; // code-only: sidecar.communication_timeout_ms
pub const STT_SIDECAR_EXECUTABLE_PATH: &str = "resources/sprite_voice_sidecar.exe"; // code-only: sidecar.executable_path
pub const STT_SIDECAR_PYTHON_ENV: &str = "apps/voice-service/venv"; // code-only: sidecar.python_env
pub const STT_DEFAULT_SPRITE_SCALE: f32 = 0.3; // hydrates config: sprite.scale (UI scale)

// TTS / Piper
pub const TTS_PIPER_CHUNK_CHARS: usize = 360; // code-only: piper.chunk_chars (split length per render)
pub const TTS_PIPER_INTER_CHUNK_GAP_MS: u64 = 120; // code-only: piper.inter_chunk_gap_ms (gap between chunks)
pub const TTS_DEFAULT_VOICE_BASENAME: &str = "en_US-amy-medium"; // code-only: base filename for bundled Piper voice (assets dir)
pub const TTS_DEFAULT_VOICE_REPO_ID: &str = "rhasspy/piper-voices"; // hydrates config: piper.download.repo_id
pub const TTS_DEFAULT_VOICE_REVISION: &str = "main"; // hydrates config: piper.download.revision
pub const TTS_DEFAULT_VOICE_PATH: &str = "en/en_US/amy/medium/en_US-amy-medium"; // hydrates config: piper.download.voice
pub const TTS_DEFAULT_VOLUME: f32 = 1.0; // hydrates config: playback.volume (panel)
pub const TTS_DEFAULT_CROSSFADE_MS: u64 = 0; // hydrates config: playback.crossfade_ms

// Worker bridge
pub const DEFAULT_WORKER_BASE_URL: &str = "https://sprite-boss.tamjad91.workers.dev"; // code-only: base worker endpoint (overridden in config/runtime)

// MCP policy defaults
pub const MCP_POLICY_HANDSHAKE_TIMEOUT_MS: u64 = 10_000; // hydrates config: policy.handshake_timeout_ms
pub const MCP_POLICY_CALL_TIMEOUT_MS: u64 = 90_000; // hydrates config: policy.call_timeout_ms

// Preferences
pub const PREFERENCES_DEFAULT_PERSONALITY: &str = "Snappy and cheeky"; // hydrates config: personality

// UI
pub const UI_PANEL_ENABLED: bool = true; // runtime: ui.enablePanel (tauri.conf.json)
pub const UI_SHOW_MAIN_IN_TASKBAR: bool = false; // runtime: ui.showMainInTaskbar (tauri.conf.json)

// Worker retry defaults
pub const WORKER_RETRY_MAX_ATTEMPTS: u32 = 1; // code-only: worker retry cap
pub const WORKER_RETRY_BASE_DELAY_MS: u64 = 400; // code-only: worker retry base delay
