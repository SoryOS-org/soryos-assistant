/**
 * Shared TypeScript contracts for the v2 desktop UI.
 *
 * These mirror the v1 panel contracts (`apps/tauri/webview/panel/modules/*`)
 * and the Rust backend payloads — field names are snake_case on purpose.
 * Do not "prettify" names: the backend expects them as-is.
 *
 * NOTE: the authoritative cross-app schemas live in `packages/shared` (Zod).
 * Once the v2 is wired into the pnpm workspace, re-export from there instead
 * of duplicating.
 */

export type MainView = "chat" | "settings";

export type RightPanelView = "terminal" | "files" | "browser";

export type VoiceState = "idle" | "listening" | "thinking" | "speaking" | "error";

/* ------------------------------------------------------------------ */
/* Chat / conversation                                                 */
/* ------------------------------------------------------------------ */

export type ChatRole = "user" | "assistant";

/** Loose intent shape: either `{type}` objects or plain strings. */
export interface ChatIntent {
  type?: string;
  [key: string]: unknown;
}

/** One transcript entry — faithful port of v1 `chatState.entries` item. */
export interface ChatEntry {
  id: string;
  role: ChatRole;
  text: string;
  /** Milliseconds since epoch (v1 normalizes seconds → ms). */
  timestamp: number;
  intents: ChatIntent[];
  /** `planner` marks planner-side notes; `summary` marks a compressed history. */
  source: string | null;
  context: string | null;
}

export interface MemoryTurn {
  role?: string;
  content?: string | null;
  timestamp?: number | null;
  intents?: ChatIntent[];
  source?: string | null;
  context?: string | null;
}

/**
 * `get_conversation_memory` payload:
 * either the full turn list or a compressed summary.
 */
export type ConversationMemory =
  | { type: "turns"; turns?: MemoryTurn[] }
  | { type: "summary"; summary?: string; lastUpdated?: number | null }
  | null;

/** `assistant://stt-result` payload (assistant reply). */
export interface SttResultPayload {
  reply?: string;
  intents?: ChatIntent[];
  source?: string;
  planner_context?: string | null;
  context?: string | null;
  [key: string]: unknown;
}

/** `assistant://stt-final` payload (final user transcript). */
export interface SttFinalPayload {
  text?: string;
  lang?: string;
  confidence?: number;
  ts_start_ms?: number;
  ts_end_ms?: number;
  [key: string]: unknown;
}

/* ------------------------------------------------------------------ */
/* MCP                                                                 */
/* ------------------------------------------------------------------ */

export type McpRuntimeStatus =
  | "unknown"
  | "starting"
  | "running"
  | "stopped"
  | "error"
  | "disabled";

export interface McpTool {
  name: string;
  description?: string | null;
}

/** A server state tracked by the registry store (v1 `defaultServer`). */
export interface McpServerState {
  id: string;
  displayName: string | null;
  serverName: string | null;
  serverVersion: string | null;
  transport: string | null;
  enabled: boolean;
  status: McpRuntimeStatus;
  statusNote: string | null;
  statusChangedAt: number | null;
  toolCount: number;
  resourceCount: number;
  updatedAtMs: number | null;
  tools: McpTool[];
}

/** Item from the `get_mcp_registry` snapshot. */
export interface McpRegistrySnapshotEntry {
  id: string;
  display_name?: string;
  server_name?: string;
  server_version?: string;
  transport?: string;
  enabled?: boolean;
  tool_count?: number;
  resource_count?: number;
  updated_at_ms?: number;
  tools?: Array<{ name?: string; description?: string }>;
}

/** Server section of the config summary (`summary.mcp.servers[].`). */
export interface McpConfigSummaryServer {
  id: string;
  display_name?: string | null;
  transport?: string | null;
  enabled?: boolean;
}

/** `assistant://mcp-status` payload. */
export interface McpHostEvent {
  kind: {
    event: "server_started" | "server_stopped" | "server_crashed" | "schemas_updated";
    pid?: number;
    exit_code?: number;
    message?: string;
    tool_schemas?: Array<{ name?: string; description?: string }>;
    resources?: unknown[];
  };
  meta: {
    server_id: string;
    timestamp_ms: number;
  };
}

export type McpProgressStatus = "queued" | "running" | "completed" | "crashed";

/** One MCP tool-call progress item (v1 `mcpState.entries` item). */
export interface McpProgressEntry {
  key: string;
  sessionToken: string | null;
  stepIndex: number;
  serverId: string;
  toolName: string;
  status: string;
  statusMessage: string | null;
  error: string | null;
  latencyMs: number | null;
  createdAt: number;
  updatedAt: number;
}

/** `assistant://mcp-progress` payload. */
export interface McpProgressPayload {
  session_token?: string;
  step_index?: number;
  server_id?: string;
  tool_name?: string;
  status?: string;
  status_note?: string;
  speech_update?: string;
  error?: string;
  latency_ms?: number;
}

/* ------------------------------------------------------------------ */
/* Config summary / editor                                             */
/* ------------------------------------------------------------------ */

export interface ConfigModel {
  id?: string | null;
  path?: string | null;
  language?: { auto?: boolean; default?: string | null };
  punctuation?: boolean;
}

export interface ConfigModelCatalogEntry {
  id: string;
  label: string;
}

export interface ConfigSummary {
  stt: {
    provider?: string;
    model?: ConfigModel | null;
    wakeword?: { keyword?: string | null; sensitivity?: number };
    vad?: {
      aggressiveness?: number;
      start_grace_ms?: number;
      max_segment_ms?: number;
      activation_ms?: number;
      post_speech_silence_ms?: number;
    };
    hotkey?: { push_to_talk?: string | null; mode_precedence?: string | null };
    telemetry?: { level?: string };
    sidecar?: {
      executable_path?: string | null;
      python_env?: string | null;
      has_access_key?: boolean;
    };
    sprite?: { scale?: number; min_scale?: number; max_scale?: number };
    whisper?: {
      threads?: number | null;
      recommended_threads?: number | null;
      physical_cores?: number | null;
    };
  };
  worker?: { base_url?: string | null; chat_endpoint?: string | null };
  ui?: { panel_enabled?: boolean; main_window_taskbar?: boolean };
  models?: ConfigModelCatalogEntry[];
  mcp?: { servers?: McpConfigSummaryServer[] };
}

/** Configuration form values (v1 `collectFormValues`). */
export interface ConfigFormValues {
  provider: string;
  model: { id: string; punctuation: boolean | null };
  vad: {
    aggressiveness: number | null;
    start_grace_ms: number | null;
    max_segment_ms: number | null;
    activation_ms: number | null;
    post_speech_silence_ms: number | null;
  };
  wakeword: { keyword: string; sensitivity: number | null };
  hotkey: { push_to_talk: string; mode_precedence: string };
  telemetry: { level: string };
  sprite: { scale: number | null };
  whisper: { threads: number | null };
}

/** `update_config` takes `{ patch }`, returns `{ summary }`. */
export type ConfigPatch = Record<string, unknown>;
export interface UpdateConfigResponse {
  summary?: ConfigSummary;
}

/* ------------------------------------------------------------------ */
/* Preferences / secrets / logs / about / HUD                          */
/* ------------------------------------------------------------------ */

export interface AppCatalogApplication {
  app_id?: string;
  appId?: string;
  identifier?: string;
  name?: string;
}

export interface PreferencesAllowlistEntry {
  appId: string;
  enabled: boolean;
}

export interface PreferencesSnapshot {
  personality?: string;
  allowlist?: Array<{ appId: string; enabled: boolean }>;
  [key: string]: unknown;
}

export interface PreferencesCatalog {
  applications?: AppCatalogApplication[];
}

export interface UpdatePreferencesPayload {
  personality?: string;
  allowlist?: Array<{ appId: string; enabled: boolean }>;
}

/** `get_recent_logs` snapshot. */
export interface LogsSnapshot {
  path?: string | null;
  lines?: string[];
}

/** One latency turn from `get_latency_hud`. */
export interface LatencyHudEntry {
  trigger?: string;
  ok?: boolean;
  err?: string | null;
  cold_start?: boolean;
  transcript_chars?: number;
  system_ms?: number;
  total_ms?: number;
  phases?: {
    capture_ms?: number;
    stt_infer_ms?: number;
    llm_ms?: number;
    tts_ms?: number;
    tts_infer_ms?: number;
    tts_latency_ms?: number;
    tts_duration_ms?: number;
    to_transcript_ms?: number;
  };
}

/** `get_latency_hud` snapshot. */
export interface LatencyHudSnapshot {
  latest?: LatencyHudEntry | null;
  entries?: LatencyHudEntry[];
}

/** `get_about_metadata` payload. */
export interface AboutMetadata {
  version?: string;
  commit?: string;
  build_timestamp?: number;
  tauri?: string;
  target?: string;
}

export interface SecretEntryStatus {
  has_key?: boolean;
  hint?: string;
}

/** `get_secrets_status` payload. */
export interface SecretsStatus {
  picovoice?: SecretEntryStatus;
  worker_auth?: SecretEntryStatus;
}

/** `get_preload_status` payload (preload banner). */
export interface PreloadStatus {
  state?: string;
  message?: string;
  [key: string]: unknown;
}

/** `get_ui_snapshot` payload (`.state` mirrors the sprite state machine). */
export interface UiSnapshot {
  state?: { state?: string; timestamp?: number; vu_level?: number | null };
}

/** `settings.sync` mirror of the legacy UI state pill. */
export interface UiStatePayload {
  state?: string;
  timestamp?: number;
  vu_level?: number | null;
}
