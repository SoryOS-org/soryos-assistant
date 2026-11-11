# Quick Reference - Clippy v2

## Common Commands

### Development

```bash
# First time setup
pnpm install
cd apps/voice-service
python setup.py
python build.py
cd ../..

# Start development
pnpm run dev

# Just Tauri app
pnpm run dev:tauri

# Just Worker
pnpm run dev:worker
```

- Vendored Whisper crates live under `third_party/whisper-rs-*`. If a fresh checkout does not contain those directories, copy them from your Cargo registry and follow `docs/issues/whisper_release_patch.md` before running the Rust build.

### Python Voice Service

```bash
cd apps/voice-service

# Install dependencies
pip install -r requirements.txt

# Build executable
python build.py

# Test script directly
python voice_processor.py

# Test executable (needs parent PID)
dist\sprite_voice_sidecar.exe <pid>
```

### Debugging

```bash
# Check if Python sidecar is running
tasklist /FI "IMAGENAME eq sprite_voice_sidecar.exe"

# View Python logs
type "%APPDATA%\com.sprite.validation\logs\sprite_voice_sidecar.log"

# Kill Python process
taskkill /F /IM sprite_voice_sidecar.exe

# Check config
type %APPDATA%\com.sprite.validation\config\stt.config.json
type %APPDATA%\com.sprite.validation\config\mcp.config.json

# Clean Python build
cd apps/voice-service
rmdir /s /q dist build
python build.py
```

### Configuration

**Config Location**: `%APPDATA%\com.sprite.validation\config\stt.config.json`
**MCP Config**: `%APPDATA%\com.sprite.validation\config\mcp.config.json`

**Required Changes**:
```json
{
  "sidecar": {
    "executable_path": "YOUR_PATH\\apps\\voice-service\\dist\\sprite_voice_sidecar.exe",
    "access_key": ""
  }
}
```

### Testing

```bash
# Test wake word
# Say "Hey Google" into microphone

# Test PTT
# Hold Ctrl+Space and speak

# View Rust console
# Check terminal where you ran pnpm run dev

# View frontend logs
# Open browser DevTools console
```

## Project Structure

```
Clippy v2/
├── apps/
│   ├── tauri/                     # Rust Tauri app
│   │   └── src-tauri/
│   │       └── src/
│   │           └── stt/            # STT module
│   │               ├── mod.rs              # Main STT service
│   │               ├── hybrid_service.rs   # Wake word integration
│   │               ├── sidecar.rs          # Python sidecar management
│   │               ├── audio_pipeline.rs   # Audio processing + state machine
│   │               ├── download.rs         # Whisper model downloads
│   │               ├── model_downloader.rs # Model metadata helpers
│   │               └── events.rs           # Event definitions
│   └── voice-service/             # Python wake word service
│       ├── voice_processor.py     # Main entry point
│       ├── wake_word_detector.py  # Porcupine integration
│       ├── communication.py       # IPC handler
│       └── build.py               # Build script
├── packages/
│   └── shared/                    # Shared TypeScript schemas

## Key Files

- **DEVELOPMENT.md**: Full development guide
- **ARCHITECTURE.md**: Current architecture overview (wake word, VAD, whisper flow)
- **CONCURRENT_AUDIO_DESIGN.md**: Concurrent audio design
- **apps/voice-service/README.md**: Python service documentation

## Common Issues & Fixes

| Issue | Fix |
|-------|-----|
| Wake word not working | Check if `sprite_voice_sidecar.exe` is running, verify config |
| Python won't terminate | Should auto-terminate; if it lingers run `taskkill /F /IM sprite_voice_sidecar.exe` |
| Build fails | Kill processes, clean build: `rmdir /s /q dist build` |
| Config not updating | Edit `%APPDATA%\com.sprite.validation\config\stt.config.json` |
| Microphone stuck | Check for orphaned Python processes |

## VAD / STT Tuning Cheatsheet

-  `vad.aggressiveness` 0–3 (higher = more aggressive silence detection). Default `2`. 
-  `vad.start_grace_ms` (default `3000`) lets you pause after the wake word before speaking. 
-  `vad.post_speech_silence_ms` (default `240`) controls how long silence is tolerated once speech has started. 
- For noisy rooms bump aggressiveness to  `3` and `post_speech_silence_ms` to `320–400`. 
- For slower speech raise  `start_grace_ms` to `4000+` and `post_speech_silence_ms` to `400–600`. 
- Extremely quiet mics may need  `aggressiveness = 1` or a lower `MIN_ENERGY_THRESHOLD` in `audio_pipeline.rs`. 

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                      Tauri Desktop App                       │
├─────────────────────────────────────────────────────────────┤
│  Frontend (HTML/JS/CSS)          Backend (Rust)             │
│  • UI                            • Audio Pipeline           │
│  • User Input                    • Whisper STT              │
│  • Display Results               • State Management         │
│                                  • Hotkey Registration      │
└──────────────────────┬───────────────────┬──────────────────┘
                       │                   │
                       │                   │ spawn/monitor
                       │                   ▼
                       │        ┌──────────────────────┐
                       │        │  Python Sidecar      │
                       │        │  • Wake Word (Porcupine)
                       │        │  • PID Monitoring    │
                       │        │  • Self-Termination  │
                       │        └──────────────────────┘
                       │
                       │ HTTP
                       ▼
          ┌─────────────────────────┐
          │  Cloudflare Worker      │
          │  • AI Processing        │
          │  • Intent Recognition   │
          │  • Command Execution    │
          └─────────────────────────┘
```

## Logging

- **Rust**: Console output (stdout)
- **Python**: `sprite_voice_sidecar.log` in `%APPDATA%\com.sprite.validation\logs` (warnings/errors only)
- **Frontend**: Browser DevTools console

## Performance Notes

- **Python Executable**: ~9.8 MB standalone
- **Wake Word Detection**: Continuous, low CPU
- **STT Processing**: Triggered by wake word or PTT
- **Self-Termination Check**: Every 2 seconds
- **Memory**: Efficient audio buffering

