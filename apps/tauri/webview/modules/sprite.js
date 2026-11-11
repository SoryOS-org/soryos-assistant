import { spriteSurface, spriteWarning } from "./dom.js";

const SPRITE_SCALE_MIN = 0.2;
const SPRITE_SCALE_MAX = 1.5;
const SPRITE_SCALE_DEFAULT = 1;

let spriteScale = null;
let warningMode = null;
let lastAccessKey = null;

function clampSpriteScale(value) {
  const numeric = Number(value);
  if (!Number.isFinite(numeric)) {
    return SPRITE_SCALE_DEFAULT;
  }
  return Math.min(SPRITE_SCALE_MAX, Math.max(SPRITE_SCALE_MIN, numeric));
}

export function applySpriteScale(nextScale) {
  const clamped = clampSpriteScale(nextScale);
  if (spriteScale === clamped) {
    return;
  }
  spriteScale = clamped;
  if (document?.documentElement?.style) {
    document.documentElement.style.setProperty("--sprite-scale", clamped.toString());
  }
}

export async function hydrateSpriteScale(invoke) {
  if (!invoke) {
    return;
  }
  try {
    const summary = await invoke("get_config_summary");
    const scale = summary?.stt?.sprite?.scale;
    applySpriteScale(scale);
    applySpriteWarning(summary?.stt?.sidecar?.has_access_key);
  } catch (error) {
    console.warn("Failed to hydrate sprite appearance", error);
  }
}

function setSpriteWarning(visible, { message, tone, mode } = {}) {
  if (!spriteWarning) {
    return;
  }

  if (!visible) {
    if (warningMode === mode) {
      warningMode = null;
      spriteWarning.hidden = true;
      spriteWarning.removeAttribute("title");
      delete spriteWarning.dataset.state;
    }
    return;
  }

  warningMode = mode || "generic";
  spriteWarning.hidden = false;
  if (tone) {
    spriteWarning.dataset.state = tone;
  } else {
    delete spriteWarning.dataset.state;
  }
  if (message) {
    spriteWarning.setAttribute("title", message);
  } else {
    spriteWarning.removeAttribute("title");
  }
}

export function applySpriteWarning(hasAccessKey) {
  lastAccessKey = hasAccessKey;
  if (warningMode && warningMode !== "access-key") {
    return;
  }
  if (hasAccessKey === false) {
    setSpriteWarning(true, {
      message: "Wake word disabled: access key missing",
      tone: "warning",
      mode: "access-key",
    });
  } else {
    setSpriteWarning(false, { mode: "access-key" });
  }
}

export function setSpriteWarningError(message) {
  setSpriteWarning(true, {
    message: message || "Speech engine failed to start",
    tone: "error",
    mode: "stt-error",
  });
}

export function clearSpriteWarningError() {
  setSpriteWarning(false, { mode: "stt-error" });
  applySpriteWarning(lastAccessKey);
}

export function setSpriteWarningVisible(visible) {
  setSpriteWarning(Boolean(visible), {
    message: "Sprite warning",
    tone: "warning",
    mode: "manual",
  });
  if (!visible) {
    warningMode = null;
  }
}

export function setSpriteState(state) {
  if (!spriteSurface) {
    return;
  }
  if (typeof state !== "string" || state.length === 0) {
    return;
  }
  spriteSurface.setAttribute("data-visual-state", state);
  if (state !== "Listening" && state !== "Speaking") {
    spriteSurface.style.setProperty("--vu-level", "0");
  }
}

export function setVuLevel(level) {
  if (!spriteSurface) {
    return;
  }
  const value = Number(level);
  if (!Number.isFinite(value)) {
    return;
  }
  const clamped = Math.min(Math.max(value, 0), 1);
  spriteSurface.style.setProperty("--vu-level", clamped.toFixed(3));
}
