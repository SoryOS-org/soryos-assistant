# MCP Integration Layer (Rust) - High-Level Plan

## Purpose

Add a Rust-native MCP host layer so the app can load or launch MCP servers from a central JSON config (Cursor/Windsurf style), fetch schemas, expose tools to the LLM pipeline, route calls, and shut everything down cleanly.

## Transport & Security

* **Primary transport:** `stdio` to local child processes (fast, minimal surface).
* **Optional:** `ws` or `http` for servers that provide it (behind allowlists).
* No user LLM keys flow into MCPs unless a tool explicitly needs them. Keep BYOK in the app process.
* All MCP servers are spawned as background child processes without showing terminal windows (`CREATE_NO_WINDOW` on Windows, piped stdio elsewhere).
* Config surface includes client timeouts + heartbeat intervals so WS/HTTP transports don’t hang indefinitely; implement a `Transport` trait with `send/recv` so additional protocols slot in later.

## Crate Structure (repackage-ready)

```
crates/mcp-host/
  src/
    config.rs        # host-facing config structs (data only; fed by AppConfig)
    launcher.rs      # spawn/monitor child processes; stdio pipes
    client.rs        # JSON-RPC 2.0 over stdio/ws/http; handshake; list tools/resources
    registry.rs      # in-memory map: server -> tools/schema
    router.rs        # call -> target server/tool; timeout/retry
    events.rs        # tokio broadcast channel for host/server events
    killswitch.rs    # graceful shutdown of all children
  lib.rs
apps/desktop/        # main Sprite app; depends on mcp-host (clean boundary for future crate split)
```

* Clean API so this folder can be turned into a separate crate later without touching app code.

## Runtime Model

* **Startup**
  1. App supplies an in-memory `HostConfig` snapshot sourced via the existing `AppConfig` loader (no file I/O inside the crate).
  2. Host stores the snapshot inside an `Arc<ConfigState>` so launcher/router/registry share the same reference without deep cloning.
  3. `HostHandle::start` performs only the minimal work required for the host to become *live* (config snapshot, sinks, registry/router wiring, JoinSet creation) and **returns immediately** once those pieces are ready.
  4. As part of `start`, the host schedules a background task that iterates configured servers and launches each transport concurrently (stdio spawn, ws/http connect) under the supervising JoinSet; success/failure is surfaced via events, not blocking the parent.
  5. Server handshakes fetch tools/resources and cache them in the `Registry`. A failed server leaves the host running so other servers (and the UI) are unaffected.
* **Per LLM call**
  * Query `Registry` for active tool schemas and merge with the worker bridge so prompts include only allowed tools.
  * On tool invocation, `Router` dispatches the JSON-RPC call to the target server and returns the result.
* **Shutdown**
  * `KillSwitch` triggers graceful termination (SIGTERM, then SIGKILL after timeout) for all transports.
  * Internally issue a watch-channel `shutdown_signal` so spawned tasks finish before resources drop; background launch tasks also observe this signal.
  * Tauri host calls `shutdown()` during global teardown to ensure background tasks stop cleanly.

## Async & Events

* Fully async via `tokio`.
* Broadcast these events:
  * `ServerStarted(name)`
  * `ServerCrashed(name, err)`
  * `SchemasUpdated`
  * `ServerStopped(name)`
* Provide optional callbacks so the Tauri layer can forward status via UI events (e.g. `assistant://mcp-status`) and emit structured `tracing` logs with correlation IDs supplied by the app.
* Run transports under a supervising `JoinSet`/`tokio::task` so spawn lifetimes are tracked; guard `update_config`/`restart` with a global `RwLock` on the registry to avoid racing active launches. Host startup enqueues these tasks and then returns control to the caller so the UI can continue its own setup while servers come online.
* Standardise host-to-app event payloads (JSON) so telemetry pipes stay in lock-step, e.g.:

```json
{
  "event": "ServerStarted",
  "server_id": "browser",
  "pid": 12345,
  "timestamp": "2025-11-03T21:15:10.123Z",
  "trace_id": "abc123",
  "details": {}
}
```

  * App provides `trace_id`/`server_id` seeds so logs, UI indicators, and MCP activity tie back to the originating user turn.

## Config (Cursor / Windsurf style)

* Support Cursor-style `mcpServers` and Windsurf-style `servers` keys; normalise internally.
* Runtime config lives under `%APPDATA%/com.sprite.validation/...` alongside other app config. The app layer loads, merges, and validates JSON, then hands pure `HostConfig` data to the crate.
* Expose `update_config(new_config)` / `restart(new_config)` APIs so the app can apply changes without teaching the crate to watch files.

**Unified config schema (accepted superset):**

```json
{
  "mcpServers": {
    "server-id": {
      "type": "stdio|ws|http",
      "command": "path-or-binary",
      "args": ["..."],
      "env": { "KEY": "VALUE" },
      "url": "ws://... or https://... when not stdio",
      "allowedTools": ["optional-whitelist"],
      "allowedDomains": ["optional-domain-scope"]
    }
  },
  "servers": {
    "alias-same-as-above": {
      "type": "stdio|ws|http",
      "command": "path-or-binary",
      "args": ["..."],
      "env": { "KEY": "VALUE" },
      "url": "ws://... or https://... when not stdio",
      "allowedTools": ["optional-whitelist"],
      "allowedDomains": ["optional-domain-scope"]
    }
  }
}
```

## Policy & Safety

* Only advertise tools from servers that started successfully, passed handshake, and satisfy user policies (tools/domains).
* Enforce domain and application whitelists before routing.
* Apply per-call timeout and payload caps.

## Ops & DX

* `update_config(new_config)` applies an app-supplied snapshot; `restart(new_config)` tears down and rebuilds transports.
* Structured `tracing` logs with server name + correlation ID; tie into existing telemetry level controls. Keep PATH or other sensitive data out of info/warn logs per `AGENTS.md`; debug builds may log counts/lengths for diagnosis instead of raw values.
* Backoff and restart crashed servers with bounded retries.
* Emit MCP-specific preload/progress events (`assistant://mcp-status`, `assistant://mcp-progress`) so both sprite shell and control panel reflect status in real time.
* Unit tests: config parsing, handshake, tool routing, shutdown behaviour.
* Provide an in-memory “dummy MCP” test binary or feature flag so CI can exercise spawn/registry/shutdown without real servers.

## Minimal API (app ⇄ host)

```rust
pub struct McpHostHandle;

impl McpHostHandle {
    pub async fn start(config: Config, sinks: HostSinks) -> anyhow::Result<Self>;
    pub fn tool_schemas(&self) -> Vec<ToolSchema>;
    pub async fn call(&self, server: &str, method: &str, params: serde_json::Value) -> anyhow::Result<serde_json::Value>;
    pub fn events(&self) -> tokio::sync::broadcast::Receiver<Event>;
    pub async fn update_config(&self, config: Config) -> anyhow::Result<()>;
    pub async fn restart(&self, config: Config) -> anyhow::Result<()>;
    pub async fn shutdown(&self) -> anyhow::Result<()>;
}

pub struct HostSinks {
    pub event_callback: Arc<dyn Fn(HostEvent) -> anyhow::Result<()> + Send + Sync>,
    pub log_span: tracing::Span,
}
```

## Kill Switch

* Immediate host shutdown API that tears down all child MCPs and closes transports; invoked by app-level kill switch as well as the MCP crate itself when fatal errors occur.
* Scope is limited to MCP transports; broader system shutdown (STT/TTS/etc.) stays under the existing app-level kill switch. If we ever need a global cascade, wire via a dedicated `SystemKillSwitch`.

## Integration Points

* Worker bridge consumes `tool_schemas()` to hydrate prompts sent to the external LLM worker, mirroring how preferences/machine-state sync already runs.
* Host emits `SchemasUpdated` (and related) when cache changes; Tauri layer forwards updated schemas to the worker `/sync_mcp` endpoint only when diffed data changes.
* Turn handling flow:
  1. Worker `/chat` call produces user-facing reply text plus intents; an `mcp_plan` intent identifies the MCP server and high-level instruction when tooling is required.
  2. App plays TTS immediately and, when an `mcp_plan` intent is present, calls `/planner` with the instruction, session token, allowlists, conversation memory, and the current execution `history` (initially empty).
  3. Planner responses provide ordered `actions[]` plus optional `reply`. The desktop executes each action via `HostHandle::call_tool_async`, emits `assistant://mcp-progress` updates, appends `{action, result}` to `history`, and re-posts `/planner` whenever additional guidance is needed.
  4. Worker may surface mid-plan narration through the `reply` field returned from `/planner`; UI throttles repeated narration while keeping the MCP panel in sync.
  5. When `/planner` returns an empty `actions[]`, the desktop delivers any concluding `reply`, persists transcripts, and stops invoking the planner for that session token.
* Tool call results remain local to the desktop; only summarised `history` entries are sent back to `/planner`, keeping transport details hidden from the worker while preserving full telemetry inside the host.

---

**Goal:** keep the MCP host modular, async, secure, UI-observable, and ready to extract as its own crate without refactoring the rest of the app.
