# Sprite Desktop UI (v2)

Next-generation frontend for the Sprite Assistant desktop app, replacing the legacy
vanilla-JS webview (`apps/tauri/webview/`) once feature-complete.

- **Stack**: React 19 + TypeScript (strict) + Vite + Tailwind CSS 4 + Zustand
- **Shell**: Tauri 2 (unchanged Rust backend in `../src-tauri/`)
- **Status**: scaffold / Phase 1 — not wired into the Tauri window yet

> The legacy UI in `../webview/` is intentionally left untouched until this v2
> reaches parity and is wired into `tauri.conf.json` (`frontendDist`).

## Directory layout

```
desktop/
├── index.html                  # Vite entry document
├── package.json                # standalone package (NOT in the pnpm workspace yet)
├── vite.config.ts              # Vite + React + Tailwind plugins, "@" alias
├── tsconfig.json               # strict TypeScript config
└── src/
    ├── main.tsx                # React root
    │
    ├── app/                    # Application layer (composition root)
    │   ├── App.tsx             # root component
    │   └── layout/             # AppShell: window chrome, view switching
    │
    ├── features/               # business domains — one folder per feature
    │   ├── chat/               # conversation view + composer + message list
    │   ├── sidebar/            # threads / projects navigation
    │   ├── terminal/           # embedded terminal panel (xterm.js, Phase 2)
    │   ├── files/              # file explorer panel (Phase 2)
    │   ├── browser/            # web browser panel (Phase 2)
    │   ├── settings/           # preferences / config / secrets
    │   └── voice/              # voice capture overlay (STT/TTS bridging)
    │
    ├── ui/                     # dumb, reusable UI primitives (Button, Modal, …)
    ├── stores/                 # Zustand global stores (ui, chat)
    ├── hooks/                  # shared React hooks
    ├── lib/                    # integrations: Tauri bridge, events, utils
    ├── types/                  # shared TypeScript contracts
    └── styles/                 # Tailwind entry + theme tokens
```

### Layering rules (keep this coherent)

1. `ui/` never imports from `features/` or `stores/` — primitives only.
2. `features/` may import from `ui/`, `stores/`, `lib/`, `types/`.
3. `app/` composes features and owns global layout — nothing imports `app/`
   except `main.tsx`.
4. All backend access goes through `lib/` (Tauri invoke/events wrappers) —
   features never call `window.__TAURI__` directly.
5. Cross-feature state communication happens via `stores/`; shared component
   reuse across features goes through the feature's public `index.ts` only
   (e.g. chat imports `@/features/mcp`, never its internals).

## Development

```bash
# from this folder (standalone for now, not part of the root workspace)
pnpm install
pnpm dev         # browser preview at http://localhost:1420 (no Tauri)
pnpm build       # type-check + production bundle in dist/
pnpm typecheck   # tsc only
```

## Wiring into Tauri (later, once v2 is ready)

1. Point `tauri.conf.json` `frontendDist` to `../desktop/dist`.
2. Add `"apps/tauri/desktop"` to `pnpm-workspace.yaml` and drop the local
   lockfile if the root workspace should own it.
3. Keep `webview/` as the sprite window target until the floating sprite is
   migrated.

## Roadmap

- Phase 1: shell + chat UI against the current worker bridge (manual send path)
- Phase 2: Codex app-server threads/turns/items + approvals, terminal, files
- Phase 3: voice overlay (wake word / PTT) on the new shell
- Phase 4: retire `webview/`
