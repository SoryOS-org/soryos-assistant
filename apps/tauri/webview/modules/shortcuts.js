import { invoke, tauriGlobalShortcut } from "./tauri.js";

export function setupGlobalShortcuts() {
  let isPttActive = false;
  const shortcut = "Ctrl+Space";
  let registeredHere = false;

  const registerShortcut = () => {
    if (!tauriGlobalShortcut?.register) {
      console.warn("? Global shortcut plugin not available");
      return Promise.resolve();
    }

    return tauriGlobalShortcut
      .register(shortcut, (event) => {
        console.log("?? Global shortcut triggered:", event);

        if (event.state === "Pressed" && !isPttActive) {
          isPttActive = true;
          console.log("?? PTT PRESSED - Hold and speak now!");
          invoke?.("toggle_ptt", { activate: true }).catch(console.error);
        } else if (event.state === "Released" && isPttActive) {
          isPttActive = false;
          console.log("?? PTT RELEASED - Processing speech...");
          invoke?.("toggle_ptt", { activate: false }).catch(console.error);
        }
      })
      .then(() => {
        registeredHere = true;
        console.log("? Global shortcut registered successfully (True PTT: Hold to speak)");
      })
      .catch((error) => {
        console.error("? Failed to register global shortcut:", error);
      });
  };

  const ensureFreshShortcut = async () => {
    if (!tauriGlobalShortcut?.isRegistered || !tauriGlobalShortcut?.unregister) {
      registerShortcut();
      return;
    }

    try {
      const alreadyRegistered = await tauriGlobalShortcut.isRegistered(shortcut);
      if (alreadyRegistered) {
        await tauriGlobalShortcut.unregister(shortcut);
      }
    } catch (error) {
      console.warn("? Failed to check/unregister existing shortcut:", error);
    } finally {
      await registerShortcut();
    }
  };

  ensureFreshShortcut();

  if (import.meta.hot) {
    import.meta.hot.dispose(() => {
      if (registeredHere) {
        tauriGlobalShortcut
          ?.unregister?.(shortcut)
          .catch((error) => console.warn("? Failed to unregister shortcut on dispose:", error));
      }
    });
  }
}
