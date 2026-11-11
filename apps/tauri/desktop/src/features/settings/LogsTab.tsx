import { useCallback, useEffect, useRef, useState } from "react";
import { getRecentLogs } from "@/lib/tauri";
import { Button } from "@/ui/Button";
import { Spinner } from "@/ui/Spinner";

/**
 * LogsTab — port of v1 `logsView.js`: recent log lines (`get_recent_logs`,
 * 200 lines), auto-scrolled to the bottom, with source/status line.
 */
export function LogsTab() {
  const [lines, setLines] = useState<string[]>([]);
  const [source, setSource] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastFetchedAt, setLastFetchedAt] = useState<number | null>(null);
  const outputRef = useRef<HTMLPreElement>(null);

  const refresh = useCallback(async () => {
    if (loading) {
      return;
    }
    setLoading(true);
    try {
      const snapshot = await getRecentLogs(200);
      setSource(snapshot?.path || null);
      setLines(Array.isArray(snapshot?.lines) ? snapshot.lines : []);
      setLastFetchedAt(Date.now());
      setError(null);
    } catch (err) {
      const reason = typeof err === "string" ? err : (err as Error)?.message;
      setError(reason || "Failed to load logs");
    } finally {
      setLoading(false);
    }
  }, [loading]);

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Keep the view scrolled to the newest line (v1 `renderLines`).
  useEffect(() => {
    const el = outputRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }, [lines]);

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <span className="text-[12px] text-sprite-muted">
          {loading
            ? "Loading logs…"
            : error
              ? `Error: ${error}`
              : source
                ? `${source}${lastFetchedAt ? ` · ${new Date(lastFetchedAt).toLocaleTimeString()}` : ""}`
                : "No log files found"}
        </span>
        <Button onClick={() => void refresh()} disabled={loading} className="ml-auto text-[11.5px]">
          {loading ? <Spinner /> : "Refresh"}
        </Button>
      </div>
      <pre
        ref={outputRef}
        data-selectable="true"
        className="max-h-[420px] overflow-auto rounded-md border border-sprite-border bg-sprite-bg p-2 font-mono text-[11.5px] leading-relaxed text-sprite-muted"
      >
        {lines.length ? lines.join("\n") : "// No log entries yet"}
      </pre>
    </div>
  );
}
