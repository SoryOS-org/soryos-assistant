use std::sync::{Arc, Mutex};

use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tracing::{debug, error, warn};

use crate::config::PanelConfig;

const PANEL_WINDOW_LABEL: &str = "panel";
const NAVIGATION_CHANNEL: &str = "panel://navigate";
pub const VISIBILITY_CHANNEL: &str = "panel://visibility";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelView {
    Config,
    Preferences,
    #[default]
    Chat,
    Logs,
    About,
    Mcp,
}

impl PanelView {
    pub fn as_str(self) -> &'static str {
        match self {
            PanelView::Config => "config",
            PanelView::Preferences => "preferences",
            PanelView::Chat => "chat",
            PanelView::Logs => "logs",
            PanelView::About => "about",
            PanelView::Mcp => "mcp",
        }
    }
}

impl TryFrom<&str> for PanelView {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.trim().to_ascii_lowercase().as_str() {
            "config" => Ok(PanelView::Config),
            "preferences" => Ok(PanelView::Preferences),
            "chat" => Ok(PanelView::Chat),
            "logs" => Ok(PanelView::Logs),
            "about" => Ok(PanelView::About),
            "mcp" => Ok(PanelView::Mcp),
            other => Err(format!("unknown panel view '{other}'")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PanelRuntimeState {
    visible: bool,
    current_view: PanelView,
}

#[derive(Clone)]
pub struct PanelManager {
    app_handle: AppHandle,
    config: PanelConfig,
    app_user_model_id: String,
    runtime: Arc<Mutex<PanelRuntimeState>>,
}

impl PanelManager {
    pub fn new(app_handle: AppHandle, config: PanelConfig, app_user_model_id: String) -> Self {
        let manager = Self {
            app_handle,
            config,
            app_user_model_id,
            runtime: Arc::new(Mutex::new(PanelRuntimeState::default())),
        };

        if manager.config.enabled() {
            if let Err(err) = manager.ensure_window() {
                error!("failed to create panel window: {err}");
            }
        }

        manager
    }

    pub fn is_available(&self) -> bool {
        self.config.enabled()
    }

    pub fn is_visible(&self) -> bool {
        self.runtime
            .lock()
            .map(|guard| guard.visible)
            .unwrap_or(false)
    }

    pub fn navigate(&self, view: PanelView) -> Result<(), String> {
        if !self.config.enabled() {
            debug!("panel navigation requested while feature is disabled");
            return Err("panel window is disabled".to_string());
        }

        let window = self
            .ensure_window()?
            .ok_or_else(|| "panel window is not available".to_string())?;

        {
            let mut guard = self
                .runtime
                .lock()
                .map_err(|_| "panel runtime state is poisoned".to_string())?;
            guard.current_view = view;
        }

        window
            .emit(NAVIGATION_CHANNEL, view.as_str())
            .map_err(|err| err.to_string())?;

        let should_focus = {
            let guard = self
                .runtime
                .lock()
                .map_err(|_| "panel runtime state is poisoned".to_string())?;
            !guard.visible
        };

        if should_focus {
            window
                .show()
                .map_err(|err| format!("failed to show panel window: {err}"))?;
            window
                .set_focus()
                .map_err(|err| format!("failed to focus panel window: {err}"))?;
            window
                .set_skip_taskbar(false)
                .map_err(|err| format!("failed to add panel window to taskbar: {err}"))?;

            if let Ok(mut guard) = self.runtime.lock() {
                guard.visible = true;
            }
        }

        self.publish_visibility(true);
        Ok(())
    }

    pub fn hide(&self) -> Result<(), String> {
        if !self.config.enabled() {
            return Ok(());
        }

        let Some(window) = self.app_handle.get_webview_window(PANEL_WINDOW_LABEL) else {
            return Ok(());
        };

        self.transition_to_hidden(&window)?;
        Ok(())
    }

    fn ensure_window(&self) -> Result<Option<WebviewWindow>, String> {
        if !self.config.enabled() {
            return Ok(None);
        }

        if let Some(window) = self.app_handle.get_webview_window(PANEL_WINDOW_LABEL) {
            return Ok(Some(window));
        }

        // Aggregate dist (ui-dist): the v1 webview sits at the root and the
        // v2 desktop UI lives under `desktop/`. `SPRITE_PANEL_V2=1` opts the
        // panel window into the v2 UI; the default stays the v1 panel until
        // the cutover is complete.
        let panel_entry = if std::env::var("SPRITE_PANEL_V2").as_deref() == Ok("1") {
            "desktop/index.html"
        } else {
            "panel/index.html"
        };

        // v2 test mode: show the panel immediately so the v2 UI is visible
        // without going through the tray menu.
        let v2_panel = std::env::var("SPRITE_PANEL_V2").as_deref() == Ok("1");

        let builder = WebviewWindowBuilder::new(
            &self.app_handle,
            PANEL_WINDOW_LABEL,
            WebviewUrl::App(panel_entry.into()),
        )
        .title("Sprite Assistant Control Panel")
        .visible(v2_panel)
        .center()
        .resizable(true)
        .inner_size(960.0, 720.0)
        .min_inner_size(960.0, 960.0)
        .decorations(true)
        .maximizable(true)
        .skip_taskbar(true);

        if !self.app_user_model_id.is_empty() {
            let _args = format!("--app-user-model-id={}", self.app_user_model_id);
            // Applying additional browser args helps group the panel with the app in production,
            // but it significantly slows down the debug/dev startup on Windows. Leave this
            // commented out unless we have a build-time toggle to control it.
            // builder = builder.additional_browser_args(&_args);
        }

        let window = builder.build().map_err(|err| err.to_string())?;

        if v2_panel {
            // Keep the runtime state consistent with the shown window so the
            // tray toggle hides it on first click.
            let _ = window.set_skip_taskbar(false);
            if let Ok(mut guard) = self.runtime.lock() {
                guard.visible = true;
            }
        }

        window
            .emit(NAVIGATION_CHANNEL, PanelView::default().as_str())
            .map_err(|err| err.to_string())?;

        let state = Arc::clone(&self.runtime);
        let window_for_events = window.clone();
        let visibility_emitter = self.app_handle.clone();
        window.on_window_event(move |event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                if let Err(err) = window_for_events.hide() {
                    warn!("failed to hide panel on close request: {err}");
                }
                if let Err(err) = window_for_events.set_skip_taskbar(true) {
                    warn!("failed to clear panel taskbar presence: {err}");
                }
                if let Err(err) =
                    window_for_events.emit(NAVIGATION_CHANNEL, PanelView::default().as_str())
                {
                    debug!("failed to emit default navigation on close: {err}");
                }
                if let Ok(mut guard) = state.lock() {
                    guard.visible = false;
                    guard.current_view = PanelView::default();
                }
                if let Err(err) = visibility_emitter.emit(VISIBILITY_CHANNEL, false) {
                    debug!("failed to emit visibility event on close: {err}");
                }
            }
            WindowEvent::Destroyed => {
                if let Ok(mut guard) = state.lock() {
                    guard.visible = false;
                    guard.current_view = PanelView::default();
                }
                if let Err(err) = visibility_emitter.emit(VISIBILITY_CHANNEL, false) {
                    debug!("failed to emit visibility event on destroy: {err}");
                }
            }
            _ => {}
        });

        Ok(Some(window))
    }

    fn transition_to_hidden(&self, window: &WebviewWindow) -> Result<(), String> {
        window
            .hide()
            .map_err(|err| format!("failed to hide panel window: {err}"))?;
        window
            .set_skip_taskbar(true)
            .map_err(|err| format!("failed to clear panel window taskbar entry: {err}"))?;
        window
            .emit(NAVIGATION_CHANNEL, PanelView::default().as_str())
            .map_err(|err| err.to_string())?;

        if let Ok(mut guard) = self.runtime.lock() {
            guard.visible = false;
            guard.current_view = PanelView::default();
        }

        self.publish_visibility(false);
        Ok(())
    }

    fn publish_visibility(&self, visible: bool) {
        if let Err(err) = self.app_handle.emit(VISIBILITY_CHANNEL, visible) {
            debug!("failed to emit panel visibility event: {err}");
        }
    }
}
