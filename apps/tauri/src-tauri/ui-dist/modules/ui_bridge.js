import { spriteSurface } from "./dom.js";
import { invoke, tauriEvent } from "./tauri.js";

export function setupUiEventBridge() {
  if (!spriteSurface || !tauriEvent || !invoke) {
    return;
  }

  const UI_CHANNEL = "sprite://ui";
  const STATE_DWELL_MS = 120;
  const EMA_ALPHA = 0.6;
  const ACTIVE_VU_STATES = new Set(["Listening", "Speaking"]);

  const machine = {
    current: spriteSurface.getAttribute("data-visual-state") || "Idle",
    lastTransitionTs: performance.now(),
    pendingState: null,
    pendingTimer: null,
    vuSmoothed: 0,
  };

  function clamp01(value) {
    if (Number.isNaN(value)) {
      return 0;
    }
    return Math.min(Math.max(value, 0), 1);
  }

  function setVuLevel(level, immediate = false) {
    const clamped = clamp01(typeof level === "number" ? level : Number(level));
    machine.vuSmoothed = immediate
      ? clamped
      : EMA_ALPHA * clamped + (1 - EMA_ALPHA) * machine.vuSmoothed;
    spriteSurface.style.setProperty("--vu-level", machine.vuSmoothed.toFixed(3));
  }

  function applyState(state, timestamp) {
    machine.current = state;
    machine.lastTransitionTs = timestamp;
    spriteSurface.setAttribute("data-visual-state", state);

    if (!ACTIVE_VU_STATES.has(state)) {
      setVuLevel(0, true);
    }
  }

  function requestStateChange(nextState) {
    if (typeof nextState !== "string" || nextState.length === 0) {
      return;
    }
    if (machine.current === nextState) {
      return;
    }

    const now = performance.now();
    const elapsed = now - machine.lastTransitionTs;

    if (elapsed < STATE_DWELL_MS && nextState !== "Error") {
      machine.pendingState = nextState;
      if (!machine.pendingTimer) {
        machine.pendingTimer = window.setTimeout(() => {
          machine.pendingTimer = null;
          const pending = machine.pendingState;
          machine.pendingState = null;
          if (pending) {
            requestStateChange(pending);
          }
        }, STATE_DWELL_MS - elapsed);
      }
      return;
    }

    if (machine.pendingTimer) {
      clearTimeout(machine.pendingTimer);
      machine.pendingTimer = null;
      machine.pendingState = null;
    }

    applyState(nextState, now);
  }

  function handleCue(payload) {
    const cue = payload?.cue;
    if (typeof cue !== "string" || cue.length === 0) {
      return;
    }
    const cueClass = `sprite__surface--cue-${cue.toLowerCase()}`;
    spriteSurface.classList.add(cueClass);
    window.setTimeout(() => {
      spriteSurface.classList.remove(cueClass);
    }, 320);
  }

  function handleUiEvent(message) {
    const kind = message?.kind;
    const payload = message?.payload;

    switch (kind) {
      case "State":
        requestStateChange(payload?.state);
        break;
      case "Vu":
        setVuLevel(payload?.level);
        break;
      case "Cue":
        handleCue(payload);
        break;
      default:
        break;
    }
  }

  (async () => {
    try {
      const snapshot = await invoke("get_ui_snapshot");
      if (snapshot?.state?.state) {
        applyState(snapshot.state.state, performance.now());
      }
      if (snapshot?.vu?.level != null) {
        const immediate = ACTIVE_VU_STATES.has(machine.current);
        setVuLevel(snapshot.vu.level, immediate);
      } else {
        setVuLevel(0, true);
      }
    } catch (error) {
      console.warn("Failed to load UI snapshot", error);
    }

    let uiUnlistenPromise = null;
    try {
      uiUnlistenPromise = tauriEvent.listen(UI_CHANNEL, (event) => {
        handleUiEvent(event.payload);
      });
    } catch (error) {
      console.warn("Failed to subscribe to UI channel", error);
    }

    const detachUiListener = () => {
      if (!uiUnlistenPromise) {
        return;
      }
      uiUnlistenPromise
        .then((unlisten) => {
          unlisten();
          uiUnlistenPromise = null;
        })
        .catch(() => {
          uiUnlistenPromise = null;
        });
    };

    window.addEventListener("beforeunload", detachUiListener, { once: true });
    if (import.meta.hot) {
      import.meta.hot.dispose(detachUiListener);
    }
  })();
}
