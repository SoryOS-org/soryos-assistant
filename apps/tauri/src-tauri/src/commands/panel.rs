//! Panel window orchestration commands shared with the webview UI.

use crate::{
    config::AppConfig,
    panel::{PanelManager, PanelView},
    ui::{
        sprite_window::{SpriteWindowController, PRELOAD_WINDOW_HEIGHT, PRELOAD_WINDOW_WIDTH},
        UiEventService, UiSnapshot,
    },
};
use tauri::{AppHandle, Manager, State};
use tracing::debug;

/// Hide the main window but leave the app running in the tray.
#[tauri::command]
pub fn close_window(app_handle: AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;

    window.hide().map_err(|err| err.to_string())?;
    let _ = window.set_skip_taskbar(true);

    Ok(())
}

/// Move the panel to a specific view.
#[tauri::command]
pub fn panel_navigate(panel: State<'_, PanelManager>, view: String) -> Result<(), String> {
    let target_view = PanelView::try_from(view.as_str())?;
    panel.navigate(target_view)
}

/// Hide the panel window without tearing it down.
#[tauri::command]
pub fn panel_hide(panel: State<'_, PanelManager>) -> Result<(), String> {
    panel.hide()
}

/// Return the latest UI snapshot for hydration.
#[tauri::command]
pub fn get_ui_snapshot(ui_state: State<'_, UiEventService>) -> Result<UiSnapshot, String> {
    Ok(ui_state.snapshot())
}

/// Resize the sprite window for preload or restore to the configured scale.
#[tauri::command]
pub fn set_sprite_preload_state(app_handle: AppHandle, visible: bool) -> Result<f32, String> {
    debug!("sprite_preload_state invoked visible={visible}");
    let controller = app_handle
        .try_state::<SpriteWindowController>()
        .ok_or_else(|| "sprite window controller not available".to_string())?;
    let config = app_handle
        .try_state::<AppConfig>()
        .ok_or_else(|| "app config not available".to_string())?;
    let stt_state = config.stt();
    let stt_config = stt_state
        .read()
        .map_err(|_| "stt config lock poisoned".to_string())?;
    let base_scale = stt_config.sprite.scale;

    if visible {
        debug!("sprite_preload_state resizing window to preload size");
        controller.apply_preload_size(&app_handle, PRELOAD_WINDOW_WIDTH, PRELOAD_WINDOW_HEIGHT)
    } else {
        debug!("sprite_preload_state restoring window to config scale");
        controller.apply_scale(&app_handle, base_scale)?;
        Ok(base_scale)
    }
}
