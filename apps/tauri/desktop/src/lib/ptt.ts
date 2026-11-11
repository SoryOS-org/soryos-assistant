/**
 * Push-to-talk (Ctrl+Space) registration — port of v1
 * `webview/modules/shortcuts.js`.
 *
 * IMPORTANT: this is DISABLED by default while the v1 sprite window still
 * runs, because the sprite window registers the same global shortcut — a
 * double registration would toggle PTT twice per press. Enable it at the
 * cutover (`VITE_ENABLE_PTT=1` in the build env, or when `webview/` is
 * retired) so the v2 window takes over the shortcut.
 */

interface GlobalShortcutApi {
  register?: (
    shortcut: string,
    handler: (event: { state: "Pressed" | "Released" }) => void,
  ) => Promise<void> | void;
  unregister?: (shortcut: string) => void | Promise<void>;
}

interface TauriGlobal {
  __TAURI__?: {
    globalShortcut?: GlobalShortcutApi;
  };
}

export const PTT_SHORTCUT = "Ctrl+Space";

/** True when the build explicitly enables v2 PTT registration. */
export function pttEnabled(): boolean {
  return import.meta.env?.VITE_ENABLE_PTT === "1";
}

/**
 * Register Ctrl+Space PTT against the Rust backend (`toggle_ptt`).
 * Returns an unregister function, or null when unavailable/disabled.
 */
export function setupPttShortcut(): (() => void) | null {
  if (!pttEnabled()) {
    return null;
  }
  const api = (window as unknown as TauriGlobal).__TAURI__?.globalShortcut;
  if (!api?.register) {
    // eslint-disable-next-line no-console
    console.warn("[ptt] global shortcut plugin unavailable");
    return null;
  }

  let pttActive = false;

  try {
    void Promise.resolve(
      api.register(PTT_SHORTCUT, (event) => {
        void (async () => {
          if (event.state === "Pressed" && !pttActive) {
            pttActive = true;
            await invokeTogglePtt(true);
          } else if (event.state === "Released" && pttActive) {
            pttActive = false;
            await invokeTogglePtt(false);
          }
        })();
      }),
    ).catch((error: unknown) => {
      // eslint-disable-next-line no-console
      console.warn("[ptt] failed to register shortcut", error);
    });
  } catch (error) {
    // eslint-disable-next-line no-console
    console.warn("[ptt] failed to register shortcut", error);
  }

  return () => {
    void api.unregister?.(PTT_SHORTCUT);
  };
}

async function invokeTogglePtt(activate: boolean): Promise<void> {
  try {
    const { invokeCommand } = await import("@/lib/tauri");
    await invokeCommand("toggle_ptt", { activate });
  } catch (error) {
    // eslint-disable-next-line no-console
    console.warn("[ptt] toggle_ptt failed", error);
  }
}
