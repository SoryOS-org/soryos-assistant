use serde::Serialize;
use std::sync::{Arc, RwLock};

use crate::secret_store;
use crate::stt::model_downloader;

use super::mcp;
use super::stt;
use super::tts;
use super::ui;
use super::worker::WorkerBridgeConfig;
use super::DEFAULT_WORKER_BASE_URL;

#[derive(Debug, Clone, Serialize)]
pub struct ConfigSummary {
    pub stt: SttSummary,
    pub tts: TtsSummary,
    pub worker: WorkerSummary,
    pub ui: UiSummary,
    pub models: Vec<ModelCatalogEntrySummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp: Option<McpSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SttSummary {
    pub provider: stt::SttProvider,
    pub model: ModelSummary,
    pub wakeword: stt::WakeWordConfig,
    pub vad: stt::VadConfig,
    pub hotkey: stt::HotkeyConfig,
    pub telemetry: stt::TelemetryConfig,
    pub sidecar: SidecarSummary,
    pub whisper: WhisperSummary,
    pub sprite: SpriteSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct TtsSummary {
    pub provider: tts::TtsProvider,
    pub voice_model: String,
    pub voice_metadata: String,
    pub chunk_chars: usize,
    pub volume: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelSummary {
    pub id: String,
    pub path: String,
    pub language: stt::LanguageConfig,
    pub punctuation: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SidecarSummary {
    pub executable_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python_env: Option<String>,
    pub startup_timeout_ms: u64,
    pub communication_timeout_ms: u64,
    pub has_access_key: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WhisperSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<u32>,
    pub recommended_threads: u32,
    pub physical_cores: u32,
    pub logical_cores: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpriteSummary {
    pub scale: f32,
    pub min_scale: f32,
    pub max_scale: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelCatalogEntrySummary {
    pub id: String,
    pub filename: String,
    pub disk_mb: u32,
    pub label: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkerSummary {
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiSummary {
    pub panel_enabled: bool,
    pub main_window_taskbar: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpSummary {
    pub server_count: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<McpServerSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpServerSummary {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub enabled: bool,
    pub transport: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigUpdateResponse {
    pub summary: ConfigSummary,
    pub diff: serde_json::Value,
}

impl From<model_downloader::ModelCatalogEntry> for ModelCatalogEntrySummary {
    fn from(entry: model_downloader::ModelCatalogEntry) -> Self {
        Self {
            id: entry.id,
            filename: entry.filename,
            disk_mb: entry.disk_mb,
            label: entry.label,
            path: entry.path,
        }
    }
}

impl From<&stt::SttConfig> for SttSummary {
    fn from(config: &stt::SttConfig) -> Self {
        Self {
            provider: config.provider,
            model: ModelSummary::from(&config.model),
            wakeword: config.wakeword.clone(),
            vad: config.vad.clone(),
            hotkey: config.hotkey.clone(),
            telemetry: config.telemetry.clone(),
            sidecar: SidecarSummary::from(&config.sidecar),
            whisper: WhisperSummary::from(&config.whisper),
            sprite: SpriteSummary::from(&config.sprite),
        }
    }
}

impl From<&tts::TtsConfig> for TtsSummary {
    fn from(config: &tts::TtsConfig) -> Self {
        Self {
            provider: config.provider,
            voice_model: config.piper.model_path.clone(),
            voice_metadata: config.piper.metadata_path.clone(),
            chunk_chars: config.piper.chunk_chars,
            volume: config.playback.volume,
        }
    }
}

impl From<&stt::ModelConfig> for ModelSummary {
    fn from(config: &stt::ModelConfig) -> Self {
        Self {
            id: config.id.clone(),
            path: config.path.clone(),
            language: config.language.clone(),
            punctuation: config.punctuation,
        }
    }
}

impl From<&stt::SidecarConfig> for SidecarSummary {
    fn from(config: &stt::SidecarConfig) -> Self {
        Self {
            executable_path: config.executable_path.clone(),
            python_env: config.python_env.clone(),
            startup_timeout_ms: config.startup_timeout_ms,
            communication_timeout_ms: config.communication_timeout_ms,
            has_access_key: !config.access_key.trim().is_empty()
                || secret_store::has_picovoice_key(),
        }
    }
}

impl From<&stt::WhisperRuntimeConfig> for WhisperSummary {
    fn from(config: &stt::WhisperRuntimeConfig) -> Self {
        let physical = num_cpus::get_physical().max(1) as u32;
        let logical = num_cpus::get().max(1) as u32;
        let recommended = std::cmp::max(1, physical / 2);
        Self {
            threads: config.threads,
            recommended_threads: recommended,
            physical_cores: physical,
            logical_cores: logical,
        }
    }
}

impl From<&stt::SpriteConfig> for SpriteSummary {
    fn from(config: &stt::SpriteConfig) -> Self {
        Self {
            scale: config.scale,
            min_scale: stt::SPRITE_SCALE_MIN,
            max_scale: stt::SPRITE_SCALE_MAX,
        }
    }
}

impl From<&WorkerBridgeConfig> for WorkerSummary {
    fn from(config: &WorkerBridgeConfig) -> Self {
        Self {
            base_url: config.base_url().to_string(),
        }
    }
}

impl WorkerSummary {
    pub(crate) fn from_current(config: &Arc<RwLock<WorkerBridgeConfig>>) -> Self {
        config
            .read()
            .map(|guard| WorkerSummary::from(&*guard))
            .unwrap_or_else(|_| WorkerSummary {
                base_url: DEFAULT_WORKER_BASE_URL.to_string(),
            })
    }
}

impl From<&ui::UiRuntimeConfig> for UiSummary {
    fn from(config: &ui::UiRuntimeConfig) -> Self {
        Self {
            panel_enabled: config.panel_enabled(),
            main_window_taskbar: config.show_main_in_taskbar(),
        }
    }
}

impl From<&mcp::McpConfig> for McpSummary {
    fn from(config: &mcp::McpConfig) -> Self {
        let servers = config
            .servers
            .iter()
            .map(|(id, server)| McpServerSummary {
                id: id.clone(),
                display_name: server.display_name.clone(),
                enabled: server.enabled,
                transport: match server.transport {
                    mcp_host::TransportConfig::Stdio { .. } => "stdio",
                    mcp_host::TransportConfig::Ws { .. } => "ws",
                    mcp_host::TransportConfig::Http { .. } => "http",
                }
                .to_string(),
            })
            .collect::<Vec<_>>();
        Self {
            server_count: servers.len(),
            servers,
        }
    }
}
