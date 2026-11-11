### Goal
Introduce a dedicated, always-on telemetry layer for turn latency and phase timings that remains independent from the logging pipeline.

### Core Requirements
- **Independent channel:** Metrics emission must not depend on log levels. Even if INFO/DEBUG logs are muted, telemetry stays active.
- **Event schema (fixed):**
  - Required fields: `trace_id`, `phase` (`stt|llm|act|tts|e2e`), `event`, `t_mono_ms`
  - Optional fields: `duration_ms`, `ok`, `err`, `metadata` (phase-specific extras such as `trigger`, `cold_start`, `chars`, `audio_ms`)
  - Mandatory runtime events: `turn_start`, `vad_stop`, `stt_done`, `llm_req`, `llm_resp`, `tts_play_start`, `turn_summary`
- **Storage backend:** Append-only JSONL rotated at ~15 MB (configurable). Resolve the target path through `config::metrics_dir_from_handle`; default location is `<AppData>/com.sprite.validation/metrics/latency.jsonl`. One JSON object per line; timestamps are monotonic milliseconds via `metrics::monotonic_now_ms`.
- **Recorder abstraction:** `MetricsHandle` wraps a `JsonlRecorder` created at app start. Each recorder implements `write_event`, `flush`, `get_recent`, and `rotate_if_needed` so we can drop in SQLite/OTLP later.
- **Performance envelope:** Hot paths enqueue events into a bounded channel (default capacity 1024) feeding a single writer thread. When the queue overflows we drop the newest event and emit a structured warning.
- **HUD integration:** The recorder maintains an in-memory ring buffer (200 entries) of `turn_summary` events. Add a Tauri command that fetches these summaries for the sprite shell/control panel HUD.
- **Cold-start awareness:** The first turn after boot is tagged with `cold_start=true`. Subsequent turns inherit the tag when they occur before preload completes or when a phase still warms (extend as needed).
- **Rotation & retention:** Active file rotates at the size threshold and keeps the latest five files (current + four rotated); older artifacts are deleted automatically.
- **Preload signalling:** Continue emitting `stt_preload_status` / `tts_preload_status` UI events so the shell can display warm-up progress alongside latency metrics.
- **Structured logging:** Log queue backpressure, rotation errors, and recorder failures with enough context for on-call triage.

### Implementation Snapshot
- Metrics recorder starts during `lib::run` setup and is managed via Tauri state.
- STT pipeline emits `turn_start`, `vad_stop`, `stt_done`, and `turn_summary` events with shared `trace_id`s. Transcript events now carry `trace_id` for downstream phases.
- LLM dispatch marks `llm_req` / `llm_resp` and forwards the trace id to TTS.
- TTS status updates (`pending`, `synthesizing`, `playing`, `completed`, `error`) drive phase metrics and close out the turn when playback ends or fails.
- On failure (dropped transcript, inference error, TTS enqueue failure) the turn is finalized with `ok=false` and an `err` payload.
- `get_latency_hud` Tauri command exposes recent summaries; the control panel header polls it to render real-time latency chips (new sprite HUD wiring can reuse the same endpoint later).

### Demo Deliverables
- Live latency HUD string, e.g. `STT 0.79 s | LLM 0.31 s | ACT 0.00 s | TTS 0.21 s | E2E 1.45 s`, sourced from the in-memory summaries.
- Single slide summarizing latest JSONL data with `p50/p95` latency per phase and a cold-start vs warm-path comparison.

### Next Steps / Future Expansion
- Hook the sprite overlay into `get_latency_hud` so both shells share the same latency surface.
- Add an optional exporter (SQLite, OTLP) by implementing the recorder trait.
- Correlate latency spikes with preload events by optionally emitting `phase=preload` metrics (flagged off by default).
