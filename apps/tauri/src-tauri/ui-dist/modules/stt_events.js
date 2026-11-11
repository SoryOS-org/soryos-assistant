import { clearSpriteWarningError, setSpriteWarningError } from "./sprite.js";
import { tauriEvent } from "./tauri.js";

export function setupSttEventListeners() {
  if (!tauriEvent) {
    console.warn("Tauri event API is not available for STT events");
    return;
  }

  tauriEvent.listen("assistant://stt-final", (event) => {
    console.log(" TRANSCRIPT RECEIVED:", event.payload.text);
    console.log(" Details:", {
      language: event.payload.lang,
      confidence: event.payload.confidence,
      duration: `${event.payload.ts_end_ms - event.payload.ts_start_ms}ms`,
    });
  });

  tauriEvent.listen("assistant://stt-started", (event) => {
    console.log(" STT STARTED:", event.payload.trigger);
  });

  tauriEvent.listen("assistant://stt-ready", (event) => {
    console.log("... STT READY:", event.payload);
    clearSpriteWarningError();
  });

  tauriEvent.listen("assistant://stt-error", (event) => {
    const message = event?.payload?.message;
    console.warn("... STT ERROR:", event.payload);
    setSpriteWarningError(message);
  });

  console.log("... STT event listeners set up");
}
