//! Commands tied to the speech-to-text pipeline.

use crate::stt::{self, SttService, SttStartedEvent};
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

/// Expose the latest preload status emitted by the STT bootstrapper.
#[tauri::command]
pub fn get_preload_status() -> Option<stt::preload::PreloadStatusPayload> {
    stt::preload::current_status()
}

/// Toggle push-to-talk capture and emit the corresponding UI events.
#[tauri::command]
pub fn toggle_ptt(app_handle: AppHandle, activate: bool) -> Result<(), String> {
    let Some(stt_service) = app_handle.try_state::<SttService>() else {
        warn!("PTT toggle requested but STT service is not initialised");
        return Ok(());
    };
    let ptt_state = stt_service.get_ptt_state();

    if activate {
        info!("PTT activated via frontend - start speaking");
        ptt_state.activate();

        let started_event = SttStartedEvent {
            trigger: "ptt".to_string(),
        };
        let _ = app_handle.emit("assistant://stt-started", started_event);
    } else {
        info!("PTT deactivated via frontend - processing speech");
        ptt_state.deactivate();

        let stopped_event = SttStartedEvent {
            trigger: "ptt_release".to_string(),
        };
        let _ = app_handle.emit("assistant://stt-started", stopped_event);
    }

    Ok(())
}
