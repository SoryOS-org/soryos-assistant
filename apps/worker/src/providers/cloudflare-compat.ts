import { GatewayConfig } from "../config";

import {
  ProviderResult,
  ResponsesClient,
  ResponsesContinueRequest,
  ResponsesCreateRequest,
  ResponsesPayload,
} from "./types";

export function createCloudflareCompatClient(
  config: GatewayConfig,
): ResponsesClient {
  const endpoint = buildResponsesUrl(config.url);
  const baseHeaders: Record<string, string> = {
    "Content-Type": "application/json",
    Accept: "application/json",
    "cf-aig-authorization": `Bearer ${config.token}`,
  };
  if (config.providerApiKey) {
    baseHeaders.Authorization = `Bearer ${config.providerApiKey}`;
  }

  async function send(payload: object): Promise<ProviderResult> {
    try {
      const response = await fetch(endpoint, {
        method: "POST",
        headers: baseHeaders,
        body: JSON.stringify(payload),
      });
      const rawBody = await safeReadText(response);
      if (!response.ok) {
        return {
          ok: false,
          reason: `Cloudflare compat request failed with status ${response.status}`,
          status: response.status,
          rawContent: rawBody,
        };
      }
      if (!rawBody) {
        return {
          ok: false,
          reason: "Cloudflare compat response was empty",
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
          reason: `Failed to parse Cloudflare compat response: ${(error as Error).message}`,
          rawContent: rawBody,
        };
      }
    } catch (error) {
      return {
        ok: false,
        reason: `Cloudflare compat request failed: ${(error as Error).message}`,
      };
    }
  }

  return {
    async createResponse(
      request: ResponsesCreateRequest,
    ): Promise<ProviderResult> {
      return send(request);
    },
    async continueResponse(
      request: ResponsesContinueRequest,
    ): Promise<ProviderResult> {
      return send(request);
    },
  };
}

function buildResponsesUrl(url: string): string {
  const trimmed = url.replace(/\/+$/, "");

  if (trimmed.endsWith("/responses")) {
    return trimmed;
  }

  if (trimmed.endsWith("/chat/completions")) {
    return trimmed.replace(/\/chat\/completions$/, "/responses");
  }

  if (trimmed.endsWith("/openai")) {
    return `${trimmed}/responses`;
  }

  if (trimmed.endsWith("/compat")) {
    return trimmed.replace(/\/compat$/, "/openai/responses");
  }

  return `${trimmed}/responses`;
}

async function safeReadText(response: Response): Promise<string | undefined> {
  try {
    return await response.text();
  } catch {
    return undefined;
  }
}
