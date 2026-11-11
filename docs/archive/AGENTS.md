# Development Constraints

This project is maintained by multiple agents. Keep these shared guidelines in mind when adding new behaviour:

- **No environment variables for runtime configuration.** Use the existing `config` module to source values and persist edits.
- **Keep the speech pipeline modular.** Preload tasks (model downloads, sidecar checks, etc.) live outside the audio pipeline and run before the STT service spins up.
- **Front-end indicators are mandatory for blocking work.** Emit events so both the sprite shell and control panel can surface progress/errors without freezing the UI.
- **Avoid hard-coded paths.** Resolve directories through the provided config helpers or Tauri path APIs.
- **When in doubt, log.** Instrument long-running steps with structured logs so operators can see what's happening without attaching a debugger.
- **ALWAYS ENSURE SRP AND KEEP THINGS MODULAR** Instead of extending the same file, follow SRP principles where applicable and make code modular.
- **Tauri commands live in `src-tauri/src/commands`.** Add new invoke handlers to the appropriate module (or a new module) and keep `lib.rs` focused on wiring.
- **NEVER NUKE A FILES CHANGES USING GIT CHECKOUT WITHOUT CONFIRMATION.**

Feel free to extend this doc as we add more coordinated behaviour.
- Emit `stt_preload_status` events whenever long-running preparation steps start, report progress, or fail so both windows can reflect state.
- Emit `tts_preload_status` for Piper downloads and send `assistant://tts-status` transitions (`pending`, `synthesizing`, `playing`, `completed`) so UI surfaces voice feedback progress.

---

## MCP Integration Layer Roadmap

1. [x] **Config + Data Structures**
   - Define `HostConfig`, transport descriptors, and event payload types in `crates/mcp-host`.
   - Wire serialization/validation and add dummy fixtures for tests.
2. [x] **Transport Abstractions**
   - Implement the generic `Transport` trait with stdio support (spawn launcher, heartbeat/timeouts).
   - Stub WS/HTTP implementations guarded behind feature flags (currently return `Unimplemented` errors until filled in).
3. [x] **Client & Registry Core**
   - Build JSON-RPC client, handshake flow, and in-memory registry using shared `Arc<ConfigState>`.
   - Emit structured events via the callback sink.
4. [x] **Router & Call Orchestration**
   - Implement call dispatch, timeouts, retries, and policy enforcement (allowed tools/domains).
   - Expose `call()` API returning structured results/errors.
5. [x] **Lifecycle Management**
   - Add `start`, `update_config`, `restart`, and `shutdown` wiring with internal shutdown signal + `JoinSet` supervision.
   - Provide kill-switch plumbing confined to MCP transports.
6. [x] **App Integration**
   - Extend Tauri `AppConfig` to hydrate MCP config snapshots and manage the host handle.
   - Sync schema updates to the worker bridge and emit `assistant://mcp-status` UI events; expose `get_mcp_registry` command for UI wiring.
7. **Testing & Tooling**
   - Deliver dummy MCP binary/feature flag for CI.
   - Cover config parsing, lifecycle, routing, and failure scenarios with unit/integration tests.

Update this roadmap as steps complete or new work is scoped.
