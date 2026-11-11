import { useLatencyHud } from "@/hooks/useLatencyHud";

/**
 * LatencyBar — slim status footer shown under the main view, port of the v1
 * latency HUD readout ("Capture X | STT X | LLM X | TTS X | E2E X") with the
 * history tooltip aggregated over the last 5 turns.
 */
export function LatencyBar() {
  const { readout, meta, status, tooltip } = useLatencyHud();

  const tone =
    status === "ok"
      ? "text-sprite-success border-sprite-success/30"
      : status === "error"
        ? "text-sprite-danger border-sprite-danger/30"
        : "text-sprite-muted border-transparent";

  return (
    <footer
      className={`flex h-6 shrink-0 items-center gap-3 border-t border-sprite-border px-3 text-[10.5px] ${tone}`}
      title={tooltip || undefined}
    >
      <span className="font-medium">{readout}</span>
      {meta && meta !== "Waiting for first turn..." ? (
        <span className="truncate text-sprite-muted" title={tooltip || undefined}>
          {meta}
        </span>
      ) : null}
    </footer>
  );
}
