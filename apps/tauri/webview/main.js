import { closeBtn, preloadDismissButton, spriteSurface } from "./modules/dom.js";
import { exposeAssistantApi } from "./modules/assistant_api.js";
import { exposeSpriteDemoApi } from "./modules/demo.js";
import {
  bindPreloadInvoke,
  handlePreloadStatus,
  hydratePreloadStatus,
} from "./modules/preload.js";
import { hydrateSpriteScale } from "./modules/sprite.js";
import { setupGlobalShortcuts } from "./modules/shortcuts.js";
import { setupSttEventListeners } from "./modules/stt_events.js";
import { setupUiEventBridge } from "./modules/ui_bridge.js";
import { appWindow, invoke, tauriEvent } from "./modules/tauri.js";

const PRELOAD_CHANNEL = "stt_preload_status";
const CONFIG_CHANGED_CHANNEL = "config_changed";

function suppressContext(event) {
  event.preventDefault();
}

document.addEventListener("contextmenu", suppressContext, { capture: true });
window.addEventListener("contextmenu", suppressContext, { capture: true });
window.addEventListener(
  "mousedown",
  (event) => {
    if (event.button === 2) {
      event.preventDefault();
    }
  },
  true,
);

if (spriteSurface) {
  spriteSurface.addEventListener("selectstart", (event) => event.preventDefault());
}

if (spriteSurface && appWindow?.startDragging) {
  spriteSurface.addEventListener("mousedown", async (event) => {
    if (event.button !== 0) {
      return;
    }

    event.preventDefault();

    try {
      await appWindow.startDragging();
    } catch (error) {
      console.error("Failed to start drag", error);
    }
  });
}

async function closeWindow() {
  if (invoke) {
    try {
      await invoke("close_window");
      return;
    } catch (error) {
      console.error("Failed to close window via command", error);
    }
  }

  try {
    await appWindow?.hide?.();
  } catch (error) {
    console.error("Failed to hide window fallback", error);
  }
}

if (closeBtn) {
  closeBtn.addEventListener("click", (event) => {
    event.preventDefault();
    closeWindow();
  });
}

if (preloadDismissButton) {
  preloadDismissButton.addEventListener("click", (event) => {
    event.preventDefault();
    closeWindow();
  });
}

exposeAssistantApi(invoke);
bindPreloadInvoke(invoke);
setupUiEventBridge();
setupGlobalShortcuts();
setupSttEventListeners();
exposeSpriteDemoApi();

if (tauriEvent?.listen) {
  tauriEvent
    .listen(PRELOAD_CHANNEL, (event) => {
      handlePreloadStatus(event.payload || {});
    })
    .catch((error) => {
      console.warn("Failed to subscribe to preload channel", error);
    });

  tauriEvent
    .listen(CONFIG_CHANGED_CHANNEL, () => {
      hydrateSpriteScale(invoke).catch((error) => {
        console.warn("Failed to apply sprite size after config change", error);
      });
    })
    .catch((error) => {
      console.warn("Failed to subscribe to config change channel", error);
    });
}

hydratePreloadStatus(invoke);
hydrateSpriteScale(invoke).catch((error) => {
  console.warn("Failed to hydrate sprite size", error);
});
