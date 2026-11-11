import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@/app/App";
import { ErrorBoundary } from "@/ui/ErrorBoundary";
import "@/styles/globals.css";

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
