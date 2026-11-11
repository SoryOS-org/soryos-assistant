import { useEffect, useMemo, useState } from "react";
import {
  useConfigStore,
  buildConfigPatch,
  useAlertStore,
} from "@/stores/alertStore";
import { getConfigSummary, updateConfig } from "@/lib/tauri";
import { Button } from "@/ui/Button";
import { Spinner } from "@/ui/Spinner";
import type { ReactNode } from "react";
import type { ConfigFormValues, ConfigPatch, ConfigSummary } from "@/types";

/**
 * ConfigTab — port of v1 `config/editor.js`: form populated from the config
 * summary (v1 `populateFormFromSummary` + `collectFormValues`), live JSON
 * patch preview diffed against the baseline (v1 `buildPatchFromValues` /
 * `updatePatchPreview`), Save through `update_config`.
 */
export function ConfigTab() {
  const summary = useConfigStore((s) => s.summary);
  const loaded = useConfigStore((s) => s.loaded);
  const loading = useConfigStore((s) => s.loading);
  const modelCatalog = useConfigStore((s) => s.modelCatalog);
  const setConfigDirty = useAlertStore((s) => s.setConfigDirty);

  const [values, setValues] = useState<ConfigFormValues | null>(null);
  const [status, setStatus] = useState<{
    text: string;
    tone: "info" | "success" | "error";
  } | null>(null);
  const [saving, setSaving] = useState(false);

  async function load(showLoading: boolean): Promise<void> {
    const store = useConfigStore.getState();
    if (store.loading) {
      return;
    }
    store.setLoading(true);
    if (showLoading) {
      setStatus({ text: "Loading configuration…", tone: "info" });
    }
    try {
      const fresh = await getConfigSummary();
      useConfigStore.getState().applySummary(fresh);
      setValues(collectFromSummary(fresh));
      setConfigDirty(false);
      setStatus(null);
    } catch {
      setStatus({ text: "Failed to load configuration.", tone: "error" });
    } finally {
      store.setLoading(false);
    }
  }

  useEffect(() => {
    void load(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const patch: ConfigPatch = useMemo(
    () => buildConfigPatch(values, summary),
    [values, summary],
  );
  const hasChanges = useMemo(() => patchHasChanges(patch), [patch]);

  async function save(): Promise<void> {
    if (!hasChanges) {
      setStatus({ text: "No changes to save.", tone: "info" });
      return;
    }
    setSaving(true);
    setStatus({ text: "Saving changes…", tone: "info" });
    try {
      const response = await updateConfig(patch);
      if (response?.summary) {
        useConfigStore.getState().applySummary(response.summary);
        setValues(collectFromSummary(response.summary));
      }
      setConfigDirty(false);
      setStatus({ text: "Configuration updated successfully.", tone: "success" });
    } catch (error) {
      const reason = typeof error === "string" ? error : (error as Error)?.message;
      setStatus({
        text: `Failed to update configuration: ${reason || "Unknown error"}`,
        tone: "error",
      });
    } finally {
      setSaving(false);
    }
  }

  function updateField<K extends keyof ConfigFormValues>(
    key: K,
    value: ConfigFormValues[K],
  ): void {
    if (!values) {
      return;
    }
    setValues({ ...values, [key]: value });
    setConfigDirty(true);
  }

  function updateSub(
    key: "model" | "vad" | "wakeword" | "hotkey" | "telemetry" | "sprite" | "whisper",
    subKey: string,
    raw: string,
    kind: "int" | "float" | "text",
  ): void {
    const value = kind === "int" ? parseIntField(raw) : kind === "float" ? parseFloatField(raw) : raw;
    if (!values) {
      return;
    }
    updateField(key, { ...values[key], [subKey]: value });
  }

  if (loading && !loaded) {
    return (
      <div className="flex items-center gap-2 text-[12.5px] text-sprite-muted">
        <Spinner /> Loading configuration…
      </div>
    );
  }
  if (!values || !summary) {
    return (
      <div className="flex flex-col items-start gap-3">
        <p className="text-[12.5px] text-sprite-danger">Failed to load configuration.</p>
        <Button onClick={() => void load(true)}>Retry</Button>
      </div>
    );
  }

  const sprite = summary.stt.sprite ?? {};
  const whisperSummary = summary.stt.whisper ?? {};

  return (
    <div className="flex flex-col gap-5" data-selectable="true">
      <FormSection title="Speech recognition">
        <Row label="Provider">
          {/* v1 parity: select with the two known providers, plus a fallback
              option when the current value is unknown (kept editable). */}
          <select
            className={inputClass}
            value={values.provider}
            onChange={(e) => updateField("provider", e.target.value.trim())}
          >
            {!["local", "api"].includes(values.provider) && values.provider ? (
              <option value={values.provider}>{values.provider} (custom)</option>
            ) : null}
            <option value="local">Local (default)</option>
            <option value="api">API (preview)</option>
          </select>
        </Row>
        <Row label="Model">
          <select
            className={inputClass}
            value={values.model.id}
            onChange={(e) =>
              updateField("model", { ...values.model, id: e.target.value })
            }
          >
            {modelCatalog.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {entry.label}
              </option>
            ))}
            {values.model.id && !modelCatalog.some((m) => m.id === values.model.id) ? (
              <option value={values.model.id}>{values.model.id} (custom)</option>
            ) : null}
          </select>
        </Row>
        <Row label="Punctuation">
          <input
            type="checkbox"
            checked={Boolean(values.model.punctuation)}
            onChange={(e) =>
              updateField("model", {
                ...values.model,
                punctuation: e.target.checked,
              })
            }
          />
        </Row>
      </FormSection>

      <FormSection title="Voice activity detection">
        <Row label={`Aggressiveness (${values.vad.aggressiveness ?? 0})`}>
          <input
            type="range"
            min={0}
            max={3}
            value={values.vad.aggressiveness ?? 0}
            onChange={(e) => updateSub("vad", "aggressiveness", e.target.value, "int")}
          />
        </Row>
        <Row label="Start grace (ms)">
          <input
            type="number"
            className={inputClass}
            value={values.vad.start_grace_ms ?? ""}
            onChange={(e) => updateSub("vad", "start_grace_ms", e.target.value, "int")}
          />
        </Row>
        <Row label="Max segment (ms)">
          <input
            type="number"
            className={inputClass}
            value={values.vad.max_segment_ms ?? ""}
            onChange={(e) => updateSub("vad", "max_segment_ms", e.target.value, "int")}
          />
        </Row>
        <Row label="Activation (ms)">
          <input
            type="number"
            className={inputClass}
            value={values.vad.activation_ms ?? ""}
            onChange={(e) => updateSub("vad", "activation_ms", e.target.value, "int")}
          />
        </Row>
        <Row label="Post-speech silence (ms)">
          <input
            type="number"
            className={inputClass}
            value={values.vad.post_speech_silence_ms ?? ""}
            onChange={(e) => updateSub("vad", "post_speech_silence_ms", e.target.value, "int")}
          />
        </Row>
      </FormSection>

      <FormSection title="Wake word">
        <Row label="Keyword">
          <input
            className={inputClass}
            value={values.wakeword.keyword}
            onChange={(e) => updateSub("wakeword", "keyword", e.target.value, "text")}
          />
        </Row>
        <Row label={`Sensitivity (${(values.wakeword.sensitivity ?? 0).toFixed(2)})`}>
          <input
            type="range"
            min={0}
            max={1}
            step={0.01}
            value={values.wakeword.sensitivity ?? 0}
            onChange={(e) => updateSub("wakeword", "sensitivity", e.target.value, "float")}
          />
        </Row>
      </FormSection>

      <FormSection title="Hotkey">
        <Row label="Push to talk">
          <input
            className={inputClass}
            value={values.hotkey.push_to_talk}
            onChange={(e) => updateSub("hotkey", "push_to_talk", e.target.value, "text")}
          />
        </Row>
        <Row label="Mode precedence">
          <select
            className={inputClass}
            value={values.hotkey.mode_precedence}
            onChange={(e) => updateSub("hotkey", "mode_precedence", e.target.value, "text")}
          >
            {/* Exact v1 values — the Rust side maps "ptt"/"wake" (see
                config/defaults.rs STT_DEFAULT_MODE_PRECEDENCE). */}
            <option value="ptt">Push-to-talk first</option>
            <option value="wake">Wake word first</option>
          </select>
        </Row>
      </FormSection>

      <FormSection title="Telemetry">
        <Row label="Level">
          <select
            className={inputClass}
            value={values.telemetry.level}
            onChange={(e) => updateSub("telemetry", "level", e.target.value, "text")}
          >
            {/* Exact v1 values (backend filter maps "errors" → ERROR). */}
            <option value="errors">Errors</option>
            <option value="info">Info</option>
            <option value="debug">Debug</option>
          </select>
        </Row>
      </FormSection>

      <FormSection title="Sprite">
        <Row
          label="Scale"
          hint={`${fmtPct(sprite.min_scale)}–${fmtPct(sprite.max_scale)}`}
        >
          <input
            type="number"
            step={0.05}
            min={typeof sprite.min_scale === "number" ? sprite.min_scale : undefined}
            max={typeof sprite.max_scale === "number" ? sprite.max_scale : undefined}
            className={inputClass}
            value={values.sprite.scale ?? 1}
            onChange={(e) => updateSub("sprite", "scale", e.target.value, "float")}
          />
        </Row>
      </FormSection>

      <FormSection title="Whisper">
        {/* v1 editor: label shows the physical-core ceiling, the placeholder
            is the backend-recommended thread count, blank = auto-tune. */}
        <Row
          label={
            typeof whisperSummary.physical_cores === "number"
              ? `Threads (max ${whisperSummary.physical_cores})`
              : "Threads"
          }
          hint="Leave blank to auto-tune based on your CPU cores."
        >
          <input
            type="number"
            min={1}
            max={
              typeof whisperSummary.physical_cores === "number"
                ? whisperSummary.physical_cores
                : undefined
            }
            placeholder={
              whisperSummary.recommended_threads != null
                ? String(whisperSummary.recommended_threads)
                : "Auto"
            }
            title={
              whisperSummary.recommended_threads != null
                ? `Recommended by the backend: ${whisperSummary.recommended_threads}`
                : "Auto-tuned from your CPU cores when left blank"
            }
            className={inputClass}
            value={values.whisper.threads ?? ""}
            onChange={(e) => updateSub("whisper", "threads", e.target.value, "int")}
          />
        </Row>
      </FormSection>

      {/* Patch preview + actions (v1 `patchPreview` + Save button) */}
      <div className="flex flex-col gap-2">
        <span className="text-[11px] uppercase tracking-wider text-sprite-muted">
          Patch preview
        </span>
        <pre className="max-h-48 w-full overflow-auto rounded-md border border-sprite-border bg-sprite-bg p-2 text-[11.5px] text-sprite-muted">
          {hasChanges ? JSON.stringify(patch, null, 2) : "// No changes"}
        </pre>
        <div className="flex items-center gap-2">
          <Button variant="primary" disabled={!hasChanges || saving} onClick={() => void save()}>
            {saving ? <Spinner /> : "Save"}
          </Button>
          <Button onClick={() => setValues(collectFromSummary(summary))} disabled={saving}>
            Discard
          </Button>
          {status ? (
            <span
              className={`text-[12px] ${
                status.tone === "error"
                  ? "text-sprite-danger"
                  : status.tone === "success"
                    ? "text-sprite-success"
                    : "text-sprite-muted"
              }`}
            >
              {status.text}
            </span>
          ) : null}
        </div>
      </div>

      <Button onClick={() => void load(true)} variant="ghost" className="self-start text-[11.5px]">
        Reload config
      </Button>
    </div>
  );
}

/* Helpers ---------------------------------------------------------------- */

const inputClass =
  "w-full max-w-[240px] rounded-md border border-sprite-border bg-sprite-bg px-2 py-1 text-[12.5px] text-sprite-text outline-none focus:border-sprite-accent";

function FormSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section>
      <h4 className="mb-2 text-[12px] font-semibold uppercase tracking-wider text-sprite-muted">
        {title}
      </h4>
      <div className="flex flex-col gap-2">{children}</div>
    </section>
  );
}

function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <label className="flex items-center justify-between gap-3">
      <span className="text-[12.5px] text-sprite-text">
        {label}
        {hint ? <span className="ml-1 text-sprite-muted">({hint})</span> : null}
      </span>
      {children}
    </label>
  );
}

function fmtPct(value: unknown): string {
  return typeof value === "number" ? `${Math.round(value * 100)}%` : "?";
}

function parseIntField(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }
  const n = Number(trimmed);
  return Number.isFinite(n) ? Math.trunc(n) : null;
}

function parseFloatField(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }
  const n = Number(trimmed);
  return Number.isFinite(n) ? n : null;
}

/** Build editable form values from the summary (v1 `collectFormValues`). */
function collectFromSummary(summary: ConfigSummary | null): ConfigFormValues | null {
  if (!summary?.stt) {
    return null;
  }
  const stt = summary.stt;
  return {
    provider: stt.provider ?? "",
    model: {
      id: stt.model?.id ?? "",
      punctuation: stt.model?.punctuation ?? false,
    },
    vad: {
      aggressiveness: stt.vad?.aggressiveness ?? null,
      start_grace_ms: stt.vad?.start_grace_ms ?? null,
      max_segment_ms: stt.vad?.max_segment_ms ?? null,
      activation_ms: stt.vad?.activation_ms ?? null,
      post_speech_silence_ms: stt.vad?.post_speech_silence_ms ?? null,
    },
    wakeword: {
      keyword: stt.wakeword?.keyword ?? "",
      sensitivity: stt.wakeword?.sensitivity ?? null,
    },
    hotkey: {
      push_to_talk: stt.hotkey?.push_to_talk ?? "",
      mode_precedence: stt.hotkey?.mode_precedence ?? "",
    },
    telemetry: { level: stt.telemetry?.level ?? "" },
    sprite: { scale: stt.sprite?.scale ?? 1 },
    whisper: { threads: stt.whisper?.threads ?? null },
  };
}

/** v1 `hasChanges`: any leaf set at any depth counts as a change. */
function patchHasChanges(patch: ConfigPatch | null | undefined): boolean {
  if (patch === null || patch === undefined) {
    return false;
  }
  if (typeof patch === "object" && !Array.isArray(patch)) {
    return Object.values(patch).some((value) => patchHasChanges(value as ConfigPatch));
  }
  return true;
}
