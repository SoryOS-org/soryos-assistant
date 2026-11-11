import { useCallback } from "react";
import {
  useMcpRegistryStore,
} from "@/stores/mcpStore";
import { toggleMcpServer, openMcpConfigFile } from "@/lib/tauri";
import { Button } from "@/ui/Button";
import { Spinner } from "@/ui/Spinner";
import type { McpServerState } from "@/types";

const STATUS_LABELS: Record<string, string> = {
  unknown: "Unknown",
  starting: "Starting",
  running: "Running",
  stopped: "Stopped",
  error: "Error",
  disabled: "Disabled",
};

const STATUS_STYLE: Record<string, string> = {
  unknown: "border-sprite-border text-sprite-muted",
  starting: "border-sprite-accent/60 text-sprite-accent",
  running: "border-sprite-success/60 text-sprite-success",
  stopped: "border-sprite-border text-sprite-muted",
  error: "border-sprite-danger/60 text-sprite-danger",
  disabled: "border-sprite-border text-sprite-muted opacity-60",
};

function formatUpdatedAt(timestamp: number | null): string {
  if (!timestamp) {
    return "No schema sync yet.";
  }
  const date = new Date(Number(timestamp));
  if (Number.isNaN(date.getTime())) {
    return "Last sync unknown.";
  }
  return `Last sync ${date.toLocaleString()}`;
}

/**
 * McpRegistryView — port of v1 `mcpRegistry.js` UI: one card per server with
 * a status badge, enable/disable toggle (`toggle_mcp_server`), tool chips,
 * status note and "Last sync" timestamp, plus the "Open config" action.
 */
export function McpRegistryView() {
  const servers = useMcpRegistryStore((s) => s.servers);
  const loaded = useMcpRegistryStore((s) => s.loaded);
  const pendingToggles = useMcpRegistryStore((s) => s.pendingToggles);
  const statusMessage = useMcpRegistryStore((s) => s.statusMessage);
  const statusTone = useMcpRegistryStore((s) => s.statusTone);

  const handleToggle = useCallback(
    (server: McpServerState) => {
      const nextEnabled = !server.enabled;
      const displayName = server.displayName || server.serverName || server.id;
      const store = useMcpRegistryStore.getState();
      if (store.pendingToggles.includes(server.id)) {
        return;
      }
      store.setPendingToggle(server.id, true);
      store.setStatus(
        `${nextEnabled ? "Enabling" : "Disabling"} ${displayName}...`,
        "info",
      );
      void toggleMcpServer(server.id, nextEnabled)
        .then((summary) => {
          if (summary && typeof summary === "object") {
            store.applyConfigSummary(summary.mcp?.servers);
            store.setStatus(
              `${displayName} ${nextEnabled ? "enabled" : "disabled"}.`,
              "success",
            );
          }
        })
        .catch(() => {
          useMcpRegistryStore
            .getState()
            .setStatus(
              `Failed to ${nextEnabled ? "enable" : "disable"} ${displayName}.`,
              "error",
            );
        })
        .finally(() => {
          useMcpRegistryStore.getState().setPendingToggle(server.id, false);
        });
    },
    [],
  );

  const openConfig = useCallback(() => {
    void openMcpConfigFile().catch(() => {
      useMcpRegistryStore
        .getState()
        .setStatus("Failed to open the MCP configuration file.", "error");
    });
  }, []);

  return (
    <section className="flex flex-col gap-3">
      <header className="flex items-center justify-between">
        <div>
          <h3 className="text-[14px] font-semibold text-sprite-text">MCP servers</h3>
          <p className="text-[11.5px] text-sprite-muted">
            Runtime registry — status comes from mcp-host events.
          </p>
        </div>
        <Button onClick={openConfig}>Open config file</Button>
      </header>

      {statusMessage ? (
        <p
          className={`text-[12px] ${
            statusTone === "error"
              ? "text-sprite-danger"
              : statusTone === "success"
                ? "text-sprite-success"
                : "text-sprite-muted"
          }`}
        >
          {statusMessage}
        </p>
      ) : null}

      {servers.length === 0 ? (
        <div className="rounded-lg border border-dashed border-sprite-border p-4 text-[12.5px] text-sprite-muted">
          {loaded ? "No MCP servers configured." : "Loading MCP registry…"}
          {!loaded ? <Spinner className="ml-2 align-middle" /> : null}
        </div>
      ) : (
        <ul className="flex flex-col gap-2">
          {servers.map((server) => (
            <ServerCard
              key={server.id}
              server={server}
              pending={pendingToggles.includes(server.id)}
              onToggle={() => handleToggle(server)}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function ServerCard({
  server,
  pending,
  onToggle,
}: {
  server: McpServerState;
  pending: boolean;
  onToggle: () => void;
}) {
  const title = server.displayName || server.serverName || server.id;
  const subtitleParts: string[] = [];
  if (server.displayName && server.displayName !== server.id) {
    subtitleParts.push(server.id);
  }
  if (server.transport) {
    subtitleParts.push(server.transport.toUpperCase());
  }
  if (server.serverVersion) {
    subtitleParts.push(`v${server.serverVersion}`);
  }

  return (
    <li
      className={`rounded-lg border border-sprite-border bg-sprite-surface p-3 ${
        server.enabled ? "" : "opacity-70"
      }`}
    >
      <header className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <h4 className="truncate text-[13.5px] font-medium text-sprite-text">{title}</h4>
          <p className="truncate text-[11px] text-sprite-muted">
            {subtitleParts.join(" • ") || server.id}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <span
            className={`rounded-full border px-2 py-0.5 text-[10.5px] ${
              STATUS_STYLE[server.status] ?? STATUS_STYLE.unknown
            }`}
          >
            {STATUS_LABELS[server.status] ?? STATUS_LABELS.unknown}
          </span>
          <Button
            variant="ghost"
            onClick={onToggle}
            disabled={pending}
            title={server.enabled ? "Disable this server" : "Enable this server"}
          >
            {pending ? <Spinner /> : server.enabled ? "Disable" : "Enable"}
          </Button>
        </div>
      </header>

      <p className="mt-2 text-[11px] text-sprite-muted">
        Tools {server.toolCount} • Resources {server.resourceCount}
      </p>

      {server.tools.length ? (
        <div className="mt-1.5 flex flex-wrap gap-1">
          {server.tools.map((tool) => (
            <span
              key={tool.name}
              title={tool.description?.trim() || undefined}
              className="rounded bg-surface-raised px-1.5 py-0.5 text-[10.5px] text-sprite-muted"
            >
              {tool.name}
            </span>
          ))}
        </div>
      ) : (
        <p className="mt-1.5 text-[11px] text-sprite-muted">
          No tools reported yet.
        </p>
      )}

      {server.statusNote ? (
        <p className="mt-1.5 text-[11px] text-sprite-muted">{server.statusNote}</p>
      ) : null}
      <p className="mt-1 text-[10.5px] text-sprite-muted">{formatUpdatedAt(server.updatedAtMs)}</p>
    </li>
  );
}
