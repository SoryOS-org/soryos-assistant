import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@/app/App";
import { ErrorBoundary } from "@/ui/ErrorBoundary";
import "@/styles/globals.css";

// Debug aid: surface uncaught frontend errors in the window title (readable
// from the shell with `xdotool getwindowname`, no devtools required).
window.addEventListener("error", (event) => {
  document.title = `[v2] JS-ERR ${String(event.message).slice(0, 70)}`;
});
window.addEventListener("unhandledrejection", (event) => {
  document.title = `[v2] REJECT ${String(event.reason).slice(0, 70)}`;
});

const rootElement = document.getElementById("root");

if (!rootElement) {
  document.body.textContent = "#root introuvable — index.html corrompu ?";
} else {
  createRoot(rootElement).render(
    <StrictMode>
      <ErrorBoundary>
        <App />
      </ErrorBoundary>
    </StrictMode>,
  );
}
