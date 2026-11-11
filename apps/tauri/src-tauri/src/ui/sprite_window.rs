use tauri::{AppHandle, LogicalSize, Manager, Size, WebviewWindow};

use crate::config::stt::{SPRITE_SCALE_MAX, SPRITE_SCALE_MIN};

const SPRITE_WINDOW_LABEL: &str = "main";
pub const PRELOAD_WINDOW_WIDTH: f64 = 520.0;
pub const PRELOAD_WINDOW_HEIGHT: f64 = 420.0;

#[derive(Clone)]
pub struct SpriteWindowController {
    base_logical: LogicalSize<f64>,
    resizable_default: bool,
}

impl SpriteWindowController {
    fn apply_size_to(
        &self,
        window: &WebviewWindow,
        target_logical: LogicalSize<f64>,
        min_logical: LogicalSize<f64>,
    ) -> Result<(), String> {
        let mut needs_restore = false;
        if !self.resizable_default {
            if let Err(err) = window.set_resizable(true) {
                return Err(format!(
                    "failed to enable resizing for sprite window: {err}"
                ));
            }
            needs_restore = true;
        }

        window
            .set_min_size(Some(Size::Logical(min_logical)))
            .map_err(|err| format!("failed to update sprite window minimum size: {err}"))?;

        window
            .set_size(Size::Logical(target_logical))
            .map_err(|err| format!("failed to update sprite window size: {err}"))?;

        if needs_restore {
            if let Err(err) = window.set_resizable(false) {
                return Err(format!(
                    "failed to restore sprite window resizable flag: {err}"
                ));
            }
        }

        Ok(())
    }

    pub fn initialise(window: &WebviewWindow) -> Result<Self, String> {
        let inner_size = window
            .inner_size()
            .map_err(|err| format!("failed to read sprite window size: {err}"))?;
        let scale_factor = window
            .scale_factor()
            .map_err(|err| format!("failed to read sprite window scale factor: {err}"))?;
        let resizable_default = window
            .is_resizable()
            .map_err(|err| format!("failed to read sprite window resizable flag: {err}"))?;

        if scale_factor <= 0.0 {
            return Err("sprite window reported invalid scale factor (<= 0)".to_string());
        }

        let base_logical = LogicalSize::new(
            inner_size.width as f64 / scale_factor,
            inner_size.height as f64 / scale_factor,
        );

        Ok(Self {
            base_logical,
            resizable_default,
        })
    }

    pub fn apply_scale_to(&self, window: &WebviewWindow, scale: f32) -> Result<(), String> {
        let clamped = scale.clamp(SPRITE_SCALE_MIN, SPRITE_SCALE_MAX) as f64;

        let target_logical = LogicalSize::new(
            self.base_logical.width * clamped,
            self.base_logical.height * clamped,
        );
        let min_logical = LogicalSize::new(
            self.base_logical.width * (SPRITE_SCALE_MIN as f64),
            self.base_logical.height * (SPRITE_SCALE_MIN as f64),
        );

        self.apply_size_to(window, target_logical, min_logical)
    }

    pub fn apply_scale(&self, app_handle: &AppHandle, scale: f32) -> Result<(), String> {
        let Some(window) = app_handle.get_webview_window(SPRITE_WINDOW_LABEL) else {
            return Err("sprite window is not available".to_string());
        };

        self.apply_scale_to(&window, scale)
    }

    pub fn preload_scale_for(&self, width: f64, height: f64) -> f32 {
        let width_scale = (width / self.base_logical.width) as f32;
        let height_scale = (height / self.base_logical.height) as f32;
        width_scale
            .min(height_scale)
            .clamp(SPRITE_SCALE_MIN, SPRITE_SCALE_MAX)
    }

    pub fn apply_preload_size_to(
        &self,
        window: &WebviewWindow,
        width: f64,
        height: f64,
    ) -> Result<f32, String> {
        let target_logical = LogicalSize::new(width, height);
        self.apply_size_to(window, target_logical, target_logical)?;
        Ok(self.preload_scale_for(width, height))
    }

    pub fn apply_preload_size(
        &self,
        app_handle: &AppHandle,
        width: f64,
        height: f64,
    ) -> Result<f32, String> {
        let Some(window) = app_handle.get_webview_window(SPRITE_WINDOW_LABEL) else {
            return Err("sprite window is not available".to_string());
        };

        self.apply_preload_size_to(&window, width, height)
    }
}
