import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

// Sprite Assistant v2 — Vite config
// Served by Tauri (frontendDist -> desktop/dist) once wired in tauri.conf.json.
const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },

  // Relative base: the production build is served by Tauri from the
  // `desktop/` subdirectory of the aggregate frontend dist (ui-dist), while
  // `tauri dev` / browser preview serve it from the root.
  base: "./",

  // Vite options tailored for Tauri development.
  //
  // 1. prevent Vite from obscuring Rust panics:
  clearScreen: false,
  // 2. Tauri expects a fixed port; fail if that port is not available:
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`:
      ignored: ["**/src-tauri/**"],
    },
  },

  // Expose `VITE_*` and `TAURI_*` env vars to the frontend.
  envPrefix: ["VITE_", "TAURI_"],

  build: {
    // Tauri's WebView2 (Windows) target — matches upstream config guidance.
    target: "chrome105",
    // Produce a smaller bundle for desktop packaging.
    minify: "esbuild",
    sourcemap: false,
  },
});
