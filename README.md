# Sprite Assistant - Voice-First Desktop App

A local-first AI voice assistant built with Tauri (Rust) + a Python voice sidecar, supporting wake-word + push-to-talk, on-device Whisper STT, Piper TTS, and MCP-based tool integrations.

[![Desktop Installer](https://github.com/tulayha/sprite-assistant/actions/workflows/desktop-installer.yml/badge.svg)](https://github.com/tulayha/sprite-assistant/actions/workflows/desktop-installer.yml)
[![Worker Tests](https://github.com/tulayha/sprite-assistant/actions/workflows/worker-tests.yml/badge.svg)](https://github.com/tulayha/sprite-assistant/actions/workflows/worker-tests.yml)

> This repository reflects an actively evolving personal project. Some documents represent exploratory design notes or partial implementations and may not reflect the current state of the code.

## Contents
- [Components](#components)
- [Features](#features)
- [Known Limitations](#known-limitations)
- [Development Workflow](#development-workflow)
- [Third-party components](#third-party-components)
- [Documentation](#documentation)

## Components
- **Desktop Sprite ([`apps/tauri`](apps/tauri))** - Tauri desktop app with the worker bridge; see the [desktop guide](apps/tauri/README.md) for setup and metrics.
- **Edge Worker ([`apps/worker`](apps/worker))** - Cloudflare Worker (Hono) mediating LLM access for `/chat`; see the [worker guide](apps/worker/README.md) for configuration and deployment.
- **Voice Service ([`apps/voice-service`](apps/voice-service))** - Python sidecar for hybrid voice processing (wake-word detection, VAD, Whisper STT).
- **MCP Host ([`crates/mcp-host`](crates/mcp-host))** - Rust host that runs MCP servers over stdio (ws/http transports stubbed behind feature flags), surfaces schemas and tools to the desktop UI and worker bridge.
- **Shared Schemas ([`packages/shared`](packages/shared))** - TypeScript contracts (Zod) reused by both surfaces.

### Workspace Layout
```
apps/
  tauri/          # desktop assistant proof-of-concept
  worker/         # Cloudflare Worker entry point
  voice-service/  # Python sidecar for voice processing
packages/
  shared/         # Reply schema/types for cross-app communication
```

## Features

### Voice & Audio
- **Wake Word + PTT**: Porcupine sidecar with configurable keywords and a global `Ctrl+Space` hotkey that share the same capture pipeline.
- **WebRTC VAD**: Tunable grace/activation/hold windows to balance responsiveness and noise rejection.
- **Whisper STT**: Local models are downloaded and preloaded automatically for on-device transcription.
- **Piper TTS**: Chunked local synthesis with configurable playback.

### Conversation & Tools
- **Worker Bridge**: `/chat` endpoint validated against the shared `ReplySchema`, including `planner_context` for MCP hand-offs.
- **MCP Host**: Built-in stdio host with lifecycle telemetry, registry view, and UI-facing registry access. WS/HTTP transports exist only as stubs.
- **Intent Dispatch**: Worker replies can trigger tool calls and app actions; summaries surface in the desktop logs and chat transcript.

### Control Panel
- **Multi-view UI**: Chat + tool rail, MCP management, config editor, preferences, logs, and about views.
- **Live Editing**: STT edits hot-reload; TTS/MCP changes require a restart.
- **Status & Warnings**: Startup errors surface in the sprite and panel until cleared.
- **Latency HUD**: End-to-end latency readout and preload status.
- **Runtime Logs**: Latest app logs are tailed in the panel (release builds); dev mode streams to stderr.

## Known Limitations
- Wake-word path requires a Picovoice key; without it the app runs in push-to-talk mode only.
- MCP transports beyond stdio (ws/http) are stubbed; selecting them will return unimplemented errors.
- TTS status events are emitted, but the panel doesn't yet show TTS status; check logs if playback fails.
- Download integrity/verification is minimal (no checksums/signatures on Whisper/Piper assets).
- Windows SmartScreen may warn on install due to unsigned binaries.

## Development Workflow
### Install (Release Builds)
- Download the Windows installer (`*.exe`, NSIS) from GitHub Releases or CI artifacts.
- Run the installer; on first launch the app creates config files under `%APPDATA%\\com.sprite.validation\\config\\`.
- Open the panel to set the Worker Auth Token (required for worker calls; temporary, will be replaced by user auth) and Picovoice key (for wake word).
- Picovoice keys are created in the [Picovoice console](https://console.picovoice.ai/).
- Dev builds create configs under `apps/tauri/src-tauri/.dev/config` (no repo defaults are bundled).

### Prerequisites
- Node.js 18+
- Python 3 with build tools required by Porcupine/audio dependencies (`pnpm install:voice` sets up deps)
- Cloudflare AI Gateway credentials for the worker (`AI_GATEWAY_URL` / `AI_GATEWAY_TOKEN` at minimum)
- Worker auth token shared between desktop and worker (`WORKER_AUTH_TOKEN`)

### Dev scripts
Run `pnpm install:voice` once on a clean machine.
```bash
pnpm dev           # worker + tauri (uses start-dev.*)
pnpm dev:worker    # worker only (requires AI gateway creds)
pnpm dev:tauri     # desktop only (expects worker)
pnpm install:voice # install Python deps
pnpm build:voice   # build sidecar exe (requires install:voice)
pnpm build:tauri   # production build
pnpm build:full    # full build (stops on first failure)
pnpm deploy:worker # deploy worker
pnpm test:voice    # win-only smoke tests (rebuilds exe)
```

### CI builds
- Rust fmt/clippy/tests run on pushes to `main` and pull requests that touch relevant code.
- The Windows installer is built only on version tags or manual workflow dispatch, and uploads as a workflow artifact.
- Versioned tags (e.g., `v0.5.0`) publish the installer to GitHub Releases.

### Assets & Config
- STT/TTS/MCP config files are generated on first run; see the [desktop guide](apps/tauri/README.md) for exact locations and examples.
- Whisper/Piper assets are managed by the desktop app; see the [architecture overview](ARCHITECTURE.md) for details.

## Third-party components
- Whisper (whisper.cpp) bindings: see [third_party/whisper-rs-0.15.1/README.md](third_party/whisper-rs-0.15.1/README.md), [BUILDING.md](third_party/whisper-rs-0.15.1/BUILDING.md), [CHANGELOG.md](third_party/whisper-rs-0.15.1/CHANGELOG.md).
- Piper TTS assets: see [apps/tauri/src-tauri/resources/piper/LICENSE.md](apps/tauri/src-tauri/resources/piper/LICENSE.md).

## Documentation

- **Architecture Overview**: [ARCHITECTURE.md](ARCHITECTURE.md)
- **Desktop App**: [apps/tauri/README.md](apps/tauri/README.md)
- **Edge Worker**: [apps/worker/README.md](apps/worker/README.md)
- **Voice Service Setup**: [apps/voice-service/README.md](apps/voice-service/README.md)
