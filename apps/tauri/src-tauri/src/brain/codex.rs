//! Codex app-server brain implementation.
//!
//! Spawns the vendored `codex app-server --listen stdio://` process and speaks
//! its JSON-RPC-ish protocol over stdin/stdout (see
//! `crates/codex-main/codex-main/codex-rs/app-server-protocol/src/rpc.rs` —
//! messages carry `id`/`method`/`params` but no `"jsonrpc": "2.0"` field).
//!
//! The desktop app registers its MCP tools (from the `mcp-host` registry) plus
//! the first-party `open_app` intent as dynamic tools; when Codex decides to
//! call one, the `item/tool/call` request is routed back to the local
//! executor (Router / IntentState), keeping the UI progress events identical.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::brain::CodexBrainSettings;
use crate::config::machine_state::MachineStateManager;
use crate::intents::{IntentRequest, IntentState};
use crate::machine::chunking::{
    ChunkStore, INTERNAL_CHUNK_FETCH_TOOL, INTERNAL_CHUNK_SERVER_ID, INTERNAL_TEXT_FIND_TOOL,
};
use crate::machine::mcp::{McpProgressEvent, McpProgressStatus};
use crate::machine::ChatWorkerOutcome;
use crate::machine::EVENT_WORKER_AUTH_OK;
use crate::ui::state_router::{self, StateFlow, StateSignal};
use crate::ui::UiVisualState;
use crate::{AssistantCommandResponse, SpeechPlan};
use mcp_host::host::ToolCallContext;

/// Timeout for control-plane calls (initialize / thread/start / turn/start).
const CONTROL_TIMEOUT: Duration = Duration::from_secs(60);
/// Maximum duration of a single agent turn before giving up.
const TURN_TIMEOUT: Duration = Duration::from_secs(600);
/// Idle timeout for the reader loop.
const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(3600);

/// Executor for dynamic tool calls routed back from Codex.
type DynToolExecutor = Arc<
    dyn Fn(
            Option<String>,
            String,
            Value,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<String, String>> + Send>,
        > + Send
        + Sync,
>;

/// Messages routed by the reader task.
enum SessionMsg {
    /// Notification (no id): deltas, turn lifecycle, errors...
    Notification { method: String, params: Value },
}

struct CodexSession {
    stdin: Arc<Mutex<ChildStdin>>,
    next_id: AtomicU64,
    pending: Arc<StdMutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>,
    /// Per-thread notification listeners (thread_id → sender).
    listeners: Arc<StdMutex<HashMap<String, Vec<mpsc::UnboundedSender<SessionMsg>>>>>,
    /// Swappable dynamic-tool executor (refreshed per flow with the current
    /// session token and UI handles).
    executor: Arc<StdMutex<Option<DynToolExecutor>>>,
    /// Serialise turns: one active agent turn per thread at a time.
    turn_lock: Arc<Mutex<()>>,
    thread_id: StdMutex<Option<String>>,
    /// Keep the child process alive; killed on drop.
    _child: StdMutex<Option<Child>>,
}

impl Drop for CodexSession {
    fn drop(&mut self) {
        if let Some(mut child) = self._child.lock().expect("codex child lock").take() {
            let _ = child.start_kill();
        }
    }
}

/// Shared session handle kept alive across flows.
static SESSION: std::sync::OnceLock<Arc<CodexSession>> = std::sync::OnceLock::new();

/// Resolve the codex binary path: explicit setting, then conventional spots.
fn resolve_binary_path(settings: &CodexBrainSettings) -> Result<PathBuf, String> {
    if !settings.binary_path.trim().is_empty() {
        let path = PathBuf::from(settings.binary_path.trim());
        if path.exists() {
            return Ok(path);
        }
        return Err(format!("codex binary not found at {}", path.display()));
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("resources/codex/codex"));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("resources/codex/codex"));
            }
        }
    }
    candidates.push(PathBuf::from("resources/codex/codex"));
    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| "codex binary not found (expected resources/codex/codex)".to_string())
}

/// Spawn the app-server process and start the reader task.
async fn spawn_session(settings: &CodexBrainSettings) -> Result<Arc<CodexSession>, String> {
    let binary = resolve_binary_path(settings)?;
    info!("Spawning Codex app-server: {}", binary.display());

    let mut child = tokio::process::Command::new(&binary)
        .args(["app-server", "--listen", "stdio://"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|err| format!("failed to spawn codex app-server: {err}"))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "codex app-server has no stdin".to_string())?;
    let stdout: ChildStdout = child
        .stdout
        .take()
        .ok_or_else(|| "codex app-server has no stdout".to_string())?;

    let session = Arc::new(CodexSession {
        stdin: Arc::new(Mutex::new(stdin)),
        next_id: AtomicU64::new(1),
        pending: Arc::new(StdMutex::new(HashMap::new())),
        listeners: Arc::new(StdMutex::new(HashMap::new())),
        executor: Arc::new(StdMutex::new(None)),
        turn_lock: Arc::new(Mutex::new(())),
        thread_id: StdMutex::new(None),
        _child: StdMutex::new(Some(child)),
    });

    // Reader task: classify every protocol line and route it.
    let reader_session = Arc::clone(&session);
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match timeout(READ_IDLE_TIMEOUT, reader.read_line(&mut line)).await {
                Ok(Ok(0)) | Err(_) => {
                    info!("Codex app-server stream closed");
                    break;
                }
                Ok(Ok(_)) => {}
                Ok(Err(err)) => {
                    warn!("Codex app-server read error: {err}");
                    break;
                }
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
                warn!("Codex app-server sent a non-JSON line, ignoring");
                continue;
            };
            route_message(&reader_session, msg).await;
        }
    });

    Ok(session)
}

/// Classify a decoded protocol line and dispatch it.
///
/// Wire shapes (rpc.rs): responses `{id, result}` / `{id, error}`,
/// server requests `{id, method, params}`, notifications `{method, params}`.
async fn route_message(session: &Arc<CodexSession>, msg: Value) {
    if msg.get("result").is_some() || msg.get("error").is_some() {
        if let Some(id) = msg.get("id").and_then(Value::as_u64) {
            let result = if let Some(result) = msg.get("result") {
                Ok(result.clone())
            } else {
                let message = msg
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown app-server error")
                    .to_string();
                Err(message)
            };
            let sender = session
                .pending
                .lock()
                .expect("codex pending lock")
                .remove(&id);
            if let Some(sender) = sender {
                let _ = sender.send(result);
            }
            return;
        }
    }

    if let (Some(id), Some(method)) = (
        msg.get("id").cloned(),
        msg.get("method").and_then(Value::as_str),
    ) {
        // Server-initiated request: dynamic tool calls are answered off-thread.
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let session = Arc::clone(session);
        let method = method.to_string();
        tokio::spawn(async move {
            handle_server_request(&session, id, &method, params).await;
        });
        return;
    }

    if let Some(method) = msg.get("method").and_then(Value::as_str) {
        let notification = SessionMsg::Notification {
            method: method.to_string(),
            params: msg.get("params").cloned().unwrap_or(Value::Null),
        };
        if let Some(thread_id) = notification_thread(&notification) {
            let mut listeners = session.listeners.lock().expect("codex listeners lock");
            if let Some(senders) = listeners.get_mut(thread_id) {
                senders.retain(|tx| {
                    tx.send(clone_notification(&notification)).is_ok()
                });
            }
        }
    }
}

/// Extract the thread id a notification belongs to (when known).
fn notification_thread(msg: &SessionMsg) -> Option<&str> {
    let SessionMsg::Notification { params, .. } = msg;
    params
        .get("threadId")
        .and_then(Value::as_str)
        .or_else(|| params.pointer("/turn/threadId").and_then(Value::as_str))
}

fn clone_notification(msg: &SessionMsg) -> SessionMsg {
    let SessionMsg::Notification { method, params } = msg;
    SessionMsg::Notification {
        method: method.clone(),
        params: params.clone(),
    }
}

/// Answer a server-initiated request (dynamic tool execution bridge).
async fn handle_server_request(
    session: &Arc<CodexSession>,
    id: Value,
    method: &str,
    params: Value,
) {
    if method != "item/tool/call" {
        warn!("Unhandled Codex server request '{method}', replying with an error");
        let _ = write_message(
            session,
            &json!({
                "id": id,
                "error": { "code": -32601, "message": format!("unsupported request {method}") }
            }),
        )
        .await;
        return;
    }

    let namespace = params
        .get("namespace")
        .and_then(Value::as_str)
        .map(str::to_string);
    let tool = params
        .get("tool")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    let executor = session
        .executor
        .lock()
        .expect("codex executor lock")
        .clone();
    let outcome = match executor {
        Some(executor) => executor(namespace.clone(), tool.clone(), arguments).await,
        None => Err("no dynamic tool executor installed yet".to_string()),
    };

    let result = match outcome {
        Ok(text) => json!({
            "contentItems": [{ "type": "inputText", "text": text }],
            "success": true
        }),
        Err(err) => json!({
            "contentItems": [{ "type": "inputText", "text": err }],
            "success": false
        }),
    };
    if let Err(err) = write_message(session, &json!({ "id": id, "result": result })).await {
        error!("Failed to answer dynamic tool call: {err}");
    }
}

/// Send a JSON line to the app-server stdin.
async fn write_message(session: &CodexSession, message: &Value) -> Result<(), String> {
    let mut stdin = session.stdin.lock().await;
    let bytes = format!("{message}\n").into_bytes();
    stdin
        .write_all(&bytes)
        .await
        .map_err(|err| format!("failed to write to codex app-server: {err}"))?;
    stdin
        .flush()
        .await
        .map_err(|err| format!("failed to flush codex app-server stdin: {err}"))
}

/// Send a request and await its response.
async fn request(
    session: &CodexSession,
    method: &str,
    params: Value,
    wait: Duration,
) -> Result<Value, String> {
    let id = session.next_id.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = oneshot::channel();
    session
        .pending
        .lock()
        .expect("codex pending lock")
        .insert(id, tx);
    write_message(session, &json!({ "id": id, "method": method, "params": params })).await?;
    match timeout(wait, rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(format!("codex request '{method}' dropped (session closed)")),
        Err(_) => {
            session
                .pending
                .lock()
                .expect("codex pending lock")
                .remove(&id);
            Err(format!("codex request '{method}' timed out after {wait:?}"))
        }
    }
}

/// Build the dynamic tool specs exposed to Codex: one namespace per MCP
/// server (from the mcp-host registry) plus the first-party `open_app` tool.
async fn build_dynamic_tools(
    host: &mcp_host::HostHandle,
    machine_id: &str,
) -> Result<Vec<Value>, String> {
    let mut specs = Vec::new();
    for entry in host.registry().snapshot().await {
        if entry.tools.is_empty() {
            continue;
        }
        let tools: Vec<Value> = entry
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.name,
                    "description": tool.description.clone().unwrap_or_default(),
                    "inputSchema": tool.input_schema.clone(),
                })
            })
            .collect();
        specs.push(json!({
            "type": "namespace",
            "name": entry.server_id,
            "description": format!(
                "MCP server '{}' tools (machine {machine_id})",
                entry.info.name
            ),
            "tools": tools
        }));
    }
    // First-party in-process tools: long-result chunking (sprite_internal).
    specs.push(json!({
        "type": "namespace",
        "name": INTERNAL_CHUNK_SERVER_ID,
        "description": "Outils internes pour les réponses d'outils tronquées : pagination et recherche dans un chunk.",
        "tools": [
            {
                "type": "function",
                "name": INTERNAL_CHUNK_FETCH_TOOL,
                "description": "Fetch a paginated chunk from a truncated MCP tool response.",
                "inputSchema": {
                    "type": "object",
                    "required": ["chunk_id"],
                    "properties": {
                        "chunk_id": { "type": "string", "description": "Opaque identifier returned alongside chunked responses." },
                        "page": { "type": "integer", "minimum": 0, "description": "Zero-based page index (omit for the first page)." }
                    }
                }
            },
            {
                "type": "function",
                "name": INTERNAL_TEXT_FIND_TOOL,
                "description": "Search within a cached chunk for a substring and return contextual previews.",
                "inputSchema": {
                    "type": "object",
                    "required": ["chunk_id", "query"],
                    "properties": {
                        "chunk_id": { "type": "string", "description": "Chunk identifier previously returned alongside chunked responses." },
                        "query": { "type": "string", "description": "Case-insensitive text to locate within the cached chunk contents." },
                        "case_sensitive": { "type": "boolean", "description": "Set true to perform a case-sensitive search.", "default": false },
                        "max_matches": { "type": "integer", "minimum": 1, "maximum": 25, "description": "Maximum number of matches to report." }
                    }
                }
            }
        ]
    }));
    specs.push(json!({
        "type": "function",
        "name": "open_app",
        "description": "Lancer une application installée sur la machine de l'utilisateur. Utilise cet outil quand l'utilisateur demande d'ouvrir ou de lancer une application.",
        "inputSchema": json!({
            "type": "object",
            "properties": {
                "app_name": { "type": "string", "description": "Nom de l'application à lancer" }
            },
            "required": ["app_name"]
        })
    }));
    Ok(specs)
}

/// Thread cwd resolution: explicit setting, else the user home directory.
fn resolve_cwd(settings: &CodexBrainSettings) -> String {
    if !settings.cwd.trim().is_empty() {
        return settings.cwd.trim().to_string();
    }
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string())
}

/// Ensure the shared session exists, is initialised and has a live thread.
async fn ensure_session(
    settings: &CodexBrainSettings,
    host: &mcp_host::HostHandle,
    machine_id: &str,
) -> Result<(Arc<CodexSession>, String), String> {
    let session = match SESSION.get() {
        Some(existing) => Arc::clone(existing),
        None => {
            let spawned = spawn_session(settings).await?;
            let _ = SESSION.set(Arc::clone(&spawned));
            spawned
        }
    };

    // Handshake (only meaningful once, but cheap and idempotent).
    request(
        &session,
        "initialize",
        json!({
            "clientInfo": {
                "name": "sprite-assistant",
                "title": "Sprite Assistant",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
        CONTROL_TIMEOUT,
    )
    .await?;

    let existing = session
        .thread_id
        .lock()
        .expect("codex thread lock")
        .clone();
    if let Some(thread_id) = existing {
        return Ok((session, thread_id));
    }

    let dynamic_tools = build_dynamic_tools(host, machine_id).await?;
    let mut params = json!({
        "cwd": resolve_cwd(settings),
        "approvalPolicy": settings.approval_policy,
        "sandbox": settings.sandbox,
        "dynamicTools": dynamic_tools,
    });
    if let Some(model) = settings.model.as_deref() {
        params["model"] = json!(model);
    }
    let response = request(&session, "thread/start", params, CONTROL_TIMEOUT).await?;
    let thread_id = response
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if thread_id.is_empty() {
        return Err("codex thread/start returned no thread id".to_string());
    }
    *session.thread_id.lock().expect("codex thread lock") = Some(thread_id.clone());
    info!("Codex thread started: {thread_id}");
    Ok((session, thread_id))
}

/// Execute a single user turn and collect the agent reply.
async fn run_turn(
    session: &CodexSession,
    thread_id: &str,
    user_query: &str,
) -> Result<String, String> {
    // One turn at a time per thread (Codex threads are single-conversation).
    let _guard = session.turn_lock.lock().await;

    // Register the listener BEFORE turn/start so no notification is missed.
    let (tx, mut rx) = mpsc::unbounded_channel::<SessionMsg>();
    session
        .listeners
        .lock()
        .expect("codex listeners lock")
        .entry(thread_id.to_string())
        .or_default()
        .push(tx.clone());

    let started = request(
        session,
        "turn/start",
        json!({
            "threadId": thread_id,
            "input": [{ "type": "text", "text": user_query }]
        }),
        CONTROL_TIMEOUT,
    )
    .await;
    let turn_id = match &started {
        Ok(value) => value
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Err(err) => {
            remove_listener(session, thread_id, &tx);
            return Err(err.clone());
        }
    };

    let mut text = String::new();
    let mut fallback_text = String::new();
    let result = loop {
        let msg = match timeout(TURN_TIMEOUT, rx.recv()).await {
            Ok(Some(msg)) => msg,
            Ok(None) => break Err("codex session closed during turn".to_string()),
            Err(_) => break Err(format!("codex turn timed out after {TURN_TIMEOUT:?}")),
        };
        let SessionMsg::Notification { method, params } = msg;
        match method.as_str() {
            "item/agentMessage/delta" => {
                if params.get("turnId").and_then(Value::as_str) == Some(turn_id.as_str()) {
                    if let Some(delta) = params.get("delta").and_then(Value::as_str) {
                        text.push_str(delta);
                    }
                }
            }
            "item/completed" => {
                // Fallback in case deltas were missed: keep the agent message
                // item text for this turn.
                if params.get("turnId").and_then(Value::as_str) == Some(turn_id.as_str()) {
                    if let Some(item_text) = params
                        .pointer("/item/text")
                        .and_then(Value::as_str)
                        .or_else(|| params.pointer("/item/content").and_then(Value::as_str))
                    {
                        fallback_text = item_text.to_string();
                    }
                }
            }
            "turn/completed" => {
                let completed = params
                    .pointer("/turn/id")
                    .and_then(Value::as_str)
                    .or_else(|| params.get("turn").and_then(Value::as_str));
                if completed.unwrap_or(turn_id.as_str()) == turn_id {
                    break Ok(());
                }
            }
            "error" => {
                let message = params
                    .get("message")
                    .and_then(Value::as_str)
                    .or_else(|| params.pointer("/error/message").and_then(Value::as_str))
                    .unwrap_or("codex turn error")
                    .to_string();
                break Err(message);
            }
            other => {
                debug!("Codex notification ignored: {other}");
            }
        }
    };

    remove_listener(session, thread_id, &tx);
    result?;

    let reply = if text.trim().is_empty() {
        fallback_text
    } else {
        text
    };
    if reply.trim().is_empty() {
        return Err("codex turn completed with an empty reply".to_string());
    }
    Ok(reply)
}

/// Remove this flow's listener for the given thread.
fn remove_listener(
    session: &CodexSession,
    thread_id: &str,
    tx: &mpsc::UnboundedSender<SessionMsg>,
) {
    let mut listeners = session.listeners.lock().expect("codex listeners lock");
    if let Some(senders) = listeners.get_mut(thread_id) {
        senders.retain(|sender| !sender.same_channel(tx));
        if senders.is_empty() {
            listeners.remove(thread_id);
        }
    }
}

/// Emit the same progress events the planner flow emits so the v2 MCP rail
/// keeps working when tools run through the Codex brain.
fn emit_progress(
    app_handle: &AppHandle,
    session_token: &str,
    step_index: u32,
    server_id: &str,
    tool_name: &str,
    status: McpProgressStatus,
    error: Option<String>,
) {
    let event = McpProgressEvent {
        session_token: Some(session_token.to_string()),
        step_index,
        server_id: server_id.to_string(),
        tool_name: tool_name.to_string(),
        status,
        status_note: None,
        error,
        latency_ms: None,
    };
    if let Err(err) = app_handle.emit("assistant://mcp-progress", event) {
        warn!("Failed to emit MCP progress event: {err}");
    }
}

/// Build the dynamic-tool executor that bridges Codex tool calls back to the
/// local host (MCP Router + open_app intent).
fn build_executor(
    app_handle: &AppHandle,
    host_handle: &mcp_host::HostHandle,
    state: &IntentState,
    session_token: &str,
    chunk_store: Arc<StdMutex<ChunkStore>>,
) -> DynToolExecutor {
    let host = host_handle.clone();
    let intents = state.clone();
    let app = app_handle.clone();
    let session_token = session_token.to_string();
    let step_counter = Arc::new(AtomicU64::new(0));
    Arc::new(move |namespace, tool, arguments| {
        let host = host.clone();
        let intents = intents.clone();
        let app = app.clone();
        let session_token = session_token.clone();
        let step_counter = Arc::clone(&step_counter);
        let chunk_store = Arc::clone(&chunk_store);
        Box::pin(async move {
            let step_index = step_counter.fetch_add(1, Ordering::SeqCst) as u32;

            // First-party intent: launch an installed application.
            if namespace.is_none() && tool == "open_app" {
                let app_name = arguments
                    .get("app_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .trim_end_matches('.')
                    .trim()
                    .to_string();
                info!("Codex brain requests open_app: \"{app_name}\"");
                if app_name.is_empty() {
                    return Err("nom d'application vide".to_string());
                }
                let app_label = app_name.clone();
                let result = tokio::task::spawn_blocking(move || {
                    intents.process_intents(vec![IntentRequest::LaunchApplication {
                        query: app_name,
                        strict: false,
                        refresh_catalog: false,
                        focus: false,
                    }])
                })
                .await
                .map_err(|err| format!("open_app task failed: {err}"))?;
                return match result.results.into_iter().next() {
                    Some(entry) if entry.response.is_some() => {
                        Ok(format!("Application « {app_label} » lancée."))
                    }
                    Some(entry) => Err(entry
                        .error
                        .unwrap_or_else(|| format!("Impossible de lancer « {app_label} »"))),
                    None => Err(format!("Impossible de lancer « {app_label} » : aucune réponse")),
                };
            }

            // First-party in-process tools: long-result chunking. These run
            // against the flow-local ChunkStore, exactly like the planner
            // engine used to (fetch_chunk / find_text).
            if namespace.as_deref() == Some(INTERNAL_CHUNK_SERVER_ID) {
                let mut store = chunk_store.lock().expect("chunk store lock");
                let outcome = match tool.as_str() {
                    INTERNAL_CHUNK_FETCH_TOOL => store.fetch_chunk(arguments),
                    INTERNAL_TEXT_FIND_TOOL => store.find_text(arguments),
                    other => Err(format!("Unsupported sprite_internal tool '{other}'")),
                };
                return match outcome {
                    Ok(value) => Ok(serde_json::to_string(&value)
                        .unwrap_or_else(|_| value.to_string())),
                    Err(err) => Err(err),
                };
            }

            // MCP namespace tool: execute through the mcp-host router.
            let Some(server_id) = namespace else {
                return Err(format!("outil inconnu sans namespace : {tool}"));
            };
            emit_progress(
                &app,
                &session_token,
                step_index,
                &server_id,
                &tool,
                McpProgressStatus::Running,
                None,
            );
            let started = std::time::Instant::now();
            let context = ToolCallContext {
                trace_id: None,
                session_token: Some(session_token.clone()),
                step_index: Some(step_index),
            };
            match host
                .call_tool_with_context(&server_id, &tool, arguments, context)
                .await
            {
                Ok(value) => {
                    emit_progress(
                        &app,
                        &session_token,
                        step_index,
                        &server_id,
                        &tool,
                        McpProgressStatus::Completed,
                        None,
                    );
                    // Chunk oversized fields exactly like the planner engine
                    // used to, so the agent can page through via sprite_internal.
                    let effective = {
                        let mut store = chunk_store.lock().expect("chunk store lock");
                        store
                            .chunk_large_fields(&session_token, step_index, &value)
                            .unwrap_or_else(|| value.clone())
                    };
                    let text = if effective.is_string() {
                        effective.as_str().unwrap_or_default().to_string()
                    } else {
                        serde_json::to_string_pretty(&effective)
                            .unwrap_or_else(|_| effective.to_string())
                    };
                    debug!(
                        "MCP tool {server_id}/{tool} completed in {:?}",
                        started.elapsed()
                    );
                    Ok(text)
                }
                Err(err) => {
                    emit_progress(
                        &app,
                        &session_token,
                        step_index,
                        &server_id,
                        &tool,
                        McpProgressStatus::Crashed,
                        Some(err.to_string()),
                    );
                    Err(format!("outil MCP {server_id}/{tool} a échoué : {err}"))
                }
            }
        })
    })
}

/// Run one user request through the Codex brain.
#[allow(clippy::too_many_arguments)]
pub async fn run_codex_flow(
    app_handle: &AppHandle,
    settings: &CodexBrainSettings,
    state: &IntentState,
    machine_state: &MachineStateManager,
    host_handle: &mcp_host::HostHandle,
    user_query: String,
) -> Result<ChatWorkerOutcome, String> {
    state_router::emit(
        app_handle,
        StateSignal::new(UiVisualState::Thinking, StateFlow::Worker),
    );

    let machine_id = machine_state.machine_id().to_string();
    let session_token = Uuid::new_v4().to_string();

    let (session, thread_id) = match ensure_session(settings, host_handle, &machine_id).await {
        Ok(pair) => pair,
        Err(err) => {
            state_router::emit(
                app_handle,
                StateSignal::new(UiVisualState::Error, StateFlow::Worker),
            );
            return Err(err);
        }
    };

    // Refresh the executor for this flow (fresh session token + UI handles).
    // The chunk store is flow-scoped, like the planner engine's store was.
    let chunk_store: Arc<StdMutex<ChunkStore>> = Arc::new(StdMutex::new(ChunkStore::default()));
    *session
        .executor
        .lock()
        .expect("codex executor lock") = Some(build_executor(
        app_handle,
        host_handle,
        state,
        &session_token,
        chunk_store,
    ));

    // Keep the auth-state event contract alive for the UI.
    let _ = app_handle.emit(
        EVENT_WORKER_AUTH_OK,
        serde_json::json!({ "endpoint": "codex://app-server" }),
    );

    let reply = match run_turn(&session, &thread_id, &user_query).await {
        Ok(reply) => reply,
        Err(err) => {
            state_router::emit(
                app_handle,
                StateSignal::new(UiVisualState::Error, StateFlow::Worker),
            );
            return Err(format!("codex brain turn failed: {err}"));
        }
    };

    state_router::emit(
        app_handle,
        StateSignal::new(UiVisualState::Idle, StateFlow::Worker),
    );
    info!("Codex brain reply ready ({} chars)", reply.len());

    // Identical response contract to the worker flow: intents execute inline
    // through dynamic tools, so nothing is deferred to a planner pass.
    Ok(ChatWorkerOutcome {
        response: AssistantCommandResponse {
            reply: reply.clone(),
            speech: SpeechPlan::with_initial(reply),
            intents: Vec::new(),
            reply_streamed: false,
            mcp_plan: None,
            mcp_action_results: Vec::new(),
            mcp_session_token: Some(session_token.clone()),
            dispatch_result: None,
        },
        planner_intent: None,
        planner_context: None,
        session_token,
        user_query,
    })
}
