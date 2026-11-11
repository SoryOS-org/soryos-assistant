# Whisper Resources

The Tauri bundler packages every file under this directory, so whichever Whisper model is configured will be shipped automatically.

The build script uses the default model from `defaults.rs` (e.g. `resources/whisper/ggml-base.en.bin`) and downloads it on first build unless `SKIP_WHISPER_MODEL_DOWNLOAD=1`.

If you provide the model manually, drop it under `resources/whisper/` using the same filename configured in `defaults.rs` before running `pnpm tauri:build`.
