# Clippy v2 - Development Guide

## Current Status (October 2025)

### Stable
- Wake word detection via Python sidecar (Porcupine, demo fallback)
- Push-to-talk (`Ctrl+Space`) and wake-word recording share the same audio pipeline
- WebRTC VAD with configurable grace/activation/hold windows (defaults: 3000 ms grace, 240 ms activation, 240 ms hold, 600 ms silence cutoff)
- Whisper STT (local) with automatic model download and transcript filtering
- Piper TTS pipeline with automatic asset preparation and chunked playback
- Live `stt.config.json` reload, including VAD activation/hold windows and optional Whisper thread overrides
- Worker bridge round-trips (transcript -> reply/intents)
- Control panel SPA ships Config, Chat, Logs, and About views (live editing, transcript preview, log tailing).
- Backend commands that drive those screens now live under `apps/tauri/src-tauri/src/commands/`, grouped by concern (assistant, panel, metrics, preferences, etc.) so `lib.rs` stays focused on bootstrap wiring.
- MCP host spins up stdio MCP servers, emits `assistant://mcp-status`, and keeps the worker in sync via `/sync_machine_state`.
- Sprite overlay caches `stt_preload_status` / `tts_preload_status` so readiness persists across hot reloads.

### Watch List
- Background noise can still trigger low-energy voice frames; raise `MIN_ENERGY_THRESHOLD` if needed
- Long pauses after the wake word may require bumping `vad.post_speech_silence_ms` above 240 ms

### Recent Updates
- Control panel chat view mirrors transcripts + assistant replies; logs view tails latest `app*.log`.
- Whisper models stored under `%APPDATA%\com.sprite.validation\models\whisper` in production builds; dev builds retain relative paths.
- Resampled audio accumulates between callbacks so VAD always sees 30 ms frames
- VAD activation/post-speech windows are configurable (`vad.activation_ms`, `vad.post_speech_silence_ms`)
- Whisper thread usage can be overridden (`whisper.threads`) and the inference worker bumps its priority on Windows
- Config edit UX now hides the read-only summary when the form is in edit mode
- Logging is controlled by the config file's telemetry level setting
- Worker responses now enqueue speech via the local Piper runtime; status is surfaced on `assistant://tts-status` and failures on `assistant://tts-error`
- New `get_mcp_registry` command surfaces the active MCP server inventory for the control panel and DevTools.
- Vendored `whisper-rs`/`whisper-rs-sys` with a release optimisation patch (see `docs/issues/whisper_release_patch.md`)
- Legacy architecture notes live under `docs/archive/`; see `ARCHITECTURE.md` for the maintained overview

## Architecture

### **Hybrid Voice Processing Pipeline**
```
Microphone → Python Sidecar (Wake Word Only) → Rust Backend (Audio + VAD + Whisper) → Cloudflare Worker
```

### **Components:**
- **Tauri App**: Rust-based desktop application with frontend
- **Python Sidecar**: Wake word detection only (minimal, self-terminating)
- **Rust Audio Pipeline**: Audio capture, WebRTC VAD, Whisper STT
- **Cloudflare Worker**: AI processing and intent handling
- **Configuration**: `stt.config.json` for all settings

## Audio Pipeline Details

### VAD & Silence Handling
- Frames are resampled to 16 kHz and buffered until 30 ms (480-sample) windows are available for WebRTC VAD.
- `vad.aggressiveness` (0-3) controls sensitivity; default `2` balances noise rejection with responsiveness.
- Wake-word recordings respect `vad.start_grace_ms` (default 3000 ms) before any silence checks. Once speech is detected, `vad.activation_ms` (default 240 ms) requires a short burst of speech before locking, and `vad.post_speech_silence_ms` (default 240 ms) controls how long we keep listening after speech tails off.
- Push-to-talk ignores VAD stop logic; the user's key release ends the segment.
- `MIN_ENERGY_THRESHOLD` (hard-coded at `5e-5` in `audio_pipeline.rs`) prevents very low-energy false positives from keeping the mic active. Raise it if background hum triggers speech, lower it if quiet microphones get cut off.

### Whisper STT Filters
- Audio shorter than 500 ms (`MIN_AUDIO_DURATION_MS`) is discarded to avoid accidental taps and noise.
- Transcripts that collapse to `[BLANK_AUDIO]`, `(sigh)` or similar artifacts are filtered before they reach the worker bridge.
- `DEFAULT_MIN_TRANSCRIPT_CHARS` (3) keeps single-character outputs from firing intents.
### Recommended Tuning
- Noisy room -> set "vad": { "aggressiveness": 3, "activation_ms": 320, "post_speech_silence_ms": 320 }.
- Slow speaker -> raise `post_speech_silence_ms` to 400-600 ms and, if needed, `start_grace_ms` to 4000-5000 ms.
- Quiet microphone -> lower `MIN_ENERGY_THRESHOLD` slightly (code change) or reduce VAD aggressiveness to `1`. If speech clips too quickly, raise `post_speech_silence_ms`.

## Development Setup

### **Prerequisites:**
- Node.js 18+
- Python 3.8+
- Rust 1.70+
- pnpm

### **Quick Start:**
```bash
# Optional helper scripts (wrapper around the Python tooling)
pnpm install:voice   # pip install + PyInstaller
pnpm build:voice     # PyInstaller build (writes dist/sprite_voice_sidecar.exe)

# Install dependencies
pnpm install

# Build Python sidecar (first time only)
cd apps/voice-service
python setup.py
python build.py

# Start development (from project root)
cd ../..
pnpm run dev
```

### **Vendored Dependencies**
- The repo includes patched copies of `whisper-rs` and `whisper-rs-sys` under `third_party/`.
- If the folders are missing (e.g., fresh clone), copy the crates from `$CARGO_HOME/registry/src/.../whisper-rs-0.15.1` and `whisper-rs-sys-0.14.1`, then reapply the optimisation patch described below.
- See `docs/issues/whisper_release_patch.md` for the `-O2`/`-DNDEBUG` change and refresh steps when bumping the crate versions.

### **Voice Service Setup:**
```bash
cd apps/voice-service

# Install Python dependencies
pip install -r requirements.txt

# Optional: Set Picovoice access key (or use demo mode)
python setup_key.py

# Build executable
python build.py
```

## Configuration

### **Main Config: `stt.config.json`**
Located at (production build): `C:\Users\<username>\AppData\Roaming\com.sprite.validation\config\stt.config.json`
Dev mode continues to read the repo copy at `apps/tauri/src-tauri/stt.config.json`, but writes propagate to the AppData mirror via `AppConfig`.

```json
{
  "sidecar": {
    "executable_path": "resources/sprite_voice_sidecar.exe",
    "access_key": ""
  },
  "wakeword": {
    "provider": "porcupine",
    "keyword": "hey google",
    "sensitivity": 0.5
  },
  "vad": {
    "aggressiveness": 2,
    "start_grace_ms": 3000,
    "max_segment_ms": 12000,
    "activation_ms": 240,
    "post_speech_silence_ms": 240
  },
  "hotkey": {
    "push_to_talk": "Ctrl+Space",
    "mode_precedence": "ptt"
  },
  "provider": "local"
}
```

### **Voice Output Config: `tts.config.json`**
Lives alongside `stt.config.json` (AppData for packaged builds, repo root in debug). Controls the text-to-speech provider and Piper asset paths:

```json
{
  "provider": "local",
  "piper": {
    "executable_path": "resources/piper/piper.exe",
    "model_path": "resources/piper/en_US-amy-medium.onnx",
    "metadata_path": "resources/piper/en_US-amy-medium.onnx.json",
    "chunk_chars": 360,
    "inter_chunk_gap_ms": 120,
    "retain_chunks": false,
    "download": {
      "repo_id": "rhasspy/piper-voices",
      "revision": "main",
      "voice": "en/en_US/amy/medium/en_US-amy-medium"
    }
  },
  "playback": {
    "device": null,
    "volume": 1.0
  }
}
```

- On launch the preload runner copies the bundled Piper executable into `%APPDATA%\com.sprite.validation\tools\piper` (release) and makes sure the configured voice model/metadata exist by pulling them from the configured Hugging Face dataset (`repo_id`/`voice`).
- Progress and failure states surface through `tts_preload_status` so the sprite shell and control panel can show feedback.
- Playback currently targets the default system output; device hints are logged for observability until per-device selection lands.
- Set `piper.retain_chunks` to `true` to keep synthesized WAV chunks for inspection; they remain in the cache directory until you delete them.
Production installs also mirror Whisper models under `%APPDATA%\com.sprite.validation\models\whisper`. Dev builds keep models inside `apps/tauri/src-tauri/resources/whisper` so hot reloads skip rewriting paths.

**Note**: 
- The runtime copy lives under `%APPDATA%\com.sprite.validation\config`; keep `executable_path` set to the bundled `resources/sprite_voice_sidecar.exe` unless you are intentionally pointing at a locally built executable.
- Leave `access_key` empty for wake word demo mode
- Update `wakeword.keyword` / `sensitivity` to change detection.
- Adjust `hotkey.push_to_talk` for global push-to-talk.
- Tune VAD responsiveness with `start_grace_ms`, `activation_ms`, and `post_speech_silence_ms`.
- Leave `whisper.threads` as `null` (auto) or set an explicit value to limit CPU usage.
- Whisper model path lives under `model.path`; the app downloads the file automatically if missing.
- Tweak `start_grace_ms` if you need more time to speak after the wake word (default 3000ms)
- The Tauri host loads the JSON once through `AppConfig` and keeps it in an `Arc<RwLock<_>>`, so edits to the runtime file are hot-reloaded without code changes

The shared `ReplySchema` now includes an optional `planner_context` field so the chat model can summarize context for the MCP planner without resending the full transcript.

### **MCP Host Config: `mcp.config.json`**
- Mirrors to `%APPDATA%\com.sprite.validation\config\mcp.config.json` in production builds (dev reads `apps/tauri/src-tauri/mcp.config.json`).
- Accepts Cursor (`mcpServers`) and Windsurf (`servers`) style maps; each entry describes `type`, `command`/`url`, optional `allowedTools`/`allowedDomains`, and heartbeat/timeouts.
- Updates hot-reload via `AppConfig`; no restart required. Host events flow over `assistant://mcp-status` and the `get_mcp_registry` command returns current tool/resource counts.
### **Whisper Model Selection:**

| Model | Size | Speed | Best For |
|-------|------|-------|----------|
| `ggml-base.en.bin` | 142MB | Fast | Default, English-only |
| `ggml-tiny.en.bin` | 75MB | Fastest | Quick iteration, English-only |
| `ggml-small.en.bin` | 466MB | Medium | High accuracy, English-only |
| `ggml-base.bin` | 142MB | Fast | Multilingual |
| `ggml-small.bin` | 466MB | Medium | Multilingual, high accuracy |

To change model, update `model.path` in config and restart. Model will download automatically.

## Testing

### **Voice Service Testing:**
```bash
cd apps/voice-service

# Test Python script directly (development)
python voice_processor.py

# Test built executable (with parent PID)
dist\sprite_voice_sidecar.exe <parent_pid>

# Build and test
python build.py
```

### **Full App Testing:**
```bash
# From project root
pnpm run dev

# Test wake word: Say "Hey Google"
# Test PTT: Hold Ctrl+Space and speak
```

### **Debugging:**
```bash
# Check Python logs (written under %APPDATA%\com.sprite.validation\logs)
type "%APPDATA%\com.sprite.validation\logs\sprite_voice_sidecar.log"

# Check if Python process is running
tasklist /FI "IMAGENAME eq sprite_voice_sidecar.exe"

# Kill orphaned processes (if needed)
taskkill /F /IM sprite_voice_sidecar.exe
```
```bash
# Adjust telemetry level in config file for verbose logging
```

Default runs log at `info`, highlighting model selection, wake-word fallback, and worker results. Adjust the telemetry level in the config file to control logging verbosity.


## Build & Deploy

### **Python Sidecar:**
```bash
cd apps/voice-service
python build.py  # Creates sprite_voice_sidecar.exe
```

### **Tauri App:**
```bash
cd apps/tauri
pnpm run tauri build
```

## Future Roadmap

### **Phase 2 (Next):**
- [ ] Enhanced wake-word detection with multiple keywords
- [ ] Improved VAD with noise reduction
- [ ] Better error handling and recovery
- [ ] Performance optimizations

### **Phase 3 (Future):**
- [ ] Custom wake word training
- [ ] Multi-language support
- [ ] Advanced audio processing
- [ ] Cloud STT provider integration

## Troubleshooting

### **Common Issues:**

1. **Wake word not detected**:
   - Check if Python sidecar is running: `tasklist | findstr sprite_voice_sidecar`
   - Verify access key in config (or use demo mode with empty string)
   - Check logs: `type "%APPDATA%\com.sprite.validation\logs\sprite_voice_sidecar.log"`

2. **Python process won't terminate**:
   - Should auto-terminate when Rust app closes (PID monitoring)
   - Manual kill: `taskkill /F /IM sprite_voice_sidecar.exe`

3. **PyInstaller build fails**:
   - Kill running processes: `taskkill /F /IM sprite_voice_sidecar.exe`
   - Clean build: `cd apps/voice-service && rmdir /s /q dist build`
   - Rebuild: `python build.py`

4. **Microphone still in use after closing app**:
   - Check for orphaned Python processes
   - Self-termination should handle this automatically

5. **Config changes not taking effect**:
   - Edit the actual config at: `%APPDATA%\com.sprite.validation\config\stt.config.json`
   - Not the source file at `apps/tauri/src-tauri/stt.config.json`

### **Debug Commands:**
```bash
# Check Python processes
tasklist /FI "IMAGENAME eq sprite_voice_sidecar.exe"

# View Python logs
type "%APPDATA%\com.sprite.validation\logs\sprite_voice_sidecar.log"

# Check config
type %APPDATA%\com.sprite.validation\config\stt.config.json

# Rebuild Python sidecar
cd apps\voice-service
taskkill /F /IM sprite_voice_sidecar.exe
python build.py
```
