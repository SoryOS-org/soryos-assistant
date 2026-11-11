import type { Context } from "hono";
import type {
  ConversationMemory,
  McpPlannerHistory,
  ResponseHandleRegistry,
} from "@sprite/shared";

import type { WorkerBindings } from "../config";
import { ConfigError, getGatewayConfig, type GatewayConfig } from "../config";
import { buildFallbackReply } from "../fallback";
import { requestGatewayReply } from "../gateway";
import type { McpPromptBundle } from "../mcp";
import {
  appendAssistantConversationTurn,
  buildPlannerFingerprint,
  clearResponseHandle,
  getMachineStateDO,
  limitConversationMemory,
  loadMachineContext,
  persistResponseHandle,
} from "../machine_state";
import {
  PLANNER_TURN_WINDOW,
  PLANNER_TOKEN_SUMMARY_THRESHOLD,
  TOKEN_SUMMARY_REASON,
} from "../constants";
import { McpPlannerHistorySchema } from "@sprite/shared";

export function registerPlannerRoutes(app: { post: Function }): void {
  app.post("/planner", async (c: Context<{ Bindings: WorkerBindings }>) => {
    const body = (await c.req.json().catch(() => ({}))) as Partial<{
      query: unknown;
      machine_id?: unknown;
      session_token?: unknown;
      instruction?: unknown;
      mcp_server?: unknown;
      history?: unknown;
    }>;
    const rawQuery = typeof body.query === "string" ? body.query : "";
    const query = rawQuery.trim();
    const machineId = typeof body.machine_id === "string" ? body.machine_id : undefined;
    const rawSessionToken =
      typeof body.session_token === "string" ? body.session_token.trim() : undefined;
    const sessionToken =
      rawSessionToken && rawSessionToken.length > 0 ? rawSessionToken : undefined;
    const plannerInstruction =
      typeof body.instruction === "string" ? body.instruction.trim() : undefined;
    const plannerServerId =
      typeof body.mcp_server === "string" ? body.mcp_server.trim() : undefined;

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

    const plannerModel = config.plannerModel ?? config.model;
    let conversationMemory: ConversationMemory | undefined;
    let loadedConversationMemory: ConversationMemory | undefined;
    let personality: string | undefined;
    let mcpToolsPrompt: McpPromptBundle | undefined;
    let plannerHistory: McpPlannerHistory | undefined;
    let responseHandles: ResponseHandleRegistry | undefined;
    let machineStub: DurableObjectStub | undefined;

    if (machineId) {
      try {
        const context = await loadMachineContext(machineId, c.env);
        machineStub = context.stub;
        loadedConversationMemory = context.conversationMemory;
        personality = context.personality;
        mcpToolsPrompt = context.mcpToolsPrompt;
        responseHandles = context.responseHandles;
      } catch (error) {
        console.error("[worker] planner failed to prepare machine state:", error);
      }
    }

    if (loadedConversationMemory) {
      conversationMemory = loadedConversationMemory;
    }

    if (body.history) {
      const parsedHistory = McpPlannerHistorySchema.safeParse(body.history);
      if (parsedHistory.success) {
        plannerHistory = parsedHistory.data;
      } else {
        console.error(
          "[worker] planner received invalid history payload:",
          parsedHistory.error.message,
        );
      }
    }

    let plannerFingerprint: string | undefined;
    let existingPlannerResponseId: string | undefined;
    let plannerTokenTotal = 0;
    let plannerSummaryRequired = false;
    if (machineId && sessionToken) {
      plannerFingerprint = buildPlannerFingerprint({
        model: plannerModel,
        plannerPromptTemplate: config.plannerPromptTemplate,
        sessionToken,
        instruction: plannerInstruction,
        serverId: plannerServerId,
        mcpToolsPrompt,
      });
      const storedHandle = responseHandles?.planner?.[sessionToken];
      if (storedHandle && storedHandle.fingerprint === plannerFingerprint) {
        existingPlannerResponseId = storedHandle.response_id;
        plannerTokenTotal = storedHandle.token_total ?? 0;
        plannerSummaryRequired = storedHandle.summary_required ?? false;
      }
    }

    const llmMemory = limitConversationMemory(
      conversationMemory,
      PLANNER_TURN_WINDOW,
    );

    try {
      const result = await requestGatewayReply(query, config, fetch, {
        conversationMemory: llmMemory,
        personality,
        promptMode: "planner",
        sessionToken,
        mcpToolsPrompt,
        plannerInstruction,
        plannerServerId,
        plannerHistory,
        previousResponseId: existingPlannerResponseId,
        requireSummary: plannerSummaryRequired,
        summaryReason: plannerSummaryRequired ? TOKEN_SUMMARY_REASON : undefined,
        modelOverride: plannerModel,
      });

      if (result.ok) {
        if (result.mode !== "planner") {
          console.error(
            "[worker] planner endpoint received chat payload unexpectedly",
          );
          c.header("X-Worker-Fallback", "chat_payload_mismatch");
          return c.json(buildFallbackReply(query));
        }
        const usageTotal = result.usage?.totalTokens ?? 0;
        const updatedPlannerTokens = plannerSummaryRequired
          ? 0
          : plannerTokenTotal + usageTotal;
        const shouldRequestPlannerSummary =
          !plannerSummaryRequired &&
          updatedPlannerTokens >= PLANNER_TOKEN_SUMMARY_THRESHOLD;
        if (machineId) {
          const replyText = result.reply.mcp_plan?.reply?.trim();
          if (replyText) {
            const contentType: "output_text" | "refusal" | undefined =
              result.contentType === "refusal" ? "refusal" : "output_text";
            await appendAssistantConversationTurn(
              machineId,
              replyText,
              c.env,
              contentType,
              {
                source: "planner",
                sessionToken,
                context: result.reply.mcp_plan?.planner_context ?? undefined,
              },
            );
          }
        }
        if (machineId && sessionToken && plannerFingerprint) {
          const stub = machineStub ?? getMachineStateDO(machineId, c.env);
          if (plannerSummaryRequired) {
            await clearResponseHandle(stub, machineId, {
              role: "planner",
              sessionToken,
            });
            c.header("X-Worker-Planner-Reset", "summary_completed");
          } else {
            await persistResponseHandle(stub, machineId, {
              role: "planner",
              responseId: result.responseId,
              fingerprint: plannerFingerprint,
              model: plannerModel,
              sessionToken,
              tokenTotal: updatedPlannerTokens,
              summaryRequired: shouldRequestPlannerSummary,
            });
            if (!result.reply.mcp_plan.actions?.length) {
              await clearResponseHandle(stub, machineId, {
                role: "planner",
                sessionToken,
              });
            } else if (shouldRequestPlannerSummary) {
              c.header("X-Worker-Planner-Reset", "summary_pending");
            }
          }
        }
        return c.json(result.reply);
      }

      console.error(
        `[worker] planner gateway request failed: ${result.reason}`,
        result.rawContent,
      );
      c.header("X-Worker-Fallback", "planner_llm_error");
    } catch (error) {
      console.error("[worker] planner gateway request threw", error);
      c.header("X-Worker-Fallback", "planner_llm_exception");
    }

    return c.json(buildFallbackReply(query));
  });
}
