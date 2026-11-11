use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use anyhow::{anyhow, Result};
use async_channel::Sender;
use serde_json::json;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tracing::{debug, error, info, trace, warn};

use crate::{
    stt::{
        audio_pipeline::{
            AudioPipeline, AudioPipelineConfig, AudioState, DEFAULT_MIN_TRANSCRIPT_CHARS,
        },
        events::TranscriptionEvent,
        sidecar::{AsyncPythonSidecar, SidecarConfig, WakeWordEvent},
        SttConfig, WakeWordProvider,
    },
    ui::{
        state_router::{self, StateFlow, StateSignal},
        UiEventSender, UiEventService, UiVisualState,
    },
};

use super::{events::SttReadyEvent, SttCommand, EVENT_STT_READY, EVENT_STT_STARTED};

pub struct HybridSttService {
    config: Arc<RwLock<SttConfig>>,
    pipeline: Arc<AudioPipeline>,
    transcript_tx: Sender<TranscriptionEvent>,
    app_handle: AppHandle,
    sidecar: Option<AsyncPythonSidecar>,
    audio_task: Option<JoinHandle<()>>,
    state_events: Option<mpsc::UnboundedReceiver<AudioState>>,
    ui_sender: Option<UiEventSender>,
}

impl HybridSttService {
    pub async fn new(
        config: Arc<RwLock<SttConfig>>,
        transcript_tx: Sender<TranscriptionEvent>,
        app_handle: AppHandle,
        app_user_model_id: String,
    ) -> Result<Self> {
        info!("Initialising hybrid STT service");
        let ui_sender = app_handle
            .try_state::<UiEventService>()
            .map(|state| state.sender());
        if ui_sender.is_none() {
            warn!("UI event service not available; UI events will be skipped");
        }

        let config_snapshot = config
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| anyhow!("Failed to acquire STT config for hybrid service"))?;
        let metrics_handle = app_handle
            .try_state::<crate::metrics::MetricsHandle>()
            .map(|state| state.inner().clone());

        let audio_config = AudioPipelineConfig::new(
            PathBuf::from(&config_snapshot.model.path),
            config_snapshot.model.language.default.clone(),
            config_snapshot.vad.aggressiveness,
            DEFAULT_MIN_TRANSCRIPT_CHARS,
            config_snapshot.vad.start_grace_ms,
            config_snapshot.vad.max_segment_ms,
            config_snapshot.vad.activation_ms,
            config_snapshot.vad.post_speech_silence_ms,
            config_snapshot.whisper.threads,
        );

        let pipeline = Arc::new(AudioPipeline::new(
            audio_config,
            ui_sender.clone(),
            metrics_handle,
        )?);
        debug!("Audio pipeline constructed");
        let state_events = pipeline
            .take_state_events()
            .ok_or_else(|| anyhow!("Audio pipeline state receiver already consumed"))?;
        debug!("Audio pipeline state receiver acquired");

        // Do NOT start audio capture yet. Start only after sidecar check or PTT-only mode.
        let audio_task: Option<JoinHandle<()>> = None;

        let wakeword_enabled = config_snapshot.wakeword.provider != WakeWordProvider::None;
        let has_access_key = !config_snapshot.sidecar.access_key.trim().is_empty();

        let sidecar = if wakeword_enabled && has_access_key {
            let identity_for_sidecar = app_user_model_id.clone();

            // Resolve sidecar path in production bundle
            let resolved_executable_path = if cfg!(debug_assertions) {
                let raw_path = &config_snapshot.sidecar.executable_path;
                let candidate = Path::new(raw_path);
                let full_path = if candidate.is_absolute() {
                    candidate.to_path_buf()
                } else {
                    Path::new(env!("CARGO_MANIFEST_DIR")).join(candidate)
                };
                full_path.to_string_lossy().to_string()
            } else {
                match app_handle.path().resolve(
                    &config_snapshot.sidecar.executable_path,
                    BaseDirectory::Resource,
                ) {
                    Ok(p) => p.to_string_lossy().to_string(),
                    Err(_) => config_snapshot.sidecar.executable_path.clone(),
                }
            };
            info!(
                "Preparing wake-word sidecar path={} keyword={} sensitivity={}",
                resolved_executable_path,
                config_snapshot.wakeword.keyword,
                config_snapshot.wakeword.sensitivity
            );

            let sidecar_config = SidecarConfig {
                executable_path: resolved_executable_path,
                python_env: config_snapshot.sidecar.python_env.clone(),
                startup_timeout_ms: config_snapshot.sidecar.startup_timeout_ms,
                communication_timeout_ms: config_snapshot.sidecar.communication_timeout_ms,
                access_key: config_snapshot.sidecar.access_key.clone(),
                wakeword_keyword: Some(config_snapshot.wakeword.keyword.clone()),
                wakeword_sensitivity: Some(config_snapshot.wakeword.sensitivity),
                app_user_model_id: Some(identity_for_sidecar.clone()),
            };

            let mut sidecar = AsyncPythonSidecar::new(sidecar_config);
            // Compute logs dir centrally
            let logs_dir = crate::config::prelude::logs_dir_from_handle(&app_handle);
            match sidecar.start(Some(logs_dir.as_path())).await {
                Ok(()) => {
                    info!("Wake-word sidecar ready");
                    Some(sidecar)
                }
                Err(err) => {
                    warn!(
                        "Failed to start wake-word sidecar ({err}); falling back to PTT-only mode"
                    );
                    None
                }
            }
        } else {
            if wakeword_enabled && !has_access_key {
                warn!(
                    "Wake-word provider configured but Picovoice key is missing; running PTT-only"
                );
            } else {
                info!("Wake-word provider disabled; running in PTT-only mode");
            }
            None
        };

        // Do not start capture here at all. It will start only on wake word or PTT press.

        Ok(Self {
            config,
            pipeline,
            transcript_tx,
            app_handle,
            sidecar,
            audio_task,
            state_events: Some(state_events),
            ui_sender,
        })
    }

    pub async fn run(
        mut self,
        mut shutdown: watch::Receiver<bool>,
        mut ptt_rx: watch::Receiver<bool>,
        mut command_rx: mpsc::UnboundedReceiver<SttCommand>,
    ) -> Result<()> {
        info!("Hybrid STT service run loop starting");
        if let Some(sidecar) = self.sidecar.as_ref() {
            if !sidecar.is_ready() {
                return Err(anyhow!("Wake word sidecar failed to report ready state"));
            }
        }

        self.emit_ready_event()?;
        self.emit_ui_vu(0.0);

        let mut state_events = self
            .state_events
            .take()
            .ok_or_else(|| anyhow!("State event receiver already consumed"))?;

        let processing_pipeline = self.pipeline.clone();
        let processing_tx = self.transcript_tx.clone();
        let ui_sender = self.ui_sender.clone();
        let processing_handle = tokio::spawn({
            let app_handle = self.app_handle.clone();
            let mut shutdown_rx = shutdown.clone();
            async move {
                Self::processing_loop(
                    processing_pipeline,
                    &mut state_events,
                    &mut shutdown_rx,
                    processing_tx,
                    app_handle,
                    ui_sender,
                )
                .await;
            }
        });

        while !*shutdown.borrow() {
            tokio::select! {
                _ = shutdown.changed() => break,
                res = ptt_rx.changed() => {
                    if res.is_err() {
                        break;
                    }
                    let active = *ptt_rx.borrow();
                    self.handle_ptt_toggle(active);
                }
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(SttCommand::RetriggerWakeword) => {
                            if let Err(err) = self.trigger_followup_listening() {
                                warn!("Failed to start follow-up listening after TTS: {err:?}");
                            }
                        }
                        None => {
                            debug!("Follow-up command channel closed; stopping auto re-listen handling");
                            break;
                        }
                    }
                }
                result = async {
                    match self.sidecar.as_mut() {
                        Some(sidecar) => sidecar.wait_for_wake_word_async().await.map(Some),
                        None => std::future::pending::<Result<Option<WakeWordEvent>>>().await,
                    }
                } => {
                    match result {
                        Ok(Some(WakeWordEvent::Detected)) => {
                            if state_router::is_busy_for_stt() {
                                debug!("Wake word detected while app busy; ignoring");
                                continue;
                            }
                            // Start audio capture lazily on wake word (always restart to ensure fresh stream)
                            self.start_wakeword_capture();
                            if let Err(err) = self.handle_wake_word_trigger() {
                                error!("Wake word handling error: {err:?}");
                            }
                        }
                        Ok(Some(WakeWordEvent::Timeout)) => {
                            // Timeout simply means we didn't receive an event in the configured window.
                            // Treat it as a heartbeat and continue listening without spamming errors.
                            trace!("Wake word detection timed out, continuing to listen...");
                        }
                        Ok(None) => {}
                        Err(err) => {
                            warn!("Wake word detection error: {err}");
                            tokio::time::sleep(Duration::from_millis(250)).await;
                        }
                    }
                }
            }
        }

        self.stop().await?;
        let _ = processing_handle.await;
        warn!("STT init: hybrid service run loop exiting");
        Ok(())
    }

    fn emit_ready_event(&self) -> Result<()> {
        let config = self
            .config
            .read()
            .map_err(|_| anyhow!("Failed to read STT config for ready event"))?;

        let payload = SttReadyEvent {
            provider: "hybrid".to_string(),
            model: config.model.path.clone(),
            lang: config.model.language.default.clone(),
        };

        self.app_handle
            .emit(EVENT_STT_READY, payload)
            .map_err(|e| anyhow!(e.to_string()))
    }

    fn handle_ptt_change(&self, active: bool) -> Result<()> {
        if active {
            self.pipeline.start_ptt_recording()?;
            self.app_handle
                .emit(EVENT_STT_STARTED, json!({ "trigger": "ptt" }))?;
        } else {
            self.pipeline.stop_recording()?;
            self.emit_ui_vu(0.0);
        }
        Ok(())
    }

    fn handle_ptt_toggle(&mut self, active: bool) {
        if active {
            if state_router::is_busy_for_stt() {
                debug!("PTT activation ignored while app busy");
                return;
            }
            self.start_capture_task();
        } else {
            self.stop_capture_task();
        }
        if let Err(err) = self.handle_ptt_change(active) {
            error!("PTT handling error: {err:?}");
        }
    }

    fn handle_wake_word_trigger(&mut self) -> Result<()> {
        if !matches!(self.pipeline.current_state(), AudioState::Idle) {
            info!("Wake word trigger ignored - pipeline busy");
            return Ok(());
        }

        info!("Wake word detected; beginning recording");
        self.pipeline.start_wake_word_recording()?;
        self.app_handle
            .emit(EVENT_STT_STARTED, json!({ "trigger": "wakeword" }))?;
        // Stop capture when the pipeline returns to Idle will be handled by processing loop
        Ok(())
    }

    fn start_wakeword_capture(&mut self) {
        self.start_capture_task();
    }

    fn start_capture_task(&mut self) {
        if let Some(handle) = self.audio_task.take() {
            handle.abort();
        }
        let capture_pipeline = self.pipeline.clone();
        self.audio_task = Some(tokio::spawn(async move {
            if let Err(err) = capture_pipeline.start_audio_capture().await {
                error!("Audio capture error: {err}");
            }
        }));
    }

    fn stop_capture_task(&mut self) {
        self.pipeline.stop_audio_capture();
        if let Some(handle) = self.audio_task.take() {
            handle.abort();
        }
    }

    fn trigger_followup_listening(&mut self) -> Result<()> {
        if !matches!(self.pipeline.current_state(), AudioState::Idle) {
            trace!("Skipping follow-up listening restart; pipeline is not idle");
            return Ok(());
        }
        self.start_wakeword_capture();
        self.handle_wake_word_trigger()?;
        self.pipeline.mark_discard_if_silent();
        Ok(())
    }

    fn emit_ui_vu(&self, level: f32) {
        if let Some(sender) = &self.ui_sender {
            sender.emit_vu_level(level);
        }
    }

    async fn stop(&mut self) -> Result<()> {
        debug!("Hybrid STT service initiating shutdown");
        self.pipeline.shutdown();

        if let Some(sidecar) = self.sidecar.as_mut() {
            sidecar.stop()?;
        }

        if let Some(handle) = self.audio_task.take() {
            if let Err(err) = handle.await {
                error!("Audio capture task error: {err}");
            }
        }

        self.emit_ui_vu(0.0);
        warn!("STT init: hybrid service shutdown complete");

        Ok(())
    }

    async fn processing_loop(
        pipeline: Arc<AudioPipeline>,
        state_rx: &mut mpsc::UnboundedReceiver<AudioState>,
        shutdown: &mut watch::Receiver<bool>,
        transcript_tx: Sender<TranscriptionEvent>,
        app_handle: AppHandle,
        ui_sender: Option<UiEventSender>,
    ) {
        let mut last_visual: Option<UiVisualState> = None;
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                maybe_state = state_rx.recv() => {
                    match maybe_state {
                        Some(audio_state) => {
                            match audio_state {
                                AudioState::WakeWordRecording | AudioState::PttRecording => {
                                    if last_visual != Some(UiVisualState::Listening) {
                                        state_router::emit(
                                            &app_handle,
                                            StateSignal::new(UiVisualState::Listening, StateFlow::Stt),
                                        );
                                        last_visual = Some(UiVisualState::Listening);
                                    }
                                }
                                AudioState::Processing => {
                                    if last_visual != Some(UiVisualState::Thinking) {
                                        state_router::emit(
                                            &app_handle,
                                            StateSignal::new(UiVisualState::Thinking, StateFlow::Stt),
                                        );
                                    }

                                    let result = pipeline
                                        .process_speech_buffer(&transcript_tx, &app_handle)
                                        .await;

                                    match result {
                                        Ok(emitted) => {
                                            if let Some(sender) = &ui_sender {
                                                sender.emit_vu_level(0.0);
                                                pipeline.stop_audio_capture();
                                            }

                                            if emitted {
                                                state_router::clear_flow_silent(StateFlow::Stt);
                                                last_visual = None;
                                            } else {
                                                state_router::emit(
                                                    &app_handle,
                                                    StateSignal::new(UiVisualState::Idle, StateFlow::Stt),
                                                );
                                                last_visual = Some(UiVisualState::Idle);
                                            }
                                        }
                                        Err(err) => {
                                            error!("Error processing speech: {err}");
                                            if let Some(sender) = &ui_sender {
                                                sender.emit_vu_level(0.0);
                                                pipeline.stop_audio_capture();
                                            }
                                            state_router::emit(
                                                &app_handle,
                                                StateSignal::new(UiVisualState::Error, StateFlow::Stt),
                                            );
                                            state_router::clear_flow(&app_handle, StateFlow::Stt);
                                            last_visual = None;
                                            pipeline.set_state(AudioState::Error);
                                            pipeline.set_state(AudioState::Idle);
                                        }
                                    }
                                }
                                AudioState::Error => {
                                    if let Some(sender) = &ui_sender {
                                        sender.emit_vu_level(0.0);
                                        pipeline.stop_audio_capture();
                                    }
                                    state_router::emit(
                                        &app_handle,
                                        StateSignal::new(UiVisualState::Error, StateFlow::Stt),
                                    );
                                    state_router::emit(
                                        &app_handle,
                                        StateSignal::new(UiVisualState::Idle, StateFlow::Stt),
                                    );
                                    last_visual = Some(UiVisualState::Idle);
                                }
                                AudioState::Idle => {
                                    if let Some(sender) = &ui_sender {
                                        sender.emit_vu_level(0.0);
                                        pipeline.stop_audio_capture();
                                    }
                                }
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    }
}
