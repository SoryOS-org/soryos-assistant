# STT Release Build Latency Discrepancy

## Context
- App: Sprite Assistant (Tauri) speech-to-text pipeline using Whisper `ggml-base.en.bin`.
- Hardware: 12th Gen Intel i7-12700H, telemetry level `debug` to surface whisper.cpp logs.
- Config: Production build reads `%APPDATA%\com.sprite.validation\config\stt.config.json`; dev build uses `apps/tauri/src-tauri/stt.config.json`. Both load the same CPU-only Whisper setup (GPU disabled, identical thread heuristics).

## Steps to Reproduce
1. Run the packaged app (`pnpm tauri build`, install, then launch the binary).
2. Trigger wake-word capture and speak a short phrase (e.g., "How are you doing today?").
3. Repeat the same flow while running `pnpm tauri dev`.

## Observed Behaviour
- **Production build:** `Starting Whisper inference...` 18:24:48.533Z -> `Transcript...` 18:24:54.362Z (~5.8 s inside `whisper_state.full`).
- **Dev build:** `Starting Whisper inference...` 10:59:51.463Z -> `Transcript...` 10:59:53.112Z (~1.65 s for the same utterance).
- Earlier stages (model load, wake word, VAD, buffer allocation, thread count = 7) are identical.

## Validated / Ruled Out
- Logging order: whisper logging hooks installed before model load, so timestamps measure actual compute time.
- GPU path: Both builds log `use gpu = 0` and `whisper_backend_init_gpu: no GPU found`.
- Config drift: Telemetry, VAD, model, and thread settings match across builds.
- Buffer warm-up: GGML allocates work buffers on first inference; later calls reuse them, yet the release run stays slower.
- Compiler flags: `cargo rustc --print cfg` shows release and dev builds expose the same CPU features; only `debug_assertions` differs.

## Expectation
Release and dev builds should exhibit comparable decode latency given identical hardware and workload. Instead, the packaged binary consistently spends ~4 s longer inside Whisper's decode step.

## Next Actions / Data Requests
- Capture CPU frequency / throttling metrics during both runs to rule out power limits.
- Test a second inference in each build to confirm whether the discrepancy only affects the first decode.
- Check for antivirus scans against `%APPDATA%\com.sprite.validation\models`.
- Applied vendor workaround: see `docs/issues/whisper_release_patch.md` for the local optimisation flags added to `whisper-rs`.
