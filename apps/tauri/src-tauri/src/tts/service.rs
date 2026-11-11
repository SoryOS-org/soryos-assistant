use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, RwLock,
};

use anyhow::{anyhow, Context, Result};
use async_channel::{bounded, Receiver, Sender};
use tauri::{async_runtime, AppHandle, Emitter, Manager};
use tokio::sync::watch;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, warn};

use crate::config::tts::TtsConfig;
use crate::ui::state_router::{self, StateFlow, StateSignal};
use crate::ui::UiVisualState;

use super::events::{TtsErrorEvent, TtsPipelineStage, TtsStatusEvent};
use super::{piper, playback, EVENT_TTS_ERROR, EVENT_TTS_STATUS};

static NEXT_UTTERANCE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct TtsRequest {
    pub utterance_id: String,
    pub text: String,
}

pub struct TtsService {
    tx: Sender<TtsRequest>,
    shutdown_tx: watch::Sender<bool>,
    worker: async_runtime::JoinHandle<()>,
}

impl TtsService {
    pub fn start(app_handle: AppHandle, config: Arc<RwLock<TtsConfig>>) -> Result<Self> {
        let (tx, rx) = bounded::<TtsRequest>(8);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let worker = async_runtime::spawn(run_worker(app_handle, config, rx, shutdown_rx));

        Ok(Self {
            tx,
            shutdown_tx,
            worker,
        })
    }

    pub fn enqueue_text(&self, text: String) -> Result<String> {
        let id = NEXT_UTTERANCE_ID.fetch_add(1, Ordering::Relaxed);
        let utterance = format!("utt-{id}");
        self.tx
            .try_send(TtsRequest {
                utterance_id: utterance.clone(),
                text,
            })
            .map_err(|err| anyhow!("Failed to enqueue TTS request: {err}"))?;
        Ok(utterance)
    }

    pub fn stop(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

impl Drop for TtsService {
    fn drop(&mut self) {
        self.stop();
        self.worker.abort();
    }
}

async fn run_worker(
    app_handle: AppHandle,
    config: Arc<RwLock<TtsConfig>>,
    rx: Receiver<TtsRequest>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                debug!("TTS worker received shutdown");
                break;
            }
            cmd = rx.recv() => {
                match cmd {
            Ok(request) => {
                let request_for_error = request.clone();
                if let Err(err) = process_request(&app_handle, &config, request).await {
                    error!("TTS request failed: {err:?}");
                    emit_error(
                        &app_handle,
                        TtsErrorEvent {
                            utterance_id: request_for_error.utterance_id,
                            message: err.to_string(),
                        },
                    );
                }
            }
            Err(_) => break,
        }
            }
        }
    }
}

async fn process_request(
    app_handle: &AppHandle,
    config: &Arc<RwLock<TtsConfig>>,
    request: TtsRequest,
) -> Result<()> {
    let snapshot = config
        .read()
        .map(|guard| guard.clone())
        .map_err(|_| anyhow!("TTS config lock poisoned"))?;

    if request.text.trim().is_empty() {
        debug!("Skipping empty TTS request {}", request.utterance_id);
        state_router::clear_flow(app_handle, StateFlow::Tts);
        return Ok(());
    }

    let chunks = chunk_text(&request.text, snapshot.piper.chunk_chars);
    if chunks.is_empty() {
        debug!(
            "No non-empty chunks from TTS request {}",
            request.utterance_id
        );
        state_router::clear_flow(app_handle, StateFlow::Tts);
        return Ok(());
    }

    emit_status(
        app_handle,
        TtsStatusEvent {
            utterance_id: request.utterance_id.clone(),
            stage: TtsPipelineStage::Pending,
            chunk_index: None,
            chunk_count: Some(chunks.len()),
            details: None,
        },
    );

    let runtime = piper::PiperRuntime::from_config(&snapshot.piper);
    let cache_dir = resolve_cache_dir(app_handle)?;

    // High-level TTS lifecycle: thinking while synthesizing.
    state_router::emit(
        app_handle,
        StateSignal::new(UiVisualState::Thinking, StateFlow::Tts),
    );

    for (idx, chunk) in chunks.iter().enumerate() {
        emit_status(
            app_handle,
            TtsStatusEvent {
                utterance_id: request.utterance_id.clone(),
                stage: TtsPipelineStage::Synthesizing,
                chunk_index: Some(idx),
                chunk_count: Some(chunks.len()),
                details: None,
            },
        );

        let output_path = cache_dir.join(format!("{}-{:02}.wav", &request.utterance_id, idx + 1));

        piper::synthesize_to_file(&runtime, chunk, &output_path).await?;
        debug!("Synthesized chunk stored at {}", output_path.display());

        emit_status(
            app_handle,
            TtsStatusEvent {
                utterance_id: request.utterance_id.clone(),
                stage: TtsPipelineStage::Playing,
                chunk_index: Some(idx),
                chunk_count: Some(chunks.len()),
                details: Some(chunk.clone()),
            },
        );

        state_router::emit(
            app_handle,
            StateSignal::new(UiVisualState::Speaking, StateFlow::Tts),
        );

        let volume = snapshot.playback.volume;
        let device_hint = snapshot.playback.device.clone();
        let playback_path = output_path.clone();
        tokio::task::spawn_blocking(move || {
            playback::play_file(&playback_path, volume, device_hint.as_deref())
        })
        .await
        .map_err(|err| anyhow!("Failed to join playback task: {err}"))??;

        if snapshot.piper.inter_chunk_gap_ms > 0 && idx + 1 < chunks.len() {
            sleep(Duration::from_millis(snapshot.piper.inter_chunk_gap_ms)).await;
        }

        if snapshot.piper.retain_chunks {
            debug!("Retaining synthesized chunk at {}", output_path.display());
        } else if let Err(err) = std::fs::remove_file(&output_path) {
            debug!(
                "Failed to remove temporary chunk {}: {err}",
                output_path.display()
            );
        }
    }

    emit_status(
        app_handle,
        TtsStatusEvent {
            utterance_id: request.utterance_id.clone(),
            stage: TtsPipelineStage::Completed,
            chunk_index: Some(chunks.len().saturating_sub(1)),
            chunk_count: Some(chunks.len()),
            details: None,
        },
    );
    state_router::clear_flow(app_handle, StateFlow::Tts);

    Ok(())
}

fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return Vec::new();
    }

    let sentences = split_sentences(text);
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for sentence in sentences {
        if sentence.is_empty() {
            continue;
        }
        let trimmed = sentence.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.len() > max_chars {
            if !current.is_empty() {
                chunks.push(current.clone());
                current.clear();
            }
            chunks.extend(chunk_by_words(trimmed, max_chars));
            continue;
        }

        if current.is_empty() {
            current.push_str(trimmed);
            continue;
        }

        let candidate_len = current.len() + 1 + trimmed.len();
        if candidate_len > max_chars {
            chunks.push(current.clone());
            current.clear();
            current.push_str(trimmed);
        } else {
            current.push(' ');
            current.push_str(trimmed);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    if chunks.is_empty() && !text.trim().is_empty() {
        chunks.extend(chunk_by_words(text.trim(), max_chars));
    }

    chunks
}

fn chunk_by_words(text: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for token in text.split_whitespace() {
        if current.is_empty() {
            current.push_str(token);
            continue;
        }

        let candidate_len = current.len() + 1 + token.len();
        if candidate_len > max_chars {
            chunks.push(current.clone());
            current.clear();
            current.push_str(token);
        } else {
            current.push(' ');
            current.push_str(token);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    if chunks.is_empty() && !text.is_empty() {
        chunks.push(text.to_string());
    }

    chunks
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            // absorb any trailing whitespace
            while let Some(next) = chars.peek() {
                if next.is_whitespace() {
                    current.push(*next);
                    chars.next();
                } else {
                    break;
                }
            }
            sentences.push(current.clone());
            current.clear();
        }
    }

    if !current.trim().is_empty() {
        sentences.push(current);
    }

    if sentences.is_empty() && !text.trim().is_empty() {
        sentences.push(text.to_string());
    }

    sentences
}

fn resolve_cache_dir(app_handle: &AppHandle) -> Result<PathBuf> {
    let dir = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tts-cache")
    } else {
        app_handle
            .path()
            .resolve("tts/cache", tauri::path::BaseDirectory::AppData)
            .context("Failed to resolve TTS cache directory")?
    };

    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create TTS cache directory {}", dir.display()))?;

    Ok(dir)
}

fn emit_status(app_handle: &AppHandle, payload: TtsStatusEvent) {
    debug!(
        "TTS status: {:?} chunk={:?}/{:?}",
        payload.stage, payload.chunk_index, payload.chunk_count
    );
    if let Some(metrics) = app_handle.try_state::<crate::metrics::MetricsHandle>() {
        let err_detail = if matches!(payload.stage, TtsPipelineStage::Error) {
            payload.details.as_deref()
        } else {
            None
        };
        metrics.mark_tts_status(&payload.utterance_id, payload.stage.as_str(), err_detail);
    }
    match payload.stage {
        TtsPipelineStage::Completed => {
            if let Some(stt) = app_handle.try_state::<crate::stt::SttService>() {
                stt.consume_followup_and_listen(&payload.utterance_id);
            }
        }
        TtsPipelineStage::Error => {
            if let Some(stt) = app_handle.try_state::<crate::stt::SttService>() {
                stt.clear_followup_request(&payload.utterance_id);
            }
        }
        _ => {}
    }
    let _ = app_handle.emit(EVENT_TTS_STATUS, payload);
}

pub fn emit_error(app_handle: &AppHandle, event: TtsErrorEvent) {
    warn!("TTS error: {} ({})", event.message, event.utterance_id);
    let status = TtsStatusEvent {
        utterance_id: event.utterance_id.clone(),
        stage: TtsPipelineStage::Error,
        chunk_index: None,
        chunk_count: None,
        details: Some(event.message.clone()),
    };
    state_router::emit(
        app_handle,
        StateSignal::new(UiVisualState::Error, StateFlow::Tts),
    );
    let _ = app_handle.emit(EVENT_TTS_STATUS, status);
    let _ = app_handle.emit(EVENT_TTS_ERROR, event);
}
