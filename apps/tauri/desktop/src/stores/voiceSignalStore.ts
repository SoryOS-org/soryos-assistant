/**
 * Voice session store — live signal levels for the voice orb.
 *
 * `vuLevel` comes from the `sprite://ui` payload (`vu_level`, 0..1) when
 * running inside Tauri, or from a local Web Audio analyser when previewing
 * the session in a plain browser.
 */
import { create } from "zustand";

interface VoiceSignalState {
  /** Smoothed voice level 0..1 driving the orb animation. */
  vuLevel: number;
  /** User muted their own mic (visual only in v2 preview). */
  muted: boolean;

  setVuLevel: (level: number) => void;
  setMuted: (muted: boolean) => void;
}

export const useVoiceSignalStore = create<VoiceSignalState>((set) => ({
  vuLevel: 0,
  muted: false,

  setVuLevel: (vuLevel) => set({ vuLevel: Math.min(1, Math.max(0, vuLevel)) }),
  setMuted: (muted) => set({ muted }),
}));
