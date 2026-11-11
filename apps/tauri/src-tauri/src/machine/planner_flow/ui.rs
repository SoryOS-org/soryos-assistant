use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tracing::warn;

use crate::metrics::MetricsHandle;
use crate::stt::EVENT_STT_RESULT;

pub(super) fn broadcast_reply_if_new(
    app_handle: &AppHandle,
    metrics: Option<&MetricsHandle>,
    trace_id: Option<&str>,
    reply: &str,
    speak: bool,
    planner_context: Option<&str>,
    last_broadcast: &mut Option<String>,
) {
    let trimmed = reply.trim();
    if trimmed.is_empty() {
        return;
    }
    let context_fingerprint = planner_context.and_then(|value| {
        let trimmed_ctx = value.trim();
        if trimmed_ctx.is_empty() {
            None
        } else {
            Some(trimmed_ctx)
        }
    });
    let fingerprint = if let Some(ctx) = context_fingerprint {
        format!("{trimmed}\n\n[context:{ctx}]")
    } else {
        trimmed.to_string()
    };
    if last_broadcast.as_deref() == Some(&fingerprint) {
        return;
    }
    emit_planner_reply_update(app_handle, trimmed, planner_context);
    if speak {
        let _ = enqueue_tts_text(app_handle, metrics, trace_id, trimmed, true);
    }
    *last_broadcast = Some(fingerprint);
}

fn emit_planner_reply_update(app_handle: &AppHandle, reply: &str, planner_context: Option<&str>) {
    let trimmed = reply.trim();
    if trimmed.is_empty() {
        return;
    }

    let payload = json!({
        "reply": trimmed,
        "intents": [],
        "source": "planner",
        "planner_context": planner_context,
    });
    let _ = app_handle.emit(EVENT_STT_RESULT, payload);
}

fn enqueue_tts_text(
    app_handle: &AppHandle,
    metrics: Option<&MetricsHandle>,
    trace_id: Option<&str>,
    text: &str,
    allow_child_trace: bool,
) -> bool {
    let normalized = crate::tts::normalize_for_tts(text);
    if normalized.is_empty() {
        return false;
    }

    if let Some(tts_service) = app_handle.try_state::<crate::tts::TtsService>() {
        match tts_service.enqueue_text(normalized) {
            Ok(utterance_id) => {
                if let (Some(metrics_handle), Some(trace)) = (metrics, trace_id) {
                    metrics_handle.register_utterance(trace, &utterance_id, allow_child_trace);
                }
                true
            }
            Err(err) => {
                warn!("Failed to enqueue initial spoken reply: {err}");
                false
            }
        }
    } else {
        false
    }
}
