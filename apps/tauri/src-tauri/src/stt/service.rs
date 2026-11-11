use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex, RwLock,
};

use anyhow::{anyhow, Result};
use async_channel::{Receiver, Sender};
use mcp_host::HostHandle;
use tauri::{async_runtime, AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};

use crate::config::stt::{SttConfig, SttProvider};
use crate::config::{stt, IdentityConfig, WorkerBridgeConfig as BridgeConfig};
use crate::{
    machine,
    metrics::{MetricsHandle, TurnTrigger},
    AssistantCommandResponse, AssistantIntent, IntentState,
};

use super::events::{SttReadyEvent, TranscriptionEvent};
use super::{preload, EVENT_STT_ERROR, EVENT_STT_READY, EVENT_STT_RESULT};

#[derive(Debug, Clone, Copy)]
pub(crate) enum SttCommand {
    RetriggerWakeword,
}

// PTT (Push-to-Talk) state management
#[derive(Debug, Clone)]
pub struct PttState {
    is_active: Arc<AtomicBool>,
    start_time: Arc<AtomicU64>,
    notifier: watch::Sender<bool>,
}

impl PttState {
    pub fn new() -> Self {
        let (notifier, _rx) = watch::channel(false);
        Self {
            is_active: Arc::new(AtomicBool::new(false)),
            start_time: Arc::new(AtomicU64::new(0)),
            notifier,
        }
    }

    pub fn activate(&self) {
        if !self.is_active.swap(true, Ordering::Relaxed) {
            self.start_time.store(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                Ordering::Relaxed,
            );
            let _ = self.notifier.send(true);
        }
    }

    pub fn deactivate(&self) {
        if self.is_active.swap(false, Ordering::Relaxed) {
            let _ = self.notifier.send(false);
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.notifier.subscribe()
    }
}

// TranscriptionEvent is now in events.rs module

struct RunningStt {
    shutdown_tx: watch::Sender<bool>,
    hybrid_task: Option<async_runtime::JoinHandle<Result<()>>>,
    pipeline_task: async_runtime::JoinHandle<()>,
    command_tx: Option<mpsc::UnboundedSender<SttCommand>>,
}

struct SttInner {
    running: Mutex<Option<RunningStt>>,
    ptt_state: PttState,
    app_handle: AppHandle,
    identity_config: IdentityConfig,
    worker_config: Arc<RwLock<BridgeConfig>>,
    stt_config: Arc<RwLock<SttConfig>>,
    intent_state: IntentState,
    reload_tx: Sender<()>,
    config_fingerprint: Mutex<Option<String>>,
    command_tx: Mutex<Option<mpsc::UnboundedSender<SttCommand>>>,
    followup_requests: Mutex<HashSet<String>>,
}

impl SttInner {
    fn new(
        app_handle: AppHandle,
        identity_config: IdentityConfig,
        worker_config: Arc<RwLock<BridgeConfig>>,
        stt_config: Arc<RwLock<SttConfig>>,
        intent_state: IntentState,
        ptt_state: PttState,
        reload_tx: Sender<()>,
    ) -> Self {
        Self {
            running: Mutex::new(None),
            ptt_state,
            app_handle,
            identity_config,
            worker_config,
            stt_config,
            intent_state,
            reload_tx,
            config_fingerprint: Mutex::new(None),
            command_tx: Mutex::new(None),
            followup_requests: Mutex::new(HashSet::new()),
        }
    }

    fn update_config_fingerprint(&self, config: &SttConfig) {
        if let Ok(serialized) = serde_json::to_string(config) {
            if let Ok(mut guard) = self.config_fingerprint.lock() {
                *guard = Some(serialized);
            }
        }
    }

    fn apply_config_update(&self, config: stt::SttConfig) {
        if let Ok(mut guard) = self.stt_config.write() {
            *guard = config.clone();
        }
        self.update_config_fingerprint(&config);
        self.request_reload();
    }

    fn set_command_sender(&self, sender: Option<mpsc::UnboundedSender<SttCommand>>) {
        if let Ok(mut guard) = self.command_tx.lock() {
            *guard = sender;
        }
    }

    fn command_sender(&self) -> Option<mpsc::UnboundedSender<SttCommand>> {
        self.command_tx.lock().ok().and_then(|guard| guard.clone())
    }

    fn track_followup(&self, utterance_id: String) {
        if let Ok(mut guard) = self.followup_requests.lock() {
            guard.insert(utterance_id);
        }
    }

    fn consume_followup(&self, utterance_id: &str) -> bool {
        if let Ok(mut guard) = self.followup_requests.lock() {
            guard.remove(utterance_id)
        } else {
            false
        }
    }

    fn spawn_runtime(&self) -> Result<RunningStt> {
        let config_snapshot = self
            .stt_config
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| anyhow!("Failed to acquire STT config for runtime initialisation"))?;

        SttService::setup_ptt_hotkey(&self.app_handle, &config_snapshot, &self.ptt_state)?;

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (transcript_tx, transcript_rx) = async_channel::unbounded();
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        let hybrid_task = match config_snapshot.provider {
            SttProvider::Local => {
                info!(
                    "Starting hybrid STT service (wakeword_provider={:?})",
                    config_snapshot.wakeword.provider
                );
                Some(spawn_hybrid_service(
                    Arc::clone(&self.stt_config),
                    transcript_tx.clone(),
                    shutdown_rx.clone(),
                    self.app_handle.clone(),
                    self.identity_config.app_user_model_id().to_string(),
                    self.ptt_state.clone(),
                    command_rx,
                ))
            }
            SttProvider::Api => {
                emit_placeholder_notice(&self.app_handle, &config_snapshot);
                let ready_event = SttReadyEvent {
                    provider: format!("{:?}", config_snapshot.provider),
                    model: config_snapshot.model.path.clone(),
                    lang: config_snapshot.model.language.default.clone(),
                };
                let _ = self.app_handle.emit(EVENT_STT_READY, ready_event);
                None
            }
        };

        let pipeline_task = spawn_pipeline(
            self.app_handle.clone(),
            transcript_rx,
            Arc::clone(&self.worker_config),
            self.intent_state.clone(),
            shutdown_rx,
        );

        Ok(RunningStt {
            shutdown_tx,
            hybrid_task,
            pipeline_task,
            command_tx: Some(command_tx),
        })
    }

    fn replace_runtime(&self) -> Result<()> {
        if let Some(previous) = self.running.lock().unwrap().take() {
            self.set_command_sender(None);
            let _ = previous.shutdown_tx.send(true);
            if let Some(handle) = previous.hybrid_task {
                match async_runtime::block_on(handle) {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => error!("Hybrid STT service shutdown error: {err:?}"),
                    Err(err) => error!("Hybrid STT task join error during shutdown: {err}"),
                }
            }
            if let Err(err) = async_runtime::block_on(previous.pipeline_task) {
                error!("Audio pipeline task join error during shutdown: {err}");
            }
        }

        let runtime = self.spawn_runtime()?;
        self.set_command_sender(runtime.command_tx.clone());
        let mut guard = self.running.lock().unwrap();
        *guard = Some(runtime);

        Ok(())
    }

    fn request_reload(&self) {
        let sender = self.reload_tx.clone();
        async_runtime::spawn(async move {
            if let Err(err) = sender.send(()).await {
                error!("Failed to queue STT reload request: {err}");
            }
        });
    }
}

impl Drop for SttInner {
    fn drop(&mut self) {
        if let Some(runtime) = self.running.lock().unwrap().take() {
            let _ = runtime.shutdown_tx.send(true);
            if let Some(handle) = runtime.hybrid_task {
                match async_runtime::block_on(handle) {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => error!("Hybrid STT service shutdown error: {err:?}"),
                    Err(err) => error!("Hybrid STT task join error during shutdown: {err}"),
                }
            }
            if let Err(err) = async_runtime::block_on(runtime.pipeline_task) {
                error!("Audio pipeline task join error during shutdown: {err}");
            }
        }

        self.set_command_sender(None);
    }
}

#[derive(Clone)]
pub struct SttService {
    inner: Arc<SttInner>,
}

impl SttService {
    pub fn update_picovoice_key(&self, key: Option<String>) {
        let mut guard = match self.inner.stt_config.write() {
            Ok(cfg) => cfg,
            Err(err) => {
                warn!("Failed to lock STT config to update Picovoice key: {err}");
                return;
            }
        };
        guard.sidecar.access_key = key.unwrap_or_default();
        self.inner.update_config_fingerprint(&guard);
        self.inner.request_reload();
    }

    pub fn start(
        app_handle: AppHandle,
        worker_config: BridgeConfig,
        identity_config: IdentityConfig,
        stt_config: Arc<RwLock<SttConfig>>,
        intent_state: IntentState,
    ) -> Result<Self> {
        info!("Initialising STT service");
        let ptt_state = PttState::new();
        let (reload_tx, reload_rx) = async_channel::bounded(1);
        let worker_config = Arc::new(RwLock::new(worker_config));

        let inner = Arc::new(SttInner::new(
            app_handle.clone(),
            identity_config.clone(),
            Arc::clone(&worker_config),
            Arc::clone(&stt_config),
            intent_state.clone(),
            ptt_state.clone(),
            reload_tx.clone(),
        ));

        inner.replace_runtime()?;

        if let Ok(cfg) = stt_config.read() {
            inner.update_config_fingerprint(&cfg);
        }

        let reload_inner = Arc::clone(&inner);
        async_runtime::spawn(async move {
            while reload_rx.recv().await.is_ok() {
                let inner = Arc::clone(&reload_inner);

                let config_snapshot = match inner.stt_config.read() {
                    Ok(cfg) => cfg.clone(),
                    Err(err) => {
                        error!("Failed to read STT config for reload: {err}");
                        continue;
                    }
                };

                let app_handle = inner.app_handle.clone();
                let app_handle_for_emit = app_handle.clone();
                let preload_config = config_snapshot;
                let preload_result = async_runtime::spawn_blocking({
                    move || preload::prepare_stt(&app_handle, &preload_config)
                })
                .await;

                match preload_result {
                    Ok(Ok(updated)) => {
                        if let Some(config) = updated {
                            if let Ok(mut guard) = inner.stt_config.write() {
                                *guard = config.clone();
                            }
                            if let Err(err) = config.save_to_handle(&inner.app_handle) {
                                error!("Failed to persist STT config during reload: {err:?}");
                            }
                            inner.update_config_fingerprint(&config);
                        }
                    }
                    Ok(Err(err)) => {
                        error!("Preload step failed during config reload: {err:?}");
                        continue;
                    }
                    Err(err) => {
                        error!("Failed to join preload task during config reload: {err}");
                        continue;
                    }
                }

                let inner_for_replace = Arc::clone(&inner);
                match async_runtime::spawn_blocking(move || inner_for_replace.replace_runtime())
                    .await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        error!("Failed to rebuild STT runtime after config change: {err:?}");
                        let _ = app_handle_for_emit.emit(
                            EVENT_STT_ERROR,
                            crate::stt::SttErrorEvent {
                                stage: "reload".to_string(),
                                message: err.to_string(),
                            },
                        );
                    }
                    Err(err) => {
                        error!("Failed to join STT runtime rebuild task: {err}");
                        let _ = app_handle_for_emit.emit(
                            EVENT_STT_ERROR,
                            crate::stt::SttErrorEvent {
                                stage: "reload".to_string(),
                                message: err.to_string(),
                            },
                        );
                    }
                }
            }
        });

        info!("STT service initialisation complete");

        Ok(Self { inner })
    }

    fn setup_ptt_hotkey(
        _app_handle: &AppHandle,
        config: &SttConfig,
        _ptt_state: &PttState,
    ) -> Result<()> {
        // PTT hotkey is handled in the frontend
        info!(
            "PTT hotkey will be registered in frontend: {}",
            config.hotkey.push_to_talk
        );
        Ok(())
    }

    pub fn update_worker_config(&self, config: BridgeConfig) {
        if let Ok(mut guard) = self.inner.worker_config.write() {
            *guard = config;
        }
    }

    pub fn plan_followup_listen(&self, utterance_id: String) {
        self.inner.track_followup(utterance_id);
    }

    pub fn consume_followup_and_listen(&self, utterance_id: &str) {
        if self.inner.consume_followup(utterance_id) {
            self.trigger_followup_listening();
        }
    }

    pub fn clear_followup_request(&self, utterance_id: &str) {
        let _ = self.inner.consume_followup(utterance_id);
    }

    pub fn trigger_followup_listening(&self) {
        match self.inner.command_sender() {
            Some(sender) => {
                if let Err(err) = sender.send(SttCommand::RetriggerWakeword) {
                    warn!("Failed to enqueue follow-up listening request: {err}");
                }
            }
            None => debug!("Follow-up listening requested but STT runtime is unavailable"),
        }
    }

    pub fn update_config_fingerprint(&self, config: &SttConfig) {
        self.inner.update_config_fingerprint(config);
    }

    pub fn apply_config_update(&self, config: stt::SttConfig) {
        self.inner.apply_config_update(config);
    }

    pub fn get_ptt_state(&self) -> &PttState {
        &self.inner.ptt_state
    }
}

fn emit_placeholder_notice(app_handle: &AppHandle, config: &SttConfig) {
    let payload = AssistantCommandResponse {
        reply: format!(
            "STT API mode not yet implemented for provider: {:?}",
            config.provider
        ),
        speech: crate::SpeechPlan::default(),
        intents: vec![],
        reply_streamed: false,
        mcp_plan: None,
        mcp_action_results: Vec::new(),
        mcp_session_token: None,
        dispatch_result: None,
    };

    let _ = app_handle.emit(EVENT_STT_RESULT, payload);
}

fn spawn_pipeline(
    app_handle: AppHandle,
    transcript_rx: Receiver<TranscriptionEvent>,
    worker_config: Arc<RwLock<BridgeConfig>>,
    intent_state: IntentState,
    mut shutdown: watch::Receiver<bool>,
) -> async_runtime::JoinHandle<()> {
    async_runtime::spawn(async move {
        let metrics = app_handle
            .try_state::<MetricsHandle>()
            .map(|state| state.inner().clone());
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                maybe_event = transcript_rx.recv() => {
                    match maybe_event {
                        Ok(event) => {
                            let TranscriptionEvent { text, trace_id } = event;
                            let worker_cfg = match worker_config.read() {
                                Ok(cfg) => cfg.clone(),
                                Err(err) => {
                                    error!("Worker config poisoned: {err}; skipping transcript");
                                    continue;
                                }
                            };

                            if let Some(metrics_handle) = metrics.as_ref() {
                                metrics_handle.mark_llm_request(&trace_id);
                            }

                            let machine_state = app_handle
                                .try_state::<crate::config::machine_state::MachineStateManager>()
                                .map(|s| s.inner().clone());
                            let app_config = app_handle
                                .try_state::<crate::config::AppConfig>()
                                .map(|s| s.preferences());
                            let host_handle = app_handle
                                .try_state::<HostHandle>()
                                .map(|s| s.inner().clone());

                            let (Some(ms), Some(prefs), Some(host)) =
                                (machine_state, app_config, host_handle)
                            else {
                                error!("Machine state, preferences, or MCP host not available in app state");
                                continue;
                            };

                            let chat_outcome = machine::run_worker_flow(
                                app_handle.clone(),
                                worker_cfg.clone(),
                                intent_state.clone(),
                                ms.clone(),
                                host.clone(),
                                prefs,
                                text.clone(),
                                Some(trace_id.clone()),
                                metrics.clone(),
                            )
                            .await;
                            match chat_outcome {
                                Ok(outcome) => {
                                    if let Some(metrics_handle) = metrics.as_ref() {
                                        metrics_handle.mark_llm_response(&trace_id, true, None);
                                    }

                                    let machine::ChatWorkerOutcome {
                                        response,
                                        planner_intent,
                                        planner_context,
                                        session_token,
                                        user_query,
                                    } = outcome;

                                    emit_worker_response(
                                        &app_handle,
                                        response,
                                        trace_id.clone(),
                                        metrics.clone(),
                                    );

                                    if let Some(planner) = planner_intent {
                                        let planner_context_clone = planner_context.clone();
                                        let session_clone = session_token.clone();
                                        let user_query_clone = user_query.clone();
                                        async_runtime::spawn({
                                            let app_handle = app_handle.clone();
                                            let config = worker_cfg.clone();
                                            let metrics = metrics.clone();
                                            let trace_id = trace_id.clone();
                                            let ms = ms.clone();
                                            let host = host.clone();
                                            async move {
                                                machine::run_planner_flow(
                                                    app_handle,
                                                    config,
                                                    ms,
                                                    host,
                                                    session_clone,
                                                    user_query_clone,
                                                    planner,
                                                    planner_context_clone,
                                                    Some(trace_id),
                                                    metrics,
                                                )
                                                .await;
                                            }
                                        });
                                    }
                                }
                                Err(err) => {
                                    if let Some(metrics_handle) = metrics.as_ref() {
                                        metrics_handle.mark_llm_response(&trace_id, false, Some(err.as_str()));
                                    }
                                    error!("Worker request failed for trace {trace_id}: {err}");
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    })
}

fn emit_worker_response(
    app_handle: &AppHandle,
    response: AssistantCommandResponse,
    trace_id: String,
    metrics: Option<MetricsHandle>,
) {
    let chat_only = response
        .intents
        .iter()
        .all(|intent| matches!(intent, AssistantIntent::Converse));
    let wakeword_turn = metrics
        .as_ref()
        .and_then(|handle| handle.turn_trigger(&trace_id))
        .is_some_and(|trigger| matches!(trigger, TurnTrigger::Wakeword));
    let mut final_utterance_id: Option<String> = None;
    let mut tts_enqueued = false;
    let mut turn_finalized = false;

    if let Some(tts_service) = app_handle.try_state::<crate::tts::TtsService>() {
        let initial_text = response
            .speech
            .initial
            .clone()
            .filter(|text| !text.trim().is_empty());
        let updates = response.speech.updates().to_vec();

        let mut enqueue = |text: String| {
            let normalized = crate::tts::normalize_for_tts(&text);
            if normalized.is_empty() {
                return;
            }
            match tts_service.enqueue_text(normalized) {
                Ok(utterance_id) => {
                    if let Some(metrics_handle) = metrics.as_ref() {
                        metrics_handle.register_utterance(&trace_id, &utterance_id, false);
                    }
                    final_utterance_id = Some(utterance_id.clone());
                    tts_enqueued = true;
                }
                Err(err) => {
                    let err_string = err.to_string();
                    error!("Failed to queue TTS playback: {err:?}");
                    crate::tts::emit_error(
                        app_handle,
                        crate::tts::TtsErrorEvent {
                            utterance_id: "worker-response".to_string(),
                            message: format!("enqueue failed: {err_string}"),
                        },
                    );
                    if let Some(metrics_handle) = metrics.as_ref() {
                        metrics_handle.finalize_turn(&trace_id, false, Some(err_string.as_str()));
                        turn_finalized = true;
                    }
                }
            }
        };

        if let Some(initial) = initial_text.clone() {
            enqueue(initial);
        }

        for update in updates {
            if let Some(initial) = initial_text.as_ref() {
                if initial.trim() == update.trim() {
                    continue;
                }
            }
            enqueue(update);
        }

        if !tts_enqueued && !turn_finalized {
            if let Some(metrics_handle) = metrics.as_ref() {
                metrics_handle.finalize_turn(&trace_id, true, None);
                turn_finalized = true;
            }
        }

        if chat_only && wakeword_turn {
            if let (Some(utterance_id), Some(stt_service)) = (
                final_utterance_id,
                app_handle.try_state::<crate::stt::SttService>(),
            ) {
                stt_service.plan_followup_listen(utterance_id);
            }
        }
    } else if let Some(metrics_handle) = metrics.as_ref() {
        metrics_handle.finalize_turn(&trace_id, false, Some("tts_service_unavailable"));
        turn_finalized = true;
    }

    if !tts_enqueued && !turn_finalized {
        if let Some(metrics_handle) = metrics.as_ref() {
            metrics_handle.finalize_turn(&trace_id, true, None);
        }
    }

    if let Err(err) = app_handle.emit(EVENT_STT_RESULT, response) {
        error!("Failed to emit STT result event: {err}");
    }
}

fn spawn_hybrid_service(
    config: Arc<RwLock<SttConfig>>,
    transcript_tx: Sender<TranscriptionEvent>,
    shutdown: watch::Receiver<bool>,
    app_handle: AppHandle,
    app_user_model_id: String,
    ptt_state: PttState,
    command_rx: mpsc::UnboundedReceiver<SttCommand>,
) -> async_runtime::JoinHandle<Result<()>> {
    debug!("Spawning hybrid STT service task");
    async_runtime::spawn(async move {
        use crate::stt::hybrid_service::HybridSttService;

        let ptt_rx = ptt_state.subscribe();
        let service =
            HybridSttService::new(config, transcript_tx, app_handle, app_user_model_id).await?;
        debug!("Hybrid STT service constructed; entering run loop");
        service.run(shutdown, ptt_rx, command_rx).await
    })
}
