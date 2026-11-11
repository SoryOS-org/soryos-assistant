import { GatewayConfig } from "../config";

import {
  ProviderResult,
  ResponsesClient,
  ResponsesContinueRequest,
  ResponsesCreateRequest,
  ResponsesPayload,
} from "./types";

export function createOpenRouterClient(
  config: GatewayConfig,
  fetcher: typeof fetch,
): ResponsesClient {
  const endpoint = normalizeResponsesUrl(config.url);

  async function sendRequest(payload: object): Promise<ProviderResult> {
    const response = await fetcher(endpoint, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
        "cf-aig-authorization": `Bearer ${config.token}`,
      },
      body: JSON.stringify(payload),
    });

    const rawBody = await safeReadText(response);

    if (!response.ok) {
      return {
        ok: false,
        reason: `Gateway responded with status ${response.status}`,
        status: response.status,
        rawContent: rawBody,
      };
    }

    if (typeof rawBody !== "string" || rawBody.length === 0) {
      return {
        ok: false,
        reason: "Gateway response body was empty",
      };
    }

    try {
      const parsed = JSON.parse(rawBody) as ResponsesPayload;
      return {
        ok: true,
        response: parsed,
      };
    } catch (error) {
      return {
        ok: false,
        reason: `Failed to parse gateway response: ${(error as Error).message}`,
        rawContent: rawBody,
      };
    }
  }

  return {
    async createResponse(
      request: ResponsesCreateRequest,
    ): Promise<ProviderResult> {
      return sendRequest(request);
    },
    async continueResponse(
      request: ResponsesContinueRequest,
    ): Promise<ProviderResult> {
      return sendRequest(request);
    },
  };
}

async function safeReadText(response: Response): Promise<string | undefined> {
  try {
    return await response.text();
  } catch {
    return undefined;
  }
}

function normalizeResponsesUrl(url: string): string {
  const trimmed = url.replace(/\/+$/, "");
  if (trimmed.endsWith("/chat/completions")) {
    return trimmed.replace(/\/chat\/completions$/, "/responses");
  }
  if (trimmed.endsWith("/responses")) {
    return trimmed;
  }
  return `${trimmed}/responses`;
}
