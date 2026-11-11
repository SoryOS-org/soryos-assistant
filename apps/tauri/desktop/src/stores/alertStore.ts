/**
 * Alert/status pills and the config store — ports of v1 `uiState`,
 * `pill state`, and `configState`.
 */
import { create } from "zustand";
import type {
  ConfigFormValues,
  ConfigSummary,
  PreloadStatus,
} from "@/types";

/* ================================================================== */
/* Alerts (the pills row above the chat)                               */
/* ================================================================== */

interface AlertState {
  /** Sprite state-machine label from `sprite://ui` / `get_ui_snapshot`. */
  uiState: string;
  /** Unapplied config edits signaled by `config_dirty`. */
  configDirty: boolean;
  /** Worker auth broken (`worker_auth_failed` / `worker_auth_ok`). */
  workerAuthFailed: boolean;
  /** STT error message (`assistant://stt-error`). */
  sttError: string | null;
  /** Preload banner state (`stt_preload_status`). */
  preload: PreloadStatus | null;
  /** Incremented on `config_changed` / `preferences_changed`. */
  preferencesTick: number;

  /** Last time `setUiState` fired (v1 pill shows "Updated HH:MM:SS"). */
  uiStateUpdatedAt: number | null;

  setUiState: (state: string) => void;
  setConfigDirty: (dirty: boolean) => void;
  setWorkerAuthFailed: (failed: boolean) => void;
  setSttError: (message: string | null) => void;
  setPreload: (status: PreloadStatus | null) => void;
  bumpPreferencesTick: () => void;
}

export const useAlertStore = create<AlertState>((set) => ({
  uiState: "Idle",
  uiStateUpdatedAt: null,
  configDirty: false,
  workerAuthFailed: false,
  sttError: null,
  preload: null,
  preferencesTick: 0,

  setUiState: (uiState) =>
    set({ uiState, uiStateUpdatedAt: Date.now() }),
  setConfigDirty: (configDirty) => set({ configDirty }),
  setWorkerAuthFailed: (workerAuthFailed) => set({ workerAuthFailed }),
  setSttError: (sttError) => set({ sttError }),
  setPreload: (preload) => set({ preload }),
  bumpPreferencesTick: () => set((s) => ({ preferencesTick: s.preferencesTick + 1 })),
}));

/* ================================================================== */
/* Config store (editor baseline + model catalog)                      */
/* ================================================================== */

interface ConfigStoreState {
  summary: ConfigSummary | null;
  loaded: boolean;
  loading: boolean;
  modelCatalog: Array<{ id: string; label: string }>;

  applySummary: (summary: ConfigSummary | null) => void;
  setLoading: (loading: boolean) => void;
}

export const useConfigStore = create<ConfigStoreState>((set) => ({
  summary: null,
  loaded: false,
  loading: false,
  modelCatalog: [],

  applySummary: (summary: ConfigSummary | null) =>
    set((s) => ({
      summary: summary ?? s.summary,
      loaded: Boolean(summary),
      modelCatalog:
        summary && Array.isArray(summary.models) ? summary.models : s.modelCatalog,
    })),
  setLoading: (loading) => set({ loading }),
}));

/* ================================================================== */
/* Config editor deltas (v1 `collectFormValues` baseline diff)          */
/* ================================================================== */

/**
 * Build the `stt` patch describing only changes vs the baseline summary
 * (v1 `buildPatchFromValues`).
 */
export function buildConfigPatch(
  values: ConfigFormValues | null,
  baseline: ConfigSummary | null,
): Record<string, unknown> {
  if (!baseline?.stt || !values) {
    return {};
  }
  const sttPatch: Record<string, unknown> = {};
  const base = baseline.stt;
  const baseObjs = {
    model: base.model || {},
    vad: base.vad || {},
    wakeword: base.wakeword || {},
    hotkey: base.hotkey || {},
    telemetry: base.telemetry || {},
    sprite: base.sprite || {},
    whisper: base.whisper || {},
  };

  const assign = (segments: string[], newValue: unknown, oldValue: unknown): void => {
    const newIsNil = newValue === null || newValue === undefined;
    const oldIsNil = oldValue === null || oldValue === undefined;
    if (newIsNil && oldIsNil) {
      return;
    }
    // Mirrors v1: numeric comparison uses Object.is, everything else `===`
    // (loose nil handling above first).
    const equal = !newIsNil && !oldIsNil && Object.is(newValue as never, oldValue as never);
    if (equal) {
      return;
    }

    let cursor = sttPatch;
    for (let i = 0; i < segments.length - 1; i += 1) {
      const key = segments[i];
      if (!cursor[key]) {
        cursor[key] = {};
      }
      cursor = cursor[key] as Record<string, unknown>;
    }
    cursor[segments[segments.length - 1]] = newIsNil ? null : newValue;
  };

  const bv = baseObjs;

  assign(["provider"], values.provider, base.provider);
  assign(["model", "id"], values.model.id, bv.model.id);
  assign(["model", "punctuation"], values.model.punctuation, bv.model.punctuation);
  assign(["vad", "aggressiveness"], values.vad.aggressiveness, bv.vad.aggressiveness);
  assign(["vad", "start_grace_ms"], values.vad.start_grace_ms, bv.vad.start_grace_ms);
  assign(["vad", "max_segment_ms"], values.vad.max_segment_ms, bv.vad.max_segment_ms);
  assign(["vad", "activation_ms"], values.vad.activation_ms, bv.vad.activation_ms);
  assign(
    ["vad", "post_speech_silence_ms"],
    values.vad.post_speech_silence_ms,
    bv.vad.post_speech_silence_ms,
  );
  assign(["wakeword", "keyword"], values.wakeword.keyword, bv.wakeword.keyword);
  assign(["wakeword", "sensitivity"], values.wakeword.sensitivity, bv.wakeword.sensitivity);
  assign(["hotkey", "push_to_talk"], values.hotkey.push_to_talk, bv.hotkey.push_to_talk);
  assign(
    ["hotkey", "mode_precedence"],
    values.hotkey.mode_precedence,
    bv.hotkey.mode_precedence,
  );
  assign(["telemetry", "level"], values.telemetry.level, bv.telemetry.level);
  assign(["sprite", "scale"], values.sprite.scale, bv.sprite.scale);
  assign(["whisper", "threads"], values.whisper.threads, bv.whisper.threads);

  return Object.keys(sttPatch).length === 0 ? {} : sttPatch;
}
