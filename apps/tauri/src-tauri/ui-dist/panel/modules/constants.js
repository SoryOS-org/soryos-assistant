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
  sttReady: "assistant://stt-ready",
  sttError: "assistant://stt-error",
  mcpProgress: "assistant://mcp-progress",
  mcpStatus: "assistant://mcp-status",
  workerAuth: "worker_auth_failed",
  workerAuthOk: "worker_auth_ok",
};

export const MODEL_FIELD_NAME = "model.id";
