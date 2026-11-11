import { setSpriteState, setSpriteWarningVisible, setVuLevel } from "./sprite.js";
import { setPreloadVisible } from "./preload.js";

export function exposeSpriteDemoApi() {
  if (window.spriteDemo) {
    return;
  }

  // Console-only sprite demo helpers. Usage:
  // - spriteDemo.start(1400) / spriteDemo.stop()
  // - spriteDemo.setState("Listening" | "Speaking" | "Thinking" | "Planning" | "Error" | "Idle")
  // - spriteDemo.setWarning(true/false)
  // - spriteDemo.setPreload(true/false, "optional message")
  // - spriteDemo.setVu(0.0..1.0)
  const demo = {
    _timer: null,
    _index: 0,
    _states: ["Idle", "Listening", "Speaking", "Thinking", "Planning", "Error"],
    _warning: false,
    _preload: false,
    start(intervalMs = 3000) {
      if (demo._timer) {
        console.info("[spriteDemo] Cycle already running.");
        return "already-running";
      }
      console.info("[spriteDemo] Starting cycle.", { intervalMs });
      demo._timer = window.setInterval(() => {
        demo._index = (demo._index + 1) % demo._states.length;
        const state = demo._states[demo._index];
        setSpriteState(state);
        if (state === "Error") {
          demo.setWarning(true);
        } else if (demo._warning) {
          demo.setWarning(false);
        }
        console.info("[spriteDemo] State:", state);
      }, intervalMs);
      return "started";
    },
    stop(reset = true) {
      if (demo._timer) {
        clearInterval(demo._timer);
        demo._timer = null;
        console.info("[spriteDemo] Cycle stopped.");
        if (reset) {
          demo.reset();
        }
        return reset ? "stopped-reset" : "stopped";
      }
      console.info("[spriteDemo] Cycle not running.");
      if (reset) {
        demo.reset();
        return "not-running-reset";
      }
      return "not-running";
    },
    reset() {
      demo._index = 0;
      setSpriteState("Idle");
      demo.setWarning(false);
      demo.setPreload(false);
      demo.setVu(0);
      console.info("[spriteDemo] Reset to Idle.");
      return "reset";
    },
    setState(state) {
      demo.stop();
      setSpriteState(state);
      console.info("[spriteDemo] State set:", state);
      return state;
    },
    setWarning(visible) {
      demo._warning = Boolean(visible);
      setSpriteWarningVisible(demo._warning);
      console.info("[spriteDemo] Warning:", demo._warning);
      return demo._warning;
    },
    setPreload(visible, message) {
      demo._preload = Boolean(visible);
      setPreloadVisible(demo._preload, message);
      console.info("[spriteDemo] Preload:", demo._preload, message || null);
      return demo._preload;
    },
    setVu(level) {
      setVuLevel(level);
      console.info("[spriteDemo] VU:", level);
      return level;
    },
  };

  Object.defineProperty(window, "spriteDemo", {
    value: demo,
    configurable: false,
    writable: false,
  });
}
