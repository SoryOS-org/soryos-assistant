import { create } from "zustand";
import type { MainView, RightPanelView, VoiceState } from "@/types";

interface UiState {
  /** Which main view the window shows. */
  mainView: MainView;
  /** Tab the settings view opens on (set by `panel://navigate`). */
  settingsTab: string;
  /** Whether the session sidebar is visible. */
  sidebarOpen: boolean;
  /** Which right panel is mounted (when `panelOpen`). */
  panelView: RightPanelView;
  /** Whether the right panel is visible at all. */
  panelOpen: boolean;
  /** Voice overlay state, mirrored from `sprite://ui` events. */
  voiceActive: boolean;
  voiceState: VoiceState;
  /** Full-screen live voice session (ChatGPT voice-mode style). */
  voiceSessionOpen: boolean;
  /** Manual MCP rail toggle (v1 "Show/Hide MCP"): the rail only shows when
      there is tool activity AND the user has not hidden it. */
  mcpRailVisible: boolean;

  setMainView: (view: MainView) => void;
  setSettingsTab: (tab: string) => void;
  toggleSidebar: () => void;
  togglePanel: () => void;
  setPanelView: (view: RightPanelView) => void;
  toggleMcpRail: () => void;
  setVoice: (state: VoiceState) => void;
  openVoiceSession: () => void;
  closeVoiceSession: () => void;
}

/** Global UI/shell state. */
export const useUiStore = create<UiState>((set) => ({
  mainView: "chat",
  settingsTab: "config",
  sidebarOpen: true,
  panelView: "terminal",
  panelOpen: false,
  voiceActive: false,
  voiceState: "idle",
  voiceSessionOpen: false,
  mcpRailVisible: true,

  setMainView: (mainView) => set({ mainView }),
  setSettingsTab: (settingsTab) => set({ settingsTab }),
  toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
  togglePanel: () => set((s) => ({ panelOpen: !s.panelOpen })),
  setPanelView: (panelView) => set({ panelView, panelOpen: true }),
  toggleMcpRail: () => set((s) => ({ mcpRailVisible: !s.mcpRailVisible })),
  setVoice: (voiceState) =>
    set({ voiceState, voiceActive: voiceState !== "idle" && voiceState !== "error" }),
  openVoiceSession: () => set({ voiceSessionOpen: true }),
  closeVoiceSession: () =>
    set({
      voiceSessionOpen: false,
      // Leaving the session resets the transient voice state.
      voiceState: "idle",
      voiceActive: false,
    }),
}));
