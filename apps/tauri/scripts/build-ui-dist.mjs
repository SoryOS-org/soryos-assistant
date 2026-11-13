#!/usr/bin/env node
/**
 * Builds the aggregate frontend dist consumed by Tauri
 * (`src-tauri/ui-dist`), so BOTH UIs ship from a single `frontendDist`:
 *
 *   ui-dist/
 *   ├── index.html, main.js, modules/ …   (v1 webview = sprite window)
 *   └── desktop/                          (v2 panel UI, the only panel)
 *
 * The sprite "main" window keeps loading the v1 root (index.html) and the
 * panel window loads `desktop/index.html` (the v1 panel was removed).
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
