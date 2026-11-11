/**
 * Tauri bridge — the ONLY place allowed to talk to the Rust backend.
 *
 * Features import from here (`lib/tauri`), never from `@tauri-apps/api`
 * directly, so that:
 *   - running the UI in a plain browser (vite dev without Tauri) degrades
 *     gracefully instead of throwing,
 *   - command names / event channels stay centralized and typed.
 *
 * Command names and argument casing are verbatim v1 ports — the Rust backend
 * expects exactly these (e.g. camelCase `serverId`).
 */
import type {
  AboutMetadata,
  ConfigPatch,
  ConfigSummary,
  ConversationMemory,
  LatencyHudSnapshot,
  LogsSnapshot,
  McpRegistrySnapshotEntry,
  PreloadStatus,
  PreferencesCatalog,
  PreferencesSnapshot,
  SecretsStatus,
  SttResultPayload,
  UiSnapshot,
  UpdateConfigResponse,
  UpdatePreferencesPayload,
} from "@/types";
import type { EventCallback } from "@tauri-apps/api/event";

/** True when the UI runs inside a Tauri WebView. */
export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Invoke a Tauri command.
 * Returns `null` when running outside Tauri (browser preview) — wrappers
 * below therefore all return `Promise<T | null>` and callers handle `null`.
 */
export async function invokeCommand<T>(
  command: string,
  payload?: Record<string, unknown>,
): Promise<T | null> {
  if (!isTauri) {
    // eslint-disable-next-line no-console
    console.warn(`[tauri] command "${command}" ignored (running outside Tauri)`);
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, payload as never);
}

/**
 * Subscribe to a Tauri event channel.
 * Returns an unsubscribe function (no-op outside Tauri).
 */
export async function listenToEvent<T>(
  channel: string,
  handler: EventCallback<T>,
): Promise<() => void> {
  if (!isTauri) {
    return () => {};
  }
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<T>(channel, handler);
  return unlisten;
}

/* ------------------------------------------------------------------ */
/* Worker / chat                                                       */
/* ------------------------------------------------------------------ */

/** Send a text request through the worker bridge (manual send path). */
export function processUserRequest(query: string): Promise<SttResultPayload | null> {
  return invokeCommand<SttResultPayload>("process_user_request", { query });
}

/** Clear conversation memory on the worker. */
export function resetWorkerSession(): Promise<unknown | null> {
  return invokeCommand("reset_worker_session");
}

/** Pull conversation memory for hydration. */
export function getConversationMemory(): Promise<ConversationMemory | null> {
  return invokeCommand<ConversationMemory>("get_conversation_memory");
}

/* ------------------------------------------------------------------ */
/* Panel window                                                        */
/* ------------------------------------------------------------------ */

/** Navigate the panel window to another view. */
export function panelNavigate(view: string): Promise<unknown | null> {
  return invokeCommand("panel_navigate", { view });
}

/** Hide the panel window (v1 `panel_hide_btn`). */
export function panelHide(): Promise<unknown | null> {
  return invokeCommand("panel_hide");
}

/** Mirror of the sprite window's UI state machine (`get_ui_snapshot`). */
export function getUiSnapshot(): Promise<UiSnapshot | null> {
  return invokeCommand<UiSnapshot>("get_ui_snapshot");
}

/** Open an external URL through the OS browser (shell plugin, global API). */
export async function openExternalUrl(url: string): Promise<void> {
  if (!isTauri) {
    // eslint-disable-next-line no-console
    console.warn(`[tauri] openExternalUrl("${url}") ignored (running outside Tauri)`);
    return;
  }
  const tauri = (window as unknown as {
    __TAURI__?: { shell?: { open?: (url: string) => Promise<void> } };
  }).__TAURI__;
  const open = tauri?.shell?.open;
  if (!open) {
    // eslint-disable-next-line no-console
    console.warn("[tauri] shell plugin API unavailable in this window");
    return;
  }
  await open(url);
}

/* ------------------------------------------------------------------ */
/* Configuration                                                       */
/* ------------------------------------------------------------------ */

export function getConfigSummary(): Promise<ConfigSummary | null> {
  return invokeCommand<ConfigSummary>("get_config_summary");
}

export function updateConfig(patch: ConfigPatch): Promise<UpdateConfigResponse | null> {
  return invokeCommand<UpdateConfigResponse>("update_config", { patch });
}

/* ------------------------------------------------------------------ */
/* Preferences                                                         */
/* ------------------------------------------------------------------ */

export function getPreferencesSnapshot(): Promise<PreferencesSnapshot | null> {
  return invokeCommand<PreferencesSnapshot>("get_preferences_snapshot");
}

/** Refresh the installed-application catalog (falls back to the cached one). */
export function refreshApplicationCatalog(): Promise<PreferencesCatalog | null> {
  return invokeCommand<PreferencesCatalog>("refresh_application_catalog");
}

/** Cached application catalog. */
export function getApplicationCatalog(): Promise<PreferencesCatalog | null> {
  return invokeCommand<PreferencesCatalog>("get_application_catalog");
}

export function updatePreferences(
  payload: UpdatePreferencesPayload,
): Promise<PreferencesSnapshot | null> {
  return invokeCommand<PreferencesSnapshot>("update_preferences", { payload });
}

/* ------------------------------------------------------------------ */
/* Logs / about / HUD / preload                                        */
/* ------------------------------------------------------------------ */

export function getRecentLogs(limit: number): Promise<LogsSnapshot | null> {
  return invokeCommand<LogsSnapshot>("get_recent_logs", { limit });
}

export function getAboutMetadata(): Promise<AboutMetadata | null> {
  return invokeCommand<AboutMetadata>("get_about_metadata");
}

export function getLatencyHud(limit: number): Promise<LatencyHudSnapshot | null> {
  return invokeCommand<LatencyHudSnapshot>("get_latency_hud", { limit });
}

export function getPreloadStatus(): Promise<PreloadStatus | null> {
  return invokeCommand<PreloadStatus>("get_preload_status");
}

/* ------------------------------------------------------------------ */
/* MCP                                                                 */
/* ------------------------------------------------------------------ */

export function getMcpRegistry(): Promise<McpRegistrySnapshotEntry[] | null> {
  return invokeCommand<McpRegistrySnapshotEntry[]>("get_mcp_registry");
}

/** Enable/disable a server; returns the refreshed config summary. */
export function toggleMcpServer(
  serverId: string,
  enabled: boolean,
): Promise<ConfigSummary | null> {
  return invokeCommand<ConfigSummary>("toggle_mcp_server", { serverId, enabled });
}

/** Open the MCP configuration file in the editor configured by the OS. */
export function openMcpConfigFile(): Promise<unknown | null> {
  return invokeCommand("open_mcp_config_file");
}

/* ------------------------------------------------------------------ */
/* Secrets                                                             */
/* ------------------------------------------------------------------ */

export function getSecretsStatus(): Promise<SecretsStatus | null> {
  return invokeCommand<SecretsStatus>("get_secrets_status");
}

function setSecret(command: string, value: string): Promise<SecretsStatus | null> {
  return invokeCommand<SecretsStatus>(command, { value: value.trim() });
}

export function setPicovoiceKey(value: string): Promise<SecretsStatus | null> {
  return setSecret("set_picovoice_key", value);
}

export function clearPicovoiceKey(): Promise<unknown | null> {
  return invokeCommand("clear_picovoice_key");
}

export function setWorkerAuthToken(value: string): Promise<SecretsStatus | null> {
  return setSecret("set_worker_auth_token", value);
}

export function clearWorkerAuthToken(): Promise<unknown | null> {
  return invokeCommand("clear_worker_auth_token");
}
