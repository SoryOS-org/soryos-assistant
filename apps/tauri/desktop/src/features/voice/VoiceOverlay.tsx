import type { VoiceState } from "@/types";

/**
 * VoiceOverlay — floating voice status shown while the assistant listens,
 * thinks or speaks. Bridges the existing `sprite://ui` state machine into the
 * v2 shell (Phase 3 wires capture locally; the overlay contract is fixed).
 */
export function VoiceOverlay({ voiceState }: { voiceState: VoiceState }) {
  const label: Record<string, string> = {
    listening: "Listening…",
    thinking: "Thinking…",
    speaking: "Speaking…",
    error: "Voice error",
  };

  return (
    <div className="pointer-events-none absolute bottom-4 left-1/2 z-40 -translate-x-1/2">
      <div className="flex items-center gap-2 rounded-full border border-sprite-border bg-sprite-surface px-4 py-2 shadow-xl">
        <span
          className={`h-2 w-2 animate-pulse rounded-full ${
            voiceState === "error" ? "bg-sprite-danger" : "bg-sprite-accent"
          }`}
        />
        <span className="text-[13px] text-sprite-text">{label[voiceState] ?? voiceState}</span>
      </div>
    </div>
  );
}
