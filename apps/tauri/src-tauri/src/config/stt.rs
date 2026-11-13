use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tauri::{App, AppHandle};
use tracing::warn;

use super::io;
use super::brain::BrainConfig;
use super::{defaults, paths, WorkerBridgeConfig};
use crate::machine::worker_client;
use crate::secret_store;
use crate::stt::model_downloader::WhisperModelInfo;
pub const SPRITE_SCALE_MIN: f32 = 0.2;
pub const SPRITE_SCALE_MAX: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SttProvider {
    #[default]
    Local,
    Api,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WakeWordProvider {
    #[default]
    Porcupine,
    OpenWakeWord,
    None,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelConfig {
    #[serde(default = "default_model_id")]
    pub id: String,
    #[serde(default = "default_model_path", skip_serializing)]
    pub path: String,
    #[serde(default)]
    pub language: LanguageConfig,
    #[serde(default = "default_true")]
    pub punctuation: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LanguageConfig {
    #[serde(default = "default_true")]
    pub auto: bool,
    #[serde(default = "default_language")]
    pub default: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WakeWordConfig {
    #[serde(default = "default_wakeword_provider", skip_serializing)]
    pub provider: WakeWordProvider,
    #[serde(default = "default_wakeword_keyword")]
    pub keyword: String,
    #[serde(default = "default_wakeword_sensitivity")]
    pub sensitivity: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VadConfig {
    #[serde(default = "default_vad_aggressiveness")]
    pub aggressiveness: u8,
    #[serde(default = "default_vad_start_grace_ms")]
    pub start_grace_ms: u64,
    #[serde(default = "default_vad_max_segment_ms")]
    pub max_segment_ms: u64,
    #[serde(default = "default_vad_activation_ms")]
    pub activation_ms: u64,
    #[serde(default = "default_vad_post_speech_ms")]
    pub post_speech_silence_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HotkeyConfig {
    #[serde(default = "default_ptt_hotkey")]
    pub push_to_talk: String,
    #[serde(default = "default_mode_precedence")]
    pub mode_precedence: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TelemetryConfig {
    #[serde(default = "default_telemetry_level")]
    pub level: String,
    #[serde(default)]
    pub raw_payload_logs: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WhisperRuntimeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpriteConfig {
    #[serde(default = "default_sprite_scale")]
    pub scale: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerConfig {
    #[serde(default = "default_worker_url", skip_serializing)]
    pub url: String,
    #[serde(default, skip_serializing)]
    pub retry: RetryConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetryConfig {
    #[serde(default = "default_retry_max")]
    pub max: u32,
    #[serde(default = "default_retry_base_ms")]
    pub base_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SidecarConfig {
    #[serde(default = "default_sidecar_executable_path", skip_serializing)]
    pub executable_path: String,
    #[serde(default = "default_sidecar_python_env", skip_serializing)]
    pub python_env: Option<String>,
    #[serde(default = "default_sidecar_startup_timeout_ms", skip_serializing)]
    pub startup_timeout_ms: u64,
    #[serde(default = "default_sidecar_communication_timeout_ms", skip_serializing)]
    pub communication_timeout_ms: u64,
    #[serde(
        default = "default_sidecar_access_key",
        skip_serializing_if = "String::is_empty"
    )]
    pub access_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wakeword_keyword: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wakeword_sensitivity: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_user_model_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SttConfig {
    #[serde(default = "default_provider")]
    pub provider: SttProvider,
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default, skip_serializing_if = "SidecarConfig::is_empty")]
    pub sidecar: SidecarConfig,
    #[serde(default)]
    pub wakeword: WakeWordConfig,
    #[serde(default)]
    pub vad: VadConfig,
    #[serde(default)]
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    #[serde(default, skip_serializing_if = "WhisperRuntimeConfig::is_empty")]
    pub whisper: WhisperRuntimeConfig,
    #[serde(default)]
    pub sprite: SpriteConfig,
    #[serde(default, skip_serializing_if = "WorkerConfig::is_default")]
    pub worker: WorkerConfig,
    #[serde(default, skip_serializing_if = "BrainConfig::is_default")]
    pub brain: BrainConfig,
}

impl SttConfig {
    pub fn load(app: &App) -> Result<Self> {
        let mut config = io::load_or_create(
            app,
            "stt.config.json",
            |content| {
                serde_json::from_str(content)
                    .with_context(|| "Failed to parse STT configuration; falling back to defaults")
            },
            serialize_for_save,
            SttConfig::default,
        )?;

        if !config.sidecar.access_key.trim().is_empty() {
            warn!("Stripping Picovoice key from config file; please set it via the secrets flow");
            config.sidecar.access_key.clear();
        }

        if let Some(secret) = secret_store::load_picovoice_key() {
            config.sidecar.access_key = secret;
        }

        config.sync_model_path_from_id();
        config.validate()?;

        Ok(config)
    }

    pub fn save_to_handle(&self, app_handle: &AppHandle) -> Result<()> {
        if !self.sidecar.access_key.trim().is_empty() {
            if let Err(err) = secret_store::store_picovoice_key(&self.sidecar.access_key) {
                warn!("Failed to persist Picovoice key to secure storage: {err:?}");
            }
        }

        io::save_to_handle(app_handle, "stt.config.json", self, serialize_for_save)
    }

    pub fn validate(&self) -> Result<()> {
        if self.model.path.trim().is_empty() {
            bail!("model.path cannot be empty");
        }

        if !(0.0..=1.0).contains(&self.wakeword.sensitivity) {
            bail!("wakeword.sensitivity must be between 0.0 and 1.0");
        }

        if !matches!(self.wakeword.provider, WakeWordProvider::None)
            && self.wakeword.keyword.trim().is_empty()
        {
            bail!("wakeword.keyword cannot be empty when a provider is configured");
        }

        if self.vad.aggressiveness > 3 {
            bail!("vad.aggressiveness must be between 0 and 3");
        }

        if self.vad.start_grace_ms == 0 || self.vad.max_segment_ms == 0 {
            bail!("vad timing values must be greater than zero");
        }

        if self.vad.activation_ms == 0 {
            bail!("vad.activation_ms must be greater than zero");
        }

        if self.vad.post_speech_silence_ms == 0 {
            bail!("vad.post_speech_silence_ms must be greater than zero");
        }

        if self.hotkey.push_to_talk.trim().is_empty() {
            bail!("hotkey.push_to_talk cannot be empty");
        }

        if self.hotkey.mode_precedence.trim().is_empty() {
            bail!("hotkey.mode_precedence cannot be empty");
        }

        if self.telemetry.level.trim().is_empty() {
            bail!("telemetry.level cannot be empty");
        }

        if !(SPRITE_SCALE_MIN..=SPRITE_SCALE_MAX).contains(&self.sprite.scale) {
            bail!(
                "sprite.scale must be between {} and {}",
                SPRITE_SCALE_MIN,
                SPRITE_SCALE_MAX
            );
        }

        if self.worker.url.trim().is_empty() {
            bail!("worker.url cannot be empty");
        }

        if let Some(threads) = self.whisper.threads {
            if threads == 0 {
                bail!("whisper.threads must be greater than zero when specified");
            }
        }

        Ok(())
    }

    pub fn sync_model_path_from_id(&mut self) {
        if self.model.id.trim().is_empty() {
            self.model.id = default_model_id();
        }
        let filename = WhisperModelInfo::get_model_info(&self.model.id)
            .map(|info| info.filename)
            .unwrap_or_else(|| defaults::STT_MODEL_FILENAME.to_string());
        self.model.path = paths::default_models_dir()
            .join(filename)
            .display()
            .to_string();
    }

    pub fn apply_worker_endpoint(&mut self, worker: &WorkerBridgeConfig) {
        let desired = worker_client::chat_endpoint(worker);
        let desired_norm = normalize_url(&desired);
        let current_norm = normalize_url(&self.worker.url);

        if current_norm == desired_norm {
            if self.worker.url != desired {
                self.worker.url = desired;
            }
            return;
        }

        let default_norm = normalize_url(&default_worker_url());

        let should_replace = self.worker.url.trim().is_empty() || current_norm == default_norm;

        if should_replace {
            self.worker.url = desired;
        }
    }
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            model: ModelConfig {
                id: default_model_id(),
                path: default_model_path(),
                language: LanguageConfig {
                    auto: default_true(),
                    default: default_language(),
                },
                punctuation: default_true(),
            },
            sidecar: SidecarConfig {
                executable_path: default_sidecar_executable_path(),
                python_env: default_sidecar_python_env(),
                startup_timeout_ms: default_sidecar_startup_timeout_ms(),
                communication_timeout_ms: default_sidecar_communication_timeout_ms(),
                access_key: default_sidecar_access_key(),
                wakeword_keyword: None,
                wakeword_sensitivity: None,
                app_user_model_id: None,
            },
            wakeword: WakeWordConfig {
                provider: default_wakeword_provider(),
                keyword: default_wakeword_keyword(),
                sensitivity: default_wakeword_sensitivity(),
            },
            vad: VadConfig {
                aggressiveness: default_vad_aggressiveness(),
                start_grace_ms: default_vad_start_grace_ms(),
                max_segment_ms: default_vad_max_segment_ms(),
                activation_ms: default_vad_activation_ms(),
                post_speech_silence_ms: default_vad_post_speech_ms(),
            },
            hotkey: HotkeyConfig {
                push_to_talk: default_ptt_hotkey(),
                mode_precedence: default_mode_precedence(),
            },
            telemetry: TelemetryConfig {
                level: default_telemetry_level(),
                raw_payload_logs: false,
            },
            whisper: WhisperRuntimeConfig::default(),
            sprite: SpriteConfig::default(),
            worker: WorkerConfig {
                url: default_worker_url(),
                retry: RetryConfig {
                    max: default_retry_max(),
                    base_ms: default_retry_base_ms(),
                },
            },
            brain: BrainConfig::default(),
        }
    }
}

fn serialize_for_save(config: &SttConfig) -> Result<String> {
    let mut serialisable = config.clone();
    serialisable.sidecar.access_key.clear();
    serde_json::to_string_pretty(&serialisable).context("Failed to serialize config")
}

pub fn parse_config(content: &str) -> Result<SttConfig> {
    serde_json::from_str(content).context("Failed to parse STT config")
}

pub fn config_file_fingerprint(config: &SttConfig) -> Option<String> {
    let mut serializable = config.clone();
    serializable.sidecar.access_key.clear();
    serde_json::to_string(&serializable).ok()
}

fn default_language() -> String {
    defaults::STT_DEFAULT_LANGUAGE.to_string()
}

fn default_model_id() -> String {
    defaults::STT_DEFAULT_MODEL_ID.to_string()
}

fn default_true() -> bool {
    true
}

fn default_model_path() -> String {
    let filename = WhisperModelInfo::get_model_info(defaults::STT_DEFAULT_MODEL_ID)
        .map(|info| info.filename)
        .unwrap_or_else(|| defaults::STT_MODEL_FILENAME.to_string());
    paths::default_models_dir()
        .join(filename)
        .display()
        .to_string()
}

fn default_wakeword_keyword() -> String {
    defaults::STT_WAKEWORD_KEYWORD.to_string()
}

fn default_wakeword_sensitivity() -> f32 {
    defaults::STT_WAKEWORD_SENSITIVITY
}

fn default_vad_aggressiveness() -> u8 {
    defaults::STT_DEFAULT_VAD_AGGRESSIVENESS
}

fn default_vad_start_grace_ms() -> u64 {
    defaults::STT_DEFAULT_VAD_START_GRACE_MS
}

fn default_vad_max_segment_ms() -> u64 {
    defaults::STT_DEFAULT_VAD_MAX_SEGMENT_MS
}

fn default_vad_activation_ms() -> u64 {
    defaults::STT_DEFAULT_VAD_ACTIVATION_MS
}

fn default_vad_post_speech_ms() -> u64 {
    defaults::STT_DEFAULT_VAD_POST_SPEECH_MS
}

fn default_ptt_hotkey() -> String {
    defaults::STT_DEFAULT_PTT_HOTKEY.to_string()
}

fn default_mode_precedence() -> String {
    defaults::STT_DEFAULT_MODE_PRECEDENCE.to_string()
}

fn default_telemetry_level() -> String {
    defaults::STT_DEFAULT_TELEMETRY_LEVEL.to_string()
}

fn default_sprite_scale() -> f32 {
    defaults::STT_DEFAULT_SPRITE_SCALE
}

fn default_worker_url() -> String {
    format!(
        "{}/chat",
        crate::config::DEFAULT_WORKER_BASE_URL.trim_end_matches('/')
    )
}

fn normalize_url(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.trim_end_matches('/').to_lowercase()
}

fn default_retry_max() -> u32 {
    defaults::STT_RETRY_MAX
}

fn default_retry_base_ms() -> u64 {
    defaults::STT_RETRY_BASE_MS
}

fn default_sidecar_executable_path() -> String {
    // In production, this is resolved relative to bundled resources
    defaults::STT_SIDECAR_EXECUTABLE_PATH.to_string()
}

fn default_sidecar_python_env() -> Option<String> {
    Some(defaults::STT_SIDECAR_PYTHON_ENV.to_string())
}

fn default_sidecar_startup_timeout_ms() -> u64 {
    defaults::STT_SIDECAR_STARTUP_TIMEOUT_MS
}

fn default_sidecar_communication_timeout_ms() -> u64 {
    defaults::STT_SIDECAR_COMMUNICATION_TIMEOUT_MS
}

fn default_sidecar_access_key() -> String {
    "".to_string()
}

fn default_provider() -> SttProvider {
    SttProvider::Local
}

fn default_wakeword_provider() -> WakeWordProvider {
    WakeWordProvider::Porcupine
}

impl Default for SidecarConfig {
    fn default() -> Self {
        Self {
            executable_path: default_sidecar_executable_path(),
            python_env: default_sidecar_python_env(),
            startup_timeout_ms: default_sidecar_startup_timeout_ms(),
            communication_timeout_ms: default_sidecar_communication_timeout_ms(),
            access_key: default_sidecar_access_key(),
            wakeword_keyword: None,
            wakeword_sensitivity: None,
            app_user_model_id: None,
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            id: default_model_id(),
            path: default_model_path(),
            language: LanguageConfig::default(),
            punctuation: default_true(),
        }
    }
}

impl Default for LanguageConfig {
    fn default() -> Self {
        Self {
            auto: default_true(),
            default: default_language(),
        }
    }
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            provider: default_wakeword_provider(),
            keyword: default_wakeword_keyword(),
            sensitivity: default_wakeword_sensitivity(),
        }
    }
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            aggressiveness: default_vad_aggressiveness(),
            start_grace_ms: default_vad_start_grace_ms(),
            max_segment_ms: default_vad_max_segment_ms(),
            activation_ms: default_vad_activation_ms(),
            post_speech_silence_ms: default_vad_post_speech_ms(),
        }
    }
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            push_to_talk: default_ptt_hotkey(),
            mode_precedence: default_mode_precedence(),
        }
    }
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            level: default_telemetry_level(),
            raw_payload_logs: false,
        }
    }
}

impl Default for SpriteConfig {
    fn default() -> Self {
        Self {
            scale: default_sprite_scale(),
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max: default_retry_max(),
            base_ms: default_retry_base_ms(),
        }
    }
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            url: default_worker_url(),
            retry: RetryConfig::default(),
        }
    }
}

impl WorkerConfig {
    pub fn is_default(&self) -> bool {
        self.url == default_worker_url()
            && self.retry.max == default_retry_max()
            && self.retry.base_ms == default_retry_base_ms()
    }
}

impl SidecarConfig {
    pub fn is_empty(&self) -> bool {
        self.access_key.trim().is_empty()
            && self.wakeword_keyword.is_none()
            && self.wakeword_sensitivity.is_none()
            && self.app_user_model_id.is_none()
    }
}

impl WhisperRuntimeConfig {
    pub fn is_empty(&self) -> bool {
        self.threads.is_none()
    }
}
