# Wake Word Detection Service

Minimal Python service for wake word detection using Porcupine. This service runs as a sidecar process to the Rust Tauri app and provides wake word detection functionality.

## Features

- Wake word detection using Picovoice Porcupine
- Self-terminating process (monitors parent Rust app PID)
- Production-ready logging
- Standalone executable packaging

## Quick Start

```bash
# From repo root (wrapper around the steps below)
pnpm install:voice
pnpm build:voice
pnpm test:voice   # runs the self-termination smoke tests (Windows only)

# Setup dependencies
python setup.py

# Build executable
python build.py

# Test (development - will exit without parent PID)
python voice_processor.py
```

## Files

- `voice_processor.py` - Main entry point, accepts parent PID as argument
- `wake_word_detector.py` - Wake word detection logic using Porcupine
- `communication.py` - IPC handler with parent process monitoring
- `requirements.txt` - Dependencies (`pvporcupine`, `pyaudio`, `psutil`)
- `build.py` - PyInstaller build script
- `setup.py` - Dependency installation script

## Configuration

The service receives configuration from the Rust host via command-line arguments derived from the runtime `stt.config.json` copy in `%APPDATA%\\com.sprite.validation\\config` (or `apps/tauri/src-tauri/.dev/config` in dev builds). The Picovoice access key is stored in the OS keychain and injected at runtime (env `PICOVOICE_ACCESS_KEY` and/or `--pv-access-key`), not persisted in the config file.

**Parent App Config (user-editable fields)** (`stt.config.json`):
```json
{
  "wakeword": {
    "keyword": "hey google",
    "sensitivity": 0.5
  }
}
```

Sidecar settings (executable path, timeouts, parent PID, etc.) are owned by the Rust host and defaults, not serialized into the config. The access key is injected from the keychain at runtime.

**Command Line Usage:**
```bash
sprite_voice_sidecar.exe \
  --parent-pid <parent_pid> \
  [--log-dir <path>] \
  [--wakeword "<keyword>"] \
  [--sensitivity <0.0-1.0>] \
  [--pv-access-key <key>] \
  [--app-user-model-id com.sprite.validation]
```

`sidecar.wakeword_keyword` and `sidecar.wakeword_sensitivity` are forwarded directly to the Porcupine detector as CLI flags; the separate top-level `wakeword` section continues to drive Rust VAD/VTT defaults. The access key is resolved from the keychain and passed to the sidecar at runtime (or overridden via `--pv-access-key` when testing manually).

Where `<parent_pid>` is the process ID of the Rust app. The Python process monitors this PID and self-terminates if the parent exits. The Tauri host supplies these flags automatically; when testing manually you can override any of them. If `--log-dir` is omitted, the sidecar falls back to `VOICE_PROCESSOR_LOG_DIR` or the current working directory.

## Output Signals

- `VOICE_PROCESSOR_READY` - Service initialized successfully
- `VOICE_PROCESSOR_FATAL {json}` - Fatal startup error with `code` and `detail`
- `WAKE_WORD_DETECTED` - Wake word detected, sent to stdout
- Logs to `sprite_voice_sidecar.log` (warnings and errors only) under `%APPDATA%\com.sprite.validation\logs`

## Building

```bash
python build.py
```

Creates `dist/sprite_voice_sidecar.exe` (~9.8 MB) - a standalone executable that includes:
- Python runtime
- PyAudio for audio capture
- Porcupine for wake word detection
- psutil for process monitoring
- All necessary dependencies

## Self-Termination Mechanism

The Python sidecar monitors the parent Rust app's process ID:
1. Rust passes its PID as a command-line argument when spawning the sidecar
2. Python creates a daemon thread that checks `psutil.pid_exists(parent_pid)` every 2 seconds
3. If the parent process terminates, Python sets an exit flag and terminates itself
4. This ensures no orphaned processes or lingering microphone access

## Logging

- **Production Mode**: Only warnings and errors logged to `sprite_voice_sidecar.log`
- **Log Location**: `%APPDATA%\com.sprite.validation\logs`
- **Log Level**: `WARNING` (errors, warnings, and self-termination events)

## Troubleshooting

**Build fails:**
```bash
# Kill any running instances
taskkill /F /IM sprite_voice_sidecar.exe

# Clean and rebuild
rmdir /s /q dist build
python build.py
```

**Process won't terminate:**
- Should auto-terminate when parent app closes
- Check logs: `type "%APPDATA%\com.sprite.validation\logs\sprite_voice_sidecar.log"`
- Manual kill: `taskkill /F /IM sprite_voice_sidecar.exe`

**Wake word not detected:**
- Ensure the Picovoice key is present in the keychain (or passed via `--pv-access-key`)
- Verify microphone permissions
- Check logs for errors

