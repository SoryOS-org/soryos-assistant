use std::sync::{Arc, RwLock};

use serde_json::json;

use crate::config::{
    self,
    machine_state::{MachineStateManager, MachineStateSync},
};
use crate::intents::application::InstalledApp;

use super::chunking::{
    INTERNAL_CHUNK_FETCH_TOOL, INTERNAL_CHUNK_SERVER_ID, INTERNAL_TEXT_FIND_TOOL,
};

/// Snapshot the preferences configuration without holding the lock across await points.
pub fn snapshot_preferences(
    preferences: &Arc<RwLock<config::preferences::PreferencesConfig>>,
) -> Option<config::preferences::PreferencesConfig> {
    preferences.read().map(|guard| guard.clone()).ok()
}

/// Synchronise preferences with the worker.
pub async fn sync_preferences(
    machine_state: MachineStateManager,
    preferences: config::preferences::PreferencesConfig,
) -> Result<(), String> {
    let payload = preferences.to_json_value();
    machine_state
        .sync_config(payload)
        .await
        .map_err(|err| err.to_string())
}

/// Synchronise the installed application catalog with the worker.
pub async fn sync_catalog(
    machine_state: MachineStateManager,
    installed_apps: Vec<InstalledApp>,
    preferences: config::preferences::PreferencesConfig,
) -> Result<(), String> {
    machine_state
        .sync_catalog(&installed_apps, &preferences)
        .await
        .map_err(|err| err.to_string())
}

/// Synchronise the MCP configuration with the worker.
pub async fn sync_mcp_config(
    machine_state: MachineStateManager,
    config: serde_json::Value,
) -> Result<(), String> {
    let timestamp_ms = current_timestamp();
    let mut servers = serde_json::Map::new();
    servers.insert(
        INTERNAL_CHUNK_SERVER_ID.to_string(),
        internal_chunk_server_snapshot(timestamp_ms),
    );

    let patch = serde_json::json!({
        "mcp": {
            "config": config,
            "servers": serde_json::Value::Object(servers),
        }
    });

    dispatch_mcp_patch(machine_state, patch).await
}

/// Synchronise MCP schema updates with the worker.
pub async fn sync_mcp_snapshot(
    machine_state: MachineStateManager,
    server_id: String,
    tools: serde_json::Value,
    resources: Option<serde_json::Value>,
) -> Result<(), String> {
    let timestamp_ms = current_timestamp();

    let mut servers = serde_json::Map::new();
    servers.insert(
        server_id,
        serde_json::json!({
            "tools": tools,
            "resources": resources.unwrap_or(serde_json::Value::Null),
            "updated_at_ms": timestamp_ms,
        }),
    );

    let patch = serde_json::json!({
        "mcp": {
            "servers": serde_json::Value::Object(servers),
        }
    });

    dispatch_mcp_patch(machine_state, patch).await
}

pub(crate) fn current_timestamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

async fn dispatch_mcp_patch(
    machine_state: MachineStateManager,
    patch: serde_json::Value,
) -> Result<(), String> {
    let sync = MachineStateSync {
        machine_id: machine_state.machine_id().to_string(),
        installed_apps: None,
        enabled_apps: None,
        disabled_apps: None,
        conversation_turn: None,
        config_patch: Some(patch),
    };

    machine_state
        .sync_state(sync)
        .await
        .map_err(|err| err.to_string())
}

fn internal_chunk_server_snapshot(timestamp_ms: u128) -> serde_json::Value {
    json!({
        "tools": [{
            "name": INTERNAL_CHUNK_FETCH_TOOL,
            "description": "Fetch a paginated chunk from a truncated MCP tool response.",
            "input_schema": {
                "type": "object",
                "required": ["chunk_id"],
                "properties": {
                    "chunk_id": {
                        "type": "string",
                        "description": "Opaque identifier returned alongside chunked responses."
                    },
                    "page": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Zero-based page index (omit for the first page)."
                    }
                }
            }
        }, {
            "name": INTERNAL_TEXT_FIND_TOOL,
            "description": "Search within a cached chunk for a substring and return contextual previews.",
            "input_schema": {
                "type": "object",
                "required": ["chunk_id", "query"],
                "properties": {
                    "chunk_id": {
                        "type": "string",
                        "description": "Chunk identifier previously returned alongside chunked responses."
                    },
                    "query": {
                        "type": "string",
                        "description": "Case-insensitive text to locate within the cached chunk contents."
                    },
                    "case_sensitive": {
                        "type": "boolean",
                        "description": "Set true to perform a case-sensitive search.",
                        "default": false
                    },
                    "max_matches": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 25,
                        "description": "Number of matches to include in the response (default 5)."
                    }
                }
            }
        }],
        "resources": serde_json::Value::Null,
        "updated_at_ms": timestamp_ms,
    })
}
