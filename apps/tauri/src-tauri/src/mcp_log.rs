use anyhow::Context;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio::{
    fs::OpenOptions,
    io::AsyncWriteExt,
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};
use tracing::warn;

static MCP_LOG_SENDER: OnceLock<UnboundedSender<McpLogEntry>> = OnceLock::new();
static RAW_PAYLOAD_LOGGING: AtomicBool = AtomicBool::new(false);

pub fn set_raw_payload_logging(enabled: bool) {
    RAW_PAYLOAD_LOGGING.store(enabled, Ordering::Relaxed);
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpDirection {
    Request,
    Response,
}

#[derive(Serialize)]
struct McpLogEntry {
    timestamp_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    step_index: Option<u32>,
    server_id: String,
    tool_name: String,
    direction: McpDirection,
    payload: Value,
}

pub fn init(app_handle: &AppHandle) {
    if MCP_LOG_SENDER.get().is_some() {
        return;
    }

    let log_dir = crate::config::prelude::logs_dir_from_handle(app_handle);
    let path = log_dir.join("mcp-tools.jsonl");
    let (tx, rx) = unbounded_channel();

    if MCP_LOG_SENDER.set(tx).is_err() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        if let Err(err) = writer_loop(rx, path).await {
            warn!("mcp trace writer exited: {err:?}");
        }
    });
}

pub fn log_tool_event(
    session_token: Option<&str>,
    step_index: Option<u32>,
    server_id: &str,
    tool_name: &str,
    direction: McpDirection,
    payload: Value,
) {
    if let Some(sender) = MCP_LOG_SENDER.get() {
        let entry = McpLogEntry {
            timestamp_ms: now_ms(),
            session_token: sanitize_session_token(session_token),
            step_index,
            server_id: server_id.to_string(),
            tool_name: tool_name.to_string(),
            direction,
            payload: summarize_payload(payload),
        };

        if sender.send(entry).is_err() {
            warn!("mcp trace channel closed; dropping entry");
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

async fn writer_loop(mut rx: UnboundedReceiver<McpLogEntry>, path: PathBuf) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }

    let mut file = open_append(&path).await?;

    while let Some(entry) = rx.recv().await {
        let line = match serde_json::to_string(&entry) {
            Ok(value) => value,
            Err(err) => {
                warn!("failed to serialize mcp trace entry: {err:?}");
                continue;
            }
        };

        if let Err(err) = file.write_all(line.as_bytes()).await {
            warn!("failed to write mcp trace entry: {err:?}; attempting reopen");
            file = open_append(&path).await?;
            file.write_all(line.as_bytes()).await.ok();
        }
        if let Err(err) = file.write_all(b"\n").await {
            warn!("failed to write newline to mcp trace log: {err:?}; attempting reopen");
            file = open_append(&path).await?;
            file.write_all(b"\n").await.ok();
        }
    }

    Ok(())
}

fn sanitize_session_token(token: Option<&str>) -> Option<String> {
    token.map(|value| value.to_string())
}

fn summarize_payload(payload: Value) -> Value {
    if RAW_PAYLOAD_LOGGING.load(Ordering::Relaxed) {
        return payload;
    }

    match payload {
        Value::Object(map) => {
            let keys = map.keys().cloned().collect::<Vec<_>>();
            let mut summary = json!({
                "redacted": true,
                "type": "object",
                "keys": keys,
            });

            let bool_fields: HashMap<_, _> = map
                .iter()
                .filter_map(|(k, v)| v.as_bool().map(|b| (k.clone(), Value::Bool(b))))
                .collect();
            if !bool_fields.is_empty() {
                summary["bools"] = Value::Object(bool_fields.into_iter().collect());
            }

            if let Some(latency) = map.get("latency_ms").and_then(|v| v.as_u64()) {
                summary["latency_ms"] = json!(latency);
            }
            if let Some(status) = map.get("status").and_then(|v| v.as_str()) {
                summary["status"] = json!(status);
            }

            if let Some(err) = map.get("error").and_then(|v| v.as_str()) {
                let (truncated, was_truncated) = truncate(err, 256);
                summary["error"] = Value::String(truncated);
                if was_truncated {
                    summary["error_truncated"] = Value::Bool(true);
                }
            }

            summary
        }
        Value::Array(items) => json!({
            "redacted": true,
            "type": "array",
            "len": items.len()
        }),
        Value::String(value) => json!({
            "redacted": true,
            "type": "string",
            "len": value.chars().count()
        }),
        Value::Number(_) => json!({
            "redacted": true,
            "type": "number"
        }),
        Value::Bool(_) => json!({
            "redacted": true,
            "type": "bool"
        }),
        Value::Null => Value::Null,
    }
}

fn truncate(value: &str, max_len: usize) -> (String, bool) {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_len).collect();
    let was_truncated = chars.next().is_some();
    (truncated, was_truncated)
}

async fn open_append(path: &PathBuf) -> anyhow::Result<tokio::fs::File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .with_context(|| format!("failed to open mcp trace log at {}", path.display()))
}
