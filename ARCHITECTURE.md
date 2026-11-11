# Architecture Overview (Current State)

Updated to reflect the codebase as of December 2025, clarifying what is shipped vs. still TODO.

## High-Level Flow

```
Microphone -> Python Wake-Word Sidecar (optional) -> Rust Audio Pipeline (VAD + Whisper) -> Worker Bridge (Cloudflare Worker) -> (if planner requested) MCP Host -> Planner flow
```

- **Desktop (Tauri/Rust)**
  - Captures audio with `cpal`, resamples to 16 kHz, feeds WebRTC VAD with 30 ms frames.
  - State machine: `Idle -> WakeWordRecording/PTTRecording -> Processing`.
  - Runs Whisper locally; energy + VAD gates drive when transcripts are emitted.
  - Emits `assistant://stt-*` events for UI (ready/started/final/result); failures emit `assistant://stt-error` for sprite/panel warnings.
  - Boots the embedded MCP host; emits `assistant://mcp-status`; planner flow uses it after a worker response requests planning.
- **Frontend (Sprite webview)**
  - Listens for `assistant://stt-final` (transcripts) and `assistant://stt-result` (assistant replies/intents).
  - Listens to `sprite://ui` for state/VU; snapshot hydration via `get_ui_snapshot`.
- **Python Sidecar**
  - Hosts Porcupine wake-word detection; reports events over stdout.
  - Self-terminates when parent PID disappears.
  - Receives runtime flags: `--parent-pid`, `--log-dir`, optional wake-word overrides, `--app-user-model-id`.
- **Cloudflare Worker**
  - Receives final transcript and returns `{ reply, intents[] }`.
  - Stores per-machine state in a Durable Object (conversation memory, allowlist, preferences, MCP context).
  - Calls the LLM with conversation history and allowlist; may request planner work via intents.
- **Configuration Layer**
  - `AppConfig` loads `stt.config.json`, `tts.config.json`, `mcp.config.json`, preferences, UI flags; held in `Arc<RwLock<_>>`.
  - Config files are created from Rust defaults on first run. Dev builds use `apps/tauri/src-tauri/.dev/config`; production uses `%APPDATA%\\com.sprite.validation\\config`.
  - File watchers emit `config_dirty` for STT/TTS/MCP when disk edits diverge from in-memory state; only STT changes saved from the panel are hot-reloaded today.
  - `MachineStateManager` handles machine ID and syncing config/memory with the worker.

## Machine State Management

Worker Durable Object holds conversation memory, allowlist, preferences, response handles; sync endpoints are driven from the Rust host (`MachineStateManager`).

## Text-to-Speech Flow

```
Worker reply -> TTS queue -> Piper synth (chunked) -> Rodio playback (sequential) -> UI status events
```

- **Desktop (Tauri/Rust)**
  - `AppConfig` loads `tts.config.json`.
  - TTS preload ensures Piper binary/voice assets exist (copies bundled assets; downloads voice/model from HF if missing).
  - `TtsService` splits text into ~360-char chunks (config default), synthesizes each sequentially, and plays with `rodio`. No overlap; next chunk starts after playback + optional inter-chunk gap.
  - Status events: `assistant://tts-status` with stages `pending | synthesizing | playing | completed`; errors on `assistant://tts-error`.
- **Assets & Config**
  - Debug builds: assets under `resources/piper/...`; release: `%APPDATA%/com.sprite.validation/tools/piper`.
  - `tts.config.json` covers provider, Piper paths, download URLs, playback defaults, chunking knobs.
  - Device hints are logged but ignored; playback uses the system default device. Volume is clamped to a minimum of 0.0 (no 2.0x cap).
- **Runtime Behaviour**
  - Preload adjusts paths in-memory; many Piper fields are `skip_serializing`, so path fixes are not persisted to disk (TODO).

## Audio Pipeline Details

| Setting                      | Source                   | Default | Purpose                                                       |
|------------------------------|--------------------------|---------|---------------------------------------------------------------|
| `vad.aggressiveness`         | `stt.config.json`        | `2`     | WebRTC VAD sensitivity.                                       |
| `vad.post_speech_silence_ms` | `stt.config.json`        | `240`   | Silence tolerated after speech before auto-stopping.          |
| `vad.start_grace_ms`         | `stt.config.json`        | `3000`  | Grace period after wake word before giving up on speech.      |
| `vad.max_segment_ms`         | `stt.config.json`        | `12000` | Absolute cap on segment length.                               |
| `MIN_ENERGY_THRESHOLD`       | `audio_pipeline.rs`      | `5e-5`  | Energy gate to damp low-level noise.                          |
| `DEFAULT_MIN_TRANSCRIPT_CHARS` | `audio_pipeline.rs`    | `3`     | Filters blank/very short outputs.                             |

Key behaviours:

- Wake-word recordings honor the 3 s grace window before silence detection engages.
- Push-to-talk ignores VAD auto-stop; user ends recording.
- Resampled frames accumulate into 30 ms slices for VAD.
- Whisper logs respect the telemetry level in config.

## UI Event Channel

- **Channel:** `sprite://ui` (backend push-only; single renderer listener)
- **Snapshot:** `invoke("get_ui_snapshot")` seeds `{ state?, vu? }`
- **Messages:** `State` (`Idle|Listening|Thinking|Speaking|Planning|Error`), `Vu` (0–1, throttled ~30 Hz)
- **Backend guarantees:** bounded queue (128), state dedupe, VU coalesce, snapshot cache
- **Frontend behaviour:** EMA smoothing (α=0.6), 120 ms dwell between states (except immediate `Error`)
- **Reference:** `apps/tauri/src-tauri/src/ui/mod.rs`, `apps/tauri/webview/main.js`
- **Note:** No `Cue` events are emitted today (TODO if needed).

## Logging Strategy

Logging level follows `telemetry.level` in STT config. Sidecar logs to `%APPDATA%\com.sprite.validation\logs\sprite_voice_sidecar.log` when `--log-dir` is set.

## Sources of Truth

- **Config:** Runtime config lives under `%APPDATA%\\com.sprite.validation\\config` (prod) or `apps/tauri/src-tauri/.dev/config` (dev). STT is hot-reloaded on panel saves; TTS/MCP/Preferences require restart. Disk edits emit `config_dirty` for STT/TTS/MCP.
- **Pipeline Code:** `apps/tauri/src-tauri/src/stt/audio_pipeline.rs`
- **Wake Word Service:** `apps/tauri/src-tauri/src/stt/hybrid_service.rs` plus `apps/voice-service/*`

## Dual Window UI (Sprite + Panel)

- Two windows: sprite (`main`) and panel (`panel`). Panel creation is gated by `ui.enablePanel` and starts hidden/off-taskbar; toggle via tray or `panel_navigate`.
- Current panel is a static HTML/JS bundle with manual view toggling (no SPA/router or lazy loading). Navigation uses `panel://navigate` events and DOM class toggles.
- Panel views include Config summary/edit, Chat log, Logs, MCP registry/progress, Preferences, About; they consume Tauri commands (no broadcast chat store or manual send-to-worker path).
- References: `apps/tauri/src-tauri/src/panel/mod.rs`, `apps/tauri/webview/panel/panel.js`.

## Security & Privacy

- Tauri `csp` is `null` in `tauri.conf.json`; no isolated-scope CSP is enforced yet (TODO).
- Log rendering and chat inputs are treated as untrusted in the panel JS, but no additional sanitization layer is present (TODO if required).

## Testing & Observability

- No Playwright/UI contract tests are present for the panel (TODO).
- Metrics/logging exist for STT/worker/MCP flows; panel has no automated tests.

## Operating Assumptions

- Tray icon focuses on sprite; panel toggle is exposed when enabled.
- Panel hides instead of closing; taskbar entry appears only while visible.
- Planner flow runs only when the worker responds with a planner intent; MCP host is started at boot to be ready for that.

## TODOs (implementation gaps)

- Add config watchers/hot-reload for TTS/MCP/Preferences (only STT is watched today).
- Persist TTS preload path adjustments despite `skip_serializing` fields, or redesign config shape.
- Restore/implement UI cues on `sprite://ui` if needed.
- Upgrade panel to a true SPA/router with lazy-loaded views, shared stores, and manual chat send path to the worker.
- Enforce a CSP/isolated scope for the panel window.
- Add Playwright (or similar) UI tests for routing, focus, validation.
