import {
  Reply,
  ReplySchema,
  ConversationMemory,
  ConversationTurn,
  SafeAllowlistEntry,
  McpPlanResult,
  McpPlanResultSchema,
  McpPlanAction,
  McpPlannerHistory,
} from "@sprite/shared";

import { GatewayConfig, renderSystemPrompt } from "./config";
import type { McpPromptBundle } from "./mcp";
import {
  JsonSchemaResponseFormat,
  ResponsesPayload,
  ResponseMessage,
  createProviderClient,
} from "./providers";

export interface GatewayReplySuccessChat {
  ok: true;
  mode: "chat";
  reply: Reply;
  rawContent: string;
  responseId: string;
  contentType?: string;
  usage?: GatewayUsage;
}

export interface GatewayReplySuccessPlanner {
  ok: true;
  mode: "planner";
  reply: McpPlanResult;
  rawContent: string;
  responseId: string;
  contentType?: string;
  usage?: GatewayUsage;
}

export type GatewayReplySuccess = GatewayReplySuccessChat | GatewayReplySuccessPlanner;

export interface GatewayReplyFailure {
  ok: false;
  reason: string;
  rawContent?: string;
  status?: number;
}

export type GatewayReplyResult = GatewayReplySuccess | GatewayReplyFailure;

interface GatewayUsage {
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
}

// Options for gateway request that can include conversation context
// TODO: This will be refactored when we add support for multiple LLM providers
// (OpenRouter, OpenAI, Gemini, etc.). The provider abstraction layer should handle
// converting these options into provider-specific request formats.
interface GatewayRequestOptions {
  conversationMemory?: ConversationMemory;
  conversationDeltas?: ConversationTurn[];
  allowlist?: SafeAllowlistEntry[];
  personality?: string;
  promptMode?: "chat" | "planner";
  sessionToken?: string;
  mcpToolsPrompt?: McpPromptBundle;
  plannerInstruction?: string;
  plannerServerId?: string;
  plannerHistory?: McpPlannerHistory;
  previousResponseId?: string;
  requireSummary?: boolean;
  summaryReason?: string;
  modelOverride?: string;
}

const UX_INTENTS_SCHEMA: Record<string, unknown> = {
  type: "array",
  items: {
    anyOf: [
      {
        type: "object",
        additionalProperties: false,
        required: ["type"],
        properties: {
          type: { type: "string", const: "converse" },
        },
      },
      {
        type: "object",
        additionalProperties: false,
        required: ["type", "app_name"],
        properties: {
          type: { type: "string", const: "open_app" },
          app_name: { type: "string" },
        },
      },
      {
        type: "object",
        additionalProperties: false,
        required: ["type", "query"],
        properties: {
          type: { type: "string", const: "open_app_not_found" },
          query: { type: "string" },
        },
      },
      {
        type: "object",
        additionalProperties: false,
        required: ["type", "query", "candidates"],
        properties: {
          type: { type: "string", const: "open_app_not_found" },
          query: { type: "string" },
          candidates: {
            type: "array",
            items: { type: "string" },
          },
        },
      },
      {
        type: "object",
        additionalProperties: false,
        required: ["type", "mcp_server", "instruction"],
        properties: {
          type: { type: "string", const: "mcp_plan" },
          mcp_server: { type: "string" },
          instruction: { type: "string" },
        },
      },
    ],
  },
};

const UX_RESPONSE_SCHEMA: Record<string, unknown> = {
  type: "object",
  additionalProperties: false,
  properties: {
    reply: { type: "string" },
    intents: UX_INTENTS_SCHEMA,
  },
  patternProperties: {
    "^summary$": { type: "string" },
    "^planner_context$": { type: "string" },
  },
  required: ["reply", "intents"],
};

const UX_RESPONSE_FORMAT: JsonSchemaResponseFormat = {
  type: "json_schema",
  name: "ux_reply",
  schema: UX_RESPONSE_SCHEMA,
  strict: true,
};

const PLANNER_RESPONSE_SCHEMA: Record<string, unknown> = {
  type: "object",
  additionalProperties: false,
  required: ["mcp_plan"],
  properties: {
    mcp_plan: {
      type: "object",
      additionalProperties: false,
      required: ["actions", "reply", "reply_speak"],
      properties: {
        actions: {
          type: "array",
          items: {
            type: "object",
            required: [
              "server_id",
              "tool_name",
              "confirmation_required",
            ],
            properties: {
              server_id: { type: "string" },
              tool_name: { type: "string" },
              confirmation_required: { type: "boolean" },
            },
            patternProperties: {
              "^arguments$": {
                type: "object",
                additionalProperties: false,
                patternProperties: {
                  "^.*$": {},
                },
              },
              "^status_note$": { type: "string" },
              "^timeout_ms$": { type: "integer" },
              "^step_index$": { type: "integer" },
            },
            additionalProperties: false,
          },
        },
        reply: { type: "string" },
        reply_speak: { type: "boolean" },
      },
      patternProperties: {
        "^session_token$": { type: "string" },
        "^planner_context$": { type: "string" },
        "^summary$": { type: "string" },
      },
    },
  },
};

const PLANNER_RESPONSE_FORMAT: JsonSchemaResponseFormat = {
  type: "json_schema",
  name: "planner_plan",
  schema: PLANNER_RESPONSE_SCHEMA,
  strict: true,
};

export async function requestGatewayReply(
  query: string,
  config: GatewayConfig,
  fetcher: typeof fetch,
  options?: GatewayRequestOptions,
): Promise<GatewayReplyResult> {
  const providerClient = createProviderClient(config, fetcher);

  const promptMode = options?.promptMode ?? "chat";
  const requestModel = options?.modelOverride ?? config.model;
  const previousResponseId =
    typeof options?.previousResponseId === "string" &&
    options.previousResponseId.length > 0
      ? options.previousResponseId
      : undefined;
  const isContinuation = Boolean(previousResponseId);
  const responseFormat =
    promptMode === "planner" ? PLANNER_RESPONSE_FORMAT : UX_RESPONSE_FORMAT;
  const reasoningConfig =
    promptMode === "planner" ? { effort: "minimal" as const } : undefined;
  const inputMessages = buildResponseInput(
    query,
    config,
    promptMode,
    isContinuation ? "continuation" : "initial",
    options,
  );

  const providerResult =
    isContinuation && previousResponseId
      ? await providerClient.continueResponse({
          previous_response_id: previousResponseId,
          input: inputMessages,
          model: requestModel,
          text: { format: responseFormat },
          reasoning: reasoningConfig,
        })
      : await providerClient.createResponse({
          model: requestModel,
          input: inputMessages,
          text: { format: responseFormat },
          reasoning: reasoningConfig,
        });

  if (!providerResult.ok) {
    if (
      isContinuation &&
      shouldRetryResponseThread(providerResult.status)
    ) {
      return requestGatewayReply(query, config, fetcher, {
        ...options,
        previousResponseId: undefined,
      });
    }
    return {
      ok: false,
      reason: providerResult.reason,
      status: providerResult.status,
      rawContent: providerResult.rawContent,
    };
  }

  const data = providerResult.response;
  let usage = extractUsage(data);
  const responseId =
    typeof data.id === "string" && data.id.length > 0 ? data.id : undefined;
  if (!responseId) {
    return {
      ok: false,
      reason: "Gateway response did not include a response_id",
    };
  }

  const content = extractMessageContent(data);

  if (!content) {
    return {
      ok: false,
      reason: "Gateway response did not include assistant content",
    };
  }

  let normalizedContent = normalizeAssistantContent(content.text);

  let candidate: unknown;
  try {
    candidate = JSON.parse(normalizedContent);
  } catch (error) {
    return {
      ok: false,
      reason: `Assistant response was not valid JSON: ${(error as Error).message}`,
      rawContent: normalizedContent,
    };
  }

  if (promptMode === "planner") {
    let plannerResponseId = responseId;
    let plannerContent = content;
    let plannerNormalized = normalizedContent;
    let plannerCandidate: unknown = candidate;
    let correctionAttempted = false;

    // eslint-disable-next-line no-constant-condition
    while (true) {
      const parsed = McpPlanResultSchema.safeParse(plannerCandidate);
      if (!parsed.success) {
        return {
          ok: false,
          reason: `Planner response did not match schema: ${parsed.error.message}`,
          rawContent: plannerNormalized,
        };
      }

      const plan = applyPlannerPostProcessing(
        parsed.data.mcp_plan,
        options?.sessionToken,
      );

      if (plan.actions.length <= 1) {
        console.debug?.(
          "[worker] planner response content type:",
          plannerContent.partType,
        );
        return {
          ok: true,
          mode: "planner",
          reply: { mcp_plan: plan },
          rawContent: plannerNormalized,
          responseId: plannerResponseId,
          contentType: plannerContent.partType,
          usage,
        };
      }

      if (correctionAttempted) {
        return {
          ok: false,
          reason:
            "Planner emitted multiple actions repeatedly; aborting to avoid duplicate execution",
          rawContent: plannerNormalized,
        };
      }

      correctionAttempted = true;
      console.warn?.(
        "[worker] planner emitted multiple actions; requesting a single-step plan",
      );
      const correctionMessages = [
        makeMessage(
          "user",
          "You must return exactly ONE MCP action per response. Provide only the next action and wait for new history before sending another.",
        ),
      ];
      const correctionResult = await providerClient.continueResponse({
        previous_response_id: plannerResponseId,
        input: correctionMessages,
        model: requestModel,
        text: { format: PLANNER_RESPONSE_FORMAT },
        reasoning: reasoningConfig,
      });

      if (!correctionResult.ok) {
        return {
          ok: false,
          reason: correctionResult.reason,
          status: correctionResult.status,
          rawContent: correctionResult.rawContent,
        };
      }

      const correctionData = correctionResult.response;
      const correctionResponseId =
        typeof correctionData.id === "string" && correctionData.id.length > 0
          ? correctionData.id
          : undefined;
      if (!correctionResponseId) {
        return {
          ok: false,
          reason: "Planner correction response did not include a response_id",
        };
      }

      const correctionContent = extractMessageContent(correctionData);
      if (!correctionContent) {
        return {
          ok: false,
          reason: "Planner correction response did not include assistant content",
        };
      }
      const correctionNormalized = normalizeAssistantContent(
        correctionContent.text,
      );

      let correctionCandidate: unknown;
      try {
        correctionCandidate = JSON.parse(correctionNormalized);
      } catch (error) {
        return {
          ok: false,
          reason: `Planner correction response was not valid JSON: ${(error as Error).message}`,
          rawContent: correctionNormalized,
        };
      }

      plannerResponseId = correctionResponseId;
      plannerContent = correctionContent;
      plannerNormalized = correctionNormalized;
      plannerCandidate = correctionCandidate;
      usage = extractUsage(correctionData) ?? usage;
    }
  }

  const parsedReply = ReplySchema.safeParse(candidate);
  if (!parsedReply.success) {
    return {
      ok: false,
      reason: `Assistant response did not match Reply schema: ${parsedReply.error.message}`,
      rawContent: normalizedContent,
    };
  }

  console.debug?.(
    "[worker] chat response content type:",
    content.partType,
  );
  return {
    ok: true,
    mode: "chat",
    reply: parsedReply.data,
    rawContent: normalizedContent,
    responseId,
    contentType: content.partType,
    usage,
  };
}

function applyPlannerPostProcessing(
  plan: McpPlanResult["mcp_plan"],
  requestedToken?: string,
): McpPlanResult["mcp_plan"] {
  const ensuredToken =
    plan.session_token && plan.session_token.trim().length > 0
      ? plan.session_token
      : requestedToken && requestedToken.trim().length > 0
      ? requestedToken
      : generateSessionToken();

  const dedupedActions = assignStepIndexes(plan.actions ?? []);
  const reply_speak =
    dedupedActions.length === 0 ? true : plan.reply_speak;

  return {
    ...plan,
    session_token: ensuredToken,
    actions: dedupedActions,
    reply_speak,
  };
}

function assignStepIndexes(actions: McpPlanAction[]): McpPlanAction[] {
  if (!actions.length) {
    return [];
  }

  let nextIndex = 0;
  return actions.map((action) => {
    if (typeof action.step_index === "number" && action.step_index >= 0) {
      nextIndex = Math.max(nextIndex, action.step_index + 1);
      return action;
    }

    const step_index = nextIndex;
    nextIndex += 1;
    return {
      ...action,
      step_index,
    };
  });
}

function generateSessionToken(): string {
  const globalCrypto = (globalThis as Record<string, unknown>).crypto as
    | { randomUUID?: () => string; getRandomValues?: (array: Uint8Array) => Uint8Array }
    | undefined;

  if (globalCrypto?.randomUUID) {
    return globalCrypto.randomUUID();
  }

  if (globalCrypto?.getRandomValues) {
    const bytes = new Uint8Array(16);
    globalCrypto.getRandomValues(bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0"));
    return `${hex.slice(0, 4).join("")}-${hex.slice(4, 6).join("")}-${hex
      .slice(6, 8)
      .join("")}-${hex.slice(8, 10).join("")}-${hex.slice(10, 16).join("")}`;
  }

  return `session_${Math.random().toString(36).slice(2)}${Date.now().toString(36)}`;
}

function buildResponseInput(
  query: string,
  config: GatewayConfig,
  promptMode: "chat" | "planner",
  phase: "initial" | "continuation",
  options?: GatewayRequestOptions,
): ResponseMessage[] {
  if (phase === "continuation") {
    return buildContinuationInput(query, promptMode, options);
  }
  return buildInitialInput(query, config, promptMode, options);
}

function buildInitialInput(
  query: string,
  config: GatewayConfig,
  promptMode: "chat" | "planner",
  options?: GatewayRequestOptions,
): ResponseMessage[] {
  const personality = options?.personality || config.personality;
  const template =
    promptMode === "planner"
      ? config.plannerPromptTemplate
      : config.systemPromptTemplate;
  const systemPrompt = renderSystemPrompt(template, personality);

  const messages: ResponseMessage[] = [makeMessage("system", systemPrompt)];

  if (promptMode === "planner" && options?.sessionToken) {
    messages.push(
      makeMessage(
        "system",
        `Reuse this session token in the mcp_plan: ${options.sessionToken}`,
      ),
    );
  }

  if (promptMode === "planner" && options?.plannerServerId) {
    messages.push(
      makeMessage(
        "system",
        `Prefer MCP server '${options.plannerServerId}' when it has the tools you need. If the required tooling is unavailable or failing there, use another allowed server or explain that the task cannot continue.`,
      ),
    );
  }

  if (promptMode === "planner" && options?.plannerInstruction) {
    messages.push(
      makeMessage(
        "system",
        `Planner instruction: ${options.plannerInstruction}`,
      ),
    );
  }

  let historySummary: string | undefined;
  if (promptMode === "planner" && options?.plannerHistory?.length) {
    historySummary = formatPlannerHistory(options.plannerHistory);
  }

  let conversationSummary: string | undefined;
  if (options?.conversationMemory) {
    if (options.conversationMemory.type === "turns") {
      const turns = options.conversationMemory.turns;
      if (turns.length > 0) {
        const context = turns
          .map((turn, index) => `Turn ${index + 1} (${turn.role}): ${turn.content}`)
          .join("\n");
        conversationSummary = context;
      }
    } else if (options.conversationMemory.type === "summary") {
      conversationSummary = `Summary: ${options.conversationMemory.summary}`;
    }
  }

  if (conversationSummary) {
    messages.push(
      makeMessage(
        "system",
        `Previous conversation context:\n${conversationSummary}`,
      ),
    );
  }

  if (options?.allowlist && options.allowlist.length > 0) {
    const allowlistText = formatAllowlistForPrompt(options.allowlist);
    messages.push(
      makeMessage(
        "system",
        `Available applications on this machine:\n${allowlistText}\n\nYou MUST only suggest apps from this list. If a user requests an app not in this list, use the 'open_app_not_found' intent. Do NOT suggest, mention, or acknowledge apps outside this list, even if asked directly or given alternative names.`,
      ),
    );
  }

  if (promptMode === "planner") {
    messages.push(
      makeMessage(
        "system",
        "Large MCP tool outputs may be paginated. When history references chunk_id metadata, call sprite_internal.fetch_chunk with the provided chunk_id/page to inspect the rest, then write a concise summary in your reply so future turns remain small. Use sprite_internal.find_text with a chunk_id + query when you need to search inside cached chunks without fetching every page.",
      ),
    );
  }

  const mcpCatalog =
    promptMode === "planner"
      ? options?.mcpToolsPrompt?.planner
      : options?.mcpToolsPrompt?.chat;
  if (mcpCatalog) {
    messages.push(
      makeMessage(
        "system",
        `${promptMode === "planner" ? "Planner MCP catalog" : "MCP tool catalog"}:\n${mcpCatalog}`,
      ),
    );
  }

  if (options?.requireSummary) {
    appendSummaryRequirement(messages, promptMode, options.summaryReason);
  }

  if (promptMode === "planner") {
    const userParts: string[] = [];
    userParts.push(
      "Based on the instruction and latest MCP history, decide the next actions.",
    );
    userParts.push(
      "Do not loop on steps that already failed. If progress is blocked, inform the user in your reply.",
    );
    userParts.push(`Original user request: ${query}`);
    if (historySummary) {
      userParts.push(`MCP history (latest first):\n${historySummary}`);
    }
    messages.push(makeMessage("user", userParts.join("\n\n")));
  } else {
    messages.push(makeMessage("user", query));
  }

  return messages;
}

function buildContinuationInput(
  query: string,
  promptMode: "chat" | "planner",
  options?: GatewayRequestOptions,
): ResponseMessage[] {
  if (promptMode === "planner") {
    const messages: ResponseMessage[] = [];
    if (options?.requireSummary) {
      appendSummaryRequirement(messages, "planner", options.summaryReason);
    }
    const segments: string[] = [];
    const deltaSummary = formatLatestPlannerDelta(options?.plannerHistory);
    if (deltaSummary) {
      segments.push(`Latest MCP result:\n${deltaSummary}`);
    }
    segments.push(
      "Use this update to decide the next MCP actions. Keep replies concise and only retry steps when you have new parameters.",
    );
    messages.push(makeMessage("user", segments.join("\n\n")));
    return messages;
  }

  const messages: ResponseMessage[] = [];
  if (options?.requireSummary) {
    appendSummaryRequirement(messages, "chat", options.summaryReason);
  }
  if (options?.conversationDeltas?.length) {
    options.conversationDeltas.forEach((turn) => {
      const role = turn.role === "assistant" ? "assistant" : "user";
      const contentType = normalizeContentType(turn.content_type, turn.role);
      messages.push(makeMessage(role, turn.content, contentType));
    });
  }
  messages.push(makeMessage("user", query, "input_text"));
  return messages;
}

function makeMessage(
  role: "system" | "user" | "assistant",
  text: string,
  contentType: "input_text" | "output_text" = "input_text",
): ResponseMessage {
  return {
    role,
    content: [
      {
        type: contentType,
        text,
      },
    ],
  };
}

function normalizeContentType(
  value: string | undefined,
  role: "user" | "assistant",
): "input_text" | "output_text" {
  if (value === "output_text") {
    return "output_text";
  }
  if (value === "input_text") {
    return "input_text";
  }
  return role === "assistant" ? "output_text" : "input_text";
}

function formatAllowlistForPrompt(allowlist: SafeAllowlistEntry[]): string {
  return allowlist
    .filter((app) => app.enabled)
    .map((app) => {
      const name = app.display_name || app.app_id;
      return `- ${name}`;
    })
    .join("\n");
}

function formatPlannerHistory(history: McpPlannerHistory): string | undefined {
  if (!history.length) {
    return undefined;
  }

  const lines: string[] = [];
  history.forEach((entry) => {
    const { action, status, mcp_response, error, latency_ms, step_index } = entry;
    const pieces = [
      `Step ${step_index}: ${action.server_id}/${action.tool_name}`,
    ];

    if (action.arguments && Object.keys(action.arguments).length > 0) {
      pieces.push(`args=${JSON.stringify(action.arguments).slice(0, 200)}`);
    }

    pieces.push(`status=${status}`);

    if (typeof latency_ms === "number") {
      pieces.push(`latency_ms=${latency_ms}`);
    }

    if (typeof error === "string" && error.length > 0) {
      pieces.push(`error=${error}`);
    }

    if (mcp_response !== undefined) {
      if (
        typeof mcp_response === "object" &&
        mcp_response !== null &&
        (mcp_response as { chunked?: unknown }).chunked
      ) {
        const chunkId =
          (mcp_response as { chunk_id?: string }).chunk_id || "unknown";
        const fieldPath =
          typeof (mcp_response as { field_path?: string }).field_path ===
          "string"
            ? (mcp_response as { field_path: string }).field_path
            : undefined;
        const totalPages =
          (mcp_response as { total_pages?: number }).total_pages;
        const preview =
          typeof (mcp_response as { preview?: string }).preview === "string"
            ? (mcp_response as { preview: string }).preview
            : "";
        pieces.push(
          `chunk_ref=${chunkId}${
            fieldPath ? ` field=${fieldPath}` : ""
          }${
            typeof totalPages === "number" ? ` (${totalPages} pages)` : ""
          }${preview ? ` preview=${preview.slice(0, 120)}` : ""}`,
        );
      } else {
        const responsePreview = JSON.stringify(mcp_response);
        pieces.push(`mcp_response=${responsePreview}`);
      }
    }

    lines.push(pieces.join(" | "));
  });

  if (!lines.length) {
    return undefined;
  }

  return lines.join("\n");
}

function appendSummaryRequirement(
  messages: ResponseMessage[],
  promptMode: "chat" | "planner",
  reason?: string,
): void {
  const reasonText = reason ? `Reason: ${reason}` : "";
  const summaryInstruction =
    promptMode === "planner"
      ? [
          "SUMMARY REQUIRED: Populate 'mcp_plan.summary' with 3-5 sentences capturing the exact in-flight state (what page/tool is open, pending waits, element refs, partial results) so the next planner iteration can resume without repeating work.",
          "Be specific about any timed waits, chunk IDs, or follow-up actions still needed, and still include any necessary reply fields and action list for this turn.",
        ]
      : [
          "SUMMARY REQUIRED: Populate the optional 'summary' field with no more than three sentences covering the key context so a new conversation can resume.",
          "Respond to the user normally in 'reply'.",
        ];
  const message = [summaryInstruction.join(" "), reasonText]
    .filter((part) => part.length > 0)
    .join("\n");
  messages.push(makeMessage("system", message));
}

function formatLatestPlannerDelta(
  history: McpPlannerHistory | undefined,
): string | undefined {
  if (!history || history.length === 0) {
    return undefined;
  }
  const latest = history[history.length - 1];
  return formatPlannerHistory([latest]);
}

function extractUsage(payload: ResponsesPayload): GatewayUsage | undefined {
  if (!payload.usage) {
    return undefined;
  }
  const input =
    typeof payload.usage.input_tokens === "number"
      ? payload.usage.input_tokens
      : undefined;
  const output =
    typeof payload.usage.output_tokens === "number"
      ? payload.usage.output_tokens
      : undefined;
  let total =
    typeof payload.usage.total_tokens === "number"
      ? payload.usage.total_tokens
      : undefined;
  if (typeof total !== "number" && (typeof input === "number" || typeof output === "number")) {
    total = (input ?? 0) + (output ?? 0);
  }
  if (
    typeof input !== "number" &&
    typeof output !== "number" &&
    typeof total !== "number"
  ) {
    return undefined;
  }
  return {
    inputTokens: input,
    outputTokens: output,
    totalTokens: total,
  };
}

interface MessageContentResult {
  text: string;
  partType?: string;
}

function extractMessageContent(
  payload: ResponsesPayload,
): MessageContentResult | undefined {
  const output = payload.output?.find(
    (entry) => Array.isArray(entry.content) && entry.content.length > 0,
  );
  if (!output?.content) {
    return undefined;
  }

  for (const part of output.content) {
    if (!part) {
      continue;
    }
    if (
      (part.type === "output_text" || part.type === "text") &&
      typeof part.text === "string"
    ) {
      const trimmed = part.text.trim();
      if (trimmed.length > 0) {
          return { text: trimmed, partType: part.type };
      }
    }
  }

  return undefined;
}

function shouldRetryResponseThread(status?: number): boolean {
  if (typeof status !== "number") {
    return false;
  }
  return status === 404 || status === 409;
}

function normalizeAssistantContent(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed.startsWith("```")) {
    return trimmed;
  }

  const lines = trimmed.split(/\r?\n/);
  const firstLine = lines[0] ?? "";
  const closingIndex = lines.lastIndexOf("```");

  if (!firstLine.startsWith("```") || closingIndex < 0) {
    return trimmed;
  }

  const body = lines.slice(1, closingIndex).join("\n").trim();
  return body.length > 0 ? body : trimmed;
}
