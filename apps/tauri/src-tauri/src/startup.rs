use std::sync::Arc;

use crate::config::{
    dirty_watchers::start_config_dirty_watchers, machine_state::MachineStateManager, AppConfig,
    PanelConfig, WorkerBridgeConfig,
};
use crate::intents::IntentState;
use crate::logging::{init_logging, telemetry_level_from_state, update_runtime_log_filter};
use crate::mcp_log;
use crate::metrics::MetricsHandle;
use crate::panel::PanelManager;
use crate::state_sync;
use crate::stt::{preload as stt_preload, SttProvider, SttService};
use crate::ui::{
    sprite_window::{SpriteWindowController, PRELOAD_WINDOW_HEIGHT, PRELOAD_WINDOW_WIDTH},
    UiEventService,
};
use crate::worker_log;
use crate::{machine, tts};
use crate::{
    mcp_log::set_raw_payload_logging as set_mcp_raw_logging,
    worker_log::set_raw_payload_logging as set_worker_raw_logging,
};
use anyhow::{anyhow, Result};
use mcp_host::{HostEventKind, HostHandle, HostSinks};
use tauri::{async_runtime, Emitter, Manager};
use tracing::{debug, error, info, warn};

pub fn setup(app: &tauri::App) -> Result<()> {
    let metrics = init_logging_subsystems(app.handle())?;
    app.manage(metrics);

    info!("Initialising Sprite runtime");
    let app_config = load_app_config(app)?;
    let identity_config = app_config.identity().clone();

    let worker_config_state = app_config.worker_state();
    let worker_config = worker_config_state
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_else(|_| app_config.worker());
    let stt_config_handle = app_config.stt();
    let tts_config_handle = app_config.tts();
    let panel_config = app_config.ui().panel().clone();
    let preferences_handle = app_config.preferences();
    let mcp_config_handle = app_config.mcp();
    let prefs_snapshot = state_sync::snapshot_preferences(&preferences_handle);
    app.manage(app_config.clone());
    debug!("AppConfig registered with app state");

    match start_config_dirty_watchers(
        app.handle().clone(),
        stt_config_handle.clone(),
        tts_config_handle.clone(),
        mcp_config_handle.clone(),
    ) {
        Ok(watchers) => {
            app.manage(watchers);
            debug!("Config dirty watchers registered");
        }
        Err(err) => {
            warn!("Failed to start config dirty watchers: {err}");
        }
    }

    let machine_state = init_machine_state_manager(app, worker_config.clone())?;
    app.manage(machine_state.clone());
    debug!("MachineStateManager registered with app state");

    let host_handle = start_mcp_host(app, &machine_state, &mcp_config_handle)?;
    app.manage(host_handle);
    info!("MCP host initialised");

    init_main_window(app, &app_config, stt_config_handle.clone());
    init_ui_services(app, panel_config, &identity_config);

    state_sync::warm_and_sync_catalog(app, &machine_state, prefs_snapshot.clone());

    spawn_audio_services(
        app.handle().clone(),
        stt_config_handle.clone(),
        tts_config_handle.clone(),
        worker_config.clone(),
        identity_config.clone(),
        app.state::<IntentState>().inner().clone(),
    );

    Ok(())
}

fn init_logging_subsystems(app_handle: &tauri::AppHandle) -> Result<MetricsHandle> {
    init_logging(app_handle);
    worker_log::init(app_handle);
    mcp_log::init(app_handle);
    MetricsHandle::start(app_handle)
}

fn load_app_config(app: &tauri::App) -> Result<AppConfig> {
    let app_config = AppConfig::load(app)?;
    if let Some(level) = telemetry_level_from_state(&app_config) {
        update_runtime_log_filter(level);
    }
    let raw_payload_logs = app_config
        .stt()
        .read()
        .ok()
        .map(|cfg| cfg.telemetry.raw_payload_logs)
        .unwrap_or(false);
    set_worker_raw_logging(raw_payload_logs);
    set_mcp_raw_logging(raw_payload_logs);
    debug!("AppConfig loaded");
    app_config.identity().ensure_runtime_contract()?;
    debug!("Identity contract ensured");
    Ok(app_config)
}

fn init_machine_state_manager(
    app: &tauri::App,
    worker_config: WorkerBridgeConfig,
) -> Result<MachineStateManager> {
    let machine_state = MachineStateManager::load(app, worker_config.clone())
        .map_err(|e| anyhow!("Failed to initialize machine state: {e}"))?;
    machine_state.update_worker_config(worker_config);
    Ok(machine_state)
}

fn start_mcp_host(
    app: &tauri::App,
    machine_state: &MachineStateManager,
    mcp_config_handle: &Arc<std::sync::RwLock<crate::config::mcp::McpConfig>>,
) -> Result<HostHandle> {
    let host_config_snapshot = mcp_config_handle
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let initial_mcp_config = host_config_snapshot.clone();
    let app_handle_for_events = app.handle().clone();
    let machine_state_for_mcp = machine_state.clone();

    let host_sinks = HostSinks::new(
        move |event| {
            let payload =
                serde_json::to_value(&event).map_err(|err: serde_json::Error| anyhow!(err))?;
            app_handle_for_events
                .emit("assistant://mcp-status", payload)
                .map_err(|err: tauri::Error| anyhow!(err))?;

            if let HostEventKind::SchemasUpdated(snapshot) = &event.kind {
                let server_id = event.meta.server_id.clone();
                let tools_json = snapshot.tool_schemas.clone();
                let resources_json = snapshot.resources.clone();
                let machine_state = machine_state_for_mcp.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = machine::sync_mcp_snapshot(
                        machine_state,
                        server_id,
                        tools_json,
                        resources_json,
                    )
                    .await
                    {
                        warn!("Failed to sync MCP snapshot to worker: {err}");
                    }
                });
            }

            Ok(())
        },
        tracing::info_span!("mcp_host"),
    );

    let host_handle = async_runtime::block_on(HostHandle::start(host_config_snapshot, host_sinks))
        .map_err(|err| anyhow!("Failed to start MCP host: {err}"))?;
    {
        let machine_state_for_config = machine_state.clone();
        match serde_json::to_value(initial_mcp_config) {
            Ok(config_value) => {
                async_runtime::spawn(async move {
                    if let Err(err) =
                        machine::sync_mcp_config(machine_state_for_config, config_value).await
                    {
                        warn!("Failed to sync MCP configuration to worker: {err}");
                    }
                });
            }
            Err(err) => {
                warn!("Failed to serialise MCP config for worker sync: {err}");
            }
        }
    }

    Ok(host_handle)
}

fn init_main_window(
    app: &tauri::App,
    app_config: &AppConfig,
    _stt_config_handle: Arc<std::sync::RwLock<crate::config::stt::SttConfig>>,
) {
    if let Some(main_window) = app.get_webview_window("main") {
        let show_in_taskbar = app_config.ui().show_main_in_taskbar();
        if let Err(err) = main_window.set_skip_taskbar(!show_in_taskbar) {
            warn!("Failed to apply taskbar policy for main window: {err}");
        }
        match SpriteWindowController::initialise(&main_window) {
            Ok(controller) => {
                if let Err(err) = controller.apply_preload_size_to(
                    &main_window,
                    PRELOAD_WINDOW_WIDTH,
                    PRELOAD_WINDOW_HEIGHT,
                ) {
                    warn!("Failed to apply preload sprite window size: {err}");
                }
                app.manage(controller);
            }
            Err(err) => {
                warn!("Failed to initialise sprite window controller: {err}");
            }
        }
    }
}

fn init_ui_services(
    app: &tauri::App,
    panel_config: PanelConfig,
    identity_config: &crate::config::IdentityConfig,
) {
    let ui_events = UiEventService::start(app.handle().clone());
    app.manage(ui_events);

    let panel_manager = PanelManager::new(
        app.handle().clone(),
        panel_config,
        identity_config.app_user_model_id().to_string(),
    );
    app.manage(panel_manager);

    crate::init_tray(app.handle());
    debug!("System tray initialised");
}

#[allow(clippy::too_many_arguments)]
fn spawn_audio_services(
    app_handle: tauri::AppHandle,
    stt_config_handle: Arc<std::sync::RwLock<crate::config::stt::SttConfig>>,
    tts_config_handle: Arc<std::sync::RwLock<crate::config::tts::TtsConfig>>,
    worker_config: crate::config::WorkerBridgeConfig,
    identity_config: crate::config::IdentityConfig,
    intent_state: IntentState,
) {
    let app_handle_clone = app_handle.clone();
    let stt_config_for_preload = stt_config_handle.clone();
    let tts_config_for_preload = tts_config_handle.clone();
    let tts_config_runtime = tts_config_handle.clone();
    let worker_config_clone = worker_config.clone();
    let identity_config_clone = identity_config.clone();
    let intent_state_clone = intent_state.clone();

    async_runtime::spawn(async move {
        let original_tts_json = tts_config_for_preload
            .read()
            .map(|guard| serde_json::to_value(&*guard).unwrap_or(serde_json::Value::Null))
            .unwrap_or(serde_json::Value::Null);

        let tts_preload_result = async_runtime::spawn_blocking({
            let app_handle = app_handle_clone.clone();
            let tts_config_lock = tts_config_for_preload.clone();
            move || {
                let tts_snapshot = tts_config_lock
                    .read()
                    .map(|guard| guard.clone())
                    .map_err(|_| anyhow!("TTS configuration lock poisoned during preload"))?;
                tts::preload::prepare_tts(&app_handle, &tts_snapshot)
            }
        })
        .await;

        match tts_preload_result {
            Ok(Ok(updated_config)) => {
                if let Some(updated) = updated_config {
                    if let Ok(mut guard) = tts_config_for_preload.write() {
                        *guard = updated.clone();
                    }
                    if let Ok(updated_json) = serde_json::to_value(&updated) {
                        if original_tts_json != updated_json {
                            warn!("Persisting TTS config after preload adjustments (paths/assets may have shifted)");
                            if let Err(err) = updated.save_to_handle(&app_handle_clone) {
                                error!("Failed to persist TTS config after preload: {err:?}");
                            }
                        }
                    }
                }
            }
            Ok(Err(err)) => {
                error!("TTS preload task failed: {err:?}");
            }
            Err(err) => {
                error!("Failed to join TTS preload task: {err}");
            }
        }

        match tts::TtsService::start(app_handle_clone.clone(), tts_config_runtime.clone()) {
            Ok(service) => {
                info!("TTS service initialised successfully");
                let _ = app_handle_clone.manage(service);
            }
            Err(err) => {
                error!("Failed to initialise TTS service: {err:?}");
            }
        }

        let stt_preload_result = async_runtime::spawn_blocking({
            let app_handle = app_handle_clone.clone();
            let stt_config_lock = stt_config_for_preload.clone();
            move || {
                let stt_snapshot = stt_config_lock
                    .read()
                    .map(|guard| guard.clone())
                    .map_err(|_| anyhow!("STT configuration lock poisoned during preload"))?;
                stt_preload::prepare_stt(&app_handle, &stt_snapshot)
            }
        })
        .await;

        match stt_preload_result {
            Ok(Ok(updated_config)) => {
                if let Some(updated) = updated_config {
                    if let Ok(mut guard) = stt_config_for_preload.write() {
                        *guard = updated.clone();
                    }
                    if let Err(err) = updated.save_to_handle(&app_handle_clone) {
                        error!("Failed to persist STT config after preload: {err:?}");
                    }
                }
            }
            Ok(Err(err)) => {
                error!("STT preload task failed: {err:?}");
                return;
            }
            Err(err) => {
                error!("Failed to join STT preload task: {err}");
                return;
            }
        }

        let stt_provider = stt_config_handle
            .read()
            .map(|cfg| cfg.provider)
            .unwrap_or(SttProvider::Local);
        info!("Starting STT service (provider={:?})", stt_provider);

        match SttService::start(
            app_handle_clone.clone(),
            worker_config_clone,
            identity_config_clone,
            stt_config_handle,
            intent_state_clone,
        ) {
            Ok(service) => {
                info!("STT service initialised successfully");
                let _ = app_handle_clone.manage(service);
            }
            Err(err) => {
                error!("Failed to initialise STT service: {err:?}");
                let _ = app_handle_clone.emit(
                    crate::stt::EVENT_STT_ERROR,
                    crate::stt::SttErrorEvent {
                        stage: "startup".to_string(),
                        message: err.to_string(),
                    },
                );
            }
        }
    });
}
