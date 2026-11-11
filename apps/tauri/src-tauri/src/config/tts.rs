use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tauri::{App, AppHandle};

use super::{defaults, io, paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TtsProvider {
    #[default]
    Local,
    Api,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    #[serde(default = "default_provider")]
    pub provider: TtsProvider,
    #[serde(default)]
    pub piper: PiperConfig,
    #[serde(default)]
    pub playback: PlaybackConfig,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            piper: PiperConfig::default(),
            playback: PlaybackConfig::default(),
        }
    }
}

impl TtsConfig {
    pub fn load(app: &App) -> Result<Self> {
        let config = io::load_or_create(
            app,
            "tts.config.json",
            |content| {
                serde_json::from_str(content)
                    .with_context(|| "Failed to parse TTS configuration; falling back to defaults")
            },
            serialize_for_save,
            TtsConfig::default,
        )?;

        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if matches!(self.provider, TtsProvider::Local) {
            self.piper.validate()?;
        }
        self.playback.validate()?;
        Ok(())
    }

    pub fn save_to_handle(&self, app_handle: &AppHandle) -> Result<()> {
        io::save_to_handle(app_handle, "tts.config.json", self, serialize_for_save)
    }
}

fn serialize_for_save(config: &TtsConfig) -> Result<String> {
    serde_json::to_string_pretty(config).context("Failed to serialise TTS configuration")
}

pub fn parse_config(content: &str) -> Result<TtsConfig> {
    serde_json::from_str(content).context("Failed to parse TTS config")
}

pub fn config_file_fingerprint(config: &TtsConfig) -> Option<String> {
    serde_json::to_string(config).ok()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiperConfig {
    #[serde(default = "default_piper_executable", skip_serializing)]
    pub executable_path: String,
    #[serde(default = "default_piper_model", skip_serializing)]
    pub model_path: String,
    #[serde(default = "default_piper_metadata", skip_serializing)]
    pub metadata_path: String,
    #[serde(default)]
    pub download: PiperDownloadConfig,
    #[serde(default = "default_chunk_chars", skip_serializing)]
    pub chunk_chars: usize,
    #[serde(default = "default_chunk_gap_ms", skip_serializing)]
    pub inter_chunk_gap_ms: u64,
    #[serde(default)]
    pub retain_chunks: bool,
}

impl Default for PiperConfig {
    fn default() -> Self {
        Self {
            executable_path: default_piper_executable(),
            model_path: default_piper_model(),
            metadata_path: default_piper_metadata(),
            download: PiperDownloadConfig::default(),
            chunk_chars: default_chunk_chars(),
            inter_chunk_gap_ms: default_chunk_gap_ms(),
            retain_chunks: false,
        }
    }
}

impl PiperConfig {
    pub fn validate(&self) -> Result<()> {
        if self.executable_path.trim().is_empty() {
            bail!("piper.executable_path cannot be empty");
        }
        if self.model_path.trim().is_empty() {
            bail!("piper.model_path cannot be empty");
        }
        if self.metadata_path.trim().is_empty() {
            bail!("piper.metadata_path cannot be empty");
        }
        if self.chunk_chars == 0 {
            bail!("piper.chunk_chars must be greater than zero");
        }
        self.download.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiperDownloadConfig {
    #[serde(default = "default_voice_repo_id")]
    pub repo_id: String,
    #[serde(default = "default_voice_revision")]
    pub revision: String,
    #[serde(default = "default_voice_repo_path")]
    pub voice: String,
}

impl Default for PiperDownloadConfig {
    fn default() -> Self {
        Self {
            repo_id: default_voice_repo_id(),
            revision: default_voice_revision(),
            voice: default_voice_repo_path(),
        }
    }
}

impl PiperDownloadConfig {
    pub fn validate(&self) -> Result<()> {
        if self.voice.trim().is_empty() {
            bail!("piper.download.voice cannot be empty");
        }
        if self.repo_id.trim().is_empty() {
            bail!("piper.download.repo_id cannot be empty");
        }
        if self.revision.trim().is_empty() {
            bail!("piper.download.revision cannot be empty");
        }
        Ok(())
    }

    pub fn hf_model_repo_path(&self) -> String {
        format!("{}.onnx", self.voice_base())
    }

    pub fn hf_metadata_repo_path(&self) -> String {
        format!("{}.onnx.json", self.voice_base())
    }

    pub fn voice_base(&self) -> &str {
        self.voice
            .trim_end_matches(".onnx.json")
            .trim_end_matches(".onnx")
    }
}

fn default_voice_repo_id() -> String {
    defaults::TTS_DEFAULT_VOICE_REPO_ID.to_string()
}

fn default_voice_revision() -> String {
    defaults::TTS_DEFAULT_VOICE_REVISION.to_string()
}

fn default_voice_repo_path() -> String {
    defaults::TTS_DEFAULT_VOICE_PATH.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default = "default_crossfade_ms")]
    pub crossfade_ms: u64,
}

impl Default for PlaybackConfig {
    fn default() -> Self {
        Self {
            device: None,
            volume: default_volume(),
            crossfade_ms: default_crossfade_ms(),
        }
    }
}

impl PlaybackConfig {
    fn validate(&self) -> Result<()> {
        if !(0.0..=2.0).contains(&self.volume) {
            bail!("playback.volume must be between 0.0 and 2.0");
        }
        Ok(())
    }
}

fn default_provider() -> TtsProvider {
    TtsProvider::Local
}

fn default_piper_executable() -> String {
    paths::default_piper_assets_dir()
        .join(platform_binary_name())
        .display()
        .to_string()
}

fn default_piper_model() -> String {
    paths::default_piper_assets_dir()
        .join(format!("{}.onnx", defaults::TTS_DEFAULT_VOICE_BASENAME))
        .display()
        .to_string()
}

fn default_piper_metadata() -> String {
    paths::default_piper_assets_dir()
        .join(format!(
            "{}.onnx.json",
            defaults::TTS_DEFAULT_VOICE_BASENAME
        ))
        .display()
        .to_string()
}

fn default_chunk_chars() -> usize {
    defaults::TTS_PIPER_CHUNK_CHARS
}

fn default_chunk_gap_ms() -> u64 {
    defaults::TTS_PIPER_INTER_CHUNK_GAP_MS
}

fn default_volume() -> f32 {
    defaults::TTS_DEFAULT_VOLUME
}

fn default_crossfade_ms() -> u64 {
    defaults::TTS_DEFAULT_CROSSFADE_MS
}

fn platform_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "piper.exe"
    } else {
        "piper"
    }
}
