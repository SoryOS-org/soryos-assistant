import { useCallback, useEffect, useState } from "react";
import {
  clearPicovoiceKey,
  clearWorkerAuthToken,
  getSecretsStatus,
  setPicovoiceKey,
  setWorkerAuthToken,
} from "@/lib/tauri";
import { Button } from "@/ui/Button";
import { Spinner } from "@/ui/Spinner";
import type { SecretsStatus } from "@/types";

/**
 * SecretsTab — port of v1 `secrets.js`: Picovoice key + worker auth token,
 * status/pill, masked hint (`xxxx.yyyy`), set & clear actions with
 * confirmation, fed by `get_secrets_status`.
 */
export function SecretsTab() {
  const [status, setStatus] = useState<SecretsStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [statusLine, setStatusLine] = useState<{ text: string; tone: "info" | "error" } | null>(
    null,
  );

  const loadStatus = useCallback(async () => {
    setLoading(true);
    try {
      const fresh = await getSecretsStatus();
      setStatus(fresh);
      setStatusLine(null);
    } catch {
      setStatusLine({ text: "Failed to load secrets status.", tone: "error" });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadStatus();
  }, [loadStatus]);

  return (
    <div className="flex flex-col gap-4">
      <SecretEntry
        title="Picovoice access key"
        prompt="Wake-word activation key (stored securely)."
        entryStatus={status?.picovoice}
        loading={loading}
        onSet={async (value) => {
          const fresh = await setPicovoiceKey(value);
          if (fresh?.picovoice) {
            setStatus(fresh);
          } else {
            await loadStatus();
          }
        }}
        onClear={async () => {
          await clearPicovoiceKey();
          await loadStatus();
        }}
      />
      <SecretEntry
        title="Worker auth token"
        prompt="Authentication token for the cloud worker (stored securely)."
        entryStatus={status?.worker_auth}
        loading={loading}
        onSet={async (value) => {
          const fresh = await setWorkerAuthToken(value);
          if (fresh) {
            setStatus(fresh);
          } else {
            await loadStatus();
          }
        }}
        onClear={async () => {
          await clearWorkerAuthToken();
          await loadStatus();
        }}
      />

      {statusLine ? (
        <p className={`text-[12px] ${statusLine.tone === "error" ? "text-sprite-danger" : "text-sprite-muted"}`}>
          {statusLine.text}
        </p>
      ) : null}

      <Button variant="ghost" className="self-start text-[11.5px]" onClick={() => void loadStatus()}>
        Refresh status
      </Button>
    </div>
  );
}

type SetFn = ((value: string) => Promise<void>) | null;
type ClearFn = (() => Promise<void>) | null;

function SecretEntry({
  title,
  prompt,
  entryStatus,
  loading,
  onSet,
  onClear,
}: {
  title: string;
  prompt: string;
  entryStatus: { has_key?: boolean; hint?: string } | undefined;
  loading: boolean;
  onSet: SetFn;
  onClear: ClearFn;
}) {
  const [value, setValue] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const hint = formatSecretHint(entryStatus?.hint ?? "");
  const hasKey = Boolean(entryStatus?.has_key);

  async function handleSet(): Promise<void> {
    const trimmed = value.trim();
    if (!trimmed || !onSet || busy) {
      return;
    }
    setBusy(true);
    try {
      await onSet(trimmed);
      setValue("");
      setConfirming(false);
    } finally {
      setBusy(false);
    }
  }

  async function handleClear(): Promise<void> {
    if (!onClear || !confirming || busy) {
      return;
    }
    setBusy(true);
    try {
      await onClear();
      setConfirming(false);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="rounded-lg border border-sprite-border bg-sprite-surface p-3">
      <header className="flex items-center justify-between gap-2">
        <h4 className="text-[13px] font-medium text-sprite-text">{title}</h4>
        <span
          className={`rounded-full border px-2 py-0.5 text-[10.5px] ${
            loading
              ? "border-sprite-border text-sprite-muted"
              : hasKey
                ? "border-sprite-success/60 text-sprite-success"
                : "border-sprite-border text-sprite-muted"
          }`}
        >
          {loading ? "Loading…" : hasKey ? "Configured" : "Not set"}
        </span>
      </header>
      <p className="mt-1 text-[11.5px] text-sprite-muted">{prompt}</p>
      {hint.compact ? (
        <p className="mt-1 font-mono text-[11.5px] text-sprite-muted">
          Current: {hint.compact} {hasKey ? "" : "(status pending)"}
        </p>
      ) : null}

      {!confirming ? (
        <div className="mt-2 flex items-center gap-2">
          <input
            type="password"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && void handleSet()}
            placeholder={loading ? "Loading…" : "Enter new value…"}
            disabled={loading || busy}
            className="w-56 rounded-md border border-sprite-border bg-sprite-bg px-2 py-1 text-[12.5px] text-sprite-text outline-none placeholder:text-sprite-muted focus:border-sprite-accent"
            data-selectable="true"
          />
          <Button
            variant="primary"
            disabled={loading || busy || !value.trim()}
            onClick={() => void handleSet()}
          >
            {busy ? <Spinner /> : "Set"}
          </Button>
          {onClear ? (
            <Button
              variant="danger"
              disabled={!hasKey || loading || busy}
              onClick={() => setConfirming(true)}
            >
              Clear
            </Button>
          ) : null}
        </div>
      ) : (
        <div className="mt-2 flex items-center gap-2">
          <span className="text-[12px] text-sprite-danger">
            Remove the stored value?
          </span>
          <Button variant="danger" onClick={() => void handleClear()}>
            Yes, remove
          </Button>
          <Button onClick={() => setConfirming(false)}>Cancel</Button>
        </div>
      )}
    </section>
  );
}

/** v1 `formatSecretHint`: compact display `xxxx.yyyy` for long secrets. */
export function formatSecretHint(hint: string): { full: string; compact: string } {
  const full = hint || "";
  if (!full) {
    return { full: "", compact: "" };
  }
  if (full.length <= 12) {
    return { full, compact: full };
  }
  return {
    full,
    compact: `${full.slice(0, 4)}.${full.slice(-4)}`,
  };
}
