import type {
  ConversationMemory,
  ConversationTurn,
  ResponseHandleRegistry,
  SafeAllowlistEntry,
} from "@sprite/shared";

import type { WorkerBindings } from "./config";
import { buildMcpToolsPrompt, extractMcpContext, type McpPromptBundle } from "./mcp";

export function getMachineStateDO(
  machineId: string,
  env: WorkerBindings,
): DurableObjectStub {
  const id = env.MACHINE_STATE.idFromName(machineId);
  return env.MACHINE_STATE.get(id);
}

export async function loadMachineContext(
  machineId: string,
  env: WorkerBindings,
): Promise<{
  stub: DurableObjectStub;
  conversationMemory?: ConversationMemory;
  allowlist?: SafeAllowlistEntry[];
  personality?: string;
  mcpToolsPrompt?: McpPromptBundle;
  responseHandles?: ResponseHandleRegistry;
}> {
  const stub = getMachineStateDO(machineId, env);
  let conversationMemory: ConversationMemory | undefined;
  let allowlist: SafeAllowlistEntry[] | undefined;
  let personality: string | undefined;
  let mcpToolsPrompt: McpPromptBundle | undefined;
  let responseHandles: ResponseHandleRegistry | undefined;

  try {
    const memoryResponse = await stub.fetch(
      new Request(
        `http://dummy/memory?machine_id=${encodeURIComponent(machineId)}`,
      ),
      {
        method: "GET",
      },
    );
    if (memoryResponse.ok) {
      conversationMemory = (await memoryResponse.json()) as ConversationMemory;
    }

    const allowlistResponse = await stub.fetch(
      new Request(
        `http://dummy/allowlist?machine_id=${encodeURIComponent(machineId)}`,
      ),
      {
        method: "GET",
      },
    );
    if (allowlistResponse.ok) {
      allowlist = (await allowlistResponse.json()) as SafeAllowlistEntry[];
    }

    const configResponse = await stub.fetch(
      new Request(
        `http://dummy/config?machine_id=${encodeURIComponent(machineId)}`,
      ),
      {
        method: "GET",
      },
    );
    if (configResponse.ok) {
      const machineConfig = (await configResponse.json()) as Record<string, unknown>;
      if (typeof machineConfig.personality === "string") {
        personality = machineConfig.personality;
      }
      const rawMcp = (machineConfig as Record<string, unknown>)["mcp"];
      if (rawMcp) {
        const context = extractMcpContext(rawMcp);
        if (context) {
          const prompt = buildMcpToolsPrompt(context);
          if (prompt) {
            mcpToolsPrompt = prompt;
          }
        }
      }
    }

    const handlesResponse = await stub.fetch(
      new Request(
        `http://dummy/responses?machine_id=${encodeURIComponent(machineId)}`,
      ),
      {
        method: "GET",
      },
    );
    if (handlesResponse.ok) {
      responseHandles = (await handlesResponse.json()) as ResponseHandleRegistry;
    }
  } catch (error) {
    console.error("[worker] failed to fetch machine state:", error);
  }

  return {
    stub,
    conversationMemory,
    allowlist,
    personality,
    mcpToolsPrompt,
    responseHandles,
  };
}

export async function appendAssistantConversationTurn(
  machineId: string,
  content: string,
  env: WorkerBindings,
  contentType: "output_text" | "refusal" = "output_text",
  extras?: {
    source?: string;
    sessionToken?: string;
    context?: string;
  },
): Promise<void> {
  const trimmed = content.trim();
  if (!trimmed.length) {
    return;
  }

  try {
    const stub = getMachineStateDO(machineId, env);
    await stub.fetch(new Request("http://dummy/sync"), {
      method: "POST",
      body: JSON.stringify({
        machine_id: machineId,
        conversation_turn: {
          role: "assistant",
          content: trimmed,
          timestamp: Math.floor(Date.now() / 1000),
          content_type: contentType,
          source: extras?.source,
          session_token: extras?.sessionToken,
          context: extras?.context,
        },
      }),
    });
  } catch (error) {
    console.error("[worker] failed to record assistant turn:", error);
  }
}

interface PersistHandlePayload {
  role: "ux" | "planner";
  responseId: string;
  fingerprint?: string;
  model: string;
  sessionToken?: string;
  lastSyncedTurnTs?: number;
  tokenTotal?: number;
  summaryRequired?: boolean;
}

export async function persistResponseHandle(
  stub: DurableObjectStub,
  machineId: string,
  payload: PersistHandlePayload,
): Promise<void> {
  try {
    await stub.fetch(
      new Request(
        `http://dummy/responses?machine_id=${encodeURIComponent(machineId)}`,
      ),
      {
        method: "POST",
        body: JSON.stringify({
          role: payload.role,
          response_id: payload.responseId,
          fingerprint: payload.fingerprint,
          model: payload.model,
          session_token: payload.sessionToken,
          last_synced_turn_ts: payload.lastSyncedTurnTs,
          token_total: payload.tokenTotal,
          summary_required: payload.summaryRequired,
        }),
      },
    );
  } catch (error) {
    console.error("[worker] failed to persist response handle:", error);
  }
}

export async function clearResponseHandle(
  stub: DurableObjectStub,
  machineId: string,
  options: { role: "ux" | "planner"; sessionToken?: string },
): Promise<void> {
  try {
    const params = new URLSearchParams({
      machine_id: machineId,
      role: options.role,
    });
    if (options.role === "planner" && options.sessionToken) {
      params.set("session_token", options.sessionToken);
    }
    await stub.fetch(
      new Request(`http://dummy/responses?${params.toString()}`),
      { method: "DELETE" },
    );
  } catch (error) {
    console.error("[worker] failed to clear response handle:", error);
  }
}

export function buildUxFingerprint(args: {
  model: string;
  personality: string;
  systemPromptTemplate: string;
  allowlist?: SafeAllowlistEntry[];
  mcpToolsPrompt?: McpPromptBundle;
}): string {
  return [
    args.model,
    args.personality,
    args.systemPromptTemplate,
    formatAllowlistSignature(args.allowlist),
    args.mcpToolsPrompt?.chat ?? "",
  ].join("::");
}

export function buildPlannerFingerprint(args: {
  model: string;
  plannerPromptTemplate: string;
  sessionToken: string;
  instruction?: string;
  serverId?: string;
  mcpToolsPrompt?: McpPromptBundle;
}): string {
  return [
    args.model,
    args.plannerPromptTemplate,
    args.sessionToken,
    args.instruction ?? "",
    args.serverId ?? "",
    args.mcpToolsPrompt?.planner ?? "",
  ].join("::");
}

function formatAllowlistSignature(
  allowlist?: SafeAllowlistEntry[],
): string {
  if (!allowlist?.length) {
    return "none";
  }
  return allowlist
    .filter((entry) => entry.enabled)
    .map((entry) => entry.app_id || entry.display_name || "unknown")
    .sort()
    .join("|");
}

export function limitConversationMemory(
  memory: ConversationMemory | undefined,
  limit: number,
): ConversationMemory | undefined {
  if (!memory) {
    return undefined;
  }
  if (memory.type === "turns") {
    const turns = memory.turns || [];
    if (turns.length > limit) {
      return { ...memory, turns: turns.slice(-limit) };
    }
  }
  return memory;
}

export type { ConversationTurn, ResponseHandleRegistry, SafeAllowlistEntry };
