use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::sync::{Arc, RwLock};
use tauri::{App, AppHandle, Manager};
use tracing::{debug, error, info, warn};

use crate::stt::model_downloader;
use crate::ui::sprite_window::SpriteWindowController;

pub mod defaults;
mod diff;
pub mod dirty_watchers;
mod identity;
pub mod io;
pub mod machine_state;
pub mod mcp;
pub mod paths;
pub mod preferences;
pub mod prelude;
pub mod stt;
mod summary;
pub mod tts;
pub mod ui;
mod worker;

pub use defaults::DEFAULT_WORKER_BASE_URL;
pub use identity::IdentityConfig;
pub use ui::PanelConfig;
pub use worker::{RetryPolicy, WorkerBridgeConfig};

use diff::{diff_values, merge_json};
use summary::{
    ConfigSummary, ConfigUpdateResponse, McpSummary, ModelCatalogEntrySummary, SttSummary,
    TtsSummary, UiSummary, WorkerSummary,
};

#[derive(Clone)]
pub struct AppConfig {
    identity: IdentityConfig,
    worker: Arc<RwLock<WorkerBridgeConfig>>,
    stt: Arc<RwLock<stt::SttConfig>>,
    tts: Arc<RwLock<tts::TtsConfig>>,
    ui: ui::UiRuntimeConfig,
    preferences: Arc<RwLock<preferences::PreferencesConfig>>,
    mcp: Arc<RwLock<mcp::McpConfig>>,
}

impl AppConfig {
    pub fn load(app: &App) -> Result<Self> {
        let identity = IdentityConfig::load(app)?;
        let mut stt = stt::SttConfig::load(app)?;
        let tts = tts::TtsConfig::load(app)?;
        let worker = WorkerBridgeConfig::load(app, &stt.worker);
        stt.apply_worker_endpoint(&worker);
        let ui = ui::UiRuntimeConfig::load(app);
        let preferences = preferences::PreferencesConfig::load(app)?;
        let mcp = mcp::load(app)?;

        let config = Self {
            identity,
            worker: Arc::new(RwLock::new(worker)),
            stt: Arc::new(RwLock::new(stt)),
            tts: Arc::new(RwLock::new(tts)),
            ui,
            preferences: Arc::new(RwLock::new(preferences)),
            mcp: Arc::new(RwLock::new(mcp)),
        };

        debug!("Loaded config summary: {:?}", config.summary());

        Ok(config)
    }

    pub fn identity(&self) -> &IdentityConfig {
        &self.identity
    }

    pub fn worker(&self) -> WorkerBridgeConfig {
        self.worker
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| WorkerBridgeConfig::new(DEFAULT_WORKER_BASE_URL.to_string()))
    }

    pub fn worker_state(&self) -> Arc<RwLock<WorkerBridgeConfig>> {
        Arc::clone(&self.worker)
    }

    pub fn stt(&self) -> Arc<RwLock<stt::SttConfig>> {
        Arc::clone(&self.stt)
    }

    pub fn tts(&self) -> Arc<RwLock<tts::TtsConfig>> {
        Arc::clone(&self.tts)
    }

    pub fn ui(&self) -> &ui::UiRuntimeConfig {
        &self.ui
    }

    pub fn preferences(&self) -> Arc<RwLock<preferences::PreferencesConfig>> {
        Arc::clone(&self.preferences)
    }

    pub fn mcp(&self) -> Arc<RwLock<mcp::McpConfig>> {
        Arc::clone(&self.mcp)
    }

    pub fn summary(&self) -> ConfigSummary {
        let stt_snapshot = self
            .stt
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        let tts_snapshot = self
            .tts
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        let mcp_snapshot = self
            .mcp
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        let mcp_summary = McpSummary::from(&mcp_snapshot);

        ConfigSummary {
            stt: SttSummary::from(&stt_snapshot),
            tts: TtsSummary::from(&tts_snapshot),
            worker: WorkerSummary::from_current(&self.worker),
            ui: UiSummary::from(&self.ui),
            models: model_downloader::catalog_entries()
                .into_iter()
                .map(ModelCatalogEntrySummary::from)
                .collect(),
            mcp: (mcp_summary.server_count > 0).then_some(mcp_summary),
        }
    }

    pub fn persist_mcp_config(&self, app_handle: &AppHandle) -> Result<()> {
        let snapshot = self
            .mcp
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| anyhow!("failed to read MCP configuration"))?;
        mcp::save_to_handle(app_handle, &snapshot)
    }

    pub fn apply_stt_patch(
        &self,
        app_handle: &AppHandle,
        patch: &Value,
    ) -> Result<ConfigUpdateResponse> {
        debug!("Applying STT config patch");
        let (current_stt, updated_config, original_value, updated_value) =
            self.merge_and_validate_stt(patch)?;

        let diff = diff_values(&original_value, &updated_value);
        if diff.is_null() {
            debug!("STT config patch produced no changes");
            return Ok(ConfigUpdateResponse {
                summary: self.summary(),
                diff,
            });
        }

        let worker_changed = current_stt.worker.url != updated_config.worker.url;
        let sprite_scale_changed =
            (current_stt.sprite.scale - updated_config.sprite.scale).abs() > f32::EPSILON;

        info!(
            "Persisting STT config update (worker_changed={}, sprite_scale_changed={})",
            worker_changed, sprite_scale_changed
        );
        self.persist_stt_update(app_handle, &updated_config)?;
        self.apply_stt_side_effects(
            app_handle,
            &updated_config,
            worker_changed,
            sprite_scale_changed,
        )?;
        info!("STT config update applied");

        Ok(ConfigUpdateResponse {
            summary: self.summary(),
            diff,
        })
    }

    fn merge_and_validate_stt(
        &self,
        patch: &Value,
    ) -> Result<(stt::SttConfig, stt::SttConfig, Value, Value)> {
        if !patch.is_object() {
            return Err(anyhow!("config patch must be a JSON object"));
        }

        let current_stt = self
            .stt
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| anyhow!("failed to read current STT configuration"))?;

        let original_value =
            serde_json::to_value(&current_stt).context("failed to serialise current config")?;
        let mut updated_value = original_value.clone();
        merge_json(&mut updated_value, patch);

        let mut updated_config: stt::SttConfig = serde_json::from_value(updated_value.clone())
            .context("failed to parse merged configuration")?;
        updated_config.sync_model_path_from_id();
        updated_config
            .validate()
            .context("configuration did not pass validation")?;

        Ok((current_stt, updated_config, original_value, updated_value))
    }

    fn persist_stt_update(
        &self,
        app_handle: &AppHandle,
        updated_config: &stt::SttConfig,
    ) -> Result<()> {
        let mut guard = self
            .stt
            .write()
            .map_err(|_| anyhow!("failed to acquire configuration lock for update"))?;
        let previous = guard.clone();
        *guard = updated_config.clone();

        if let Err(err) = persist_stt_config(app_handle, updated_config) {
            warn!("Failed to persist STT config update; reverting in-memory state: {err:?}");
            *guard = previous;
            return Err(err);
        }

        Ok(())
    }

    fn apply_stt_side_effects(
        &self,
        app_handle: &AppHandle,
        updated_config: &stt::SttConfig,
        worker_changed: bool,
        sprite_scale_changed: bool,
    ) -> Result<()> {
        if sprite_scale_changed {
            if let Some(controller) = app_handle.try_state::<SpriteWindowController>() {
                if let Err(err) = controller.apply_scale(app_handle, updated_config.sprite.scale) {
                    warn!("Failed to apply sprite window scale after config update: {err}");
                }
            } else {
                warn!("Sprite window controller not available when applying config update");
            }
        }

        if worker_changed {
            info!("Applying STT worker config update");
            let mut worker_guard = self
                .worker
                .write()
                .map_err(|_| anyhow!("failed to update worker configuration"))?;
            worker_guard.apply_stt_worker_config(&updated_config.worker);
        }

        if worker_changed {
            if let Some(state) =
                app_handle.try_state::<crate::config::machine_state::MachineStateManager>()
            {
                state.inner().update_worker_config(self.worker());
            }
        }

        if let Some(service) = app_handle.try_state::<crate::stt::SttService>() {
            if worker_changed {
                service.update_worker_config(self.worker());
            }
            info!("Forwarding STT config update to running service");
            service.apply_config_update(updated_config.clone());
            return Ok(());
        }

        info!("STT service not running; scheduling preload after config update");
        let app_handle_clone = app_handle.clone();
        let preload_config = updated_config.clone();
        let stt_state = Arc::clone(&self.stt);
        tauri::async_runtime::spawn(async move {
            let preload_handle = app_handle_clone.clone();
            let preload_result = tauri::async_runtime::spawn_blocking(move || {
                crate::stt::preload::prepare_stt(&preload_handle, &preload_config)
            })
            .await;

            match preload_result {
                Ok(Ok(updated)) => {
                    if let Some(config) = updated {
                        if let Ok(mut guard) = stt_state.write() {
                            *guard = config.clone();
                        }
                        if let Err(err) = config.save_to_handle(&app_handle_clone) {
                            error!("Failed to persist STT config after preload: {err:?}");
                        } else if let Some(service) =
                            app_handle_clone.try_state::<crate::stt::SttService>()
                        {
                            service.update_config_fingerprint(&config);
                        }
                    }
                }
                Ok(Err(err)) => {
                    error!("Preload task failed after config update: {err:?}");
                }
                Err(err) => {
                    error!("Failed to join preload task after config update: {err}");
                }
            }
        });
        Ok(())
    }
}

fn persist_stt_config(app_handle: &AppHandle, config: &stt::SttConfig) -> Result<()> {
    debug!("Writing STT config to disk");
    let result = config.save_to_handle(app_handle);
    if result.is_ok() {
        info!("STT config saved to disk");
    }
    result
}
