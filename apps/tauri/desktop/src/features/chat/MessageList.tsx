import { useEffect, useMemo, useRef, useState } from "react";
import type { ChatEntry } from "@/types";
import { useChatStore, plannerAwareLabel } from "@/stores/chatStore";

function formatTime(timestamp: number): string {
  if (!timestamp) {
    return "";
  }
  return new Date(timestamp).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatDate(timestamp: number): string {
  if (!timestamp) {
    return "";
  }
  return new Date(timestamp).toLocaleDateString([], {
    weekday: "short",
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

function isSameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/**
 * Transcript renderer — ChatGPT-style conversation column.
 *
 * Ports the v1 `chatView.js` behaviors: planner-aware labels, planner-text
 * normalization, context blocks, date dividers and the meta/intents footer.
 * Visual language follows ChatGPT: user messages in a right-aligned rounded
 * bubble, assistant messages as plain text with an avatar.
 *
 * `variant="voice"` renders the exact same transcript for the live voice
 * session (same session store = same discussion), minus the welcome state.
 */
export function MessageList({ variant = "chat" }: { variant?: "chat" | "voice" }) {
  const session = useChatStore((s) => s.sessions.find((x) => x.id === s.activeId));
  const entries = session?.entries ?? [];
  const bottomRef = useRef<HTMLDivElement>(null);
  // v1 expand-on-click accordion: clicking an entry reveals its meta/footer
  // even when the global "Détails" toggle is off.
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const showMeta = useChatStore((s) => s.showMeta);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [entries.length]);

  const dividers = useMemo(() => {
    const lastDates = new Map<number, string>();
    let lastDate: Date | null = null;
    entries.forEach((entry, index) => {
      const entryDate = new Date(entry.timestamp);
      if (!lastDate || !isSameDay(lastDate, entryDate)) {
        lastDate = entryDate;
        lastDates.set(index, formatDate(entry.timestamp));
      }
    });
    return lastDates;
  }, [entries]);

  if (variant === "voice" && entries.length === 0) {
    return <div className="min-h-0 flex-1" />;
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto" data-selectable="true">
      <div className="mx-auto flex max-w-3xl flex-col gap-6 px-4 py-6">
        {entries.length === 0 ? (
          variant === "chat" ? (
            <div className="flex flex-col items-center gap-3 py-24">
              <span className="flex h-14 w-14 items-center justify-center rounded-full bg-surface-raised text-[22px]">
                ✳️
              </span>
              <h2 className="text-[22px] font-semibold text-sprite-text">
                Comment puis-je vous aider&nbsp;?
              </h2>
              <p className="max-w-md text-center text-[13px] text-sprite-muted">
                Parlez à Sprite (micro/Ctrl+Espace dans la fenêtre sprite) ou écrivez
                ci-dessous.
              </p>
            </div>
          ) : null
        ) : (
          entries.map((entry, index) => (
            <div key={entry.id} className="flex flex-col gap-6">
              {dividers.get(index) ? (
                <div className="flex items-center justify-center">
                  <span className="text-[11px] text-sprite-muted">{dividers.get(index)}</span>
                </div>
              ) : null}
              <MessageRow
                entry={entry}
                expanded={expandedId === entry.id}
                onToggle={() =>
                  setExpandedId((current) => (current === entry.id ? null : entry.id))
                }
                expandable={!showMeta}
              />
            </div>
          ))
        )}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}

function MessageRow({
  entry,
  expanded = false,
  expandable = true,
  onToggle,
}: {
  entry: ChatEntry;
  expanded?: boolean;
  /** Disabled while the global "Détails" toggle already reveals everything. */
  expandable?: boolean;
  onToggle?: () => void;
}) {
  const showMeta = useChatStore((s) => s.showMeta);
  const contextVisible = useChatStore((s) => s.contextVisible);

  const isUser = entry.role === "user";
  const isPlanner = entry.source === "planner";
  const isSummary = entry.source === "summary";

  // v1: planner entries have their leading "planner:" prefix stripped.
  const rawText = entry.text || "(empty)";
  const normalized =
    isPlanner ? rawText.replace(/^\s*planner[:\-\s]+/i, "").trim() || rawText : rawText;

  if (isUser) {
    return (
      <article
        className={`flex flex-col items-end ${expandable ? "cursor-pointer" : ""}`}
        onClick={expandable ? onToggle : undefined}
        title={expandable ? (expanded ? "Réduire" : "Afficher les détails") : undefined}
      >
        <div className="max-w-[80%] whitespace-pre-wrap rounded-3xl bg-surface-raised px-4 py-2.5 text-[14.5px] leading-relaxed text-sprite-text">
          {normalized}
        </div>
        {showMeta || expanded ? (
          <footer className="mt-1 flex items-center gap-2 text-[10.5px] text-sprite-muted">
            <span>{formatTime(entry.timestamp)}</span>
          </footer>
        ) : null}
      </article>
    );
  }

  return (
    <article
      className={`flex flex-col gap-1 ${expandable ? "cursor-pointer" : ""}`}
      onClick={expandable ? onToggle : undefined}
      title={expandable ? (expanded ? "Réduire" : "Afficher les détails") : undefined}
    >
      <div className="flex items-start gap-3">
        <span className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full border border-sprite-border bg-sprite-bg text-[13px]">
          ✳️
        </span>
        <div className="min-w-0 flex-1">
          {showMeta || expanded || isPlanner || isSummary ? (
            <span className="mb-0.5 block text-[11px] uppercase tracking-wider text-sprite-muted">
              {plannerAwareLabel(entry)}
            </span>
          ) : null}
          <div
            className={`whitespace-pre-wrap text-[14.5px] leading-relaxed ${
              isPlanner || isSummary ? "text-sprite-muted" : "text-sprite-text"
            }`}
          >
            {normalized}
          </div>

          {entry.context && contextVisible ? (
            <div
              className="mt-2 whitespace-pre-wrap rounded-lg border border-sprite-border bg-sprite-surface px-3 py-1.5 text-[11.5px] text-sprite-muted"
              data-context
            >
              {entry.context}
            </div>
          ) : null}

          {showMeta || expanded ? (
            <footer className="mt-1 flex items-center gap-3 text-[10.5px] text-sprite-muted">
              <span>{formatTime(entry.timestamp)}</span>
              {entry.context ? <span>Contexte</span> : null}
              {entry.intents?.length ? (
                <span
                  title={entry.intents
                    .map((intent) => String(intent.type ?? ""))
                    .filter(Boolean)
                    .join(", ")}
                >
                  Intents:{" "}
                  {entry.intents
                    .map((intent) => String(intent.type ?? ""))
                    .filter(Boolean)
                    .join(", ")}
                </span>
              ) : null}
            </footer>
          ) : null}
        </div>
      </div>
    </article>
  );
}
