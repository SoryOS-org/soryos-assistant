import { useEffect, useState } from "react";
import {
  formatLatencyHud,
  LATENCY_HUD_LIMIT,
  LATENCY_HUD_REFRESH_MS,
  type LatencyHudView,
} from "@/lib/latency";
import { getLatencyHud } from "@/lib/tauri";

/**
 * Polls `get_latency_hud` on a 4s interval (v1 `latencyHud.js` cadence)
 * and exposes the formatted readout/tooltip for the status bar.
 */
export function useLatencyHud(): LatencyHudView {
  const [view, setView] = useState<LatencyHudView>(() =>
    formatLatencyHud(null),
  );

  useEffect(() => {
    let disposed = false;
    let failed = false;

    async function refresh() {
      try {
        const snapshot = await getLatencyHud(LATENCY_HUD_LIMIT);
        if (!disposed && snapshot && typeof snapshot === "object") {
          setView(formatLatencyHud(snapshot));
        }
      } catch {
        // Poll in the background; swallow repeated failures once logged.
        if (!failed) {
          // eslint-disable-next-line no-console
          console.warn("[latency-hud] fetch failed; will keep retrying");
          failed = true;
        }
      }
    }

    void refresh();
    const timerId = setInterval(() => void refresh(), LATENCY_HUD_REFRESH_MS);

    return () => {
      disposed = true;
      clearInterval(timerId);
    };
  }, []);

  return view;
}
