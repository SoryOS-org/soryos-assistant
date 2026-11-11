//! Commands for inspecting latency metrics captured by the recorder.

use tauri::State;

use crate::metrics::{MetricsEvent, MetricsEventKind, MetricsHandle};

#[derive(Debug, Clone, serde::Serialize)]
pub struct LatencyHudSnapshot {
    pub latest: Option<LatencyTurnSummary>,
    pub entries: Vec<LatencyTurnSummary>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LatencyTurnSummary {
    pub trace_id: String,
    pub trigger: String,
    pub cold_start: bool,
    pub total_ms: u64,
    pub system_ms: Option<u64>,
    pub ok: bool,
    pub err: Option<String>,
    pub t_mono_ms: u64,
    pub phases: LatencyPhaseDurations,
    pub transcript_chars: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LatencyPhaseDurations {
    pub stt_infer_ms: Option<u64>,
    pub stt_total_ms: Option<u64>,
    pub capture_ms: Option<u64>,
    pub to_transcript_ms: Option<u64>,
    pub llm_ms: Option<u64>,
    pub tts_infer_ms: Option<u64>,
    pub tts_latency_ms: Option<u64>,
    pub tts_duration_ms: Option<u64>,
    pub tts_ms: Option<u64>,
    pub audio_ms: Option<u64>,
}

impl LatencyTurnSummary {
    fn from_event(event: &MetricsEvent) -> Option<Self> {
        if event.event != MetricsEventKind::TurnSummary {
            return None;
        }

        let metadata = event.metadata.as_ref();
        let get_u64 = |key: &str| -> Option<u64> {
            metadata
                .and_then(|meta| meta.get(key))
                .and_then(|value| value.as_u64())
        };

        let phases = LatencyPhaseDurations {
            capture_ms: get_u64("capture_ms"),
            stt_total_ms: get_u64("stt_total_ms"),
            stt_infer_ms: get_u64("stt_infer_ms"),
            to_transcript_ms: get_u64("to_transcript_ms"),
            llm_ms: get_u64("llm_ms"),
            tts_infer_ms: get_u64("tts_infer_ms"),
            tts_latency_ms: get_u64("tts_latency_ms"),
            tts_duration_ms: get_u64("tts_duration_ms"),
            tts_ms: get_u64("tts_ms"),
            audio_ms: get_u64("audio_ms"),
        };

        Some(Self {
            trace_id: event.trace_id.clone(),
            trigger: metadata
                .and_then(|meta| meta.get("trigger"))
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
                .to_string(),
            cold_start: metadata
                .and_then(|meta| meta.get("cold_start"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
            total_ms: event.duration_ms.unwrap_or(0),
            system_ms: get_u64("system_ms"),
            ok: event.ok.unwrap_or(true),
            err: event.err.clone(),
            t_mono_ms: event.t_mono_ms,
            phases,
            transcript_chars: get_u64("transcript_chars"),
        })
    }
}

/// Collect the most recent turn summaries for the latency HUD.
#[tauri::command]
pub fn get_latency_hud(
    metrics: State<'_, MetricsHandle>,
    limit: Option<usize>,
) -> LatencyHudSnapshot {
    let limit = limit.unwrap_or(5).clamp(1, 20);
    let events = metrics.recent_turn_summaries();
    let mut entries = Vec::new();

    for event in events.into_iter().rev() {
        if entries.len() >= limit {
            break;
        }

        if let Some(summary) = LatencyTurnSummary::from_event(&event) {
            entries.push(summary);
        }
    }

    let latest = entries.first().cloned();

    LatencyHudSnapshot { latest, entries }
}
