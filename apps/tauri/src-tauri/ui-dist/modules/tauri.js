export const tauriCore = window.__TAURI__?.core;
export const tauriWindow = window.__TAURI__?.window;
export const tauriEvent = window.__TAURI__?.event;
export const tauriGlobalShortcut = window.__TAURI__?.globalShortcut;
export const invoke = tauriCore?.invoke;
export const appWindow = tauriWindow?.getCurrent?.();
