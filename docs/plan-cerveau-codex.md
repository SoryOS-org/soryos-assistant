# Plan de migration : cerveau unique Codex

> **Objectif** : remplacer le cerveau distant (worker Cloudflare) par le cerveau local
> **Codex app-server** (vendu dans `crates/codex-main/`), **sans perdre aucune
> fonctionnalité** de l'ancien cerveau. Un seul système, plus de 401, plus de
> dépendance à un serveur tiers.

## État d'avancement (mise à jour)

- [x] **Phase 1 — Abstraction `Brain`** : `src/brain/mod.rs` (dispatcher
  `run_chat_flow`), `src/config/brain.rs` (`brain.provider: worker|codex` dans
  `stt.config.json`), branché sur les deux points d'appel
  (`commands/assistant.rs`, `stt/service.rs`). Défaut = `worker` (comportement
  inchangé).
- [x] **Phase 2/3 — `CodexBrain`** : `src/brain/codex.rs` — spawn
  `codex app-server --listen stdio://`, `initialize` → `thread/start`
  (dynamicTools = registre MCP namespacé + `open_app`), `turn/start` avec
  collecte des deltas, pont `item/tool/call` → Router/IntentState + events
  `assistant://mcp-progress` identiques.
- [ ] **Phase 0 — Compilation du binaire** : `cd
  crates/codex-main/codex-main/codex-rs && cargo build -p codex-cli -j 2`
  (plusieurs heures), puis copier vers
  `apps/tauri/src-tauri/resources/codex/codex`.
- [ ] **Phase 4 — Bascule** : mettre `"brain": {"provider": "codex"}` dans
  `.dev/config/stt.config.json` et rejouer la checklist de non-régression.
  Script de test standalone prêt : `/tmp/opencode/test-appserver.py`.


## 1. État des lieux — tout ce que fait l'ancien cerveau (worker Cloudflare)

Chaque contrat HTTP de `apps/worker` consommé par l'app (`src-tauri`) :

| # | Endpoint | Appelant | Rôle | Type attendu |
|---|----------|----------|------|--------------|
| 1 | `POST /chat` | `machine/worker_dispatch.rs` | Répondre à l'utilisateur + intentions | `WorkerReply { reply, summary, planner_context, intents, reply_speak }` |
| 2 | `POST /planner` | `machine/planner_flow/engine.rs` (boucle ×20) | Planifier des actions MCP (outils) | `PlannerResponse { mcp_plan: { actions: [McpAction] } }` |
| 3 | `POST /sync_machine_state` | `config/machine_state.rs` | Sync apps installées + schémas MCP | `Value` (ignoré) |
| 4 | `POST /config` | `config/machine_state.rs` | Sync préférences | `Value` (ignoré) |
| 5 | `GET/DELETE /memory` | `config/machine_state.rs` | Mémoire de conversation + reset | `ConversationMemory` |
| 6 | Auth Bearer | `worker_client.rs` | Token (401 → `worker_auth_failed`) | — |

**Fonctionnalités implicites à préserver** : émission `assistant://worker-auth-ok/failed`
(UI), `SpeechPlan` (parole), suivi `planner_context`, événements `assistant://mcp-progress`
(Queued→Running→Completed/Crashed), chunking des longs résultats (`sprite_internal`),
ouverture d'apps (`IntentState::process_intents`), hydratation du chat v2 depuis `/memory`.

## 2. Architecture cible (un seul système)

```
Voix Whisper / Composer v2
        │
        ▼
┌──────────────────────────────┐
│  CodexBrain (Rust, local)    │   ← remplace worker_client + planner engine
│  codex app-server (stdio)    │
│  threads persistés           │
└──────────┬───────────────────┘
           │ thread/start(dynamic_tools) + turn/start
           │ ← notifications AgentMessageDelta (parole fluide possible)
           │ ← requêtes DynamicToolCall
           ▼
┌──────────────────────────────┐
│  mcp-host (Router) — INCHANGÉ│   ← exécuteur : outils MCP + open_app
│  + IntentState (open_app)    │      + sprite_internal (chunking)
└──────────────────────────────┘
           │
           ▼
  UI v2 : mêmes commandes Tauri, mêmes événements (contract gelé)
```

**Pont clé vérifié dans le code** : `ThreadStartParams.dynamic_tools`
(`app-server-protocol/src/protocol/v2/thread.rs:146`, expérimental
`thread/start.dynamicTools`). Quand le cerveau veut appeler un outil hôte, il
envoie `DynamicToolCallParams` au client, qui répond `DynamicToolCallResponse`.

## 3. Tableau de correspondance — zéro perte de fonctionnalité

| Fonctionnalité ancienne | Remplacement Codex | Statut |
|---|---|---|
| `reply` (texte final) | Message agent final du turn | ✅ direct |
| `reply_speak` + `SpeechPlan` | Streaming `AgentMessageDelta` → TTS progressif (bonus : on peut parler pendant la génération) | ✅ mieux |
| `intents: open_app` | **Dynamic tool** `open_app` exécutée par `IntentState` | ✅ direct |
| `intents: open_app_not_found` | Résultat d'erreur de la tool `open_app` | ✅ direct |
| `intents: mcp_plan` + boucle `/planner` ×20 | **Supprimée** : boucle agent native de Codex (il décide seul, itère, replanifie) | ✅ mieux |
| Exécution MCP + `mcp-progress` | Dynamic tools par serveur (namespacées) → `Router::call_tool` + events | ✅ direct |
| Chunking (`sprite_internal`) | Dynamic tools hôtes (inchangées) | ✅ direct |
| `/memory` (turns/summary) | Threads Codex persistés `~/.codex/sessions/*.jsonl` + pont sur la commande `get_conversation_memory` | ✅ direct |
| Reset session (`DELETE /memory`) | Nouveau thread (thread/start) + archivage | ✅ direct |
| `/sync_machine_state` (allowlist apps, schémas MCP) | Contexte injecté au `thread/start` (instructions + specs des dynamic tools issues du Registry) | ✅ direct |
| `/config` (préférences) | Déjà local (`PreferencesConfig`) — le sync cloud devient no-op | ✅ supprimé |
| Personnalité (`AI_GATEWAY_SYSTEM_PROMPT/PERSONALITY`) | `instructions` du `thread/start` (ou AGENTS.md) | ✅ direct |
| Modèle (`AI_GATEWAY_MODEL`) | `~/.codex/config.toml` `[model_providers]` + `model` | ✅ direct |
| Auth token / 401 | **Supprimé** (local) — UI : événements auth émis en `ok` constant | ✅ supprimé |
| Machine ID | Conservé (utile pour nommer les threads/sessions) | ✅ conservé |

## 4. Stratégie : abstraction `Brain` + migration incrémentale

Principe : **l'ancien cerveau reste disponible derrière un flag** pendant toute
la migration (repli instantané en cas de problème).

### Phase 0 — Préparer Codex (aucun changement app)
1. Compiler le binaire : `cd crates/codex-main/codex-main/codex-rs && cargo build -p codex-cli`
   (⚠️ machine 3,3 Go RAM : utiliser `-j 2`, compter plusieurs heures avec swap).
2. Placer le binaire dans `apps/tauri/src-tauri/resources/codex/codex`.
3. `~/.codex/config.toml` : provider (`openai` + clé, ou `ollama` local) + sandbox
   + approbations adaptées au mode vocal.
4. Test standalone : script Python/Node → `codex app-server --listen stdio://`
   → `initialize` → `thread/start` → `turn/start` → vérifier deltas + dynamic tool.

### Phase 1 — Abstraction `Brain` (refactor neutre)
- Nouveau module `src-tauri/src/brain/` :
  - `trait Brain` : `async fn chat(req: BrainRequest) -> Result<WorkerReply, String>`
    (le type `WorkerReply` est conservé tel quel = contrat stable).
  - `WorkerBrain` : implémentation actuelle (code existant déplacé tel quel).
  - `CodexBrain` : implémentation Codex (Phase 2).
- Flag de config : `stt.config.json → "brain": { "provider": "worker" | "codex" }`
  (défaut : `worker` → comportement inchangé).
- Tous les appelants (`run_worker_flow`, `stt/service.rs`) passent par le trait.
- `machine_state` (`/sync`, `/config`, `/memory`) : encapsulé dans `BrainStateBridge`
  avec deux backends (cloud | local).

### Phase 2 — `CodexBrain` : équivalent `/chat`
- Spawn `codex app-server --listen stdio://` (ou unix socket) au démarrage de l'app.
- `initialize` → `thread/start` (model, cwd, sandbox, approvals, `dynamic_tools`)
  → garder l'id de thread côté service.
- `turn/start` par requête utilisateur ; agréger les deltas :
  - message final → `WorkerReply.reply`
  - deltas → `SpeechPlan.updates` (parole progressive)
  - `planner_context`/`summary` : récupérés des items du turn (bookkeeping Codex natif).
- Session token : conservé (UUID) pour les logs/metrics existants.
- **Aucun changement UI** : `AssistantCommandResponse` et tous les événements restent identiques.

### Phase 3 — Pont MCP : nos outils deviennent ceux de Codex
- Au boot (et sur `SchemasUpdated`), construire les `DynamicToolSpec` depuis
  `Registry::snapshot()` (namespacées `mcp__<server>__<tool>`), + `open_app`,
  + `sprite_internal` (chunk_fetch, text_find).
- Handler `DynamicToolCallParams` :
  - MCP → `Router::call_tool(...)` avec `ToolCallContext` (trace/session/step),
    émettre `assistant://mcp-progress` (Queued/Running/Completed/Crashed) comme aujourd'hui.
  - `open_app` → `IntentState::process_intents([LaunchApplication])`.
- Approbations : policy par défaut `Never` + sandbox `WorkspaceWrite`
  (UX vocale) — réglable dans la config.

### Phase 4 — Retrait de la boucle planner (cerveau Codex actif)
- Quand `brain = codex` : sauter `run_planner_flow` (Codex itère tout seul).
- `McpPlan`/`McpAction` restent compilés pour compatibilité (fallback `worker`).
- Le flag bascule : `brain.provider = "codex"`.

### Phase 5 — Mémoire & sessions
- `get_conversation_memory` : lire les items du thread Codex courant
  (`thread_data`/rollout) et mapper vers `ConversationMemory` (contrat UI inchangé).
- `reset_worker_session` : archiver le thread + en ouvrir un neuf.
- Bonus (v2 UI) : liste des sessions Codex pour reprendre une conversation.

### Phase 6 — Nettoyage & non-régression
- Supprimer (ou geler) les appels HTTP worker quand `codex` est stable.
- Mettre à jour `ARCHITECTURE.md`.
- **Checklist de non-régression** (à rejouer sur les 2 providers) :
  chat texte, voix E2E (test loopback déjà validé), open_app,
  outil MCP réel + progress, reset session, hydratation mémoire,
  préférences, logs, À propos, chemins d'erreur (provider coupé).

## 5. Risques & mitigations

| Risque | Mitigation |
|---|---|
| Compilation Codex lourde (151 crates, 2,3M lignes) sur 3,3 Go RAM | Build `-j 2` nocturne ; binaire compilé une fois puis embarqué dans `resources/` |
| `dynamic_tools` = API expérimentale | Version Codex **pinée** (vendored) ; couche `CodexBrain` isolée |
| Provider LLM : seul `wire_api = "responses"` existe | Provider OpenAI natif ; `ollama`/`lmstudio` built-in (à vérifier selon version) ; la clé API vit dans `~/.codex/auth` |
| RAM en exécution (modèle local ollama + Whisper + app) | 3,3 Go : préférer provider cloud OpenAI/OpenRouter-compatible, ou modèle ≤1B |
| Approvals bloquants en mode vocal | Policy `Never` par défaut en mode voix, ask-to-confirm en mode texte |
| Licence | Codex = Apache 2.0 → vendoring OK, mentions à conserver (NOTICE) |

## 6. Décisions à confirmer avant de démarrer

1. **Provider du modèle** : clé API OpenAI (payant, simple) ou OpenRouter-compatible
   (modèle `:free`, possible) ou 100% local ollama (0 €, RAM serrée) ?
2. **Transport** : `stdio` (recommandé, binaire embarqué) ou in-process ?
3. **Calendrier** : démarrer Phase 0 (compilation nocturne) quand ?

---
*Références code : `machine/worker_dispatch.rs`, `machine/worker_client.rs`,
`machine/planner_flow/engine.rs`, `config/machine_state.rs`, `crates/mcp-host/src/*`,
`crates/codex-main/codex-main/codex-rs/app-server-protocol/src/protocol/v2/{thread,item}.rs`.*
