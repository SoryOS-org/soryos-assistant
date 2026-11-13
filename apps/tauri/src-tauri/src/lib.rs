mod brain;
mod catalog;
mod commands;
mod config;
pub mod download;
mod intents;
mod logging;
mod machine;
mod mcp_log;
mod metrics;
mod panel;
mod secret_store;
mod startup;
mod state_sync;
mod stt;
mod tts;
mod ui;
mod worker_log;
use intents::{IntentBatchResult, IntentState};
use panel::{PanelManager, PanelView, VISIBILITY_CHANNEL};
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Listener, Manager,
};
use tracing::{error, warn};

use commands::{
    clear_picovoice_key, clear_worker_auth_token, close_window, get_about_metadata,
    get_application_catalog, get_config_summary, get_conversation_memory, get_latency_hud,
    get_mcp_registry, get_preferences_snapshot, get_preload_status, get_recent_logs,
    get_secrets_status, get_ui_snapshot, handle_intent, handle_intents_batch, open_mcp_config_file,
    panel_hide, panel_navigate, process_user_request, refresh_application_catalog,
    reset_worker_session, set_picovoice_key, set_sprite_preload_state, set_worker_auth_token,
    toggle_mcp_server, toggle_ptt, update_config, update_preferences,
};
pub use metrics::{monotonic_now_ms, MetricsEvent, MetricsEventKind, MetricsPhase};

const MENU_ID_PANEL_TOGGLE: &str = "tray.panel.toggle";
const MENU_ID_APP_QUIT: &str = "tray.app.quit";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum AssistantIntent {
    Converse,
    OpenApp {
        app_name: String,
    },
    OpenAppNotFound {
        query: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        candidates: Option<Vec<String>>,
    },
    McpPlan {
        mcp_server: String,
        instruction: String,
    },
}

#[derive(Debug, Clone, Default)]
struct SpeechPlan {
    initial: Option<String>,
    updates: Vec<String>,
}

impl SpeechPlan {
    fn with_initial(text: String) -> Self {
        Self {
            initial: Some(text),
            updates: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn clear_initial(&mut self) {
        self.initial = None;
    }

    fn updates(&self) -> &[String] {
        &self.updates
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct AssistantCommandResponse {
    reply: String,
    #[serde(skip_serializing)]
    speech: SpeechPlan,
    intents: Vec<AssistantIntent>,
    #[serde(default)]
    reply_streamed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_plan: Option<crate::machine::mcp::McpPlan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_action_results: Vec<crate::machine::mcp::McpActionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_session_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dispatch_result: Option<IntentBatchResult>,
}

fn icon_image() -> Option<Image<'static>> {
    Image::from_bytes(include_bytes!("../icons/icon.ico")).ok()
}

fn init_tray(app: &tauri::AppHandle) {
    let panel_state = app.try_state::<PanelManager>();
    let panel_available = panel_state
        .as_ref()
        .map(|state| state.inner().is_available())
        .unwrap_or(false);
    let panel_visible = panel_state
        .as_ref()
        .map(|state| state.inner().is_visible())
        .unwrap_or(false);

    let panel_label = if panel_available {
        if panel_visible {
            "Hide Panel"
        } else {
            "Open Panel"
        }
    } else {
        "Panel Unavailable"
    };

    let panel_toggle_item =
        MenuItem::with_id(app, MENU_ID_PANEL_TOGGLE, panel_label, true, None::<&str>)
            .inspect(|item| {
                let _ = item.set_enabled(panel_available);
            })
            .ok();

    let quit_item = MenuItem::with_id(app, MENU_ID_APP_QUIT, "Quit", true, None::<&str>).ok();

    let menu = if let Some(panel_item) = &panel_toggle_item {
        let mut builder = MenuBuilder::new(app).item(panel_item);
        if let Some(quit_item) = &quit_item {
            builder = builder.separator().item(quit_item);
        }
        builder.build().ok()
    } else {
        None
    };

    if let Some(image) = icon_image() {
        let mut tray_builder = TrayIconBuilder::new()
            .icon(image)
            .tooltip("Sprite Assistant")
            .on_tray_icon_event(|tray, event| match event {
                TrayIconEvent::Click {
                    button,
                    button_state,
                    ..
                } if button == MouseButton::Left && button_state == MouseButtonState::Up => {
                    if let Some(window) = tray.app_handle().get_webview_window("main") {
                        let is_visible = window.is_visible().unwrap_or(true);
                        let is_minimized = window.is_minimized().unwrap_or(false);

                        if !is_visible || is_minimized {
                            let _ = window.unminimize();
                            let _ = window.show();
                            let _ = window.set_focus();
                        } else {
                            let _ = window.minimize();
                        }
                    }
                }
                _ => {}
            })
            .show_menu_on_left_click(false);

        if let Some(menu) = menu.as_ref() {
            tray_builder = tray_builder.menu(menu);
        }

        if let Err(err) = tray_builder.build(app) {
            error!("Failed to initialise tray icon: {err}");
        }
    }

    if let Some(panel_item) = panel_toggle_item.clone() {
        let app_handle = app.clone();
        let panel_item_for_events = panel_item.clone();
        app.on_menu_event(move |_, event| match event.id().as_ref() {
            MENU_ID_PANEL_TOGGLE => {
                if let Some(panel_state) = app_handle.try_state::<PanelManager>() {
                    let panel_manager = panel_state.inner();
                    if !panel_manager.is_available() {
                        warn!("Panel toggle requested while panel is disabled");
                        return;
                    }

                    let result = if panel_manager.is_visible() {
                        panel_manager.hide()
                    } else {
                        panel_manager.navigate(PanelView::Chat)
                    };

                    if let Err(err) = result {
                        warn!("Failed to toggle panel from tray: {err}");
                    } else {
                        let available = panel_manager.is_available();
                        let label = if !available {
                            "Panel Unavailable"
                        } else if panel_manager.is_visible() {
                            "Hide Panel"
                        } else {
                            "Open Panel"
                        };
                        let _ = panel_item_for_events.set_text(label);
                        let _ = panel_item_for_events.set_enabled(available);
                    }
                } else {
                    warn!("Panel manager state not available when handling tray menu");
                }
            }
            MENU_ID_APP_QUIT => {
                app_handle.exit(0);
            }
            _ => {}
        });

        let panel_item_for_visibility = panel_item.clone();
        let panel_available_for_visibility = panel_available;
        let _ = app.listen_any(VISIBILITY_CHANNEL, move |event| {
            if let Ok(visible) = event.payload().parse::<bool>() {
                let label = if !panel_available_for_visibility {
                    "Panel Unavailable"
                } else if visible {
                    "Hide Panel"
                } else {
                    "Open Panel"
                };
                let _ = panel_item_for_visibility.set_text(label);
                let _ = panel_item_for_visibility.set_enabled(panel_available_for_visibility);
            }
        });
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(desktop)]
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init());

    #[cfg(not(desktop))]
    let builder = tauri::Builder::default();

    builder
        .manage(IntentState::default())
        .setup(|app| startup::setup(app).map_err(|e| e.into()))
        .invoke_handler(tauri::generate_handler![
            close_window,
            handle_intent,
            handle_intents_batch,
            process_user_request,
            refresh_application_catalog,
            get_application_catalog,
            get_ui_snapshot,
            get_preload_status,
            get_config_summary,
            get_preferences_snapshot,
            get_recent_logs,
            reset_worker_session,
            get_conversation_memory,
            update_config,
            update_preferences,
            panel_navigate,
            panel_hide,
            set_sprite_preload_state,
            toggle_ptt,
            toggle_mcp_server,
            get_mcp_registry,
            open_mcp_config_file,
            get_latency_hud,
            get_about_metadata,
            get_secrets_status,
            set_picovoice_key,
            clear_picovoice_key,
            set_worker_auth_token,
            clear_worker_auth_token
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
