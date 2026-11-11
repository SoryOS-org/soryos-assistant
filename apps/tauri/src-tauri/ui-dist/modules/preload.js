import {
  preloadMessage,
  preloadOverlay,
  preloadProgressBar,
  preloadProgressContainer,
  spriteContainer,
} from "./dom.js";
import { applySpriteScale, hydrateSpriteScale } from "./sprite.js";

let invokeHandle = null;
let preloadSizingRequestId = 0;
let preloadSizingVisible = null;

export function bindPreloadInvoke(invoke) {
  invokeHandle = invoke || null;
}

function applyPreloadSizing(visible) {
  if (!invokeHandle) {
    return;
  }
  preloadSizingRequestId += 1;
  const requestId = preloadSizingRequestId;
  preloadSizingVisible = visible;
  invokeHandle("set_sprite_preload_state", { visible })
    .then((scale) => {
      if (requestId !== preloadSizingRequestId || preloadSizingVisible !== visible) {
        return;
      }
      if (typeof scale === "number") {
        applySpriteScale(scale);
      } else if (!visible) {
        hydrateSpriteScale(invokeHandle);
      }
    })
    .catch((error) => {
      console.warn("Failed to update preload sizing", error);
    });
}

function setPreloadMessage(message) {
  if (preloadMessage) {
    preloadMessage.textContent = message;
  }
}

function setPreloadProgress(percent) {
  if (!preloadProgressBar || !preloadProgressContainer) {
    return;
  }
  const clamped = Math.max(0, Math.min(percent, 100));
  preloadProgressBar.style.width = `${clamped}%`;
  preloadProgressContainer.setAttribute("aria-valuenow", clamped.toFixed(0));
  preloadProgressContainer.setAttribute("aria-valuetext", `${Math.round(clamped)}%`);
}

function showPreloadOverlay() {
  if (!preloadOverlay) {
    return;
  }
  const wasHidden = preloadOverlay.hidden;
  preloadOverlay.hidden = false;
  if (wasHidden) {
    applyPreloadSizing(true);
  }
}

export function showSprite() {
  setPreloadProgress(100);
  if (spriteContainer) {
    spriteContainer.classList.remove("sprite--hidden");
  }
  const wasVisible = preloadOverlay && !preloadOverlay.hidden;
  if (preloadOverlay) {
    preloadOverlay.hidden = true;
  }
  if (wasVisible) {
    applyPreloadSizing(false);
  }
}

export function handlePreloadStatus(payload = {}) {
  const { state, model, details } = payload;

  if (!preloadOverlay) {
    return;
  }

  switch (state) {
    case "started": {
      if (spriteContainer) {
        spriteContainer.classList.add("sprite--hidden");
      }
      setPreloadProgress(0);
      const label = model ? `Downloading speech model "${model}"...` : "Downloading speech model...";
      setPreloadMessage(label);
      showPreloadOverlay();
      break;
    }
    case "progress": {
      if (spriteContainer) {
        spriteContainer.classList.add("sprite--hidden");
      }
      const percent = typeof payload.progress === "number" ? payload.progress * 100 : 0;
      setPreloadProgress(percent);
      if (model) {
        setPreloadMessage(`Downloading speech model "${model}"... ${Math.round(percent)}%`);
      } else {
        setPreloadMessage(`Downloading speech model... ${Math.round(percent)}%`);
      }
      showPreloadOverlay();
      break;
    }
    case "cached":
    case "completed": {
      showSprite();
      setPreloadProgress(100);
      break;
    }
    case "error": {
      const label = details
        ? `Failed to prepare model${model ? ` "${model}"` : ""}: ${details}`
        : `Failed to prepare model${model ? ` "${model}"` : ""}. Check logs for details.`;
      setPreloadMessage(label);
      showPreloadOverlay();
      if (spriteContainer) {
        spriteContainer.classList.add("sprite--hidden");
      }
      setPreloadProgress(0);
      break;
    }
    default:
      break;
  }
}

export async function hydratePreloadStatus(invoke) {
  if (!invoke) {
    return;
  }
  try {
    const snapshot = await invoke("get_preload_status");
    if (snapshot) {
      handlePreloadStatus(snapshot);
    }
  } catch (error) {
    console.warn("Failed to hydrate preload snapshot", error);
  }
}

export function setPreloadVisible(visible, message = "Preparing model...") {
  if (!preloadOverlay || !spriteContainer) {
    return;
  }
  if (visible) {
    spriteContainer.classList.add("sprite--hidden");
    setPreloadMessage(message);
    setPreloadProgress(0);
    showPreloadOverlay();
  } else {
    showSprite();
  }
}
