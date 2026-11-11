import { useEffect } from "react";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { EVENT_CHANNELS } from "@/lib/channels";
import {
  getConversationMemory,
  getConfigSummary,
  getMcpRegistry,
  getPreloadStatus,
  getUiSnapshot,
} from "@/lib/tauri";
import {
  useAlertStore,
  useConfigStore,
} from "@/stores/alertStore";
import { useChatStore } from "@/stores/chatStore";
import {
  useMcpProgressStore,
  useMcpRegistryStore,
} from "@/stores/mcpStore";
import type {
  McpHostEvent,
  McpProgressPayload,
  PreloadStatus,
  SttFinalPayload,
  SttResultPayload,
  UiStatePayload,
  VoiceState,
} from "@/types";
import { useUiStore } from "@/stores/uiStore";
import { setupPttShortcut } from "@/lib/ptt";

/**
 * BackendBridges — all Tauri event subscriptions in one place, ported from
 * v1 `panel.js` wiring (the sprite pipeline ports feeding the panel UI):
 *
 * - `assistant://stt-final` → user transcript entry
 * - `assistant://stt-result` → assistant reply entry (intents / source /
 *   planner context preserved)
 * - `assistant://stt-ready` / `assistant://stt-error` → alert pills
 * - `assistant://mcp-progress` → MCP progress rail
 * - `assistant://mcp-status` → MCP registry state machine
 * - `sprite://ui` → UI-state pill
 * - `config_changed` → reload config summary + preferences tick
 * - `config_dirty` → "config changes pending" pill
 * - `preferences_changed` → preferences tick (PreferencesTab reloads)
 * - `worker_auth_failed` / `worker_auth_ok` → worker auth pill
 * - `stt_preload_status` → preload banner
 *
 * NOTE (Phase 3): PTT (Ctrl+Space) stays registered in the v1 sprite window —
 * it is intentionally NOT re-registered here to avoid double handling.
 */
export function BackendBridges() {
  return (
    <>
      <ChatBridges />
      <McpBridges />
      <SystemBridges />
    </>
  );
}

function ChatBridges() {
  useTauriEvent<SttFinalPayload>(EVENT_CHANNELS.sttFinal, (payload) => {
    if (payload?.text) {
      useChatStore.getState().addTranscript({ text: payload.text });
    }
  });

  useTauriEvent<SttResultPayload>(EVENT_CHANNELS.sttResult, (payload) => {
    if (payload?.reply) {
      useChatStore.getState().addAssistant(payload);
    }
  });

  useTauriEvent<{ message?: string }>(EVENT_CHANNELS.sttError, (payload) => {
    useAlertStore
      .getState()
      .setSttError(payload?.message ? `STT error: ${payload.message}` : "STT error");
  });

  useTauriEvent<unknown>(EVENT_CHANNELS.sttReady, () => {
    useAlertStore.getState().setSttError(null);
  });

  return null;
}

function McpBridges() {
  useTauriEvent<McpProgressPayload>(EVENT_CHANNELS.mcpProgress, (payload) => {
    useMcpProgressStore.getState().handleProgress(payload);
  });

  useTauriEvent<McpHostEvent>(EVENT_CHANNELS.mcpStatus, (payload) => {
    useMcpRegistryStore.getState().handleHostEvent(payload);
  });

  return null;
}

function SystemBridges() {
  useTauriEvent<UiStatePayload>(EVENT_CHANNELS.ui, (payload) => {
    if (payload?.state) {
      useAlertStore.getState().setUiState(payload.state);
      // Mirror the sprite state machine into the voice overlay
      // (listening / thinking / speaking).
      const voice = mapSpriteStateToVoice(payload.state);
      useUiStore.getState().setVoice(voice);
    }
  });

  // `panel://navigate` — remote view switching (v1 `navigation.js` port).
  useTauriEvent<{ view?: string }>(EVENT_CHANNELS.navigation, (payload) => {
    const view = payload?.view;
    if (!view) {
      return;
    }
    const { setMainView, setSettingsTab } = useUiStore.getState();
    if (view === "chat") {
      setMainView("chat");
      return;
    }
    // Every other v1 panel view maps onto the settings tabs.
    setSettingsTab(view);
    setMainView("settings");
  });

  // `assistant://stt-started` — capture started: open the live voice session
  // (ChatGPT voice-mode behavior) and mirror the state.
  useTauriEvent<{ trigger?: string }>(EVENT_CHANNELS.sttStarted, () => {
    const ui = useUiStore.getState();
    ui.openVoiceSession();
    ui.setVoice("listening");
  });

  // `intent_catalog_updated` — applications catalog changed: the preferences
  // view reloads its catalog.
  useTauriEvent<unknown>(EVENT_CHANNELS.catalogChanged, () => {
    useAlertStore.getState().bumpPreferencesTick();
  });

  useTauriEvent<PreloadStatus>(EVENT_CHANNELS.preload, (payload) => {
    useAlertStore.getState().setPreload(payload ?? null);
  });

  useTauriEvent<unknown>(EVENT_CHANNELS.configChanged, () => {
    void (async () => {
      try {
        const summary = await getConfigSummary();
        useConfigStore.getState().applySummary(summary);
        useAlertStore.getState().setConfigDirty(false);
      } catch {
        // Reload failures surface in the config editor status line.
      }
      useAlertStore.getState().bumpPreferencesTick();
    })();
  });

  useTauriEvent<unknown>(EVENT_CHANNELS.configDirty, () => {
    useAlertStore.getState().setConfigDirty(true);
  });

  useTauriEvent<unknown>(EVENT_CHANNELS.preferencesChanged, () => {
    useAlertStore.getState().bumpPreferencesTick();
  });

  useTauriEvent<unknown>(EVENT_CHANNELS.workerAuth, () => {
    useAlertStore.getState().setWorkerAuthFailed(true);
  });

  useTauriEvent<unknown>(EVENT_CHANNELS.workerAuthOk, () => {
    useAlertStore.getState().setWorkerAuthFailed(false);
  });

  // One-time hydration on mount: UI snapshot, preload status, MCP registry
  // snapshot and conversation memory.
  useEffect(() => {
    let disposed = false;

    void (async () => {
      try {
        const snapshot = await getUiSnapshot();
        if (!disposed && snapshot?.state?.state) {
          useAlertStore.getState().setUiState(snapshot.state.state);
        }
      } catch {
        // Non-fatal: the pill stays at "Idle".
      }

      try {
        const preload = await getPreloadStatus();
        if (!disposed && preload) {
          useAlertStore.getState().setPreload(preload);
        }
      } catch {
        // Non-fatal.
      }

      try {
        const registry = await getMcpRegistry();
        if (!disposed) {
          useMcpRegistryStore
            .getState()
            .applyRegistrySnapshot(Array.isArray(registry) ? registry : []);
        }
      } catch {
        useMcpRegistryStore.getState().setStatus("Failed to load MCP registry.", "error");
      }

      try {
        const memory = await getConversationMemory();
        if (!disposed) {
          useChatStore.getState().hydrateFromMemory(memory);
        }
      } catch {
        // Non-fatal: the transcript stays empty until the first turn.
      }
    })();

    // PTT (Ctrl+Space) — v2 registration, disabled by default while the v1
    // sprite window still owns the shortcut (see lib/ptt.ts).
    const unregisterPtt = setupPttShortcut();

    return () => {
      disposed = true;
      unregisterPtt?.();
    };
  }, []);

  return null;
}

/** Map the sprite state machine labels onto the v2 voice overlay states. */
function mapSpriteStateToVoice(state: string): VoiceState {
  const normalized = state.toLowerCase();
  if (normalized.includes("listen")) {
    return "listening";
  }
  if (normalized.includes("think") || normalized.includes("process") || normalized.includes("plan")) {
    return "thinking";
  }
  if (normalized.includes("speak") || normalized.includes("tts")) {
    return "speaking";
  }
  if (normalized.includes("error") || normalized.includes("fail")) {
    return "error";
  }
  return "idle";
}
