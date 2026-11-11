import { friendlyLabel } from "../catalog.js";
import { configState } from "../state.js";

function setFieldText(fields, id, value) {
  const element = fields(id);
  if (!element) {
    return;
  }
  const textValue = value ?? "-";
  element.textContent = textValue;
  if (typeof textValue === "string" && textValue.trim()) {
    element.title = textValue;
  } else {
    element.removeAttribute("title");
  }
}

function formatBoolean(value, affirmative = "Enabled", negative = "Disabled") {
  return value ? affirmative : negative;
}

function formatProvider(value = "") {
  if (!value) {
    return "Unknown";
  }
  return value.charAt(0).toUpperCase() + value.slice(1).toLowerCase();
}

function formatLanguage(language) {
  if (!language) {
    return "-";
  }
  if (!language.auto) {
    return language.default || "Manual";
  }
  return language.default ? `${language.default} (auto)` : "Auto detect";
}

function formatWakeword(wakeword = {}) {
  if (!wakeword.keyword) {
    return "...";
  }
  return wakeword.keyword;
}

function formatVad(vad = {}) {
  const agg = vad.aggressiveness ?? "?";
  const grace = vad.start_grace_ms != null ? `${vad.start_grace_ms}ms` : "?";
  const activation = vad.activation_ms != null ? `${vad.activation_ms}ms` : "?";
  const hold = vad.post_speech_silence_ms != null ? `${vad.post_speech_silence_ms}ms` : "?";
  return `Aggressive ${agg}  Grace ${grace}  Activate ${activation}  Hold ${hold}`;
}

function formatHotkey(hotkey = {}) {
  return hotkey.push_to_talk || "...";
}

function formatSpriteScale(sprite = {}) {
  if (!sprite || typeof sprite.scale !== "number") {
    return "100% (range 20-150%)";
  }
  const current = Math.round(sprite.scale * 100);
  const min = Number.isFinite(sprite.min_scale) ? Math.round(sprite.min_scale * 100) : null;
  const max = Number.isFinite(sprite.max_scale) ? Math.round(sprite.max_scale * 100) : null;
  if (min != null && max != null) {
    return `${current}% (range ${min}-${max}%)`;
  }
  return `${current}%`;
}

function formatWhisperThreads(whisper = {}) {
  const configured = Number.isFinite(whisper.threads) ? whisper.threads : null;
  const recommended = Number.isFinite(whisper.recommended_threads)
    ? whisper.recommended_threads
    : null;
  const physical = Number.isFinite(whisper.physical_cores) ? whisper.physical_cores : null;

  if (configured === null && recommended === null) {
    return "Auto";
  }

  const configuredLabel = configured === null ? "Auto" : configured.toString();
  if (recommended !== null && physical !== null) {
    return `${configuredLabel} (recommended ${recommended}, physical ${physical})`;
  }
  if (recommended !== null) {
    return `${configuredLabel} (recommended ${recommended})`;
  }
  return configuredLabel;
}

export function renderSummary(fields, summary) {
  if (!summary || !summary.stt) {
    return;
  }
  configState.summary = summary;
  configState.loaded = true;

  setFieldText(fields, "stt-provider", formatProvider(summary.stt.provider));
  if (summary.stt.model) {
    const modelId = summary.stt.model.id || "-";
    setFieldText(fields, "stt-model-friendly", friendlyLabel(modelId));
    setFieldText(fields, "stt-model-path", summary.stt.model.path || "-");
    setFieldText(fields, "stt-language", formatLanguage(summary.stt.model.language));
    setFieldText(
      fields,
      "stt-punctuation",
      formatBoolean(summary.stt.model.punctuation, "On", "Off"),
    );
  } else {
    setFieldText(fields, "stt-model-friendly", "-");
    setFieldText(fields, "stt-model-path", "-");
    setFieldText(fields, "stt-language", "-");
    setFieldText(fields, "stt-punctuation", "-");
  }
  setFieldText(fields, "stt-wakeword", formatWakeword(summary.stt.wakeword));
  setFieldText(fields, "stt-vad", formatVad(summary.stt.vad));
  setFieldText(fields, "stt-hotkey", formatHotkey(summary.stt.hotkey));
  setFieldText(fields, "stt-telemetry", summary.stt.telemetry.level || "Info");

  setFieldText(fields, "stt-sidecar-executable", summary.stt.sidecar.executable_path);
  setFieldText(fields, "stt-sidecar-python", summary.stt.sidecar.python_env || "Bundled");
  setFieldText(
    fields,
    "stt-sidecar-access",
    formatBoolean(summary.stt.sidecar.has_access_key, "Present", "Missing"),
  );

  setFieldText(fields, "worker-base-url", summary.worker.base_url || "-");
  setFieldText(fields, "worker-chat-endpoint", summary.worker.chat_endpoint || "-");

  setFieldText(fields, "ui-panel-enabled", formatBoolean(summary.ui.panel_enabled));
  setFieldText(
    fields,
    "ui-main-taskbar",
    formatBoolean(summary.ui.main_window_taskbar, "Shown", "Hidden"),
  );
  setFieldText(fields, "sprite-scale", formatSpriteScale(summary.stt.sprite));
  setFieldText(fields, "stt-whisper-threads", formatWhisperThreads(summary.stt.whisper));
}
