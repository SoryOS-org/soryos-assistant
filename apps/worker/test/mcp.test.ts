import { describe, expect, it } from "vitest";

import { buildMcpToolsPrompt, extractMcpContext } from "../src/mcp";

describe("MCP context helpers", () => {
  it("extracts servers and tools from raw config", () => {
    const raw = {
      servers: {
        browsermcp: {
          tools: [
            {
              name: "browser_navigate",
              description: "Open a URL in the browser",
              input_schema: {
                type: "object",
                properties: {
                  url: {
                    type: "string",
                    description: "Destination URL",
                  },
                  new_tab: {
                    type: "boolean",
                    description: "Open in a new tab",
                  },
                },
                required: ["url"],
              },
            },
          ],
          resources: null,
          updated_at_ms: 1234,
        },
      },
    };

    const context = extractMcpContext(raw);
    expect(context).toBeDefined();
    expect(context?.servers).toHaveLength(1);
    const server = context?.servers[0];
    expect(server?.serverId).toBe("browsermcp");
    expect(server?.tools).toHaveLength(1);
    expect(server?.tools[0]?.name).toBe("browser_navigate");

    const prompt = context ? buildMcpToolsPrompt(context) : undefined;
    expect(prompt).toBeDefined();
    expect(prompt?.chat).toContain("browsermcp");
    expect(prompt?.chat).toContain("browser_navigate");
    expect(prompt?.planner).toContain("browsermcp");
    expect(prompt?.planner).toContain("browser_navigate");
    expect(prompt?.planner).toContain("\"required\": [");
    expect(prompt?.planner).toContain("\"url\"");
  });

  it("returns undefined when no servers exist", () => {
    const context = extractMcpContext({ servers: {} });
    expect(context).toBeUndefined();
  });
});

