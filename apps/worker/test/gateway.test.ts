import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../src/providers", () => ({
  createProviderClient: vi.fn(),
}));

import { requestGatewayReply } from "../src/gateway";
import { createProviderClient } from "../src/providers";
import type { GatewayConfig } from "../src/config";
import type { McpPlannerHistory } from "@sprite/shared";

type MessagePart = { text?: string };
type InputMessage = { role: string; content: MessagePart[] };
type ProviderPayload = { input: InputMessage[] };

const baseConfig: GatewayConfig = {
  url: "https://gateway.example.com",
  token: "test-token",
  model: "test-model",
  provider: "openrouter",
  providerApiKey: undefined,
  systemPromptTemplate: "SYSTEM",
  plannerPromptTemplate: "PLANNER",
  personality: "Neutral",
};

const mockCreateResponse = vi.fn();
const mockContinueResponse = vi.fn();
const mockedCreateProviderClient = vi.mocked(createProviderClient);
const fetchMock = vi.fn();
const dummyFetch = fetchMock as unknown as typeof fetch;

beforeEach(() => {
  mockCreateResponse.mockReset();
  mockContinueResponse.mockReset();
  fetchMock.mockReset();
  mockedCreateProviderClient.mockReturnValue({
    createResponse: mockCreateResponse,
    continueResponse: mockContinueResponse,
  });
});

describe("requestGatewayReply", () => {
  it("parses planner responses with an MCP plan", async () => {
    const assistantPayload = JSON.stringify({
      mcp_plan: {
        session_token: "abc123",
        actions: [
          {
            server_id: "browsermcp",
            tool_name: "browser_navigate",
            arguments: { url: "https://example.com/docs" },
            confirmation_required: false,
            status_note: "Opening docs",
          },
        ],
        reply: "Trying the documentation site.",
        reply_speak: false,
      },
    });

    mockCreateResponse.mockResolvedValue({
      ok: true,
      response: {
        id: "resp-planner-1",
        output: [
          {
            content: [{ type: "output_text", text: assistantPayload }],
          },
        ],
      },
    });

    const result = await requestGatewayReply("Open the docs", baseConfig, dummyFetch, {
      promptMode: "planner",
    });

    const payload = mockCreateResponse.mock.calls[0]?.[0];
    expect(payload?.text?.format?.name).toBe("planner_plan");

    expect(result.ok).toBe(true);
    if (result.ok && result.mode === "planner") {
      expect(result.reply.mcp_plan.actions).toHaveLength(1);
      expect(result.reply.mcp_plan.actions[0]).toMatchObject({
        server_id: "browsermcp",
        tool_name: "browser_navigate",
        status_note: "Opening docs",
      });
      expect(result.reply.mcp_plan.reply).toBe("Trying the documentation site.");
    }
  });

  it("fails planner responses that omit an MCP plan", async () => {
    const assistantPayload = JSON.stringify({
      reply: "Sure, here is what I found.",
      intents: [{ type: "converse" }],
    });

    mockCreateResponse.mockResolvedValue({
      ok: true,
      response: {
        id: "resp-planner-2",
        output: [
          {
            content: [{ type: "output_text", text: assistantPayload }],
          },
        ],
      },
    });

    const result = await requestGatewayReply("Find the docs", baseConfig, dummyFetch, {
      promptMode: "planner",
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toMatch(/planner response did not match schema/i);
    }
  });

  it("fills session tokens when planner response omits one", async () => {
    const assistantPayload = JSON.stringify({
      mcp_plan: {
        actions: [
          {
            server_id: "browsermcp",
            tool_name: "browser_navigate",
            confirmation_required: false,
          },
        ],
        reply: "Opening docs.",
        reply_speak: false,
      },
    });

    mockCreateResponse.mockResolvedValue({
      ok: true,
      response: {
        id: "resp-planner-3",
        output: [
          {
            content: [{ type: "output_text", text: assistantPayload }],
          },
        ],
      },
    });

    const result = await requestGatewayReply("Open docs", baseConfig, dummyFetch, {
      promptMode: "planner",
    });

    expect(result.ok).toBe(true);
    if (result.ok && result.mode === "planner") {
      expect(result.reply.mcp_plan.session_token).toMatch(/^session_|[0-9a-f-]{36}$/);
      expect(result.reply.mcp_plan.actions[0]?.step_index).toBe(0);
    }
  });

  it("reuses provided session token when planner response omits one", async () => {
    const assistantPayload = JSON.stringify({
      mcp_plan: {
        actions: [
          {
            server_id: "browsermcp",
            tool_name: "browser_snapshot",
            confirmation_required: false,
          },
        ],
        reply: "Taking a snapshot.",
        reply_speak: false,
      },
    });

    mockCreateResponse.mockResolvedValue({
      ok: true,
      response: {
        id: "resp-chat-1",
        output: [
          {
            content: [{ type: "output_text", text: assistantPayload }],
          },
        ],
      },
    });

    const result = await requestGatewayReply("Snapshot the page", baseConfig, dummyFetch, {
      promptMode: "planner",
      sessionToken: "session-existing",
    });

    expect(result.ok).toBe(true);
    if (result.ok && result.mode === "planner") {
      expect(result.reply.mcp_plan.session_token).toBe("session-existing");
    }
  });

  it("accepts planner responses with empty actions", async () => {
    const assistantPayload = JSON.stringify({
      mcp_plan: {
        session_token: "abc123",
        actions: [],
        reply: "Workflow complete.",
        reply_speak: true,
      },
    });

    mockCreateResponse.mockResolvedValue({
      ok: true,
      response: {
        id: "resp-chat-2",
        output: [
          {
            content: [{ type: "output_text", text: assistantPayload }],
          },
        ],
      },
    });

    const result = await requestGatewayReply("All done?", baseConfig, dummyFetch, {
      promptMode: "planner",
    });

    expect(result.ok).toBe(true);
    if (result.ok && result.mode === "planner") {
      expect(result.reply.mcp_plan.actions).toHaveLength(0);
      expect(result.reply.mcp_plan.reply).toBe("Workflow complete.");
    }
  });

  it("includes planner history context in prompt messages", async () => {
    const assistantPayload = JSON.stringify({
      mcp_plan: {
        session_token: "session-1",
        actions: [],
        reply: "Retrying the plan.",
        reply_speak: false,
      },
    });

    mockCreateResponse.mockImplementation(async (payload: ProviderPayload) => {
      const userMessage = payload.input[payload.input.length - 1];
      expect(userMessage.role).toBe("user");
      const combined = userMessage.content
        .map((part: MessagePart) => part.text ?? "")
        .join("\n");
      expect(combined).toContain("MCP history");
      expect(combined).toContain("status=transport_error");
      return {
        ok: true,
        response: {
          id: "resp-history-ctx",
          output: [
            {
              content: [{ type: "output_text", text: assistantPayload }],
            },
          ],
        },
      };
    });

    const plannerHistory: McpPlannerHistory = [
      {
        step_index: 0,
        action: {
          server_id: "browsermcp",
          tool_name: "browser_navigate",
          arguments: { url: "https://example.com" },
        },
        status: "transport_error",
        error: "Navigation failed",
        mcp_response: { message: "Navigation failed" },
      },
    ];

    const result = await requestGatewayReply("Try again", baseConfig, dummyFetch, {
      promptMode: "planner",
      plannerHistory,
    });

    expect(result.ok).toBe(true);
  });

  it("limits planner conversation memory to the last 20 turns", async () => {
    const assistantPayload = JSON.stringify({
      mcp_plan: {
        session_token: "session-1",
        actions: [],
        reply: "Summarizing recent context.",
        reply_speak: false,
      },
    });

    mockCreateResponse.mockImplementation(async (payload: ProviderPayload) => {
      const contextMessage = payload.input.find((message: InputMessage) =>
        message.role === "system"
          ? message.content.some((part: MessagePart) =>
              part.text?.startsWith("Previous conversation context"),
            )
          : false,
      );
      expect(contextMessage).toBeDefined();
      const content = contextMessage
        ? contextMessage.content.map((part: MessagePart) => part.text ?? "").join("")
        : "";
      expect(content.includes("token-24")).toBe(true);
      expect(content.includes("token-05")).toBe(true);
      expect(content.includes("token-04")).toBe(false);
      expect(content.includes("token-00")).toBe(false);
      return {
        ok: true,
        response: {
          id: "resp-history",
          output: [
            {
              content: [{ type: "output_text", text: assistantPayload }],
            },
          ],
        },
      };
    });

    const turns = Array.from({ length: 25 }, (_, i) => ({
      role: i % 2 === 0 ? "user" as const : "assistant" as const,
      content: `token-${i.toString().padStart(2, "0")}`,
      timestamp: Date.now() + i,
    }));

    const result = await requestGatewayReply("Remember", baseConfig, dummyFetch, {
      promptMode: "planner",
      conversationMemory: { type: "turns", turns: turns.slice(-20) },
    });

    expect(result.ok).toBe(true);
  });

  it("instructs the planner to prefer the nominated server", async () => {
    const assistantPayload = JSON.stringify({
      mcp_plan: {
        session_token: "session-1",
        actions: [],
        reply: "Continuing the plan.",
        reply_speak: false,
      },
    });

    mockCreateResponse.mockImplementation(async (payload: ProviderPayload) => {
      expect(
        payload.input.some(
          (message: InputMessage) =>
            message.role === "system" &&
            message.content.some((part: MessagePart) =>
              part.text?.includes("Prefer MCP server 'browsermcp'"),
            ),
        ),
      ).toBe(true);
      const userMessage = payload.input[payload.input.length - 1];
      expect(userMessage.role).toBe("user");
      expect(
        userMessage.content.some((part: MessagePart) =>
          part.text?.includes("Do not loop on steps that already failed"),
        ),
      ).toBe(true);
      return {
        ok: true,
        response: {
          id: "resp-prefer",
          output: [
            {
              content: [{ type: "output_text", text: assistantPayload }],
            },
          ],
        },
      };
    });

    const plannerHistory: McpPlannerHistory = [
      {
        step_index: 0,
        action: {
          server_id: "browsermcp",
          tool_name: "browser_navigate",
          confirmation_required: false,
        },
        status: "transport_error",
      },
    ];

    const result = await requestGatewayReply("Plan again", baseConfig, dummyFetch, {
      promptMode: "planner",
      plannerServerId: "browsermcp",
      plannerHistory,
    });

    expect(result.ok).toBe(true);
  });

  it("parses chat responses without MCP plans", async () => {
    const assistantPayload = JSON.stringify({
      reply: "Here is the information you requested.",
      intents: [{ type: "converse" }],
    });

    mockCreateResponse.mockResolvedValue({
      ok: true,
      response: {
        id: "resp-chat-tail",
        output: [
          {
            content: [{ type: "output_text", text: assistantPayload }],
          },
        ],
      },
    });

    const result = await requestGatewayReply("Tell me something", baseConfig, dummyFetch);

    expect(result.ok).toBe(true);
    if (result.ok && result.mode === "chat") {
      expect(result.reply.reply).toBe("Here is the information you requested.");
    }
  });

  it("includes unsynced conversation turns when continuing a chat thread", async () => {
    const assistantPayload = JSON.stringify({
      reply: "Sure, let's continue.",
      intents: [{ type: "converse" }],
    });

    mockContinueResponse.mockResolvedValue({
      ok: true,
      response: {
        id: "resp-chat-continue",
        output: [
          {
            content: [{ type: "output_text", text: assistantPayload }],
          },
        ],
      },
    });

    const deltas = [
      {
        role: "assistant" as const,
        content: "Finished reviewing your inbox.",
        timestamp: 111,
      },
    ];

    const result = await requestGatewayReply("What's next?", baseConfig, dummyFetch, {
      previousResponseId: "resp-chat-prev",
      conversationDeltas: deltas,
    });

    expect(result.ok).toBe(true);
    const payload = mockContinueResponse.mock.calls[0]?.[0];
    expect(payload.previous_response_id).toBe("resp-chat-prev");
    expect(payload.input?.length).toBe(2);
    expect(payload.input?.[0]?.role).toBe("assistant");
    expect(payload.input?.[0]?.content?.[0]?.text).toContain("Finished reviewing your inbox.");
    expect(payload.input?.[1]?.role).toBe("user");
    expect(payload.input?.[1]?.content?.[0]?.text).toBe("What's next?");
  });

  it("sends only the latest planner delta when continuing a thread", async () => {
    const assistantPayload = JSON.stringify({
      mcp_plan: {
        session_token: "session-1",
        actions: [],
        reply: "Continuing the thread.",
        reply_speak: false,
      },
    });

    mockContinueResponse.mockResolvedValue({
      ok: true,
      response: {
        id: "resp-planner-continue",
        output: [
          {
            content: [{ type: "output_text", text: assistantPayload }],
          },
        ],
      },
    });

    const history: McpPlannerHistory = [
      {
        step_index: 0,
        action: { server_id: "browsermcp", tool_name: "browser_navigate" },
        status: "completed",
      },
      {
        step_index: 1,
        action: { server_id: "browsermcp", tool_name: "browser_snapshot" },
        status: "transport_error",
        error: "Page timed out",
      },
    ];

    const result = await requestGatewayReply("Find docs", baseConfig, dummyFetch, {
      promptMode: "planner",
      plannerHistory: history,
      previousResponseId: "resp-prev",
    });

    expect(result.ok).toBe(true);
    const payload = mockContinueResponse.mock.calls[0]?.[0];
    expect(payload.previous_response_id).toBe("resp-prev");
    const userText = payload.input?.[0]?.content?.[0]?.text ?? "";
    expect(userText).toContain("Step 1");
    expect(userText).toContain("browser_snapshot");
    expect(userText).not.toContain("Step 0");
    expect(userText).not.toContain("Original user request");
  });
});

