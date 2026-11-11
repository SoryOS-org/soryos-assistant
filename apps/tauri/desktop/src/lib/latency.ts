/**
 * Latency HUD formatting — port of v1 `latencyHud.js` formatting helpers.
 */
import type { LatencyHudEntry, LatencyHudSnapshot } from "@/types";

export const LATENCY_HUD_REFRESH_MS = 4000;
export const LATENCY_HUD_LIMIT = 5;

export function formatLatencyDuration(ms: number | undefined | null): string {
  if (typeof ms !== "number" || Number.isNaN(ms) || ms < 0) {
    return "-";
  }
  if (ms >= 1000) {
    const seconds = ms / 1000;
    return seconds >= 10
      ? `${seconds.toFixed(1)} s`
      : `${seconds.toFixed(2)} s`;
  }
  return `${Math.round(ms)} ms`;
}

export function humanizeTrigger(trigger: string | undefined | null): string {
  if (!trigger) {
    return "";
  }
  const normalized = String(trigger).toLowerCase();
  if (normalized === "ptt" || normalized === "push_to_talk") {
    return "PTT";
  }
  if (normalized === "wakeword" || normalized === "wake_word") {
    return "Wake word";
  }
  return normalized
    .split(/[_\s-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

export interface LatencyHudView {
  readout: string;
  meta: string;
  /** Status class hint: `ok` | `error` | `empty`. */
  status: "ok" | "error" | "empty";
  /** Tooltip with the recent history (v1 `buildLatencyHistoryTooltip`). */
  tooltip: string;
}

/** Build the HUD text/tooltip from a `get_latency_hud` snapshot. */
export function formatLatencyHud(snapshot: LatencyHudSnapshot | null): LatencyHudView {
  const empty: LatencyHudView = {
    readout: "Latency: --",
    meta: "Waiting for first turn...",
    status: "empty",
    tooltip: "",
  };

  const latest = snapshot?.latest;
  if (!latest) {
    return empty;
  }

  const phases = latest.phases || {};
  const segments: string[] = [];
  if (typeof phases.capture_ms === "number") {
    segments.push(`Capture ${formatLatencyDuration(phases.capture_ms)}`);
  }
  if (typeof phases.stt_infer_ms === "number") {
    segments.push(`STT ${formatLatencyDuration(phases.stt_infer_ms)}`);
  }
  if (typeof phases.llm_ms === "number") {
    segments.push(`LLM ${formatLatencyDuration(phases.llm_ms)}`);
  }
  // Prefer tts_latency_ms (user-perceived) over tts_ms (legacy)
  if (typeof phases.tts_latency_ms === "number") {
    segments.push(`TTS ${formatLatencyDuration(phases.tts_latency_ms)}`);
  } else if (typeof phases.tts_ms === "number") {
    segments.push(`TTS ${formatLatencyDuration(phases.tts_ms)}`);
  }
  if (typeof latest.system_ms === "number") {
    segments.push(`System ${formatLatencyDuration(latest.system_ms)}`);
  }
  segments.push(`E2E ${formatLatencyDuration(latest.total_ms)}`);

  const metaParts: string[] = [];
  const triggerLabel = humanizeTrigger(latest.trigger);
  if (triggerLabel) {
    metaParts.push(triggerLabel);
  }
  if (latest.cold_start) {
    metaParts.push("Cold start");
  }
  if (typeof phases.to_transcript_ms === "number") {
    metaParts.push(`Transcript ${formatLatencyDuration(phases.to_transcript_ms)}`);
  }
  if (typeof latest.transcript_chars === "number") {
    metaParts.push(`${latest.transcript_chars} chars`);
  }
  if (typeof latest.system_ms === "number") {
    metaParts.push(`System ${formatLatencyDuration(latest.system_ms)}`);
  }
  if (!latest.ok && latest.err) {
    metaParts.push(latest.err);
  }

  const tooltip = buildLatencyHistoryTooltip(snapshot?.entries || []);

  return {
    readout: segments.join(" | "),
    meta: metaParts.join(" | ") || (latest.ok ? "All phases nominal" : "Turn failed"),
    status: latest.ok ? "ok" : "error",
    tooltip,
  };
}

function buildLatencyHistoryTooltip(entries: LatencyHudEntry[]): string {
  if (!Array.isArray(entries) || entries.length === 0) {
    return "";
  }
  return entries
    .map((entry, index) => {
      const scope = index === 0 ? "Latest" : `-${index}`;
      const triggerLabel = humanizeTrigger(entry.trigger);
      const phases = entry.phases || {};
      const parts = [scope];
      if (triggerLabel) {
        parts.push(triggerLabel);
      }
      if (typeof phases.capture_ms === "number") {
        parts.push(`Capture ${formatLatencyDuration(phases.capture_ms)}`);
      }
      if (typeof phases.stt_infer_ms === "number") {
        parts.push(`STT ${formatLatencyDuration(phases.stt_infer_ms)}`);
      }
      if (typeof phases.llm_ms === "number") {
        parts.push(`LLM ${formatLatencyDuration(phases.llm_ms)}`);
      }
      if (typeof phases.tts_infer_ms === "number") {
        parts.push(`TTS infer ${formatLatencyDuration(phases.tts_infer_ms)}`);
      }
      if (typeof phases.tts_latency_ms === "number") {
        parts.push(`TTS latency ${formatLatencyDuration(phases.tts_latency_ms)}`);
      } else if (typeof phases.tts_ms === "number") {
        parts.push(`TTS ${formatLatencyDuration(phases.tts_ms)}`);
      }
      if (typeof phases.tts_duration_ms === "number") {
        parts.push(`TTS playback ${formatLatencyDuration(phases.tts_duration_ms)}`);
      }
      if (typeof entry.system_ms === "number") {
        parts.push(`System ${formatLatencyDuration(entry.system_ms)}`);
      }
      parts.push(`E2E ${formatLatencyDuration(entry.total_ms)}`);
      if (entry.cold_start) {
        parts.push("Cold start");
      }
      if (!entry.ok && entry.err) {
        parts.push(entry.err);
      }
      return parts.join(" | ");
    })
    .join("\n");
}
