use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_channel::{bounded, Sender};
use serde_json::json;
use tauri::AppHandle;
use tracing::warn;

use super::recorder::JsonlRecorder;
use super::time::monotonic_now_ms;
use super::worker::{spawn_writer, MetricsCommand, MetricsWorker};
use super::{
    MetricsEvent, MetricsEventKind, MetricsPhase, MetricsStats, TurnTrigger, DEFAULT_MAX_BYTES,
    DEFAULT_RETENTION, METRICS_FILENAME, QUEUE_CAPACITY, SUMMARY_CACHE_CAPACITY,
};
static TURN_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct MetricsHandle {
    sender: Sender<MetricsCommand>,
    turn_summaries: Arc<Mutex<VecDeque<MetricsEvent>>>,
    worker: Arc<MetricsWorker>,
    turns: Arc<Mutex<TurnRegistry>>,
    first_turn_seen: Arc<AtomicBool>,
    #[allow(dead_code)]
    // Atomically tracks write failures for later inspection via `stats`.
    write_errors: Arc<AtomicU64>,
    dropped_events: Arc<AtomicU64>,
}

#[derive(Default)]
struct TurnRegistry {
    turns: HashMap<String, TurnState>,
    utterance_index: HashMap<String, String>,
    reusable_turn: Option<(String, TurnState)>,
    discardable: std::collections::HashSet<String>,
}

#[derive(Debug, Clone)]
struct TurnState {
    trigger: TurnTrigger,
    cold_start: bool,
    start_ms: u64,
    audio_duration_ms: Option<u64>,
    transcript_chars: Option<usize>,
    vad_stop_ms: Option<u64>,
    stt_infer_start_ms: Option<u64>,
    stt_done_ms: Option<u64>,
    llm_req_ms: Option<u64>,
    llm_resp_ms: Option<u64>,
    tts_infer_start_ms: Option<u64>,
    tts_audio_ready_ms: Option<u64>,
    tts_play_start_ms: Option<u64>,
    tts_complete_ms: Option<u64>,
    ok: Option<bool>,
    err: Option<String>,
    utterances: Vec<String>,
    child_span: bool,
}

impl TurnState {
    fn snapshot(&self) -> Self {
        Self {
            trigger: self.trigger,
            cold_start: self.cold_start,
            start_ms: monotonic_now_ms(),
            audio_duration_ms: None,
            transcript_chars: None,
            vad_stop_ms: None,
            stt_infer_start_ms: None,
            stt_done_ms: None,
            llm_req_ms: None,
            llm_resp_ms: None,
            tts_infer_start_ms: None,
            tts_audio_ready_ms: None,
            tts_play_start_ms: None,
            tts_complete_ms: None,
            ok: None,
            err: None,
            utterances: Vec::new(),
            child_span: false,
        }
    }

    fn child_span() -> Self {
        Self {
            trigger: TurnTrigger::Ptt,
            cold_start: false,
            start_ms: monotonic_now_ms(),
            audio_duration_ms: None,
            transcript_chars: None,
            vad_stop_ms: None,
            stt_infer_start_ms: None,
            stt_done_ms: None,
            llm_req_ms: None,
            llm_resp_ms: None,
            tts_infer_start_ms: None,
            tts_audio_ready_ms: None,
            tts_play_start_ms: None,
            tts_complete_ms: None,
            ok: None,
            err: None,
            utterances: Vec::new(),
            child_span: true,
        }
    }
}

impl MetricsHandle {
    pub fn start(app_handle: &AppHandle) -> Result<Self> {
        let metrics_dir = crate::config::prelude::metrics_dir_from_handle(app_handle);
        let active_path = metrics_dir.join(METRICS_FILENAME);

        let recorder = JsonlRecorder::open(active_path, DEFAULT_MAX_BYTES, DEFAULT_RETENTION)?;
        let summaries = Arc::new(Mutex::new(VecDeque::with_capacity(SUMMARY_CACHE_CAPACITY)));
        let turns = Arc::new(Mutex::new(TurnRegistry::default()));

        let (sender, receiver) = bounded(QUEUE_CAPACITY);
        let write_errors = Arc::new(AtomicU64::new(0));
        let dropped_events = Arc::new(AtomicU64::new(0));
        let worker = spawn_writer(recorder, receiver, write_errors.clone());

        Ok(Self {
            sender,
            turn_summaries: summaries,
            worker: Arc::new(worker),
            turns,
            first_turn_seen: Arc::new(AtomicBool::new(false)),
            write_errors,
            dropped_events,
        })
    }

    pub fn begin_turn(&self, trigger: TurnTrigger) -> String {
        let now = monotonic_now_ms();
        let trace_id = format!("turn-{}", TURN_COUNTER.fetch_add(1, Ordering::Relaxed));
        let cold_start = !self.first_turn_seen.swap(true, Ordering::Relaxed);

        {
            let mut registry = self.turns.lock().unwrap();
            registry.turns.insert(
                trace_id.clone(),
                TurnState {
                    trigger,
                    cold_start,
                    start_ms: now,
                    audio_duration_ms: None,
                    transcript_chars: None,
                    vad_stop_ms: None,
                    stt_infer_start_ms: None,
                    stt_done_ms: None,
                    llm_req_ms: None,
                    llm_resp_ms: None,
                    tts_infer_start_ms: None,
                    tts_audio_ready_ms: None,
                    tts_play_start_ms: None,
                    tts_complete_ms: None,
                    ok: None,
                    err: None,
                    utterances: Vec::new(),
                    child_span: false,
                },
            );
        }

        let event = MetricsEvent::new(
            trace_id.clone(),
            MetricsPhase::E2e,
            MetricsEventKind::TurnStart,
        )
        .with_metadata(json!({
            "trigger": trigger.as_str(),
            "cold_start": cold_start,
        }));
        self.record(event);

        trace_id
    }

    pub fn turn_trigger(&self, trace_id: &str) -> Option<TurnTrigger> {
        let registry = self.turns.lock().unwrap();
        registry.turns.get(trace_id).map(|state| state.trigger)
    }

    pub fn mark_discardable(&self, trace_id: &str) {
        let mut registry = self.turns.lock().unwrap();
        registry.discardable.insert(trace_id.to_string());
    }

    pub fn clear_discardable(&self, trace_id: &str) {
        let mut registry = self.turns.lock().unwrap();
        registry.discardable.remove(trace_id);
    }

    /// Drop a turn entirely without emitting a summary or error.
    pub fn discard_turn(&self, trace_id: &str) {
        let mut registry = self.turns.lock().unwrap();
        registry.discardable.remove(trace_id);
        if let Some(state) = registry.turns.remove(trace_id) {
            for utterance in &state.utterances {
                registry.utterance_index.remove(utterance);
            }
            if let Some((cached_id, _)) = &registry.reusable_turn {
                if cached_id == trace_id {
                    registry.reusable_turn = None;
                }
            }
        }
    }

    pub fn mark_vad_stop(&self, trace_id: &str) {
        let event_opt = {
            let mut registry = self.turns.lock().unwrap();
            let Some(state) = registry.turns.get_mut(trace_id) else {
                return;
            };
            if state.vad_stop_ms.is_some() {
                return;
            }
            let now = monotonic_now_ms();
            state.vad_stop_ms = Some(now);
            let duration = now.saturating_sub(state.start_ms);
            Some(
                MetricsEvent::new(
                    trace_id.to_string(),
                    MetricsPhase::Stt,
                    MetricsEventKind::VadStop,
                )
                .with_duration(duration)
                .with_metadata(json!({
                    "trigger": state.trigger.as_str(),
                    "cold_start": state.cold_start,
                })),
            )
        };

        if let Some(event) = event_opt {
            self.record(event);
        }
    }

    pub fn mark_stt_inference_start(&self, trace_id: &str) {
        let mut registry = self.turns.lock().unwrap();
        if let Some(state) = registry.turns.get_mut(trace_id) {
            if state.stt_infer_start_ms.is_none() {
                state.stt_infer_start_ms = Some(monotonic_now_ms());
            }
        }
    }

    pub fn mark_stt_done(
        &self,
        trace_id: &str,
        transcript_chars: usize,
        audio_duration_ms: Option<u64>,
        ok: bool,
        err: Option<&str>,
    ) {
        let event_opt = {
            let mut registry = self.turns.lock().unwrap();
            let Some(state) = registry.turns.get_mut(trace_id) else {
                return;
            };
            let now = monotonic_now_ms();
            state.stt_done_ms = Some(now);
            state.transcript_chars = Some(transcript_chars);
            state.audio_duration_ms = audio_duration_ms;
            state.ok = Some(ok && state.ok.unwrap_or(true));
            if let Some(err_msg) = err {
                state.err = Some(err_msg.to_string());
            }

            let duration = match state.stt_infer_start_ms {
                Some(start) => now.saturating_sub(start),
                None => now.saturating_sub(state.start_ms),
            };

            let mut event = MetricsEvent::new(
                trace_id.to_string(),
                MetricsPhase::Stt,
                MetricsEventKind::SttDone,
            )
            .with_duration(duration)
            .with_ok(ok)
            .with_metadata(json!({
                "trigger": state.trigger.as_str(),
                "cold_start": state.cold_start,
                "chars": transcript_chars,
                "audio_ms": audio_duration_ms,
            }));

            if let Some(err_msg) = err {
                event = event.with_err(err_msg.to_string());
            }

            Some(event)
        };

        if let Some(event) = event_opt {
            self.record(event);
        }

        if !ok {
            self.finalize_turn(trace_id, false, err);
        }
    }

    pub fn finalize_turn(&self, trace_id: &str, ok: bool, err: Option<&str>) {
        let event_opt = {
            let mut registry = self.turns.lock().unwrap();
            let discardable = registry.discardable.remove(trace_id);
            let Some(mut state) = registry.turns.remove(trace_id) else {
                return;
            };
            if discardable {
                for utterance in &state.utterances {
                    registry.utterance_index.remove(utterance);
                }
                if let Some((cached_id, _)) = &registry.reusable_turn {
                    if cached_id == trace_id {
                        registry.reusable_turn = None;
                    }
                }
                return;
            }
            let err_message = err.map(|value| value.to_string()).or(state.err.clone());
            state.ok = Some(ok && state.ok.unwrap_or(true));
            state.err = err_message.clone();

            for utterance in &state.utterances {
                registry.utterance_index.remove(utterance);
            }

            let final_ms = state
                .tts_complete_ms
                .or(state.llm_resp_ms)
                .unwrap_or_else(monotonic_now_ms);
            let total_ms = final_ms.saturating_sub(state.start_ms);
            let capture_ms = state
                .vad_stop_ms
                .map(|value| value.saturating_sub(state.start_ms));
            let stt_total_ms = state
                .stt_done_ms
                .map(|value| value.saturating_sub(state.start_ms));
            let stt_infer_ms = match (state.stt_infer_start_ms, state.stt_done_ms) {
                (Some(start), Some(end)) => Some(end.saturating_sub(start)),
                _ => None,
            };
            let llm_ms = match (state.llm_req_ms, state.llm_resp_ms) {
                (Some(start), Some(end)) => Some(end.saturating_sub(start)),
                _ => None,
            };
            // TTS inference time (compute only)
            let tts_infer_ms = match (state.tts_infer_start_ms, state.tts_audio_ready_ms) {
                (Some(start), Some(end)) => Some(end.saturating_sub(start)),
                _ => None,
            };
            // TTS latency (user-perceived: inference start to first audible frame)
            let tts_latency_ms = match (state.tts_infer_start_ms, state.tts_play_start_ms) {
                (Some(start), Some(end)) => Some(end.saturating_sub(start)),
                _ => None,
            };
            // TTS playback duration (NOT latency - this is just how long it's speaking)
            let tts_duration_ms = match (state.tts_play_start_ms, state.tts_complete_ms) {
                (Some(start), Some(end)) => Some(end.saturating_sub(start)),
                _ => None,
            };
            // Legacy tts_ms kept for compatibility (now equals tts_latency_ms)
            let tts_ms = tts_latency_ms;
            let to_transcript_ms = match (capture_ms, stt_infer_ms) {
                (Some(capture), Some(infer)) => Some(capture.saturating_add(infer)),
                (Some(capture), None) => Some(capture),
                (None, Some(infer)) => Some(infer),
                (None, None) => None,
            };
            // System time includes inference time, not playback duration
            let system_ms_value = stt_infer_ms
                .unwrap_or(0)
                .saturating_add(llm_ms.unwrap_or(0))
                .saturating_add(tts_latency_ms.unwrap_or(0));
            let system_ms =
                if stt_infer_ms.is_some() || llm_ms.is_some() || tts_latency_ms.is_some() {
                    Some(system_ms_value)
                } else {
                    None
                };

            let summary_event = if !state.child_span {
                let mut event = MetricsEvent::new(
                    trace_id.to_string(),
                    MetricsPhase::E2e,
                    MetricsEventKind::TurnSummary,
                )
                .with_duration(total_ms)
                .with_ok(state.ok.unwrap_or(ok))
                .with_metadata(json!({
                    "trigger": state.trigger.as_str(),
                    "cold_start": state.cold_start,
                    "capture_ms": capture_ms,
                    "stt_total_ms": stt_total_ms,
                    "stt_infer_ms": stt_infer_ms,
                    "llm_ms": llm_ms,
                    "tts_infer_ms": tts_infer_ms,
                    "tts_latency_ms": tts_latency_ms,
                    "tts_duration_ms": tts_duration_ms,
                    "tts_ms": tts_ms,
                    "to_transcript_ms": to_transcript_ms,
                    "system_ms": system_ms,
                    "audio_ms": state.audio_duration_ms,
                    "transcript_chars": state.transcript_chars,
                }));

                if let Some(err_msg) = err_message {
                    event = event.with_err(err_msg);
                }

                Some(event)
            } else {
                None
            };

            registry.cache_reusable_turn(trace_id, &state);

            summary_event
        };

        if let Some(event) = event_opt {
            self.record(event);
        }
    }

    pub fn mark_llm_request(&self, trace_id: &str) {
        let event_opt = {
            let mut registry = self.turns.lock().unwrap();
            let Some(state) = registry.turns.get_mut(trace_id) else {
                return;
            };
            let now = monotonic_now_ms();
            state.llm_req_ms = Some(now);
            Some(
                MetricsEvent::new(
                    trace_id.to_string(),
                    MetricsPhase::Llm,
                    MetricsEventKind::LlmReq,
                )
                .with_metadata(json!({
                    "trigger": state.trigger.as_str(),
                    "cold_start": state.cold_start,
                })),
            )
        };

        if let Some(event) = event_opt {
            self.record(event);
        }
    }

    pub fn mark_llm_response(&self, trace_id: &str, ok: bool, err: Option<&str>) {
        let event_opt = {
            let mut registry = self.turns.lock().unwrap();
            let Some(state) = registry.turns.get_mut(trace_id) else {
                return;
            };
            let now = monotonic_now_ms();
            state.llm_resp_ms = Some(now);
            state.ok = Some(ok && state.ok.unwrap_or(true));
            if let Some(err_msg) = err {
                state.err = Some(err_msg.to_string());
            }
            let duration = match state.llm_req_ms {
                Some(start) => now.saturating_sub(start),
                None => 0,
            };

            let mut event = MetricsEvent::new(
                trace_id.to_string(),
                MetricsPhase::Llm,
                MetricsEventKind::LlmResp,
            )
            .with_duration(duration)
            .with_ok(ok)
            .with_metadata(json!({
                "trigger": state.trigger.as_str(),
                "cold_start": state.cold_start,
            }));

            if let Some(err_msg) = err {
                event = event.with_err(err_msg.to_string());
            }

            Some(event)
        };

        if let Some(event) = event_opt {
            self.record(event);
        }

        if !ok {
            self.finalize_turn(trace_id, false, err);
        }
    }

    pub fn register_utterance(&self, trace_id: &str, utterance_id: &str, allow_child: bool) {
        let mut registry = self.turns.lock().unwrap();
        if !registry.turns.contains_key(trace_id) && allow_child {
            registry.promote_reusable_turn(trace_id);
        }
        registry.discardable.remove(trace_id);

        if let Some(state) = registry.turns.get_mut(trace_id) {
            state.utterances.push(utterance_id.to_string());
            registry
                .utterance_index
                .insert(utterance_id.to_string(), trace_id.to_string());
        } else {
            warn!(
                "Attempted to register TTS utterance {} for unknown trace {}",
                utterance_id, trace_id
            );
        }
    }

    pub fn mark_tts_status(&self, utterance_id: &str, stage: &str, err: Option<&str>) {
        let trace_id_opt = {
            let registry = self.turns.lock().unwrap();
            registry.utterance_index.get(utterance_id).cloned()
        };

        let Some(trace_id) = trace_id_opt else {
            return;
        };

        match stage {
            "synthesizing" => {
                let event_opt = {
                    let mut registry = self.turns.lock().unwrap();
                    let Some(state) = registry.turns.get_mut(&trace_id) else {
                        return;
                    };
                    // Only mark inference start on the first synthesizing event
                    if state.tts_infer_start_ms.is_none() {
                        let now = monotonic_now_ms();
                        state.tts_infer_start_ms = Some(now);
                        Some(
                            MetricsEvent::new(
                                trace_id.clone(),
                                MetricsPhase::Tts,
                                MetricsEventKind::TtsInferStart,
                            )
                            .with_metadata(json!({
                                "trigger": state.trigger.as_str(),
                                "cold_start": state.cold_start,
                                "utterance_id": utterance_id,
                            })),
                        )
                    } else {
                        None
                    }
                };

                if let Some(event) = event_opt {
                    self.record(event);
                }
            }
            "playing" => {
                let event_opt = {
                    let mut registry = self.turns.lock().unwrap();
                    let Some(state) = registry.turns.get_mut(&trace_id) else {
                        return;
                    };
                    let now = monotonic_now_ms();
                    // Mark audio ready on first playing event (audio file/buffer ready)
                    if state.tts_audio_ready_ms.is_none() {
                        state.tts_audio_ready_ms = Some(now);
                        // Emit TtsAudioReady event
                        self.record(
                            MetricsEvent::new(
                                trace_id.clone(),
                                MetricsPhase::Tts,
                                MetricsEventKind::TtsAudioReady,
                            )
                            .with_metadata(json!({
                                "trigger": state.trigger.as_str(),
                                "cold_start": state.cold_start,
                                "utterance_id": utterance_id,
                            })),
                        );
                    }
                    // Mark play start (first audible frame)
                    if state.tts_play_start_ms.is_none() {
                        state.tts_play_start_ms = Some(now);
                        Some(
                            MetricsEvent::new(
                                trace_id.clone(),
                                MetricsPhase::Tts,
                                MetricsEventKind::TtsPlayStart,
                            )
                            .with_metadata(json!({
                                "trigger": state.trigger.as_str(),
                                "cold_start": state.cold_start,
                                "utterance_id": utterance_id,
                            })),
                        )
                    } else {
                        None
                    }
                };

                if let Some(event) = event_opt {
                    self.record(event);
                }
            }
            "completed" => {
                {
                    let mut registry = self.turns.lock().unwrap();
                    if let Some(state) = registry.turns.get_mut(&trace_id) {
                        state.tts_complete_ms = Some(monotonic_now_ms());
                    }
                }
                self.finalize_turn(&trace_id, true, None);
            }
            "error" => {
                {
                    let mut registry = self.turns.lock().unwrap();
                    if let Some(state) = registry.turns.get_mut(&trace_id) {
                        state.tts_complete_ms = Some(monotonic_now_ms());
                        state.ok = Some(false);
                        if let Some(err_msg) = err {
                            state.err = Some(err_msg.to_string());
                        }
                    }
                }
                self.finalize_turn(&trace_id, false, err);
            }
            _ => {}
        }
    }

    pub fn record(&self, event: MetricsEvent) {
        if event.event == MetricsEventKind::TurnSummary {
            if let Ok(mut guard) = self.turn_summaries.lock() {
                guard.push_back(event.clone());
                if guard.len() > SUMMARY_CACHE_CAPACITY {
                    guard.pop_front();
                }
            }
        }

        let trace_id = event.trace_id.clone();
        let kind = event.event;
        if let Err(err) = self.sender.try_send(MetricsCommand::Event(event)) {
            self.dropped_events.fetch_add(1, Ordering::Relaxed);
            warn!(
                "Dropping metrics event {:?} for trace {} due to channel backpressure: {err}",
                kind, trace_id
            );
        }
    }

    /// Force a flush of pending metrics writes.
    /// This ensures data is persisted to disk immediately.
    #[allow(dead_code)]
    pub fn flush(&self) {
        let _ = self.sender.try_send(MetricsCommand::Flush);
    }

    /// Get statistics about metrics recording.
    #[allow(dead_code)]
    pub fn stats(&self) -> MetricsStats {
        MetricsStats {
            write_errors: self.write_errors.load(Ordering::Relaxed),
            dropped_events: self.dropped_events.load(Ordering::Relaxed),
        }
    }

    pub fn recent_turn_summaries(&self) -> Vec<MetricsEvent> {
        let guard = self.turn_summaries.lock().unwrap();
        guard.iter().cloned().collect()
    }
}

impl TurnRegistry {
    fn promote_reusable_turn(&mut self, trace_id: &str) {
        if let Some((cached_id, mut cached_state)) = self.reusable_turn.take() {
            if cached_id == trace_id {
                cached_state.child_span = true;
                cached_state.start_ms = monotonic_now_ms();
                cached_state.tts_infer_start_ms = None;
                cached_state.tts_audio_ready_ms = None;
                cached_state.tts_play_start_ms = None;
                cached_state.tts_complete_ms = None;
                cached_state.ok = None;
                cached_state.err = None;
                cached_state.utterances.clear();
                self.discardable.remove(trace_id);
                self.turns.insert(trace_id.to_string(), cached_state);
                return;
            } else {
                self.reusable_turn = Some((cached_id, cached_state));
            }
        }

        self.turns
            .insert(trace_id.to_string(), TurnState::child_span());
    }

    fn cache_reusable_turn(&mut self, trace_id: &str, state: &TurnState) {
        self.reusable_turn = Some((trace_id.to_string(), state.snapshot()));
    }
}

impl Drop for MetricsHandle {
    fn drop(&mut self) {
        if Arc::strong_count(&self.worker) == 1 {
            let _ = self.sender.send_blocking(MetricsCommand::Shutdown);
        }
    }
}
