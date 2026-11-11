use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use tauri::{path::BaseDirectory, AppHandle, Emitter, Manager};
use tracing::debug;

use crate::config::paths;
use crate::config::tts::{self, TtsConfig, TtsProvider};
use crate::download::download_file_hf;

use super::events::PreloadStatusPayload;
use super::EVENT_TTS_PRELOAD_STATUS;

const BINARY_STAGE: &str = "binary";
const MODEL_STAGE: &str = "model";
const METADATA_STAGE: &str = "metadata";

#[derive(Copy, Clone)]
enum VoiceAssetKind {
    Model,
    Metadata,
}

pub fn prepare_tts(app_handle: &AppHandle, config: &TtsConfig) -> Result<Option<TtsConfig>> {
    if !matches!(config.provider, TtsProvider::Local) {
        return Ok(None);
    }

    let assets_dir = paths::piper_assets_dir_from_handle(app_handle)?;
    let mut updated = config.clone();
    let mut changed = false;

    changed |= ensure_piper_binary(app_handle, &mut updated, &assets_dir)?;
    changed |= ensure_voice_asset(
        app_handle,
        MODEL_STAGE,
        &mut updated.piper.model_path,
        &updated.piper.download,
        VoiceAssetKind::Model,
        &assets_dir,
    )?;
    changed |= ensure_voice_asset(
        app_handle,
        METADATA_STAGE,
        &mut updated.piper.metadata_path,
        &updated.piper.download,
        VoiceAssetKind::Metadata,
        &assets_dir,
    )?;

    if changed {
        Ok(Some(updated))
    } else {
        Ok(None)
    }
}

fn ensure_piper_binary(
    app_handle: &AppHandle,
    config: &mut TtsConfig,
    assets_dir: &Path,
) -> Result<bool> {
    let mut changed = false;
    let current_path = PathBuf::from(&config.piper.executable_path);
    emit_status(
        app_handle,
        PreloadStatusPayload {
            stage: BINARY_STAGE,
            state: "checking",
            asset: Some("piper_binary".to_string()),
            progress: None,
            details: Some(current_path.display().to_string()),
        },
    );

    if let Some(resolved) = canonical_if_exists(&current_path) {
        emit_status(
            app_handle,
            PreloadStatusPayload {
                stage: BINARY_STAGE,
                state: "present",
                asset: Some("piper_binary".to_string()),
                progress: Some(1.0),
                details: Some(resolved.display().to_string()),
            },
        );
        changed |= update_path_if_changed(&mut config.piper.executable_path, &resolved);
        return Ok(changed);
    }

    let binary_name = current_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("Invalid Piper executable path {}", current_path.display()))?;

    let roaming_path = assets_dir.join(binary_name);
    if let Some(resolved) = canonical_if_exists(&roaming_path) {
        emit_status(
            app_handle,
            PreloadStatusPayload {
                stage: BINARY_STAGE,
                state: "present",
                asset: Some("piper_binary".to_string()),
                progress: Some(1.0),
                details: Some(resolved.display().to_string()),
            },
        );
        changed |= update_path_if_changed(&mut config.piper.executable_path, &resolved);
        return Ok(changed);
    }

    if let Some(resource_path) = locate_resource_file(app_handle, binary_name) {
        emit_status(
            app_handle,
            PreloadStatusPayload {
                stage: BINARY_STAGE,
                state: "present",
                asset: Some("piper_binary".to_string()),
                progress: Some(1.0),
                details: Some(resource_path.display().to_string()),
            },
        );
        changed |= update_path_if_changed(&mut config.piper.executable_path, &resource_path);
        return Ok(changed);
    }

    emit_status(
        app_handle,
        PreloadStatusPayload {
            stage: BINARY_STAGE,
            state: "missing",
            asset: Some("piper_binary".to_string()),
            progress: None,
            details: Some(format!(
                "Piper executable missing at {} and no bundled copy was found",
                config.piper.executable_path
            )),
        },
    );

    Err(anyhow!(
        "Piper executable missing at {} and no bundled copy available",
        config.piper.executable_path
    ))
}

fn ensure_voice_asset(
    app_handle: &AppHandle,
    stage: &'static str,
    path_str: &mut String,
    download: &tts::PiperDownloadConfig,
    kind: VoiceAssetKind,
    assets_dir: &Path,
) -> Result<bool> {
    let current_path = PathBuf::from(&*path_str);
    let filename = current_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("Invalid Piper asset path {}", current_path.display()))?;

    emit_status(
        app_handle,
        PreloadStatusPayload {
            stage,
            state: "checking",
            asset: Some(stage.to_string()),
            progress: None,
            details: Some(current_path.display().to_string()),
        },
    );

    if let Some(resolved) = canonical_if_exists(&current_path) {
        emit_status(
            app_handle,
            PreloadStatusPayload {
                stage,
                state: "present",
                asset: Some(stage.to_string()),
                progress: Some(1.0),
                details: Some(resolved.display().to_string()),
            },
        );
        return Ok(update_path_if_changed(path_str, &resolved));
    }

    let roaming_path = assets_dir.join(filename);
    if let Some(resolved) = canonical_if_exists(&roaming_path) {
        emit_status(
            app_handle,
            PreloadStatusPayload {
                stage,
                state: "present",
                asset: Some(stage.to_string()),
                progress: Some(1.0),
                details: Some(resolved.display().to_string()),
            },
        );
        return Ok(update_path_if_changed(path_str, &resolved));
    }

    if let Some(resource_path) = locate_resource_file(app_handle, filename) {
        emit_status(
            app_handle,
            PreloadStatusPayload {
                stage,
                state: "present",
                asset: Some(stage.to_string()),
                progress: Some(1.0),
                details: Some(resource_path.display().to_string()),
            },
        );
        return Ok(update_path_if_changed(path_str, &resource_path));
    }

    let download_target = roaming_path;
    if let Some(parent) = download_target.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create directory for Piper asset: {}",
                parent.display()
            )
        })?;
    }

    let mut progress_cb = |downloaded: u64, total: Option<u64>| {
        let percent = total
            .and_then(|t| (t > 0).then_some((downloaded as f32 / t as f32).min(1.0)))
            .unwrap_or(0.0);
        emit_status(
            app_handle,
            PreloadStatusPayload {
                stage,
                state: "progress",
                asset: Some(stage.to_string()),
                progress: Some(percent),
                details: None,
            },
        );
    };

    let repo_path = match kind {
        VoiceAssetKind::Model => download.hf_model_repo_path(),
        VoiceAssetKind::Metadata => download.hf_metadata_repo_path(),
    };
    let detail_path = format!(
        "hf://{}/{}@{}",
        download.repo_id,
        repo_path.as_str(),
        download.revision
    );
    emit_status(
        app_handle,
        PreloadStatusPayload {
            stage,
            state: "started",
            asset: Some(stage.to_string()),
            progress: Some(0.0),
            details: Some(detail_path),
        },
    );
    download_file_hf(
        &download.repo_id,
        hf_hub::RepoType::Dataset,
        Some(download.revision.as_str()),
        repo_path.as_str(),
        &download_target,
        Some(&mut progress_cb),
    )?;

    emit_status(
        app_handle,
        PreloadStatusPayload {
            stage,
            state: "completed",
            asset: Some(stage.to_string()),
            progress: Some(1.0),
            details: Some(download_target.display().to_string()),
        },
    );

    update_path_if_changed(path_str, &download_target);

    Ok(true)
}

fn canonical_if_exists(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        path.canonicalize()
            .ok()
            .or_else(|| Some(path.to_path_buf()))
    } else {
        None
    }
}

fn update_path_if_changed(target: &mut String, new_path: &Path) -> bool {
    let new_value = new_path.display().to_string();
    if *target != new_value {
        *target = new_value;
        true
    } else {
        false
    }
}

fn locate_resource_file(app_handle: &AppHandle, filename: &str) -> Option<PathBuf> {
    let resource_relative = PathBuf::from("piper").join(filename);
    if let Ok(path) = app_handle
        .path()
        .resolve(&resource_relative, BaseDirectory::Resource)
    {
        if path.exists() {
            return Some(path);
        }
    }

    let dev_path = PathBuf::from("resources").join("piper").join(filename);
    if dev_path.exists() {
        return Some(dev_path);
    }

    None
}

fn emit_status(app_handle: &AppHandle, payload: PreloadStatusPayload) {
    debug!(
        "TTS preload stage={} state={} asset={:?} progress={:?}",
        payload.stage, payload.state, payload.asset, payload.progress
    );
    let _ = app_handle.emit(EVENT_TTS_PRELOAD_STATUS, payload);
}
