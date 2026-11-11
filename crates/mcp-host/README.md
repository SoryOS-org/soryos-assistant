# MCP Host (Rust)

Rust crate that embeds a Model Context Protocol (MCP) host inside the desktop
assistant. It handles spawning servers, routing tool calls, and emitting status
events so both the UI and worker bridge stay in sync.

## Features

- **Transport-Agnostic Core** – `Transport` trait with stdio implementation
  today; websocket/HTTP connectors stubbed behind feature flags (`ws`, `http`).
- **JSON-RPC Client** – Performs MCP handshake, lists tools/resources, and
  dispatches method calls with timeout/error handling.
- **Shared Registry** – Tracks server metadata, tool schemas, and timestamps
  for UI/telemetry consumption.
- **Router** – Enforces global/server-level policy (allowed tools/domains),
  applies per-call timeouts, and retries transport errors.
- **Lifecycle Management** – `HostHandle::start/update_config/restart/shutdown`
  manage transports via `JoinSet`, broadcast events via `HostSinks`, and keep
  config under an `Arc<RwLock<_>>`.

## Key Types

| Type            | Purpose                                             |
|-----------------|------------------------------------------------------|
| `HostConfig`    | Normalised MCP config (`mcpServers` / `servers`).    |
| `HostHandle`    | Owning handle that spins transports + router.        |
| `HostEventKind` | `ServerStarted/Stopped/Crashed/SchemasUpdated`.      |
| `Registry`      | In-memory cache of server info & tool/resource lists.|
| `Router`        | Policy-enforced dispatcher used by the desktop app.  |

## Host Events

Events are emitted through the `HostSinks` callback supplied by the desktop
app. The authoring app serializes these into Tauri events (`assistant://mcp-status`)
and mirrors schema updates to the worker via `/sync_machine_state`.

Example payload:

```json
{
  "meta": {
    "server_id": "browser",
    "timestamp_ms": 1751562210123,
    "trace_id": "abc123..."
  },
  "kind": {
    "event": "SchemasUpdated",
    "tool_schemas": { "...": "..." },
    "resources": { "...": "..." }
  }
}
```

## Configuration Schema

`HostConfig` accepts a superset of Cursor and Windsurf formats:

```json
{
  "mcpServers": {
    "browser": {
      "type": "stdio",
      "command": "apps/mcp/browser/server.exe",
      "args": ["--serve"],
      "allowedTools": ["search", "open_url"]
    }
  },
  "servers": {
    "another": {
      "type": "ws",
      "url": "ws://localhost:9000",
      "allowedDomains": ["example.com"]
    }
  }
}
```

### TransportConfig

- `Stdio` – command, args, env, optional `current_dir`, timeouts, heartbeat.
- `Ws` / `Http` – URL + headers. Currently return `TransportError::Unimplemented`
  unless the respective feature flag is enabled and a connector is provided.

## Integration (Desktop App)

1. Desktop loads MCP config via `AppConfig`.
2. Passes snapshot + `HostSinks` to `HostHandle::start`.
3. `HostHandle` spawns transports, initialises servers, populates registry.
4. `HostSinks` callback forwards `assistant://mcp-status` events and triggers
   `sync_mcp_snapshot` to push tool schemas to the worker.
5. Router API is handed the original `HostHandle::router()` to service tool calls.

## Tests

- `cargo test -p mcp-host` exercises:
  - Config/serde round-trips (`config.rs`).
  - Event emission + payload serialisation (`events.rs`).
  - Transport helpers (timeout/duration calculations).
  - JSON-RPC client ID matching & init parsing.
  - Registry insertion/snapshot.
  - Router policy enforcement and call timeouts.
  - Host lifecycle (mock transport factory).

## Roadmap

- Real websocket/HTTP transport connectors behind `ws`/`http` feature flags.
- Schema diffing + incremental host updates (avoid full restart on small config change).
- Structured logging hooks for per-server spans/correlation IDs.
- Planner helper utilities for the worker (`/mcp_planner` flow).

---

Maintained alongside the desktop app; see `apps/tauri/src-tauri/src/lib.rs` for
integration details and `apps/tauri/MCP Integration Layer- High level plan.md`
for long-term coordination.
