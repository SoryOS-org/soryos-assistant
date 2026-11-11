import { useState, type KeyboardEvent } from "react";
import { useChatStore } from "@/stores/chatStore";
import { useUiStore } from "@/stores/uiStore";
import { processUserRequest } from "@/lib/tauri";
import { Spinner } from "@/ui/Spinner";

/**
 * Composer — ChatGPT-style pill input (the v1 TODO manual send path).
 *
 * The pipeline also emits `assistant://stt-result`, which BackendBridges
 * feeds into the store; the composer only adds the returned reply as a
 * fallback when no event landed within the last few seconds, so a single
 * turn can never appear twice.
 */
export function Composer() {
  const [value, setValue] = useState("");
  const pending = useChatStore((s) => s.pending);
  const addTranscript = useChatStore((s) => s.addTranscript);
  const addAssistant = useChatStore((s) => s.addAssistant);
  const setPending = useChatStore((s) => s.setPending);

  const canSend = Boolean(value.trim()) && !pending;
  const openVoiceSession = useUiStore((s) => s.openVoiceSession);

  async function send(): Promise<void> {
    const query = value.trim();
    if (!query || pending) {
      return;
    }

    setValue("");
    addTranscript({ text: query });
    setPending(true);

    try {
      const reply = await processUserRequest(query);
      // Fallback: only if the pipeline has NOT already delivered the reply
      // through the `assistant://stt-result` event channel.
      const lastAssistantAtMs = useChatStore.getState().lastAssistantAtMs;
      if (reply?.reply && Date.now() - lastAssistantAtMs > 3000) {
        addAssistant(reply);
      }
    } finally {
      setPending(false);
    }
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void send();
    }
  }

  return (
    <div className="shrink-0 pb-3 pt-1">
      <div className="mx-auto w-full max-w-3xl px-4">
        <div className="flex items-end gap-2 rounded-[26px] border border-composer-border bg-composer px-4 py-2.5">
          {/* Mic — opens the live voice session (ChatGPT voice mode) */}
          <button
            type="button"
            onClick={openVoiceSession}
            title="Session vocale"
            aria-label="Session vocale"
            className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-sprite-muted transition-colors hover:bg-surface-raised hover:text-sprite-text"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
              <rect x="6" y="1" width="4" height="8" rx="2" />
              <path d="M3.5 8a4.5 4.5 0 0 0 9 0h-1.4a3.1 3.1 0 0 1-6.2 0H3.5Z" />
              <rect x="7.4" y="12" width="1.2" height="3" />
            </svg>
          </button>

          <input
            value={value}
            onChange={(event) => setValue(event.target.value)}
            onKeyDown={onKeyDown}
            placeholder={pending ? "Sprite réfléchit…" : "Demandez à Sprite"}
            className="min-w-0 flex-1 bg-transparent text-[14.5px] text-sprite-text outline-none placeholder:text-sprite-muted"
            data-selectable="true"
            disabled={pending}
          />

          <button
            type="button"
            onClick={() => void send()}
            disabled={!canSend}
            title="Envoyer"
            aria-label="Envoyer"
            className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-full transition-colors ${
              canSend
                ? "bg-sprite-text text-sprite-bg hover:opacity-90"
                : "bg-surface-raised text-sprite-muted"
            }`}
          >
            {pending ? (
              <Spinner />
            ) : (
              <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                <path d="M2.7 8.7 1.3 7.3 9 1.6v12.8L7.6 12 2.7 8.7Z" opacity="0.95" />
              </svg>
            )}
          </button>
        </div>

        <p className="pt-1.5 text-center text-[10.5px] text-sprite-muted">
          Sprite peut faire des erreurs. Vérifiez les informations importantes.
        </p>
      </div>
    </div>
  );
}
