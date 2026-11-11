import { useMcpProgressStore } from "@/stores/mcpStore";

const STATUS_LABELS: Record<string, string> = {
  queued: "Queued",
  running: "Running",
  completed: "Completed",
  crashed: "Crashed",
};

const STATUS_STYLE: Record<string, string> = {
  queued: "text-sprite-muted",
  running: "text-sprite-accent",
  completed: "text-sprite-success",
  crashed: "text-sprite-danger",
};

function formatLatency(latencyMs: number | null): string | null {
  if (typeof latencyMs !== "number" || Number.isNaN(latencyMs) || latencyMs < 0) {
    return null;
  }
  if (latencyMs >= 1000) {
    return `${(latencyMs / 1000).toFixed(latencyMs >= 5000 ? 1 : 2)} s`;
  }
  return `${Math.round(latencyMs)} ms`;
}

/**
 * McpProgressRail — port of v1 `mcpProgress.js`: the side rail listing
 * running/completed MCP tool calls next to the transcript, newest first.
 */
export function McpProgressRail() {
  const entries = useMcpProgressStore((s) => s.entries);
  const visible = [...entries].sort((a, b) => (b.updatedAt || 0) - (a.updatedAt || 0));

  return (
    <aside className="flex w-64 shrink-0 flex-col border-l border-sprite-border bg-sprite-surface">
      <div className="flex h-9 shrink-0 items-center border-b border-sprite-border px-3">
        <span className="text-[11px] uppercase tracking-wide text-sprite-muted">
          MCP progress
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        {visible.length === 0 ? (
          <p className="p-2 text-[11.5px] text-sprite-muted">
            No MCP tool calls yet. Steps appear here while a plan executes.
          </p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {visible.map((entry) => {
              const titleParts = [entry.serverId, entry.toolName].filter(Boolean);
              const latency = formatLatency(entry.latencyMs);
              return (
                <li
                  key={entry.key}
                  className="rounded-md border border-sprite-border bg-sprite-bg px-2 py-1.5"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span
                      className="truncate text-[12px] text-sprite-text"
                      title={titleParts.join(" · ")}
                    >
                      {titleParts.length ? titleParts.join(" · ") : `Step ${entry.stepIndex}`}
                    </span>
                    <span
                      className={`shrink-0 text-[10.5px] font-medium ${
                        STATUS_STYLE[entry.status] ?? "text-sprite-muted"
                      }`}
                    >
                      {STATUS_LABELS[entry.status] ?? "Pending"}
                    </span>
                  </div>
                  {entry.statusMessage && entry.statusMessage.trim() ? (
                    <p className="mt-0.5 text-[11.5px] leading-snug text-sprite-muted">
                      {entry.statusMessage.trim()}
                    </p>
                  ) : null}
                  {entry.error && entry.error.trim() ? (
                    <p className="mt-0.5 text-[11.5px] text-sprite-danger">{entry.error.trim()}</p>
                  ) : null}
                  <div className="mt-0.5 flex items-center gap-1.5 text-[10px] text-sprite-muted">
                    {entry.sessionToken ? <span>{entry.sessionToken.slice(0, 8)}</span> : null}
                    <span>Step {entry.stepIndex}</span>
                    {latency ? <span>· Latency {latency}</span> : null}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </aside>
  );
}
