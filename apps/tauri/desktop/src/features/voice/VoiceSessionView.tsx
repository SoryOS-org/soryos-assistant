import { useEffect } from "react";
import { useChatStore } from "@/stores/chatStore";
import { useVoiceSignalStore } from "@/stores/voiceSignalStore";
import { useUiStore } from "@/stores/uiStore";
import { useMcpProgressStore } from "@/stores/mcpStore";
import { MessageList, PillsRow, McpSummaryChip, ControlChips, Composer } from "@/features/chat";
import { McpProgressRail } from "@/features/mcp/McpProgressRail";
import { isTauri, listenToEvent } from "@/lib/tauri";
import { EVENT_CHANNELS } from "@/lib/channels";
import type { VoiceState } from "@/types";

/**
 * VoiceSessionView — live real-time voice session over the SAME discussion
 * as the chat. It is not a separate conversation: it renders the exact same
 * session store (synchronized with the chat view) with the exact same
 * capabilities — full transcript, planner labels, context blocks, intents,
 * status pills, MCP progress rail, Details/Context toggles and even the
 * composer. The difference is that here the user can simply SPEAK instead
 * of typing: the transcript is fed live by the backend STT/TTS events and
 * a reactive orb mirrors the voice activity.
 */
export function VoiceSessionView() {
  const voiceState = useUiStore((s) => s.voiceState);
  const closeVoiceSession = useUiStore((s) => s.closeVoiceSession);
  const setGlobalMetaVisible = useChatStore((s) => s.setGlobalMetaVisible);
  const setContextVisible = useChatStore((s) => s.setContextVisible);
  const vuLevel = useVoiceSignalStore((s) => s.vuLevel);
  const muted = useVoiceSignalStore((s) => s.muted);
  const setMuted = useVoiceSignalStore((s) => s.setMuted);

  const session = useChatStore((s) => s.sessions.find((x) => x.id === s.activeId));
  const mcpEntries = useMcpProgressStore((s) => s.entries);
  const showMcpRail = mcpEntries.length > 0;

  return (
    <div className="absolute inset-0 z-40 flex flex-col bg-sprite-bg">
      {/* Top bar — same identity as the chat (title, MCP chip, toggles) */}
      <header className="flex h-11 shrink-0 items-center justify-between px-3">
        <span className="max-w-[320px] truncate text-[13.5px] font-medium text-sprite-text">
          {session?.title ?? "Sprite"}
        </span>
        <div className="flex items-center gap-2">
          <McpSummaryChip />
          <ControlChips
            onMeta={(active) => setGlobalMetaVisible(active)}
            onContext={(active) => setContextVisible(active)}
          />
          <button
            type="button"
            onClick={closeVoiceSession}
            title="Terminer la session vocale"
            aria-label="Terminer la session vocale"
            className="flex h-8 w-8 items-center justify-center rounded-full text-sprite-muted transition-colors hover:bg-surface-raised hover:text-sprite-text"
          >
            <svg width="15" height="15" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
              <path d="M4 4.7 4.7 4l3.3 3.3L11.3 4l.7.7L8.7 8l3.3 3.3-.7.7L8 8.7 4.7 12l-.7-.7L7.3 8 4 4.7Z" />
            </svg>
          </button>
        </div>
      </header>

      <PillsRow />

      {/* SAME transcript as the chat (same store, same rendering) */}
      <div className="flex min-h-0 flex-1">
        <MessageList variant="voice" />
        {showMcpRail ? <McpProgressRail /> : null}
      </div>

      {/* Orb + state + controls between transcript and composer */}
      <div className="flex shrink-0 flex-col items-center gap-2 pb-1">
        <VoiceOrb voiceState={muted ? "muted" : voiceState} vuLevel={muted ? 0 : vuLevel} />
        <StateLabel voiceState={muted ? "muted" : voiceState} />
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => setMuted(!muted)}
            title={muted ? "Réactiver le micro" : "Couper le micro"}
            aria-label={muted ? "Réactiver le micro" : "Couper le micro"}
            className={`flex h-10 w-10 items-center justify-center rounded-full border transition-colors ${
              muted
                ? "border-sprite-danger/60 text-sprite-danger"
                : "border-sprite-border text-sprite-muted hover:text-sprite-text"
            }`}
          >
            {muted ? <MicOffIcon /> : <MicIcon />}
          </button>
          <button
            type="button"
            onClick={closeVoiceSession}
            title="Terminer la session vocale"
            aria-label="Terminer la session vocale"
            className="flex h-10 w-10 items-center justify-center rounded-full bg-sprite-danger/90 text-white transition-colors hover:bg-sprite-danger"
          >
            <svg width="15" height="15" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
              <rect x="7.25" y="1.5" width="1.5" height="13" rx="0.75" />
            </svg>
          </button>
        </div>
      </div>

      {/* SAME composer as the chat — typing stays possible, voice is a plus */}
      <Composer />

      {/* Live signal feed: Tauri uses `sprite://ui` vu_level; browser uses a
          local Web Audio analyser so the orb is genuinely live. */}
      <VoiceSignalFeed />
    </div>
  );
}

/* ------------------------------------------------------------------ */

function MicIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <rect x="6" y="1" width="4" height="8" rx="2" />
      <path d="M3.5 8a4.5 4.5 0 0 0 9 0h-1.4a3.1 3.1 0 0 1-6.2 0H3.5Z" />
      <rect x="7.4" y="12" width="1.2" height="3" />
    </svg>
  );
}

function MicOffIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M10 5.5v-.4A2.1 2.1 0 0 0 8 3a2.1 2.1 0 0 0-2 2.1V8a2 2 0 0 0 2.8 1.9L10 8.6V5.5Z" />
      <path d="M3.5 8a4.5 4.5 0 0 0 4.2 4.5v2h1.6v-2c.6-.1 1.2-.3 1.7-.6l1.3 1.3.8-.8-2.3-2.3-1.4-1.4L4.9 4.2 3.2 2.5l-.8.8 1.5 1.5A4.5 4.5 0 0 0 3.5 8Z" />
    </svg>
  );
}

const STATE_LABELS: Record<string, string> = {
  idle: "Parlez — Sprite vous écoute (ou écrivez ci-dessous)",
  listening: "J'écoute…",
  thinking: "Je réfléchis…",
  speaking: "Je réponds…",
  muted: "Micro coupé",
  error: "Erreur vocale",
};

function StateLabel({ voiceState }: { voiceState: string }) {
  return (
    <p className="text-[13px] text-sprite-muted">{STATE_LABELS[voiceState] ?? voiceState}</p>
  );
}

/** The big reactive orb. */
function VoiceOrb({
  voiceState,
  vuLevel,
}: {
  voiceState: VoiceState | "muted";
  vuLevel: number;
}) {
  const palette: Record<string, string> = {
    idle: "from-surface-raised to-sprite-surface",
    listening: "from-sprite-accent to-emerald-700",
    thinking: "from-amber-500 to-amber-700",
    speaking: "from-sprite-accent-hover to-sprite-accent",
    muted: "from-surface-raised to-sprite-surface",
    error: "from-sprite-danger to-red-800",
  };

  const scale = 1 + Math.min(1, Math.max(0, vuLevel)) * 0.3;
  const active = voiceState === "listening" || voiceState === "speaking";

  return (
    <div
      className={`voice-orb flex h-24 w-24 items-center justify-center rounded-full bg-gradient-to-br shadow-2xl ${
        palette[voiceState] ?? palette.idle
      } ${active && vuLevel > 0.02 ? "" : "voice-orb--idle"}`}
      style={{ transform: `scale(${scale.toFixed(3)})` }}
    >
      <span className="text-[24px] drop-shadow">✳️</span>
    </div>
  );
}

/**
 * Voice signal feed — `vu_level` from `sprite://ui` inside Tauri; a real
 * Web Audio analyser on the local mic in browser preview, so the orb
 * genuinely reacts to the voice during development.
 */
function VoiceSignalFeed() {
  const setVuLevel = useVoiceSignalStore((s) => s.setVuLevel);
  const muted = useVoiceSignalStore((s) => s.muted);
  const voiceState = useUiStore((s) => s.voiceState);

  useEffect(() => {
    if (isTauri) {
      // Tauri: the Rust pipeline publishes the voice level. Mirror the v1
      // `ui_bridge.js` semantics: EMA smoothing (alpha 0.6) and a forced 0
      // whenever the state is not actively listening/speaking.
      const EMA_ALPHA = 0.6;
      const ACTIVE_STATES = new Set<VoiceState>(["listening", "speaking"]);
      let disposed = false;
      let vuSmoothed = 0;
      let currentState = voiceState;
      void listenToEvent<{ vu_level?: number | null }>(EVENT_CHANNELS.ui, (event) => {
        if (disposed || muted) {
          return;
        }
        const raw = event.payload?.vu_level;
        const level = typeof raw === "number" ? Math.min(Math.max(raw, 0), 1) : 0;
        vuSmoothed = ACTIVE_STATES.has(currentState)
          ? EMA_ALPHA * level + (1 - EMA_ALPHA) * vuSmoothed
          : 0;
        setVuLevel(vuSmoothed);
      });
      // Keep the state captured by the listener fresh without resubscribing.
      const stateUnsub = useUiStore.subscribe((state) => {
        currentState = state.voiceState;
        if (!ACTIVE_STATES.has(state.voiceState) && !disposed && !muted) {
          // v1: leaving an active state snaps the level back to 0.
          vuSmoothed = 0;
          setVuLevel(0);
        }
      });
      return () => {
        disposed = true;
        stateUnsub();
      };
    }

    // Browser preview: capture the local mic purely for the visual level.
    if (muted) {
      setVuLevel(0);
      return;
    }
    let disposed = false;
    let cleanup: (() => void) | null = null;

    void navigator.mediaDevices
      ?.getUserMedia({ audio: true })
      .then((stream) => {
        if (disposed) {
          stream.getTracks().forEach((track) => track.stop());
          return;
        }
        const audioContext = new AudioContext();
        const source = audioContext.createMediaStreamSource(stream);
        const analyser = audioContext.createAnalyser();
        analyser.fftSize = 512;
        source.connect(analyser);
        const buffer = new Uint8Array(analyser.frequencyBinCount);

        let rafId = 0;
        const tick = () => {
          if (disposed) {
            return;
          }
          analyser.getByteTimeDomainData(buffer);
          let peak = 0;
          for (const sample of buffer) {
            const deviation = Math.abs(sample - 128) / 128;
            if (deviation > peak) {
              peak = deviation;
            }
          }
          setVuLevel(peak);
          rafId = requestAnimationFrame(tick);
        };
        rafId = requestAnimationFrame(tick);

        cleanup = () => {
          cancelAnimationFrame(rafId);
          void audioContext.close();
          stream.getTracks().forEach((track) => track.stop());
        };
      })
      .catch(() => {
        // No mic permission in the browser: the orb stays idle-animated.
      });

    return () => {
      disposed = true;
      cleanup?.();
    };
  }, [muted, setVuLevel]);

  return null;
}
