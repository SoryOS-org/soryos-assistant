use crate::config::stt::SttConfig;
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use tauri::{AppHandle, Emitter};
use tracing::info;

use super::model_downloader::{ModelDownloader, WhisperModelInfo};
use super::EVENT_PRELOAD_STATUS;

pub struct PreloadManager {
    app_handle: AppHandle,
    steps: Vec<PreloadStep>,
}

static PRELOAD_SNAPSHOT: OnceLock<Mutex<Option<PreloadStatusPayload>>> = OnceLock::new();

type PreloadRunner = Box<dyn Fn(&AppHandle) -> Result<Option<SttConfig>> + Send + Sync>;

struct PreloadStep {
    name: &'static str,
    runner: PreloadRunner,
}

impl PreloadStep {
    fn new<F>(name: &'static str, runner: F) -> Self
    where
        F: Fn(&AppHandle) -> Result<Option<SttConfig>> + Send + Sync + 'static,
    {
        Self {
            name,
            runner: Box::new(runner),
        }
    }
}

impl PreloadManager {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            steps: Vec::new(),
        }
    }

    pub fn with_stt_model(mut self, config: SttConfig) -> Result<Self> {
        let step_config = config.clone();
        self.steps
            .push(PreloadStep::new("ensure_stt_model", move |handle| {
                ensure_model_for_config(handle, &step_config)
            }));
        Ok(self)
    }

    pub fn execute(&self) -> Result<Option<SttConfig>> {
        let mut updated_config: Option<SttConfig> = None;

        for step in &self.steps {
            info!("Running preload step: {}", step.name);
            let result = (step.runner)(&self.app_handle)?;
            if let Some(config) = result {
                updated_config = Some(config);
            }
        }

        Ok(updated_config)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PreloadStatusPayload {
    stage: &'static str,
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

fn emit_status(app_handle: &AppHandle, payload: PreloadStatusPayload) {
    if let Some(lock) = PRELOAD_SNAPSHOT.get() {
        if let Ok(mut guard) = lock.lock() {
            *guard = Some(payload.clone());
        }
    } else {
        let _ = PRELOAD_SNAPSHOT.set(Mutex::new(Some(payload.clone())));
    }

    let _ = app_handle.emit(EVENT_PRELOAD_STATUS, payload);
}

fn ensure_model_for_config(
    app_handle: &AppHandle,
    config: &SttConfig,
) -> Result<Option<SttConfig>> {
    let model_path = PathBuf::from(&config.model.path);

    let model_filename = model_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("Invalid model filename: {}", model_path.display()))?;

    let model_name = infer_model_name(model_filename)?;
    let model_info = WhisperModelInfo::get_model_info(&model_name)
        .ok_or_else(|| anyhow!("Unknown model requested: {}", model_name))?;

    let models_dir = crate::config::paths::models_dir_from_handle(app_handle)?;
    let desired_path = models_dir.join(&model_info.filename);

    if desired_path.exists() {
        info!(
            "Whisper model already present at {}",
            desired_path.display()
        );
        emit_status(
            app_handle,
            PreloadStatusPayload {
                stage: "model",
                state: "cached",
                model: Some(model_name.clone()),
                progress: Some(1.0),
                details: None,
            },
        );

        let store_path = if cfg!(debug_assertions) && config.model.path.starts_with("resources/") {
            config.model.path.clone()
        } else {
            desired_path.to_string_lossy().into_owned()
        };

        if store_path == config.model.path {
            return Ok(None);
        }

        let mut new_config = config.clone();
        new_config.model.path = store_path;
        return Ok(Some(new_config));
    }

    emit_status(
        app_handle,
        PreloadStatusPayload {
            stage: "model",
            state: "started",
            model: Some(model_name.clone()),
            progress: Some(0.0),
            details: None,
        },
    );

    let downloader = ModelDownloader::new(&models_dir)?;
    let mut progress = |downloaded: u64, total: Option<u64>| {
        let percent = total
            .and_then(|t| (t > 0).then_some((downloaded as f32 / t as f32).min(1.0)))
            .unwrap_or(0.0);
        emit_status(
            app_handle,
            PreloadStatusPayload {
                stage: "model",
                state: "progress",
                model: Some(model_name.clone()),
                progress: Some(percent),
                details: None,
            },
        );
    };

    match downloader.ensure_model_with_progress(&model_name, Some(&mut progress)) {
        Ok(downloaded_path) => {
            info!("Whisper model ready at {}", downloaded_path.display());
            emit_status(
                app_handle,
                PreloadStatusPayload {
                    stage: "model",
                    state: "completed",
                    model: Some(model_name.clone()),
                    progress: Some(1.0),
                    details: Some(downloaded_path.display().to_string()),
                },
            );

            let store_path =
                if cfg!(debug_assertions) && config.model.path.starts_with("resources/") {
                    config.model.path.clone()
                } else {
                    downloaded_path.to_string_lossy().into_owned()
                };

            if store_path == config.model.path {
                Ok(None)
            } else {
                let mut new_config = config.clone();
                new_config.model.path = store_path;
                Ok(Some(new_config))
            }
        }
        Err(err) => {
            emit_status(
                app_handle,
                PreloadStatusPayload {
                    stage: "model",
                    state: "error",
                    model: Some(model_name),
                    progress: None,
                    details: Some(err.to_string()),
                },
            );
            Err(err)
        }
    }
}

fn infer_model_name(filename: &str) -> Result<String> {
    if let Some(info) = WhisperModelInfo::get_model_info_by_filename(filename) {
        return Ok(info.name);
    }

    if let Some(name) = filename
        .strip_prefix("ggml-")
        .and_then(|value| value.strip_suffix(".bin"))
    {
        return Ok(name.to_string());
    }

    Err(anyhow!(
        "Unable to infer model name from filename '{}'",
        filename
    ))
}

pub fn current_status() -> Option<PreloadStatusPayload> {
    PRELOAD_SNAPSHOT
        .get()
        .and_then(|lock| lock.lock().ok().and_then(|guard| guard.clone()))
}

pub fn prepare_stt(app_handle: &AppHandle, config: &SttConfig) -> Result<Option<SttConfig>> {
    PreloadManager::new(app_handle.clone())
        .with_stt_model(config.clone())?
        .execute()
}
