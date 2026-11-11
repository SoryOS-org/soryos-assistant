use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tauri::{App, AppHandle};

use super::defaults;
use super::io;

const CONFIG_FILENAME: &str = "preferences.config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferencesConfig {
    #[serde(default = "default_personality")]
    pub personality: String,

    /// Allowlist overrides.
    ///
    /// Values are only written when a user explicitly flips a switch in the UI; the map stays empty otherwise.
    /// Because `app_states` is empty on first run, every discovered app resolves to
    /// `PreferencesConfig::is_app_enabled(...) == true`, which is the behaviour we rely on during the initial sync.
    #[serde(default)]
    pub allowlist: AllowlistPreferences,
    // Add other user preferences here as needed
    // Example: theme, language, etc.
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AllowlistPreferences {
    /// Map of app_id -> enabled state
    /// Only stored for apps that have been explicitly enabled/disabled
    /// If an app_id is not in this map, it defaults to enabled (true)
    #[serde(default)]
    pub app_states: HashMap<String, bool>,
}

fn default_personality() -> String {
    defaults::PREFERENCES_DEFAULT_PERSONALITY.to_string()
}

impl Default for PreferencesConfig {
    fn default() -> Self {
        Self {
            personality: default_personality(),
            allowlist: AllowlistPreferences::default(),
        }
    }
}

impl PreferencesConfig {
    pub fn load(app: &App) -> Result<Self> {
        io::load_or_create(
            app,
            CONFIG_FILENAME,
            |content| {
                serde_json::from_str::<PreferencesConfig>(content)
                    .context("Failed to parse preferences config")
            },
            |config| {
                serde_json::to_string_pretty(config)
                    .context("Failed to serialise preferences configuration")
            },
            PreferencesConfig::default,
        )
    }

    pub fn save_to_handle(&self, app_handle: &AppHandle) -> Result<()> {
        io::save_to_handle(app_handle, CONFIG_FILENAME, self, |config| {
            serde_json::to_string_pretty(config)
                .context("Failed to serialise preferences configuration")
        })
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::json!({
            "personality": self.personality,
        })
    }

    /// Save preferences and return config JSON for syncing
    #[allow(dead_code)]
    pub fn save_and_prepare_sync(&self, app_handle: &AppHandle) -> Result<serde_json::Value> {
        self.save_to_handle(app_handle)?;
        Ok(self.to_json_value())
    }

    /// Check if an app is enabled (defaults to true if not explicitly set)
    pub fn is_app_enabled(&self, app_id: &str) -> bool {
        self.allowlist
            .app_states
            .get(app_id)
            .copied()
            .unwrap_or(true)
    }

    /// Set the enabled state for an app
    #[allow(dead_code)]
    pub fn set_app_enabled(&mut self, app_id: String, enabled: bool) {
        if enabled {
            // Remove entry if setting to default (true)
            self.allowlist.app_states.remove(&app_id);
        } else {
            // Only store if explicitly disabled
            self.allowlist.app_states.insert(app_id, false);
        }
    }

    /// Get all explicitly disabled apps
    pub fn disabled_apps(&self) -> Vec<String> {
        self.allowlist
            .app_states
            .iter()
            .filter_map(|(app_id, &enabled)| if !enabled { Some(app_id.clone()) } else { None })
            .collect()
    }

    /// Get all explicitly enabled apps (those that were disabled and re-enabled)
    pub fn enabled_apps(&self) -> Vec<String> {
        self.allowlist
            .app_states
            .iter()
            .filter_map(|(app_id, &enabled)| if enabled { Some(app_id.clone()) } else { None })
            .collect()
    }

    /// Return a clone of the explicit allowlist overrides.
    pub fn allowlist_overrides(&self) -> HashMap<String, bool> {
        self.allowlist.app_states.clone()
    }

    /// Replace the explicit allowlist overrides with the provided map.
    pub fn set_allowlist_overrides(&mut self, overrides: HashMap<String, bool>) {
        self.allowlist.app_states = overrides;
    }
}
