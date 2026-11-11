import { z } from "zod";

export const McpPlanActionSchema = z.object({
  server_id: z.string().min(1),
  tool_name: z.string().min(1),
  arguments: z.record(z.unknown()).optional(),
  confirmation_required: z.boolean(),
  timeout_ms: z.number().int().positive().optional(),
  step_index: z.number().int().nonnegative().optional(),
  status_note: z.string().optional()
});
export type McpPlanAction = z.infer<typeof McpPlanActionSchema>;

export const McpPlanSchema = z.object({
  session_token: z.string().min(1).optional(),
  actions: z.array(McpPlanActionSchema).default([]),
  reply: z.string(),
  planner_context: z.string().optional(),
  reply_speak: z.boolean(),
  summary: z.string().optional()
});
export type McpPlan = z.infer<typeof McpPlanSchema>;

export const McpPlanResultSchema = z.object({
  mcp_plan: McpPlanSchema
});
export type McpPlanResult = z.infer<typeof McpPlanResultSchema>;

export const McpPlannerHistoryResultSchema = z.object({
  status: z.enum(["completed", "transport_error", "skipped", "timeout", "cancelled", "retry"]).optional(),
  error: z.string().optional(),
  output: z.unknown().optional(),
  latency_ms: z.number().int().nonnegative().optional()
});
export type McpPlannerHistoryResult = z.infer<typeof McpPlannerHistoryResultSchema>;

export const McpPlannerHistoryEntrySchema = z.object({
  step_index: z.number().int().nonnegative(),
  action: McpPlanActionSchema.partial().extend({
    server_id: z.string().min(1),
    tool_name: z.string().min(1)
  }),
  status: z.enum(["completed", "transport_error", "skipped", "timeout", "cancelled", "retry"]),
  mcp_response: z.unknown().optional(),
  error: z.string().optional(),
  latency_ms: z.number().int().nonnegative().optional()
});
export type McpPlannerHistoryEntry = z.infer<typeof McpPlannerHistoryEntrySchema>;

export const McpPlannerHistorySchema = z.array(McpPlannerHistoryEntrySchema);
export type McpPlannerHistory = z.infer<typeof McpPlannerHistorySchema>;

export const ReplySchema = z.object({
  reply: z.string(),
  summary: z.string().optional(),
  planner_context: z.string().optional(),
  intents: z.array(z.union([
    z.object({ type: z.literal("converse") }),
    z.object({ type: z.literal("open_app"), app_name: z.string() }),
    z.object({
      type: z.literal("open_app_not_found"),
      query: z.string(),
      candidates: z.array(z.string()).optional()
    }),
    z.object({
      type: z.literal("mcp_plan"),
      mcp_server: z.string(),
      instruction: z.string()
    })
  ]))
});
export type Reply = z.infer<typeof ReplySchema>;

export const ResponseHandleSchema = z.object({
  response_id: z.string(),
  role: z.enum(["ux", "planner"]),
  fingerprint: z.string().optional(),
  session_token: z.string().optional(),
  model: z.string().optional(),
  updated_at_ms: z.number().int().nonnegative().optional(),
  last_synced_turn_ts: z.number().int().nonnegative().optional(),
  token_total: z.number().int().nonnegative().optional(),
  summary_required: z.boolean().optional(),
});
export type ResponseHandle = z.infer<typeof ResponseHandleSchema>;

export const ResponseHandleRegistrySchema = z.object({
  ux: ResponseHandleSchema.optional(),
  planner: z.record(z.string(), ResponseHandleSchema).optional(),
});
export type ResponseHandleRegistry = z.infer<
  typeof ResponseHandleRegistrySchema
>;

export const UiStateSchema = z.object({
  state: z.enum(["Idle", "Listening", "Thinking", "Error"]),
  timestamp: z.number().int().nonnegative().optional()
});
export type UiStatePayload = z.infer<typeof UiStateSchema>;

export const UiVuSchema = z.object({
  level: z.number().min(0).max(1),
  timestamp: z.number().int().nonnegative().optional()
});
export type UiVuPayload = z.infer<typeof UiVuSchema>;

export const UiEventSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("State"), payload: UiStateSchema }),
  z.object({ kind: z.literal("Vu"), payload: UiVuSchema })
]);
export type UiEvent = z.infer<typeof UiEventSchema>;

export const UiSnapshotSchema = z.object({
  state: UiStateSchema.optional(),
  vu: UiVuSchema.optional()
});
export type UiSnapshot = z.infer<typeof UiSnapshotSchema>;

// Machine State Types
export const ConversationTurnSchema = z.object({
  role: z.enum(["user", "assistant"]),
  content: z.string(),
  timestamp: z.number().int().nonnegative(),
  content_type: z.string().optional(),
  source: z.string().optional(),
  intents: z.array(z.string()).optional(),
  context: z.string().optional(),
  session_token: z.string().optional()
});
export type ConversationTurn = z.infer<typeof ConversationTurnSchema>;

export const ConversationMemorySchema = z.union([
  z.object({
    type: z.literal("turns"),
    turns: z.array(ConversationTurnSchema)
  }),
  z.object({
    type: z.literal("summary"),
    summary: z.string(),
    lastUpdated: z.number().int().nonnegative()
  })
]);
export type ConversationMemory = z.infer<typeof ConversationMemorySchema>;

export const AllowlistEntrySchema = z.object({
  enabled: z.boolean(),
  present: z.boolean(),
  version: z.string().optional(),
  default_for: z.array(z.string()).optional(),
  extensions: z.array(z.string()).optional()
});
export type AllowlistEntry = z.infer<typeof AllowlistEntrySchema>;

export const MachineConfigSchema = z.record(z.unknown());
export type MachineConfig = z.infer<typeof MachineConfigSchema>;

export const MachineStateSchema = z.object({
  machine_id: z.string(),
  memory: ConversationMemorySchema,
  allowlist: z.record(z.string(), AllowlistEntrySchema),
  config: MachineConfigSchema,
  last_updated_ts: z.number().int().nonnegative(),
  responses: ResponseHandleRegistrySchema.optional()
});
export type MachineState = z.infer<typeof MachineStateSchema>;

export const MachineSyncRequestSchema = z.object({
  machine_id: z.string(),
  installed_apps: z.record(z.string(), AllowlistEntrySchema).optional(),
  enabled_apps: z.array(z.string()).optional(),
  disabled_apps: z.array(z.string()).optional(),
  conversation_turn: ConversationTurnSchema.optional(),
  config_patch: MachineConfigSchema.optional()
});
export type MachineSyncRequest = z.infer<typeof MachineSyncRequestSchema>;

export const SafeAllowlistEntrySchema = z.object({
  app_id: z.string(),
  enabled: z.boolean(),
  display_name: z.string().optional(),
  verbs: z.array(z.string()).optional()
});
export type SafeAllowlistEntry = z.infer<typeof SafeAllowlistEntrySchema>;
