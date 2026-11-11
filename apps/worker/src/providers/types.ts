export type ResponseRole = "system" | "user" | "assistant";

export interface ResponseContentPart {
  type: "input_text" | "output_text";
  text: string;
}

export interface ResponseMessage {
  role: ResponseRole;
  content: ResponseContentPart[];
}

export interface JsonSchemaResponseFormat {
  type: "json_schema";
  name: string;
  schema: Record<string, unknown>;
  strict?: boolean;
}

export interface ResponsesCreateRequest {
  model: string;
  input: ResponseMessage[];
  text?: {
    format?: JsonSchemaResponseFormat;
  };
  reasoning?: ResponsesReasoning;
  metadata?: Record<string, unknown>;
}

export interface ResponsesContinueRequest {
  previous_response_id: string;
  input: ResponseMessage[];
  model?: string;
  text?: {
    format?: JsonSchemaResponseFormat;
  };
  reasoning?: ResponsesReasoning;
  metadata?: Record<string, unknown>;
}

export interface ResponsesReasoning {
  effort?: "none" | "minimal" | "low" | "medium" | "high";
}

export interface ResponseOutputContent {
  type: string;
  text?: string;
}

export interface ResponseOutputItem {
  id?: string;
  role?: string;
  content?: ResponseOutputContent[];
}

export interface ResponsesPayload {
  id?: string;
  output?: ResponseOutputItem[];
  usage?: ResponseUsage;
}

export interface ResponseUsage {
  input_tokens?: number;
  output_tokens?: number;
  total_tokens?: number;
}

export interface ProviderSuccess {
  ok: true;
  response: ResponsesPayload;
}

export interface ProviderFailure {
  ok: false;
  reason: string;
  status?: number;
  rawContent?: string;
}

export type ProviderResult = ProviderSuccess | ProviderFailure;

export interface ResponsesClient {
  createResponse(
    request: ResponsesCreateRequest,
  ): Promise<ProviderResult>;
  continueResponse(
    request: ResponsesContinueRequest,
  ): Promise<ProviderResult>;
}
