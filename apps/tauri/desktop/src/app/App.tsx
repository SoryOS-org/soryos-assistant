import { AppShell } from "@/app/layout/AppShell";
import { BackendBridges } from "@/app/bridges/BackendBridges";

/**
 * Root component of the v2 desktop UI.
 *
 * Composition root only — layout, view switching and feature mounting live in
 * `app/layout/AppShell`, feature internals stay in `features/*`. The Tauri
 * event bridge is mounted once, globally, so stores always stay in sync.
 */
export function App() {
  return (
    <>
      <BackendBridges />
      <AppShell />
    </>
  );
}
