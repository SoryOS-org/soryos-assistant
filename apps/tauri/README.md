# Sprite Assistant (Clippy v2) - Desktop App

Tauri-based desktop assistant for Windows with voice capture, wake-word/PTT control, and application launching.

## Features

- **Voice pipeline**: Wake-word (Porcupine) + VAD + Whisper STT + Piper TTS
- **App intents (Windows)**: Natural-language app launching via AppUserModelIds/executables
- **UI**: Transparent always-on-top sprite + Control Panel (chat, logs, MCP status, config/preferences)
- **MCP host**: Built-in stdio host with worker sync and schema forwarding
- **Worker bridge**: Default Cloudflare Worker endpoint, overrideable via `stt.config.json` (`worker.url`).

## Quick Start

```bash
# From project root
pnpm install
pnpm dev
```

What `pnpm dev` actually does (via `start-dev.(bat|sh)`):
- Build shared schemas (`packages/shared`)
- Build the Python voice sidecar
- Start the worker (`apps/worker dev`)
- Start the Tauri app (`apps/tauri tauri:dev`)

## Development

### Prerequisites
- Windows 10/11 (app launching is Windows-only)
- Rust (stable)
- Node.js 18+ and pnpm
- Python 3.8+ (voice sidecar toolchain)
- Windows: MSVC toolchain for Whisper

### Common scripts
```bash
# Dev (runs the full stack described above)
pnpm dev

# Dev only the desktop app (worker already running)
pnpm -C apps/tauri tauri:dev

# Install Python dependencies for the sidecar
pnpm install:voice

# Build for production
pnpm build:tauri

# Test voice features (requires `pnpm build:voice` to generate the sidecar)
pnpm test:voice
```

## Configuration

Config files live under `%APPDATA%\com.sprite.validation\config\` in production (created under `apps/tauri/src-tauri/.dev/config` in dev builds):

- **STT**: `stt.config.json` is generated from code defaults on first run and *is hot-reloaded* when updated via the panel. Example: `config-examples/stt.config.example.json`.
- **TTS**: `tts.config.json` is generated from code defaults on first run; restart to apply edits. Example: `config-examples/tts.config.example.json`.
- **MCP**: `mcp.config.json` is generated on first run; restart to apply edits. Example: `config-examples/mcp.config.example.json`.
- **Secrets**: Picovoice access key and worker auth token are stored in the OS keychain via the secrets flow, not in the config file.
  - Get a Picovoice key at [console.picovoice.ai](https://console.picovoice.ai/).

If the wake-word key is missing, the runtime automatically falls back to PTT-only mode.
If a config file is missing or fails to parse, the app writes defaults back to disk.
Edits made directly on disk emit `config_dirty` for STT/TTS/MCP; only STT panel saves are hot-reloaded today.

STT example (`config-examples/stt.config.example.json`):

```json
{
  "provider": "local",
  "model": {
    "id": "base.en",
    "language": { "auto": true, "default": "en" },
    "punctuation": true
  },
  "wakeword": {
    "provider": "porcupine",
    "keyword": "jarvis",
    "sensitivity": 0.5
  },
  "vad": {
    "aggressiveness": 2,
    "start_grace_ms": 3000,
    "max_segment_ms": 12000,
    "activation_ms": 240,
    "post_speech_silence_ms": 1500
  },
  "hotkey": {
    "push_to_talk": "Ctrl+Space",
    "mode_precedence": "ptt"
  },
  "whisper": {
    "threads": null
  },
  "telemetry": {
    "level": "errors",
    "raw_payload_logs": false
  },
  "sprite": {
    "scale": 0.3
  }
}
```

- Tune wake-word keyword/sensitivity, VAD timings, and PTT hotkey here.
- Whisper model path resolves from the bundled `resources/whisper/ggml-base.en.bin` unless overridden.
- Telemetry level controls runtime log filtering.

### Text-to-Speech (`tts.config.json`, see `config-examples/tts.config.example.json`)

```json
{
  "provider": "local",
  "piper": {
    "executable_path": "resources/piper/piper.exe",
    "model_path": "resources/piper/en_US-amy-medium.onnx",
    "metadata_path": "resources/piper/en_US-amy-medium.onnx.json",
    "download": { "repo_id": "rhasspy/piper-voices", "revision": "main", "voice": "en/en_US/amy/medium/en_US-amy-medium" }
  },
  "playback": {
    "volume": 1.0,
    "crossfade_ms": 0
  }
}
```

- The bundled Piper binary and voice files live under `src-tauri/resources/piper`; the preload step resolves them in place or downloads missing assets to the runtime data dir. Path fixes are applied in-memory today (not persisted yet).
- If a voice file is missing the loader fetches it from Hugging Face (`repo_id`/`voice`), emitting progress via `tts_preload_status`.
- Replies play through the system default device right now; volume is adjustable in the config (>= `0.0`). Device hints are logged until selectable outputs land.
- Set `piper.retain_chunks` to `true` to keep synthesized WAV chunks for inspection; they remain in the cache directory until you delete them.

### MCP Host (`mcp.config.json`, see `config-examples/mcp.config.example.json`)

```json
{
  "mcpServers": {
    "browsermcp": {
      "type": "stdio",
      "command": "npx",
      "args": ["@browsermcp/mcp@latest"]
    }
  }
}
```

- Supports both Cursor-style (`mcpServers`) and Windsurf-style (`servers`) maps; the host normalises them at runtime.
- Packaged builds create `%APPDATA%\com.sprite.validation\config\mcp.config.json` on first run; dev builds generate `apps/tauri/src-tauri/.dev/config/mcp.config.json`.
- Lifecycle updates stream through `assistant://mcp-status`; call the `get_mcp_registry` command from the frontend/DevTools to inspect active servers and tool counts.
- Worker sync is automatic: when schemas change, the host forwards the snapshot to `/sync_machine_state` via `sync_mcp_snapshot`.

## Worker dependency

- The desktop app talks to a worker service (default: `https://sprite-boss.tamjad91.workers.dev`).
- Override the worker URL by setting `worker.url` in `stt.config.json`, then restart.
- The dev script starts `apps/worker dev`; ensure that service is reachable for intents and MCP bridging.
- Worker API calls require the shared bearer token set in the panel Secrets view.
## Production Logging

- In production builds, logs are written with `tracing` to `%APPDATA%/com.sprite.validation/logs/app.log` (daily rotation).
- The Python sidecar receives the logs directory via CLI `--log-dir <path>` and writes `sprite_voice_sidecar.log` in the same folder.
- In dev builds, logging uses `env_logger` to the console.

## Audio Capture Policy (Privacy Critical)

- Audio capture is strictly lazy in all modes.
  - Capture starts only when: (a) wake word is detected, or (b) PTT is pressed.
  - Capture stops immediately when: (a) the pipeline returns to Idle (wake-word path), or (b) PTT is released.
- There is no startup or always-on capture, including PTT-only mode.

## Resource Bundling

- `sprite_voice_sidecar.exe` is bundled via `src-tauri/tauri.conf.json` resources.
- Whisper models are bundled under `src-tauri/resources/whisper/*.bin`.
- Piper binary and voice assets are bundled under `src-tauri/resources/piper/**/*`.
- Config files are generated on first run from in-code defaults.

## Runtime Path Resolution

- Production resource paths (models/sidecar) are resolved with `app.path().resolve(..., BaseDirectory::Resource)`.
- Centralized helpers for runtime paths live in `src-tauri/src/config/mod.rs`.

## Testing

### Voice Testing
1. Press `Ctrl+Space` to activate PTT
2. Speak: "Open calculator" 
3. Release `Ctrl+Space` to process
4. Watch console for transcript and app launch

### DevTools Testing
```javascript
await spriteAssistant.request("open calculator");
await spriteAssistant.launchApp("Calculator");
```

For detailed testing, see the main README notes and component guides.
