/**
 * MCP stores — ports of v1 `mcpRegistry.js` (server state machine) and
 * `mcpProgress.js` (tool-call progress entries).
 */
import { create } from "zustand";
import type {
  McpConfigSummaryServer,
  McpHostEvent,
  McpProgressEntry,
  McpProgressPayload,
  McpRegistrySnapshotEntry,
  McpRuntimeStatus,
  McpServerState,
} from "@/types";

/* ================================================================== */
/* Registry                                                            */
/* ================================================================== */

interface McpRegistryState {
  /** Ordered map of known servers (insertion order preserved). */
  servers: McpServerState[];
  loading: boolean;
  loaded: boolean;
  pendingToggles: string[];
  statusMessage: string;
  statusTone: "info" | "success" | "error";

  setStatus: (message: string, tone?: McpRegistryState["statusTone"]) => void;
  /** Merge the config summary's declared servers (v1 `applyConfigSummary`). */
  applyConfigSummary: (mcpSummary: McpConfigSummaryServer[] | undefined) => void;
  /** Merge the `get_mcp_registry` snapshot (v1 `applyRegistrySnapshot`). */
  applyRegistrySnapshot: (entries: McpRegistrySnapshotEntry[]) => void;
  /** Handle an `assistant://mcp-status` host event (v1 `handleEvent`). */
  handleHostEvent: (payload: McpHostEvent) => void;
  setPendingToggle: (serverId: string, pending: boolean) => void;
  setLoading: (loading: boolean) => void;
}

function orderedIdsOrder(state: McpServerState[], headIds: string[]): McpServerState[] {
  const seen = new Set<string>();
  const ordered: McpServerState[] = [];
  for (const id of headIds) {
    const entry = state.find((server) => server.id === id);
    if (entry) {
      ordered.push(entry);
      seen.add(entry.id);
    }
  }
  for (const entry of state) {
    if (!seen.has(entry.id)) {
      ordered.push(entry);
    }
  }
  return ordered;
}

export const useMcpRegistryStore = create<McpRegistryState>((set, get) => ({
  servers: [],
  loading: false,
  loaded: false,
  pendingToggles: [],
  statusMessage: "",
  statusTone: "info",

  setStatus: (message, tone = "info") =>
    set({ statusMessage: message, statusTone: tone }),

  applyConfigSummary: (mcpSummary) => {
    const declaredServers = Array.isArray(mcpSummary) ? mcpSummary : [];
    if (!declaredServers.length) {
      set((s) => ({ servers: [...s.servers] }));
      return;
    }

    const current = [...get().servers];
    const byId = new Map(current.map((server) => [server.id, { ...server }]));
    const orderedIds: string[] = [];

    declaredServers.forEach((server) => {
      if (!server?.id) {
        return;
      }
      const entry = byId.get(server.id) ?? defaultServer(server.id);
      byId.set(server.id, entry);
      entry.displayName = server.display_name || null;
      entry.transport = server.transport || entry.transport;
      entry.enabled = server.enabled !== false;
      if (!entry.enabled) {
        entry.status = "disabled";
        entry.statusNote = "Disabled in configuration";
      } else if (entry.status === "disabled") {
        entry.status = "unknown";
        entry.statusNote = null;
      }
      orderedIds.push(server.id);
    });

    set({
      servers: orderedIdsOrder([...byId.values()], orderedIds),
    });
  },

  applyRegistrySnapshot: (registryEntries) => {
    if (!Array.isArray(registryEntries)) {
      return;
    }
    const current = [...get().servers];
    const byId = new Map(current.map((server) => [server.id, { ...server }]));

    registryEntries.forEach((entry) => {
      if (!entry?.id) {
        return;
      }
      const server = byId.get(entry.id) ?? defaultServer(entry.id);
      byId.set(entry.id, server);
      server.displayName = entry.display_name || server.displayName;
      server.serverName = entry.server_name || server.serverName;
      server.serverVersion = entry.server_version || null;
      server.transport = entry.transport || server.transport;
      server.enabled = entry.enabled !== false;
      const tools = Array.isArray(entry.tools)
        ? entry.tools
            .map((tool) => ({ name: tool?.name || "tool", description: tool?.description ?? null }))
            .sort((a, b) => a.name.localeCompare(b.name))
        : [];
      server.tools = tools;
      server.toolCount = tools.length;
      server.resourceCount =
        typeof entry.resource_count === "number" ? entry.resource_count : server.resourceCount;
      const updated = Number(entry.updated_at_ms);
      if (!Number.isNaN(updated)) {
        server.updatedAtMs = updated;
      }
      const previousStatus = server.status;
      if (!server.enabled) {
        server.status = "disabled";
        server.statusNote = "Disabled in configuration";
      } else {
        server.status = "running";
        if (previousStatus === "error") {
          server.statusNote = null;
        }
      }
    });

    set({ servers: [...byId.values()], loaded: true });
  },

  handleHostEvent: (payload) => {
    if (!payload || typeof payload !== "object") {
      return;
    }
    const serverId = payload.meta?.server_id;
    if (!serverId) {
      return;
    }
    const timestamp = Number(payload.meta?.timestamp_ms) || Date.now();
    const eventName = payload.kind?.event;

    set((state) => {
      const byId = new Map(state.servers.map((server) => [server.id, { ...server }]));
      const server = byId.get(serverId) ?? defaultServer(serverId);
      byId.set(serverId, server);

      switch (eventName) {
        case "server_started": {
          updateStatus(server, "running", payload.kind?.pid ? `PID ${payload.kind.pid}` : null, timestamp);
          break;
        }
        case "server_stopped": {
          const exitCode =
            typeof payload.kind?.exit_code === "number" ? payload.kind.exit_code : null;
          updateStatus(
            server,
            "stopped",
            exitCode !== null ? `Exited with code ${exitCode}` : "Server stopped",
            timestamp,
          );
          break;
        }
        case "server_crashed": {
          const message =
            typeof payload.kind?.message === "string" && payload.kind.message.trim()
              ? payload.kind.message.trim()
              : "Server crashed";
          updateStatus(server, "error", message, timestamp);
          break;
        }
        case "schemas_updated": {
          const tools = Array.isArray(payload.kind?.tool_schemas)
            ? payload.kind.tool_schemas
                .map((tool) => ({
                  name: typeof tool?.name === "string" ? tool.name : "tool",
                  description: typeof tool?.description === "string" ? tool.description : null,
                }))
                .sort((a, b) => a.name.localeCompare(b.name))
            : [];
          server.tools = tools;
          server.toolCount = tools.length;
          if (Array.isArray(payload.kind?.resources)) {
            server.resourceCount = payload.kind.resources.length;
          }
          server.updatedAtMs = timestamp || Date.now();
          updateStatus(server, "running", server.statusNote, timestamp);
          break;
        }
        default:
          return {};
      }

      return { servers: [...byId.values()] };
    });
  },

  setPendingToggle: (serverId, pending) =>
    set((s) => ({
      pendingToggles: pending
        ? [...s.pendingToggles, serverId]
        : s.pendingToggles.filter((id) => id !== serverId),
    })),

  setLoading: (loading) => set({ loading }),
}));

export function defaultServer(id: string): McpServerState {
  return {
    id,
    displayName: null,
    serverName: null,
    serverVersion: null,
    transport: null,
    enabled: true,
    status: "unknown",
    statusNote: null,
    statusChangedAt: null,
    toolCount: 0,
    resourceCount: 0,
    updatedAtMs: null,
    tools: [],
  };
}

function updateStatus(
  server: McpServerState,
  status: McpRuntimeStatus,
  note: string | null,
  timestamp: number,
): void {
  server.status = status;
  server.statusNote = note || null;
  server.statusChangedAt = timestamp || Date.now();
  if (status === "running" && !note) {
    server.statusNote = null;
  }
}

/* ================================================================== */
/* Progress                                                            */
/* ================================================================== */

const MAX_PROGRESS_ENTRIES = 50;

interface McpProgressState {
  entries: McpProgressEntry[];

  /** Handle an `assistant://mcp-progress` payload (v1 `upsertEntry`). */
  handleProgress: (payload: McpProgressPayload) => void;
  clear: () => void;
}

function buildKey(payload: McpProgressPayload): string {
  const token = payload.session_token || "session";
  const step = typeof payload.step_index === "number" ? payload.step_index : 0;
  return `${token}-${step}`;
}

export const useMcpProgressStore = create<McpProgressState>((set) => ({
  entries: [],

  handleProgress: (payload) => {
    if (!payload || typeof payload !== "object") {
      return;
    }
    set((s) => {
      const key = buildKey(payload);
      const idx = s.entries.findIndex((item) => item.key === key);
      let entries = [...s.entries];

      let entry: McpProgressEntry;
      if (idx === -1) {
        entry = {
          key,
          sessionToken: payload.session_token || null,
          stepIndex: typeof payload.step_index === "number" ? payload.step_index : 0,
          serverId: payload.server_id || "",
          toolName: payload.tool_name || "",
          status: "queued",
          statusMessage: null,
          error: null,
          latencyMs: null,
          createdAt: Date.now(),
          updatedAt: Date.now(),
        };
        entries.push(entry);
      } else {
        entry = { ...entries[idx] };
      }

      entry.status = String(payload.status || "").toLowerCase();
      const statusNote = typeof payload.status_note === "string" ? payload.status_note : null;
      const legacySpeech = typeof payload.speech_update === "string" ? payload.speech_update : null;
      entry.statusMessage = statusNote || legacySpeech;
      entry.error = payload.error || null;
      entry.latencyMs = typeof payload.latency_ms === "number" ? payload.latency_ms : null;
      entry.updatedAt = Date.now();
      if (payload.server_id) {
        entry.serverId = payload.server_id;
      }
      if (payload.tool_name) {
        entry.toolName = payload.tool_name;
      }
      if (idx !== -1) {
        entries[idx] = entry;
      }

      if (entries.length > MAX_PROGRESS_ENTRIES) {
        entries.sort((a, b) => (a.updatedAt || 0) - (b.updatedAt || 0));
        entries = entries.slice(entries.length - MAX_PROGRESS_ENTRIES);
      }
      return { entries };
    });
  },

  clear: () => set({ entries: [] }),
}));

/** v1 `summarizeEntries`. */
export function summarizeProgressEntries(
  entries: McpProgressEntry[],
): { total: number; running: number; queued: number; completed: number; crashed: number } {
  return entries.reduce(
    (acc, entry) => {
      const status = String(entry.status || "").toLowerCase();
      acc.total += 1;
      if (status === "running") {
        acc.running += 1;
      } else if (status === "crashed") {
        acc.crashed += 1;
      } else if (status === "completed") {
        acc.completed += 1;
      } else {
        acc.queued += 1;
      }
      return acc;
    },
    { total: 0, running: 0, queued: 0, completed: 0, crashed: 0 },
  );
}
