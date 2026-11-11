use crate::config::prelude::McpSummary;
use crate::config::{self, AppConfig};
use mcp_host::{HostHandle, RegistryEntry, TransportConfig};
use serde::Serialize;
use std::path::Path;
use std::process::Command;
use tauri::{AppHandle, State};
use tracing::{info, warn};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[derive(Debug, Serialize)]
pub struct McpRegistryTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct McpRegistryServer {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    pub transport: String,
    pub enabled: bool,
    pub tool_count: usize,
    pub resource_count: usize,
    pub updated_at_ms: u128,
    #[serde(default)]
    pub tools: Vec<McpRegistryTool>,
}

impl From<RegistryEntry> for McpRegistryServer {
    fn from(entry: RegistryEntry) -> Self {
        let transport = match &entry.config.transport {
            TransportConfig::Stdio { .. } => "stdio",
            TransportConfig::Ws { .. } => "ws",
            TransportConfig::Http { .. } => "http",
        }
        .to_string();

        Self {
            id: entry.server_id,
            display_name: entry.config.display_name.clone(),
            server_name: Some(entry.info.name.clone()).filter(|value| !value.is_empty()),
            server_version: entry.info.version.clone(),
            transport,
            enabled: entry.config.enabled,
            tool_count: entry.tools.len(),
            resource_count: entry.resources.len(),
            updated_at_ms: entry.updated_at_ms,
            tools: entry
                .tools
                .into_iter()
                .map(|tool| McpRegistryTool {
                    name: tool.name,
                    description: tool.description,
                })
                .collect(),
        }
    }
}

#[tauri::command]
pub async fn get_mcp_registry(
    host: State<'_, HostHandle>,
) -> Result<Vec<McpRegistryServer>, String> {
    let registry = host.registry();
    let entries = registry.snapshot().await;
    Ok(entries.into_iter().map(McpRegistryServer::from).collect())
}

#[tauri::command]
pub async fn toggle_mcp_server(
    app_handle: AppHandle,
    app_config: State<'_, AppConfig>,
    host: State<'_, HostHandle>,
    server_id: String,
    enabled: bool,
) -> Result<McpSummary, String> {
    let server_id = server_id.trim().to_string();
    if server_id.is_empty() {
        return Err("Server id is required".into());
    }

    info!("Toggling MCP server id={} enabled={}", server_id, enabled);
    let mcp_state = app_config.mcp();
    let (snapshot, changed) = {
        let mut guard = mcp_state
            .write()
            .map_err(|_| "Failed to lock MCP configuration for update".to_string())?;
        let server = guard
            .servers
            .get_mut(&server_id)
            .ok_or_else(|| format!("MCP server `{server_id}` not found."))?;
        let changed = server.enabled != enabled;
        server.enabled = enabled;
        (guard.clone(), changed)
    };

    if changed {
        app_config
            .persist_mcp_config(&app_handle)
            .map_err(|err| err.to_string())?;
        host.restart(snapshot.clone())
            .await
            .map_err(|err| err.to_string())?;
        info!("MCP config persisted and host restarted");
    } else {
        warn!("MCP server toggle had no effect; state unchanged");
    }

    Ok(McpSummary::from(&snapshot))
}

#[tauri::command]
pub fn open_mcp_config_file(
    app_handle: AppHandle,
    app_config: State<'_, AppConfig>,
) -> Result<(), String> {
    // Persist the in-memory snapshot before opening the file so editors see the latest view.
    info!("Persisting MCP config before opening file");
    app_config
        .persist_mcp_config(&app_handle)
        .map_err(|err| err.to_string())?;

    let path = config::paths::config_path("mcp.config.json");

    let result = open_with_default_app(&path);
    if result.is_ok() {
        info!("Opened MCP config file: {}", path.display());
    }
    result
}

fn open_with_default_app(path: &Path) -> Result<(), String> {
    let target = path
        .to_str()
        .ok_or_else(|| "Path contains invalid UTF-8".to_string())?;

    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        Command::new("cmd")
            .args(["/C", "start", "", target])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|err| err.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(target)
            .spawn()
            .map_err(|err| err.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map_err(|err| err.to_string())?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("Opening files is not supported on this platform.".to_string())
}
