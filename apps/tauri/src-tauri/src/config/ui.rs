use tauri::App;

use super::defaults;

#[derive(Clone)]
pub struct UiRuntimeConfig {
    panel: PanelConfig,
    show_main_in_taskbar: bool,
}

impl UiRuntimeConfig {
    pub fn load(app: &App) -> Self {
        let enable_panel = app
            .config()
            .plugins
            .0
            .get("sprite")
            .and_then(|value| value.get("ui"))
            .and_then(|value| value.get("enablePanel"))
            .and_then(|value| value.as_bool())
            .unwrap_or(defaults::UI_PANEL_ENABLED);

        let show_main_in_taskbar = app
            .config()
            .plugins
            .0
            .get("sprite")
            .and_then(|value| value.get("ui"))
            .and_then(|value| value.get("showMainInTaskbar"))
            .and_then(|value| value.as_bool())
            .unwrap_or(defaults::UI_SHOW_MAIN_IN_TASKBAR);

        Self {
            panel: PanelConfig::new(enable_panel),
            show_main_in_taskbar,
        }
    }

    pub fn panel(&self) -> &PanelConfig {
        &self.panel
    }

    pub fn panel_enabled(&self) -> bool {
        self.panel.enabled()
    }

    pub fn show_main_in_taskbar(&self) -> bool {
        self.show_main_in_taskbar
    }
}

#[derive(Clone)]
pub struct PanelConfig {
    enabled: bool,
}

impl PanelConfig {
    fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }
}
