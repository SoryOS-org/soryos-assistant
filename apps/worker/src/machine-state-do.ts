import {
  AllowlistEntry,
  ConversationMemory,
  ConversationTurn,
  MachineConfig,
  MachineState,
  MachineSyncRequest,
  ResponseHandle,
  ResponseHandleRegistry,
  SafeAllowlistEntry,
} from "@sprite/shared";

const MAX_CONVERSATION_TURNS = 100;

export class MachineStateDO {
  private state: DurableObjectState;
  private env: Record<string, unknown>;

  constructor(state: DurableObjectState, env: Record<string, unknown>) {
    this.state = state;
    this.env = env;
  }

  private async handleResponses(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const machineId = url.searchParams.get("machine_id");
    if (!machineId) {
      return new Response(
        JSON.stringify({ ok: false, error: "missing machine_id" }),
        { status: 400, headers: { "Content-Type": "application/json" } },
      );
    }

    const state = await this.loadState(machineId);
    await this.ensureMachineId(state, machineId);
    state.responses = normalizeResponseRegistry(state.responses);

    if (request.method === "GET") {
      const role = url.searchParams.get("role");
      if (role === "ux") {
        return new Response(JSON.stringify(state.responses.ux ?? null), {
          headers: { "Content-Type": "application/json" },
        });
      }
      if (role === "planner") {
        const sessionToken = url.searchParams.get("session_token");
        if (!sessionToken) {
          return new Response(
            JSON.stringify({ ok: false, error: "missing session_token" }),
            { status: 400, headers: { "Content-Type": "application/json" } },
          );
        }
        const handle = state.responses.planner?.[sessionToken] ?? null;
        return new Response(JSON.stringify(handle), {
          headers: { "Content-Type": "application/json" },
        });
      }

      return new Response(JSON.stringify(state.responses), {
        headers: { "Content-Type": "application/json" },
      });
    }

    if (request.method === "POST") {
      try {
        const payload = (await request.json()) as ResponseHandleRequest;
        if (!payload?.role || !payload.response_id) {
          return new Response(
            JSON.stringify({
              ok: false,
              error: "missing role or response_id",
            }),
            { status: 400, headers: { "Content-Type": "application/json" } },
          );
        }

        const handle: ResponseHandle = {
          response_id: payload.response_id,
          role: payload.role,
          fingerprint:
            typeof payload.fingerprint === "string"
              ? payload.fingerprint
              : undefined,
          session_token:
            typeof payload.session_token === "string"
              ? payload.session_token
              : undefined,
          model:
            typeof payload.model === "string" ? payload.model : undefined,
          updated_at_ms: Date.now(),
          last_synced_turn_ts:
            typeof payload.last_synced_turn_ts === "number"
              ? payload.last_synced_turn_ts
              : undefined,
          token_total:
            typeof payload.token_total === "number"
              ? Math.max(0, Math.floor(payload.token_total))
              : undefined,
          summary_required:
            typeof payload.summary_required === "boolean"
              ? payload.summary_required
              : undefined,
        };

        if (handle.role === "ux") {
          state.responses.ux = handle;
        } else {
          if (!handle.session_token) {
            return new Response(
              JSON.stringify({
                ok: false,
                error: "session_token required for planner handles",
              }),
              { status: 400, headers: { "Content-Type": "application/json" } },
            );
          }
          if (!state.responses.planner) {
            state.responses.planner = {};
          }
          state.responses.planner[handle.session_token] = handle;
        }

        await this.saveState(state);
        return new Response(JSON.stringify({ ok: true }), {
          headers: { "Content-Type": "application/json" },
        });
      } catch (error) {
        console.error("[MachineStateDO] responses set error:", error);
        return new Response(
          JSON.stringify({ ok: false, error: String(error) }),
          { status: 400, headers: { "Content-Type": "application/json" } },
        );
      }
    }

    if (request.method === "DELETE") {
      const role = url.searchParams.get("role");
      if (!role) {
        return new Response(
          JSON.stringify({ ok: false, error: "missing role" }),
          { status: 400, headers: { "Content-Type": "application/json" } },
        );
      }

      if (role === "ux") {
        if (state.responses.ux) {
          delete state.responses.ux;
          await this.saveState(state);
        }
      } else if (role === "planner") {
        const sessionToken = url.searchParams.get("session_token");
        if (!sessionToken) {
          return new Response(
            JSON.stringify({ ok: false, error: "missing session_token" }),
            { status: 400, headers: { "Content-Type": "application/json" } },
          );
        }
        if (state.responses.planner?.[sessionToken]) {
          delete state.responses.planner[sessionToken];
          await this.saveState(state);
        }
      } else {
        return new Response(
          JSON.stringify({ ok: false, error: "unknown role" }),
          { status: 400, headers: { "Content-Type": "application/json" } },
        );
      }

      return new Response(JSON.stringify({ ok: true }), {
        headers: { "Content-Type": "application/json" },
      });
    }

    return new Response("Method Not Allowed", { status: 405 });
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const path = url.pathname;

    if (request.method === "POST" && path === "/sync") {
      return this.handleSync(request);
    }
    if (path === "/responses") {
      return this.handleResponses(request);
    }
    if (request.method === "GET" && path === "/memory") {
      return this.handleGetMemory(request);
    }
    if (request.method === "DELETE" && path === "/memory") {
      return this.handleDeleteMemory(request);
    }
    if (request.method === "GET" && path === "/allowlist") {
      return this.handleGetAllowlist(request);
    }
    if (request.method === "GET" && path === "/config") {
      return this.handleGetConfig(request);
    }
    if (request.method === "POST" && path === "/config") {
      return this.handleUpdateConfig(request);
    }

    return new Response("Not Found", { status: 404 });
  }

  private async handleSync(request: Request): Promise<Response> {
    try {
      const update = (await request.json()) as MachineSyncRequest;
      await this.applySync(update);
      return new Response(JSON.stringify({ ok: true }), {
        headers: { "Content-Type": "application/json" },
      });
    } catch (error) {
      console.error("[MachineStateDO] sync error:", error);
      return new Response(
        JSON.stringify({ ok: false, error: String(error) }),
        { status: 400, headers: { "Content-Type": "application/json" } },
      );
    }
  }

  private async handleGetMemory(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      const machineId = url.searchParams.get("machine_id");
      const memory = await this.getMemory(machineId || undefined);
      return new Response(JSON.stringify(memory), {
        headers: { "Content-Type": "application/json" },
      });
    } catch (error) {
      console.error("[MachineStateDO] getMemory error:", error);
      return new Response(
        JSON.stringify({ ok: false, error: String(error) }),
        { status: 500, headers: { "Content-Type": "application/json" } },
      );
    }
  }

  private async handleDeleteMemory(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      const machineId = url.searchParams.get("machine_id");
      if (!machineId) {
        return new Response(
          JSON.stringify({ ok: false, error: "missing machine_id" }),
          { status: 400, headers: { "Content-Type": "application/json" } },
        );
      }

      const state = await this.loadState(machineId);
      await this.ensureMachineId(state, machineId);
      state.memory = {
        type: "turns",
        turns: [],
      };
      state.responses = { planner: {} };
      await this.saveState(state);

      return new Response(JSON.stringify({ ok: true }), {
        headers: { "Content-Type": "application/json" },
      });
    } catch (error) {
      console.error("[MachineStateDO] deleteMemory error:", error);
      return new Response(
        JSON.stringify({ ok: false, error: String(error) }),
        { status: 500, headers: { "Content-Type": "application/json" } },
      );
    }
  }

  private async handleGetAllowlist(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      const machineId = url.searchParams.get("machine_id");
      const allowlist = await this.getAllowlist(machineId || undefined);
      return new Response(JSON.stringify(allowlist), {
        headers: { "Content-Type": "application/json" },
      });
    } catch (error) {
      console.error("[MachineStateDO] getAllowlist error:", error);
      return new Response(
        JSON.stringify({ ok: false, error: String(error) }),
        { status: 500, headers: { "Content-Type": "application/json" } },
      );
    }
  }

  private async handleGetConfig(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      const machineId = url.searchParams.get("machine_id");
      const config = await this.getConfig(machineId || undefined);
      return new Response(JSON.stringify(config), {
        headers: { "Content-Type": "application/json" },
      });
    } catch (error) {
      console.error("[MachineStateDO] getConfig error:", error);
      return new Response(
        JSON.stringify({ ok: false, error: String(error) }),
        { status: 500, headers: { "Content-Type": "application/json" } },
      );
    }
  }

  private async handleUpdateConfig(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      const machineId = url.searchParams.get("machine_id");
      const patch = (await request.json()) as MachineConfig;
      
      await this.updateConfig(patch, machineId || undefined);
      return new Response(JSON.stringify({ ok: true }), {
        headers: { "Content-Type": "application/json" },
      });
    } catch (error) {
      console.error("[MachineStateDO] updateConfig error:", error);
      return new Response(
        JSON.stringify({ ok: false, error: String(error) }),
        { status: 400, headers: { "Content-Type": "application/json" } },
      );
    }
  }

  private async loadState(machineId?: string): Promise<MachineState> {
    const stored = await this.state.storage.get<MachineState>("state");
    if (stored) {
      if (!stored.responses) {
        stored.responses = { planner: {} };
      } else if (!stored.responses.planner) {
        stored.responses.planner = {};
      }
      // If we have a machineId from the request, ensure it matches
      if (machineId && stored.machine_id !== machineId) {
        stored.machine_id = machineId;
      }
      return stored;
    }

    // Initialize default state
    // Use provided machineId or fallback to DO ID string representation
    const stateMachineId = machineId || this.state.id.toString();
    const defaultState: MachineState = {
      machine_id: stateMachineId,
      memory: {
        type: "turns",
        turns: [],
      },
      allowlist: {},
      config: {},
      last_updated_ts: Math.floor(Date.now() / 1000),
      responses: { planner: {} },
    };

    await this.state.storage.put("state", defaultState);
    return defaultState;
  }

  private async ensureMachineId(state: MachineState, requestedMachineId: string): Promise<void> {
    // Ensure the stored machine_id matches the requested one
    if (state.machine_id !== requestedMachineId) {
      state.machine_id = requestedMachineId;
    }
  }

  private async saveState(state: MachineState): Promise<void> {
    state.last_updated_ts = Math.floor(Date.now() / 1000);
    await this.state.storage.put("state", state);
  }

  async applySync(update: MachineSyncRequest): Promise<void> {
    const state = await this.loadState(update.machine_id);
    await this.ensureMachineId(state, update.machine_id);

    // Update installed apps
    if (update.installed_apps) {
      const incoming = update.installed_apps;
      const incomingIds = new Set(Object.keys(incoming));

      for (const [appId, entry] of Object.entries(state.allowlist)) {
        if (!incomingIds.has(appId)) {
          entry.present = false;
        }
      }

      for (const [appId, entry] of Object.entries(incoming)) {
        state.allowlist[appId] = {
          ...entry,
          present: entry.present ?? true,
        };
      }
    }

    // Update enabled/disabled flags
    if (update.enabled_apps) {
      for (const appId of update.enabled_apps) {
        if (state.allowlist[appId]) {
          state.allowlist[appId].enabled = true;
        }
      }
    }

    if (update.disabled_apps) {
      for (const appId of update.disabled_apps) {
        if (state.allowlist[appId]) {
          state.allowlist[appId].enabled = false;
        }
      }
    }

    // Add conversation turn(s)
    if (update.conversation_turn) {
      this.addConversationTurn(state, update.conversation_turn);
    }
    const batchTurns = (update as { conversation_turns?: ConversationTurn[] }).conversation_turns;
    if (batchTurns) {
      for (const turn of batchTurns) {
        this.addConversationTurn(state, turn);
      }
    }

    // Update config
    if (update.config_patch) {
      state.config = mergeMachineConfig(state.config, update.config_patch);
    }

    await this.saveState(state);
  }

  private addConversationTurn(
    state: MachineState,
    turn: ConversationTurn,
  ): void {
    if (state.memory.type === "turns") {
      state.memory.turns.push(turn);
      // Rotate if we exceed max turns
      if (state.memory.turns.length > MAX_CONVERSATION_TURNS) {
        state.memory.turns = state.memory.turns.slice(
          -MAX_CONVERSATION_TURNS,
        );
      }
    } else {
      // If using summary mode, switch to turns mode with the new turn
      // (In a real implementation, you might want to preserve the summary)
      state.memory = {
        type: "turns",
        turns: [turn],
      };
    }
  }

  async getMemory(machineId?: string): Promise<ConversationMemory> {
    const state = await this.loadState(machineId);
    return state.memory;
  }

  async getAllowlist(machineId?: string): Promise<SafeAllowlistEntry[]> {
    const state = await this.loadState(machineId);
    const entries: SafeAllowlistEntry[] = [];

    for (const [appId, entry] of Object.entries(state.allowlist)) {
      // Only include enabled apps that are present
      if (entry.enabled && entry.present) {
        entries.push({
          app_id: appId,
          enabled: entry.enabled,
          // Extract display name from app_id (could be enhanced)
          display_name: this.extractDisplayName(appId),
          // Extract verbs from default_for or extensions
          verbs: this.extractVerbs(entry),
        });
      }
    }

    return entries;
  }

  private extractDisplayName(appId: string): string | undefined {
    // Simple heuristic: if app_id looks like a path, extract the filename
    // Otherwise, use the app_id as-is
    const parts = appId.split(/[/\\]/);
    const lastPart = parts[parts.length - 1];
    if (lastPart && lastPart !== appId) {
      // Remove extension if present
      return lastPart.replace(/\.[^.]+$/, "");
    }
    return appId;
  }

  private extractVerbs(entry: AllowlistEntry): string[] | undefined {
    const verbs: string[] = [];
    if (entry.default_for && entry.default_for.length > 0) {
      verbs.push(...entry.default_for);
    }
    if (entry.extensions && entry.extensions.length > 0) {
      verbs.push(...entry.extensions.map((ext) => `open ${ext}`));
    }
    return verbs.length > 0 ? verbs : undefined;
  }

  async getConfig(machineId?: string): Promise<MachineConfig> {
    const state = await this.loadState(machineId);
    return state.config;
  }

  async updateConfig(patch: MachineConfig, machineId?: string): Promise<void> {
    const state = await this.loadState(machineId);
    if (machineId) {
      await this.ensureMachineId(state, machineId);
    }
    state.config = mergeMachineConfig(state.config, patch);
    await this.saveState(state);
  }
}

function mergeMachineConfig(
  current: MachineConfig,
  patch: MachineConfig,
): MachineConfig {
  const result: MachineConfig = { ...current };

  for (const [key, value] of Object.entries(patch)) {
    if (key === "mcp" && isRecord(value)) {
      const existingMcp = isRecord(result.mcp) ? { ...result.mcp } : {};
      const mergedMcp: Record<string, unknown> = { ...existingMcp };

      for (const [mcpKey, mcpValue] of Object.entries(value)) {
        if (mcpKey === "servers" && isRecord(mcpValue)) {
          const existingServers = isRecord(mergedMcp.servers)
            ? { ...(mergedMcp.servers as Record<string, unknown>) }
            : {};

          for (const [serverId, serverData] of Object.entries(mcpValue)) {
            existingServers[serverId] = mergeServerSnapshot(
              existingServers[serverId],
              serverData,
            );
          }

          mergedMcp.servers = existingServers;
        } else {
          mergedMcp[mcpKey] = mcpValue;
        }
      }

      result.mcp = mergedMcp;
    } else {
      result[key] = value;
    }
  }

  return result;
}

function mergeServerSnapshot(
  current: unknown,
  incoming: unknown,
): unknown {
  if (!isRecord(incoming)) {
    return incoming;
  }

  const existing = isRecord(current) ? current : {};
  return {
    ...existing,
    ...incoming,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

interface ResponseHandleRequest {
  role: "ux" | "planner";
  response_id: string;
  fingerprint?: string;
  session_token?: string;
  model?: string;
  last_synced_turn_ts?: number;
  token_total?: number;
  summary_required?: boolean;
}

function normalizeResponseRegistry(
  current?: ResponseHandleRegistry,
): ResponseHandleRegistry {
  if (current && typeof current === "object") {
    return {
      ux: current.ux,
      planner: current.planner ? { ...current.planner } : {},
    };
  }
  return { planner: {} };
}

