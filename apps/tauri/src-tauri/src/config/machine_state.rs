use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tauri::{App, AppHandle, Emitter};
use tracing::debug;

use crate::config::WorkerBridgeConfig;
use crate::intents::application::InstalledApp;
use crate::machine::worker_client::{
    auth_error_status, build_http_client, config_endpoint, delete_with_retry, get_json_with_retry,
    is_auth_error_message, memory_endpoint, post_json_with_retry, sync_endpoint,
};
use crate::machine::{EVENT_WORKER_AUTH_FAILED, EVENT_WORKER_AUTH_OK};

const PAYLOAD_PREVIEW_LIMIT: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineStateSync {
    pub machine_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_apps: Option<std::collections::HashMap<String, AllowlistEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_apps: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_apps: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_turn: Option<ConversationTurn>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_patch: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub role: String, // "user" | "assistant"
    pub content: String,
    #[serde(default)]
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intents: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ConversationMemory {
    Turns {
        turns: Vec<ConversationTurn>,
    },
    Summary {
        summary: String,
        #[serde(rename = "lastUpdated")]
        last_updated: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowlistEntry {
    pub enabled: bool,
    pub present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_for: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SafeAllowlistEntry {
    pub app_id: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbs: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct MachineStateManager {
    machine_id: String,
    app_handle: AppHandle,
    worker_config: Arc<RwLock<WorkerBridgeConfig>>,
}

impl MachineStateManager {
    pub fn load(app: &App, worker_config: WorkerBridgeConfig) -> Result<Self> {
        let machine_id = Self::get_or_create_machine_id(app)?;

        Ok(Self {
            machine_id,
            app_handle: app.handle().clone(),
            worker_config: Arc::new(RwLock::new(worker_config)),
        })
    }

    fn get_or_create_machine_id(_app: &App) -> Result<String> {
        // Generate machine ID from hardware (deterministic, same ID every time)
        // No need to persist - hardware ID is stable across restarts/reinstalls
        Self::generate_hardware_id()
    }

    fn generate_hardware_id() -> Result<String> {
        // Try to get machine ID from hardware
        // This is deterministic and will return the same ID every time on the same machine
        machine_uid::machine_id::get_machine_id()
            .map_err(|e| anyhow::anyhow!("Failed to get machine ID: {}", e))
    }

    pub fn machine_id(&self) -> &str {
        &self.machine_id
    }

    pub fn update_worker_config(&self, config: WorkerBridgeConfig) {
        if let Ok(mut guard) = self.worker_config.write() {
            *guard = config;
        }
    }

    fn emit_auth_failure(&self, endpoint: &str, error: &str) {
        if !is_auth_error_message(error) {
            return;
        }
        #[derive(Clone, serde::Serialize)]
        struct WorkerAuthFailureEvent<'a> {
            endpoint: &'a str,
            status: Option<u16>,
            message: &'a str,
        }
        let payload = WorkerAuthFailureEvent {
            endpoint,
            status: auth_error_status(error),
            message: error,
        };
        let _ = self.app_handle.emit(EVENT_WORKER_AUTH_FAILED, payload);
    }

    fn emit_auth_success(&self, endpoint: &str) {
        #[derive(Clone, serde::Serialize)]
        struct WorkerAuthOkEvent<'a> {
            endpoint: &'a str,
        }
        let _ = self
            .app_handle
            .emit(EVENT_WORKER_AUTH_OK, WorkerAuthOkEvent { endpoint });
    }

    /// Build allowlist from installed apps and preferences
    /// This is the source of truth - Rust builds it locally
    #[allow(dead_code)]
    pub fn build_allowlist(
        installed_apps: &[InstalledApp],
        preferences: &crate::config::preferences::PreferencesConfig,
    ) -> Vec<SafeAllowlistEntry> {
        installed_apps
            .iter()
            .filter_map(|app| {
                let app_id = app.app_id.as_ref().or(Some(&app.name))?;
                let enabled = preferences.is_app_enabled(app_id);

                if enabled {
                    Some(SafeAllowlistEntry {
                        app_id: app_id.clone(),
                        enabled: true,
                        display_name: Some(app.name.clone()),
                        verbs: None, // Can be enhanced later
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Convert installed apps to AllowlistEntry format for syncing
    pub fn build_allowlist_entries(
        installed_apps: &[InstalledApp],
        preferences: &crate::config::preferences::PreferencesConfig,
    ) -> std::collections::HashMap<String, AllowlistEntry> {
        installed_apps
            .iter()
            .filter_map(|app| {
                let app_id = app.app_id.as_ref().or(Some(&app.name))?;
                let enabled = preferences.is_app_enabled(app_id);

                Some((
                    app_id.clone(),
                    AllowlistEntry {
                        enabled,
                        present: true,
                        version: None,
                        default_for: None,
                        extensions: None,
                    },
                ))
            })
            .collect()
    }

    /// Sync installed apps catalog to worker (async, fire-and-forget)
    pub async fn sync_catalog(
        &self,
        installed_apps: &[InstalledApp],
        preferences: &crate::config::preferences::PreferencesConfig,
    ) -> Result<()> {
        let installed_apps_map = Self::build_allowlist_entries(installed_apps, preferences);

        let sync = MachineStateSync {
            machine_id: self.machine_id.clone(),
            installed_apps: Some(installed_apps_map),
            enabled_apps: Some(preferences.enabled_apps()),
            disabled_apps: Some(preferences.disabled_apps()),
            conversation_turn: None,
            config_patch: None,
        };

        self.sync_state(sync).await
    }

    /// Sync state to worker (async, fire-and-forget)
    pub async fn sync_state(&self, sync: MachineStateSync) -> Result<()> {
        let worker_config = self
            .worker_config
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to read worker config"))?
            .clone();

        let endpoint = sync_endpoint(&worker_config);
        let client = build_http_client().map_err(|e: String| anyhow::anyhow!(e))?;
        let sync_value = serde_json::to_value(&sync)
            .map_err(|err| anyhow::anyhow!("Failed to serialize sync payload: {err}"))?;

        let payload_preview = preview_json(&sync);
        debug!(
            machine_id = %self.machine_id,
            endpoint = %endpoint,
            installed_app_entries = %sync
                .installed_apps
                .as_ref()
                .map(|apps| apps.len())
                .unwrap_or(0),
            payload_preview = %payload_preview,
            "sending machine state sync"
        );

        let post_result: Result<serde_json::Value, String> = post_json_with_retry(
            &client,
            &endpoint,
            &sync_value,
            &worker_config,
            "Machine state sync payload",
        )
        .await;
        if let Err(err) = post_result {
            self.emit_auth_failure(&endpoint, &err);
            return Err(anyhow::anyhow!("Worker sync failed: {err}"));
        }
        self.emit_auth_success(&endpoint);

        debug!(
            machine_id = %self.machine_id,
            endpoint = %endpoint,
            installed_app_entries = %sync
                .installed_apps
                .as_ref()
                .map(|apps| apps.len())
                .unwrap_or(0),
            "machine state sync succeeded"
        );
        Ok(())
    }

    /// Sync config preferences to worker (async, fire-and-forget)
    pub async fn sync_config(&self, config: serde_json::Value) -> Result<()> {
        debug!(machine_id = %self.machine_id, "syncing machine config to worker");

        let worker_config = self
            .worker_config
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to read worker config"))?
            .clone();

        let endpoint = config_endpoint(&worker_config);
        let client = build_http_client().map_err(|e: String| anyhow::anyhow!(e))?;

        let payload = serde_json::json!({
            "machine_id": self.machine_id,
            "config": config
        });

        let payload_preview = preview_json(&payload);
        debug!(
            machine_id = %self.machine_id,
            endpoint = %endpoint,
            payload_preview = %payload_preview,
            "sending machine config sync"
        );

        let post_result: Result<serde_json::Value, String> = post_json_with_retry(
            &client,
            &endpoint,
            &payload,
            &worker_config,
            "Machine config sync payload",
        )
        .await;
        if let Err(err) = post_result {
            self.emit_auth_failure(&endpoint, &err);
            return Err(anyhow::anyhow!("Worker config sync failed: {err}"));
        }
        self.emit_auth_success(&endpoint);

        debug!(
            machine_id = %self.machine_id,
            endpoint = %endpoint,
            payload_preview = %payload_preview,
            "machine config sync succeeded"
        );
        Ok(())
    }

    pub async fn reset_session(&self) -> Result<()> {
        debug!(machine_id = %self.machine_id, "resetting worker session state");

        let worker_config = self
            .worker_config
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to read worker config"))?
            .clone();

        let mut endpoint = reqwest::Url::parse(memory_endpoint(&worker_config).as_str())
            .context("Failed to parse worker memory endpoint")?;
        endpoint
            .query_pairs_mut()
            .append_pair("machine_id", &self.machine_id);

        let endpoint_string = endpoint.to_string();
        let client = build_http_client().map_err(|e: String| anyhow::anyhow!(e))?;
        if let Err(err) = delete_with_retry(&client, endpoint.as_str(), &worker_config).await {
            self.emit_auth_failure(endpoint.as_str(), &err);
            return Err(anyhow::anyhow!("Failed to reset worker session: {err}"));
        }
        self.emit_auth_success(endpoint.as_str());

        debug!(
            machine_id = %self.machine_id,
            endpoint = %endpoint_string,
            "worker session reset succeeded"
        );
        Ok(())
    }

    /// Fetch the latest stored conversation memory from the worker.
    pub async fn fetch_memory(&self) -> Result<ConversationMemory> {
        let worker_config = self
            .worker_config
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to read worker config"))?
            .clone();

        let endpoint = format!(
            "{}?machine_id={}",
            memory_endpoint(&worker_config),
            urlencoding::encode(&self.machine_id)
        );

        let client = build_http_client().map_err(|e: String| anyhow::anyhow!(e))?;
        let memory_result = get_json_with_retry::<ConversationMemory>(
            &client,
            &endpoint,
            &worker_config,
            "Conversation memory response",
        )
        .await;
        let memory = match memory_result {
            Ok(memory) => memory,
            Err(err) => {
                self.emit_auth_failure(&endpoint, &err);
                return Err(anyhow::anyhow!(err));
            }
        };
        self.emit_auth_success(&endpoint);

        Ok(memory)
    }
}

fn preview_json<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_string(value) {
        Ok(serialised) => {
            if serialised.len() > PAYLOAD_PREVIEW_LIMIT {
                format!(
                    "{}… ({} bytes total)",
                    &serialised[..PAYLOAD_PREVIEW_LIMIT],
                    serialised.len()
                )
            } else {
                serialised
            }
        }
        Err(err) => format!("<failed to serialise payload: {err}>"),
    }
}
