const DEFAULT_REFRESH_MS = 4000;

function formatLatencyDuration(ms) {
  if (typeof ms !== "number" || Number.isNaN(ms) || ms < 0) {
    return "-";
  }
  if (ms >= 1000) {
    const seconds = ms / 1000;
    const precision = seconds >= 10 ? 1 : 2;
    return `${seconds.toFixed(precision)} s`;
  }
  return `${Math.round(ms)} ms`;
}

function humanizeTrigger(trigger) {
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
    .split(/[_\s]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function buildLatencyHistoryTooltip(entries) {
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
      // Show split TTS metrics in tooltip: inference time and latency
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

function applyLatencyHudSnapshot(readoutEl, metaEl, snapshot) {
  if (!readoutEl || !metaEl) {
    return;
  }

  const latest = snapshot?.latest;
  if (!latest) {
    readoutEl.textContent = "Latency: --";
    readoutEl.dataset.status = "empty";
    readoutEl.removeAttribute("title");

    metaEl.textContent = "Waiting for first turn...";
    metaEl.dataset.status = "idle";
    metaEl.removeAttribute("title");
    return;
  }

  const phases = latest.phases || {};
  const segments = [];
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
  readoutEl.textContent = segments.join(" | ");
  readoutEl.dataset.status = latest.ok ? "ok" : "error";

  const metaParts = [];
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

  const metaText = metaParts.join(" | ") || (latest.ok ? "All phases nominal" : "Turn failed");
  metaEl.textContent = metaText;
  metaEl.dataset.status = latest.ok ? "ok" : "error";

  const historyTooltip = buildLatencyHistoryTooltip(snapshot?.entries || []);
  if (historyTooltip) {
    readoutEl.title = historyTooltip;
    metaEl.title = historyTooltip;
  } else {
    readoutEl.removeAttribute("title");
    metaEl.removeAttribute("title");
  }
}

export function createLatencyHud({
  readoutEl,
  metaEl,
  invoke,
  refreshMs = DEFAULT_REFRESH_MS,
} = {}) {
  let timerId = null;
  let fetchFailed = false;

  async function refresh() {
    if (!readoutEl || !invoke) {
      return;
    }

    try {
      const snapshot = await invoke("get_latency_hud", { limit: 5 });
      applyLatencyHudSnapshot(readoutEl, metaEl, snapshot);
      fetchFailed = false;
    } catch (error) {
      if (!fetchFailed) {
        console.warn("Failed to fetch latency HUD", error);
      }
      fetchFailed = true;
    }
  }

  function start() {
    if (!readoutEl || !invoke) {
      return;
    }
    if (timerId) {
      clearInterval(timerId);
    }
    refresh();
    timerId = setInterval(refresh, refreshMs);
  }

  function stop() {
    if (timerId) {
      clearInterval(timerId);
      timerId = null;
    }
  }

  return {
    start,
    refresh,
    stop,
  };
}
