import { useEffect } from "react";
import { listenToEvent } from "@/lib/tauri";

/**
 * Subscribe to a Tauri event channel for the lifetime of the component.
 * No-op when running outside Tauri (browser preview).
 *
 * @example
 *   useTauriEvent<{ state: string }>("sprite://ui", (payload) => { ... })
 */
export function useTauriEvent<T>(channel: string, handler: (payload: T) => void): void {
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    listenToEvent<T>(channel, (event) => handler(event.payload)).then((fn) => {
      if (disposed) {
        fn();
        return;
      }
      unlisten = fn;
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
    // Channel strings are static; handler identity is not part of the contract.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [channel]);
}
