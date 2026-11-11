import { GatewayConfig } from "../config";

import { createCloudflareCompatClient } from "./cloudflare-compat";
import { createOpenRouterClient } from "./openrouter";
import { ResponsesClient } from "./types";

export function createProviderClient(
  config: GatewayConfig,
  fetcher: typeof fetch,
): ResponsesClient {
  switch (config.provider) {
    case "cloudflare_compat":
      return createCloudflareCompatClient(config);
    case "openrouter":
    default:
      return createOpenRouterClient(config, fetcher);
  }
}

export type { ResponsesClient } from "./types";
export {
  JsonSchemaResponseFormat,
  ResponseContentPart,
  ResponseMessage,
  ResponsesContinueRequest,
  ResponsesCreateRequest,
  ResponsesPayload,
} from "./types";
