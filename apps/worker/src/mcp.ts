type JsonRecord = Record<string, unknown>;

const INTERNAL_SERVER_ID = "sprite_internal";

export interface McpContext {
  servers: McpServerContext[];
  rawConfig?: unknown;
}

export interface McpServerContext {
  serverId: string;
  tools: McpToolContext[];
  resources?: unknown;
  updatedAtMs?: number;
}

export interface McpToolContext {
  name: string;
  description?: string;
  inputSchema?: JsonRecord;
}

export function extractMcpContext(raw: unknown): McpContext | undefined {
  if (!isRecord(raw)) {
    return undefined;
  }

  const serversField = raw.servers;
  const servers: McpServerContext[] = [];
  const configRecord = isRecord(raw.config) ? raw.config : undefined;
  const enabledMap = buildEnabledMap(configRecord);

  if (isRecord(serversField)) {
    for (const [serverId, value] of Object.entries(serversField)) {
      if (!isRecord(value)) {
        continue;
      }

      if (!isServerEnabled(serverId, enabledMap)) {
        continue;
      }

      const tools = parseTools(value.tools);
      const serverContext: McpServerContext = {
        serverId,
        tools,
        resources: value.resources,
        updatedAtMs: typeof value.updated_at_ms === "number" ? value.updated_at_ms : undefined,
      };
      servers.push(serverContext);
    }
  }

  if (servers.length === 0) {
    return undefined;
  }

  return {
    servers,
    rawConfig: raw.config,
  };
}

export interface McpPromptBundle {
  chat?: string;
  planner?: string;
}

export function buildMcpToolsPrompt(context: McpContext): McpPromptBundle | undefined {
  if (!context.servers.length) {
    return undefined;
  }

  const chatLines: string[] = [
    "Available MCP capabilities (reference only—the planner decides which tools to invoke):",
  ];
  const plannerLines: string[] = [
    "Planner MCP catalog (complete reference for tool invocation):",
  ];

  for (const server of context.servers) {
    chatLines.push(`Server '${server.serverId}':`);
    plannerLines.push(`Server '${server.serverId}':`);

    if (!server.tools.length) {
      chatLines.push("  - (no tools exposed)");
      plannerLines.push("  - (no tools exposed)");
      continue;
    }

    for (const tool of server.tools) {
      chatLines.push(`  - ${tool.name}`);
      plannerLines.push(`  - ${tool.name}`);

      const trimmedDescription =
        typeof tool.description === "string" && tool.description.trim().length > 0
          ? tool.description.trim()
          : undefined;
      if (trimmedDescription) {
        const descriptionLine = `      description: ${trimmedDescription}`;
        chatLines.push(descriptionLine);
        plannerLines.push(descriptionLine);
      }
      if (tool.inputSchema) {
        plannerLines.push("      args_schema:");
        const schemaLines = JSON.stringify(tool.inputSchema, null, 2)
          .split("\n")
          .map((entry) => `        ${entry}`);
        plannerLines.push(...schemaLines);
      }
    }
  }

  return {
    chat: chatLines.join("\n"),
    planner: plannerLines.join("\n"),
  };
}

function parseTools(value: unknown): McpToolContext[] {
  if (!Array.isArray(value)) {
    return [];
  }

  const tools: McpToolContext[] = [];
  for (const entry of value) {
    if (!isRecord(entry)) {
      continue;
    }
    const name = typeof entry.name === "string" ? entry.name : undefined;
    if (!name) {
      continue;
    }
    const rawInputSchema =
      isRecord((entry as Record<string, unknown>).input_schema)
        ? ((entry as Record<string, JsonRecord>).input_schema)
        : isRecord((entry as Record<string, unknown>).inputSchema)
        ? ((entry as Record<string, JsonRecord>).inputSchema)
        : undefined;
    const tool: McpToolContext = {
      name,
      description: typeof entry.description === "string" ? entry.description : undefined,
      inputSchema: rawInputSchema,
    };
    tools.push(tool);
  }
  return tools;
}


function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function buildEnabledMap(config: JsonRecord | undefined): Record<string, boolean> {
  if (!config) {
    return {};
  }

  const modernField = config.mcpServers;
  const legacyField = config.servers;

  const source = isRecord(modernField)
    ? modernField
    : isRecord(legacyField)
    ? legacyField
    : undefined;

  if (!source) {
    return {};
  }

  const map: Record<string, boolean> = {};
  for (const [serverId, details] of Object.entries(source)) {
    if (!isRecord(details)) {
      continue;
    }
    const enabledValue = details.enabled;
    if (typeof enabledValue === "boolean") {
      map[serverId] = enabledValue;
    }
  }

  return map;
}

function isServerEnabled(serverId: string, enabledMap: Record<string, boolean>): boolean {
  if (serverId === INTERNAL_SERVER_ID) {
    return true;
  }
  if (serverId in enabledMap) {
    return enabledMap[serverId];
  }
  // Default to enabled when no explicit flag is present.
  return true;
}


