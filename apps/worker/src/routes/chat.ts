import type { Context } from "hono";
import type { ConversationMemory, ConversationTurn } from "@sprite/shared";

import type { WorkerBindings } from "../config";
import { ConfigError, getGatewayConfig, type GatewayConfig } from "../config";
import { buildFallbackReply } from "../fallback";
import {
  requestGatewayReply,
  type GatewayReplySuccessChat,
} from "../gateway";
import type { McpPromptBundle } from "../mcp";
import {
  buildUxFingerprint,
  clearResponseHandle,
  getMachineStateDO,
  limitConversationMemory,
  loadMachineContext,
  persistResponseHandle,
  type ResponseHandleRegistry,
  type SafeAllowlistEntry,
} from "../machine_state";
import {
  LLM_TURN_WINDOW,
  TOKEN_SUMMARY_REASON,
  UX_TOKEN_SUMMARY_THRESHOLD,
} from "../constants";

export function registerChatRoutes(app: { post: Function }): void {
  app.post("/chat", async (c: Context<{ Bindings: WorkerBindings }>) => {
    const { query, machineId } = await parseChatPayload(c);

    if (!query) {
      return c.json(buildFallbackReply(""));
    }

    let config: GatewayConfig;
    try {
      config = getGatewayConfig(c.env);
    } catch (error) {
      if (error instanceof ConfigError) {
        console.error(`[worker] configuration error: ${error.message}`);
        return c.text(`worker configuration error: ${error.message}`, 500);
      }
      console.error("[worker] unexpected configuration error", error);
      return c.text("worker configuration error", 500);
    }

    let chatContext: ChatContext = {};
    if (machineId) {
      try {
        chatContext = await loadChatContextForChat(machineId, c.env, query);
      } catch (error) {
        console.error("[worker] failed to prepare machine state:", error);
      }
    }

    const {
      conversationMemory,
      allowlist,
      personality,
      mcpToolsPrompt,
      responseHandles,
      pendingUserTurn,
      machineStub,
    } = chatContext;

    const uxFingerprint = machineId
      ? buildUxFingerprint({
          model: config.model,
          personality: personality ?? config.personality,
          systemPromptTemplate: config.systemPromptTemplate,
          allowlist,
          mcpToolsPrompt,
        })
      : undefined;

    const matchingUxHandle =
      machineId &&
      uxFingerprint &&
      responseHandles?.ux &&
      responseHandles.ux.fingerprint === uxFingerprint
        ? responseHandles.ux
        : undefined;
    const existingUxResponseId = matchingUxHandle?.response_id;
    const lastSyncedTurnTs =
      machineId && responseHandles?.ux
        ? responseHandles.ux.last_synced_turn_ts
        : undefined;
    const uxTokenTotal = matchingUxHandle?.token_total ?? 0;
    const uxSummaryRequired = matchingUxHandle?.summary_required ?? false;

    let conversationDeltas: ConversationTurn[] | undefined;
    if (
      existingUxResponseId &&
      conversationMemory?.type === "turns" &&
      conversationMemory.turns.length
    ) {
      if (typeof lastSyncedTurnTs === "number") {
        conversationDeltas = conversationMemory.turns.filter(
          (turn) => turn.timestamp > lastSyncedTurnTs,
        );
        if (!conversationDeltas.length) {
          conversationDeltas = undefined;
        }
      } else {
        conversationDeltas = undefined;
      }
    }

    const llmMemory = limitConversationMemory(conversationMemory, LLM_TURN_WINDOW);

    try {
      const result = await requestGatewayReply(query, config, fetch, {
        conversationMemory: llmMemory,
        conversationDeltas,
        allowlist,
        personality,
        mcpToolsPrompt,
        previousResponseId: existingUxResponseId,
        requireSummary: uxSummaryRequired,
        summaryReason: uxSummaryRequired ? TOKEN_SUMMARY_REASON : undefined,
      });

      if (result.ok && result.mode === "chat" && machineId) {
        await syncChatReply({
          c,
          env: c.env,
          config,
          result,
          machineId,
          uxFingerprint,
          uxTokenTotal,
          uxSummaryRequired,
          conversationDeltas,
          lastSyncedTurnTs,
          pendingUserTurn,
          machineStub,
        });
      }

      if (result.ok) {
        if (result.mode !== "chat") {
          console.error("[worker] chat endpoint received planner payload unexpectedly");
          c.header("X-Worker-Fallback", "planner_payload_mismatch");
          return c.json(buildFallbackReply(query));
        }

        return c.json(result.reply);
      }

      console.error(
        `[worker] gateway request failed: ${result.reason}`,
        result.rawContent,
      );
      c.header("X-Worker-Fallback", "llm_error");
    } catch (error) {
      console.error("[worker] gateway request threw", error);
      c.header("X-Worker-Fallback", "llm_exception");
    }

    return c.json(buildFallbackReply(query));
  });
}

type PendingUserTurn = {
  machine_id: string;
  conversation_turn: ConversationTurn;
};

type ChatContext = {
  conversationMemory?: ConversationMemory;
  allowlist?: SafeAllowlistEntry[];
  personality?: string;
  mcpToolsPrompt?: McpPromptBundle;
  responseHandles?: ResponseHandleRegistry;
  pendingUserTurn?: PendingUserTurn;
  machineStub?: DurableObjectStub;
};

async function parseChatPayload(
  c: Context<{ Bindings: WorkerBindings }>,
): Promise<{ query: string; machineId?: string }> {
  const body = (await c.req.json().catch(() => ({}))) as {
    query?: unknown;
    machine_id?: unknown;
  };
  const rawQuery = typeof body.query === "string" ? body.query : "";
  const query = rawQuery.trim();
  const machineId = typeof body.machine_id === "string" ? body.machine_id : undefined;

  return { query, machineId };
}

async function loadChatContextForChat(
  machineId: string,
  env: WorkerBindings,
  query: string,
): Promise<ChatContext> {
  const context = await loadMachineContext(machineId, env);

  const pendingUserTurn: PendingUserTurn = {
    machine_id: machineId,
    conversation_turn: {
      role: "user",
      content: query,
      timestamp: Math.floor(Date.now() / 1000),
      content_type: "input_text",
    },
  };

  return {
    conversationMemory: context.conversationMemory,
    allowlist: context.allowlist,
    personality: context.personality,
    mcpToolsPrompt: context.mcpToolsPrompt,
    responseHandles: context.responseHandles,
    pendingUserTurn,
    machineStub: context.stub,
  };
}


async function syncChatReply({
  c,
  env,
  config,
  result,
  machineId,
  uxFingerprint,
  uxTokenTotal,
  uxSummaryRequired,
  conversationDeltas,
  lastSyncedTurnTs,
  pendingUserTurn,
  machineStub,
}: {
  c: Context<{ Bindings: WorkerBindings }>;
  env: WorkerBindings;
  config: GatewayConfig;
  result: GatewayReplySuccessChat;
  machineId: string;
  uxFingerprint?: string;
  uxTokenTotal: number;
  uxSummaryRequired: boolean;
  conversationDeltas?: ConversationTurn[];
  lastSyncedTurnTs?: number;
  pendingUserTurn?: PendingUserTurn;
  machineStub?: DurableObjectStub;
}): Promise<void> {
  const usageTotal = result.usage?.totalTokens ?? 0;
  const updatedTokenTotal = uxSummaryRequired ? 0 : uxTokenTotal + usageTotal;
  const shouldRequestSummaryNext =
    !uxSummaryRequired && updatedTokenTotal >= UX_TOKEN_SUMMARY_THRESHOLD;
  const stub = machineStub ?? getMachineStateDO(machineId, env);
  const contentType: "output_text" | "refusal" | undefined =
    result.contentType === "refusal" ? "refusal" : "output_text";

  const assistantTurn = {
    machine_id: machineId,
    conversation_turn: {
      role: "assistant",
      content: result.reply.reply,
      timestamp: Math.floor(Date.now() / 1000),
      content_type: contentType,
      source: "assistant",
      intents: Array.isArray(result.reply.intents)
        ? result.reply.intents
            .map((intent) => {
              if (typeof intent === "string") {
                return intent;
              }
              if (intent && typeof intent === "object" && "type" in intent) {
                return String(intent.type);
              }
              return null;
            })
            .filter((value): value is string => Boolean(value))
        : undefined,
      context: result.reply.planner_context ?? result.reply.summary ?? undefined,
    },
  };
  const updates = [];
  if (pendingUserTurn) {
    updates.push(pendingUserTurn.conversation_turn);
  }
  updates.push(assistantTurn.conversation_turn);

  await stub.fetch(new Request("http://dummy/sync"), {
    method: "POST",
    body: JSON.stringify({
      machine_id: machineId,
      conversation_turns: updates,
    }),
  });

  if (!uxFingerprint) {
    return;
  }

  const syncedTimestamps: number[] = [];
  if (conversationDeltas?.length) {
    syncedTimestamps.push(...conversationDeltas.map((turn) => turn.timestamp));
  }
  if (pendingUserTurn) {
    syncedTimestamps.push(pendingUserTurn.conversation_turn.timestamp);
  }
  syncedTimestamps.push(assistantTurn.conversation_turn.timestamp);
  const maxSynced = syncedTimestamps.length
    ? Math.max(...syncedTimestamps)
    : undefined;
  const newLastSynced =
    typeof lastSyncedTurnTs === "number" && typeof maxSynced === "number"
      ? Math.max(lastSyncedTurnTs, maxSynced)
      : maxSynced ?? lastSyncedTurnTs;

  if (uxSummaryRequired) {
    await clearResponseHandle(stub, machineId, { role: "ux" });
    c.header("X-Worker-Conversation-Reset", "summary_completed");
    return;
  }

  await persistResponseHandle(stub, machineId, {
    role: "ux",
    responseId: result.responseId,
    fingerprint: uxFingerprint,
    model: config.model,
    lastSyncedTurnTs: newLastSynced,
    tokenTotal: updatedTokenTotal,
    summaryRequired: shouldRequestSummaryNext,
  });
  if (shouldRequestSummaryNext) {
    c.header("X-Worker-Conversation-Reset", "summary_pending");
  }
}
