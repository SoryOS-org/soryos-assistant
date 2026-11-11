/**
 * Event channel names — single source of truth for the v2 UI.
 *
 * Ported verbatim from the v1 panel (`webview/panel/modules/constants.js`);
 * the Rust backend emits to these exact strings.
 */
export const EVENT_CHANNELS = {
  navigation: "panel://navigate",
  ui: "sprite://ui",
  configChanged: "config_changed",
  configDirty: "config_dirty",
  preferencesChanged: "preferences_changed",
  catalogChanged: "intent_catalog_updated",
  preload: "stt_preload_status",
  sttFinal: "assistant://stt-final",
  sttResult: "assistant://stt-result",
  sttStarted: "assistant://stt-started",
  sttReady: "assistant://stt-ready",
  sttError: "assistant://stt-error",
  mcpProgress: "assistant://mcp-progress",
  mcpStatus: "assistant://mcp-status",
  workerAuth: "worker_auth_failed",
  workerAuthOk: "worker_auth_ok",
} as const;

/** Model select field name used by the config editor. */
export const MODEL_FIELD_NAME = "model.id";
