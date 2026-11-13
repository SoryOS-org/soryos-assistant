import { useState } from "react";
import { useAlertStore } from "@/stores/alertStore";
import { useUiStore } from "@/stores/uiStore";
import { useMcpProgressStore, summarizeProgressEntries } from "@/stores/mcpStore";
import { togglePtt } from "@/lib/tauri";

/**
 * Shared chat status pieces (pills, MCP summary chip, Details/Context
 * toggles) — used by both the chat view and the live voice session so the
 * two stay feature-identical.
 */

export function PillsRow() {
  const uiState = useAlertStore((s) => s.uiState);
  const uiStateUpdatedAt = useAlertStore((s) => s.uiStateUpdatedAt);
  const configDirty = useAlertStore((s) => s.configDirty);
  const workerAuthFailed = useAlertStore((s) => s.workerAuthFailed);
  const sttError = useAlertStore((s) => s.sttError);
  const preload = useAlertStore((s) => s.preload);

  const pills: Array<{
    key: string;
    text: string;
    tone: "info" | "warn" | "error";
    dot?: DotTone;
  }> = [];
  // v1 pill: colored status dot + "Updated HH:MM:SS" timestamp.
  pills.push({
    key: "ui",
    text:
      uiStateUpdatedAt != null
        ? `${uiState || "Idle"} · Updated ${formatClock(uiStateUpdatedAt)}`
        : uiState || "Idle",
    tone: "info",
    dot: dotToneForState(uiState),
  });
  if (preload?.message && String(preload.state ?? "") !== "ready") {
    // v1 preload banner: progress is a 0..1 ratio, shown as a percentage.
    const progress = typeof preload.progress === "number" ? preload.progress : null;
    const percent =
      progress != null ? ` ${Math.round(Math.min(Math.max(progress, 0), 1) * 100)} %` : "";
    pills.push({ key: "preload", text: `${String(preload.message)}${percent}`, tone: "warn" });
  }
  if (configDirty) {
    pills.push({
      key: "config",
      text: "Modifs config en attente (Settings)",
      tone: "warn",
    });
  }
  if (workerAuthFailed) {
    pills.push({ key: "worker", text: "Auth worker échouée", tone: "error" });
  }
  if (sttError) {
    pills.push({ key: "stt", text: sttError, tone: "error" });
  }

  const toneClass: Record<string, string> = {
    info: "border-sprite-border text-sprite-muted",
    warn: "border-amber-500/40 text-amber-300",
    error: "border-sprite-danger/40 text-sprite-danger",
  };
  const dotClass: Record<string, string> = {
    ok: "bg-emerald-400",
    busy: "bg-amber-400",
    error: "bg-sprite-danger",
    idle: "bg-sprite-muted",
  };

  if (pills.length === 0) {
    return null;
  }

  return (
    <div className="flex flex-wrap gap-1.5 border-b border-sprite-border px-3 py-1.5">
      {pills.map((pill) => (
        <span
          key={pill.key}
          className={`flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[10.5px] ${toneClass[pill.tone]}`}
        >
          {pill.key === "ui" ? (
            <span
              className={`h-1.5 w-1.5 shrink-0 rounded-full ${dotClass[pill.dot ?? "idle"]}`}
              aria-hidden="true"
            />
          ) : null}
          {pill.text}
        </span>
      ))}
    </div>
  );
}

type DotTone = "ok" | "busy" | "error" | "idle";

/** v1 pill dot: red on error, amber while actively working, green otherwise. */
function dotToneForState(uiState: string | null): DotTone {
  const state = (uiState ?? "").toLowerCase();
  if (state.includes("error") || state.includes("crash") || state.includes("failed")) {
    return "error";
  }
  if (state.includes("listen") || state.includes("speak") || state.includes("think") || state.includes("transcri")) {
    return "busy";
  }
  return "ok";
}

function formatClock(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString([], { hour12: false });
}

export function McpSummaryChip() {
  const entries = useMcpProgressStore((s) => s.entries);
  const summary = summarizeProgressEntries(entries);
  const parts: string[] = [];
  if (summary.running) {
    parts.push(`${summary.running} en cours`);
  }
  if (summary.crashed) {
    parts.push(`${summary.crashed} erreur${summary.crashed > 1 ? "s" : ""}`);
  }
  if (summary.queued) {
    parts.push(`${summary.queued} en attente`);
  }
  if (!parts.length && summary.completed) {
    parts.push(`${summary.completed} terminé${summary.completed > 1 ? "s" : ""}`);
  }
  return (
    <span className="text-[11px] text-sprite-muted">
      MCP : {parts.length ? parts.join(" | ") : "inactif"}
    </span>
  );
}

export function ControlChips({
  onMeta,
  onContext,
}: {
  onMeta: (active: boolean) => void;
  onContext: (active: boolean) => void;
}) {
  const [metaActive, setMetaActive] = useState(false);
  const [contextActive, setContextActive] = useState(true);
  // v1 "Show/Hide MCP": manual toggle for the MCP progress rail.
  const mcpRailVisible = useUiStore((s) => s.mcpRailVisible);
  const toggleMcpRail = useUiStore((s) => s.toggleMcpRail);

  return (
    <div className="flex items-center gap-1">
      <button
        type="button"
        aria-pressed={metaActive ? "true" : "false"}
        onClick={() => {
          const next = !metaActive;
          setMetaActive(next);
          onMeta(next);
        }}
        className={`rounded-full px-2 py-1 text-[11px] transition-colors ${
          metaActive
            ? "bg-surface-raised text-sprite-text"
            : "text-sprite-muted hover:bg-surface-raised/60 hover:text-sprite-text"
        }`}
      >
        {metaActive ? "Masquer détails" : "Détails"}
      </button>
      <button
        type="button"
        aria-pressed={contextActive ? "true" : "false"}
        onClick={() => {
          const next = !contextActive;
          setContextActive(next);
          onContext(next);
        }}
        className={`rounded-full px-2 py-1 text-[11px] transition-colors ${
          contextActive
            ? "bg-surface-raised text-sprite-text"
            : "text-sprite-muted hover:bg-surface-raised/60 hover:text-sprite-text"
        }`}
      >
        {contextActive ? "Masquer contexte" : "Contexte"}
      </button>
      <button
        type="button"
        aria-pressed={mcpRailVisible ? "true" : "false"}
        onClick={toggleMcpRail}
        className={`rounded-full px-2 py-1 text-[11px] transition-colors ${
          mcpRailVisible
            ? "bg-surface-raised text-sprite-text"
            : "text-sprite-muted hover:bg-surface-raised/60 hover:text-sprite-text"
        }`}
      >
        {mcpRailVisible ? "Masquer MCP" : "MCP"}
      </button>
    </div>
  );
}

/**
 * Push-to-talk button — manual trigger for the voice capture when the global
 * Ctrl+Space hotkey is unavailable (Wayland). Click toggles listening; the
 * Rust `toggle_ptt` command does the actual work and emits the STT events.
 */
export function PttButton() {
  const [active, setActive] = useState(false);
  const sttError = useAlertStore((s) => s.sttError);

  const handleClick = () => {
    const next = !active;
    setActive(next);
    // Debug aid: mark the click itself (before the async invoke overwrites
    // the title) so we can tell "click not firing" from "invoke failing".
    document.title = "[v2] click PTT";
    void togglePtt(next);
  };

  return (
    <button
      type="button"
      aria-pressed={active ? "true" : "false"}
      onClick={handleClick}
      title={
        active
          ? "Arrêter l'écoute (transcrire)"
          : "Écouter (push-to-talk) — parle après le clic"
      }
      className={`flex h-7 items-center gap-1 rounded-full px-2 py-1 text-[11px] transition-colors ${
        active
          ? "bg-sprite-danger/90 text-white"
          : "text-sprite-muted hover:bg-surface-raised/60 hover:text-sprite-text"
      }`}
    >
      {active ? (
        <>
          <span className="h-2 w-2 animate-pulse rounded-full bg-white" aria-hidden="true" />
          Écoute…
        </>
      ) : (
        <>🎙 Écouter</>
      )}
      {sttError ? <span className="text-sprite-danger" title={sttError}>⚠</span> : null}
    </button>
  );
}
