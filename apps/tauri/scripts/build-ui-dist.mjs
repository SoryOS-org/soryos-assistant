#!/usr/bin/env node
/**
 * Builds the aggregate frontend dist consumed by Tauri
 * (`src-tauri/ui-dist`), so BOTH UIs ship from a single `frontendDist`:
 *
 *   ui-dist/
 *   ├── index.html, main.js, modules/, panel/ …   (v1 webview, copied)
 *   └── desktop/                                  (v2 build output)
 *
 * The sprite "main" window keeps loading the v1 root (index.html) and the
 * v1 panel keeps loading `panel/index.html`; the panel window opts into the
 * v2 UI with `SPRITE_PANEL_V2=1` (see `src/panel/mod.rs`). Nothing here
 * modifies v1 sources — only copies.
 */
import { cpSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const appsTauri = resolve(here, "..");

const webviewDir = join(appsTauri, "webview");
const desktopDist = join(appsTauri, "desktop", "dist");
const outDir = join(appsTauri, "src-tauri", "ui-dist");

if (!existsSync(webviewDir)) {
  console.error(`webview dir not found: ${webviewDir}`);
  process.exit(1);
}
if (!existsSync(join(desktopDist, "index.html"))) {
  console.error(`desktop build missing — run \`pnpm build\` in apps/tauri/desktop first`);
  process.exit(1);
}

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

// v1 webview at the dist root (sprite window + v1 panel entry).
cpSync(webviewDir, outDir, { recursive: true });

// v2 desktop UI under `desktop/`.
cpSync(desktopDist, join(outDir, "desktop"), { recursive: true });

console.log(`ui-dist generated at ${outDir}`);
