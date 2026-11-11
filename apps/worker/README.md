# Edge Worker

Cloudflare Worker responsible for mediating chat requests between the desktop
assistant and the configured Large Language Model (LLM) endpoint. It also
exposes a planner endpoint for MCP tool orchestration.

- Receives `/chat` POST requests from the Tauri bridge with the user's utterance.
- All worker endpoints (except `/health`) require `Authorization: Bearer <token>` using `WORKER_AUTH_TOKEN`.
- Manages per-machine state via Durable Objects (conversation memory, allowlist, preferences).
- Calls the Cloudflare AI Gateway (OpenRouter by default, or the Cloudflare compat endpoint) with a curated system prompt that includes conversation history and allowlist context.
- Validates the model response against the shared `Reply` schema.
- Falls back to deterministic intent parsing when the LLM is unavailable.

## High-Level Flow

```
Desktop (Tauri) -> /chat -> Worker -> Cloudflare AI Gateway -> LLM
                   |                      |
                   |                      -> fallback reply (schema-compliant)
                   -> Durable Object (MachineStateDO)
                      - Sync user turn
                      - Fetch conversation memory
                      - Fetch allowlist
                      - Fetch personality config
                      - Sync assistant turn
```

## Chat Request Flow

1. **Receives request** with `query` and optional `machine_id`
2. **Queues user turn** for Durable Object sync (batched with assistant turn after a successful LLM reply)
3. **Fetches conversation memory** from DO (last 25 turns)
4. **Fetches allowlist** from DO
5. **Fetches personality** from DO config
6. **Builds LLM prompt** with:
   - System prompt (with personality)
   - Conversation history
   - Allowlist context (available apps)
   - Current user query
7. **Calls LLM Gateway**
8. **Syncs assistant turn** to DO after successful response
9. **Returns response** to desktop (adds `X-Worker-Conversation-Reset` when a summary/reset is required)

## Planner Request Flow

1. **Receives request** at `/planner` with `query`, optional `machine_id`, `session_token`, `instruction`, `mcp_server`, `history`
2. **Loads machine context** (conversation summary, MCP catalog, personality, response handles)
3. **Builds planner prompt** (planner template + history summary + optional instruction/server hint)
4. **Calls LLM Gateway** with planner schema
5. **Appends planner reply** to conversation history in the DO
6. **Persists planner response handle** (per `session_token`) and emits `X-Worker-Planner-Reset` when a summary/reset is required
7. **Returns planner JSON** to desktop

## Machine State Endpoints

- `POST /sync_machine_state` - Syncs machine state updates (apps, turn, config)
- `config_patch.mcp` carries MCP server/tool snapshots pushed by the desktop host (`sync_mcp_snapshot`).
- `GET /allowlist?machine_id=...` - Returns allowlist for machine
- `GET /config?machine_id=...` - Returns config for machine
- `POST /config` - Updates config for machine
- `GET /memory?machine_id=...` - Returns conversation memory
- `DELETE /memory?machine_id=...` - Clears conversation memory and response handles
- `GET /health` - Simple health probe

See the architecture overview in the root README for a high-level flow.

## Environment & Configuration

The worker pulls configuration from environment variables or Wrangler secrets.

| Variable | Required | Description |
| -------- | -------- | ----------- |
| `AI_GATEWAY_URL` | Yes | Endpoint for the AI Gateway `/responses` API (the worker will map legacy `/chat/completions` URLs to `/responses`). When targeting the compat endpoint you can continue pointing at the `/compat` base; the worker appends `/responses` automatically. |
| `AI_GATEWAY_TOKEN` | Yes | Cloudflare AI Gateway token passed via `cf-aig-authorization`. |
| `WORKER_AUTH_TOKEN` | Yes | Shared bearer token required on all worker API calls from the desktop app. |
| `AI_GATEWAY_PROVIDER` | No | Selects the downstream integration; accepts `openrouter` (default) or `cloudflare_compat`. |
| `AI_GATEWAY_PROVIDER_API_KEY` | When compat | Provider API key supplied to the OpenAI SDK when `AI_GATEWAY_PROVIDER=cloudflare_compat`. |
| `AI_GATEWAY_MODEL` | No | Model identifier for the UX/chat thread (defaults to `mistralai/mistral-small-3.2-24b-instruct:free`). |
| `AI_GATEWAY_PLANNER_MODEL` | No | Optional planner-specific model. Falls back to `AI_GATEWAY_MODEL` when unset so you can tune UX vs planner capacity independently. |
| `AI_GATEWAY_SYSTEM_PROMPT` | No | Custom system prompt template. The token `{{PERSONALITY}}` is replaced at runtime. |
| `AI_GATEWAY_PERSONALITY` | No | Personality string injected into the system prompt (defaults to `Professional and concise`). |
| `AI_GATEWAY_PLANNER_PROMPT` | No | Custom planner prompt template (same `{{PERSONALITY}}` substitution). |

When targeting the compat endpoint, configure `AI_GATEWAY_URL` with the base URL
ending in `/compat` (or the explicit `/responses` path) and supply the provider
API key Cloudflare should forward via `AI_GATEWAY_PROVIDER_API_KEY`. The worker
will normalize the URL so the downstream call always hits `/responses`.

All responses returned to the desktop must satisfy the `ReplySchema` described
in `packages/shared`. If you extend the schema (e.g., add new intents), also
update the default system prompt so the LLM emits the new shape.

### Local `.env`

For local development you can drop an `.env` (or `.env.local`) file alongside
`wrangler.toml` holding the variables above:

```
cp apps/worker/.env.example apps/worker/.env
# edit the values in .env with your credentials
```

Wrangler automatically loads `.env`/`.env.local` when running `pnpm dev:worker`.
The file is git-ignored; make sure not to commit real credentials.

For production deployments, continue using `pnpm exec wrangler secret put ...`
(or a globally installed `wrangler`) so Cloudflare stores the values securely
in your Worker environment.

### System Prompt Template

`DEFAULT_SYSTEM_PROMPT` (chat/UX) enforces:
- JSON-only responses (no fences/markdown) with required `reply` and `intents`
- Allowed intents: `converse`, `open_app`, `open_app_not_found`, `mcp_plan`
- Concise replies (2 sentences / ~180 characters) without emoji/decorative characters
- Optional `summary` and `planner_context` fields when requested by the worker
- Allowlist enforcement (use `open_app_not_found` for non-listed apps)

Override `AI_GATEWAY_SYSTEM_PROMPT` for custom behavior; the worker performs
runtime substitution for `{{PERSONALITY}}`.

### Planner Prompt Template

`DEFAULT_PLANNER_PROMPT` enforces the MCP planning JSON shape (`mcp_plan` root,
single action per response, optional `reply`/`summary` fields plus required `reply_speak`) and
includes catalog guidance and safety constraints. Override with
`AI_GATEWAY_PLANNER_PROMPT`.

If you prefer to keep prompt text private in a public repo, override the defaults via
environment variables/secrets rather than in-repo changes.

## Local Development

1. Install dependencies from the repository root:
   ```bash
   pnpm install
   ```
2. Start the worker in development mode:
   ```bash
   pnpm dev:worker
   ```
   Wrangler serves the worker at `http://127.0.0.1:8787` by default. The Tauri
   app uses this URL unless overridden via `stt.config.json` (`worker.url`).
   
   **Durable Objects run locally** - Wrangler includes a local DO emulator that runs
   in-memory. All DO functionality (state storage, conversation memory, allowlist, etc.)
   works exactly as it does in production, but data is stored locally and cleared when
   you stop the dev server.
3. Optionally run type checking:
   ```bash
   pnpm --filter worker typecheck
   ```

### Testing the Endpoint

```bash
curl -X POST http://127.0.0.1:8787/chat \
  -H "Authorization: Bearer $WORKER_AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query":"Open Slack"}'
```

Depending on your gateway configuration, the worker will either return the LLM
response or the fallback reply (with header `X-Worker-Fallback` indicating the
reason).

## Deployment

```bash
pnpm deploy:worker
```

Make sure the required environment variables or Wrangler secrets are defined in
your Cloudflare account before deploying.

### Durable Objects Setup

The worker uses a Durable Object (`MachineStateDO`) for per-machine state storage.
The binding is configured in `wrangler.toml`:

```toml
[[durable_objects.bindings]]
name = "MACHINE_STATE"
class_name = "MachineStateDO"
script_name = "sprite-boss"
```

**No manual Cloudflare configuration needed** - the DO is automatically created and
managed by Wrangler on first deployment. An initial migration is already defined in
`wrangler.toml` (`[[migrations]] tag = "v1"` with `new_sqlite_classes = ["MachineStateDO"]`)
and will run on first deploy.

If you need to migrate the DO schema later (e.g., change `MachineState` structure),
you'll need to add a migration section to `wrangler.toml`. See [Cloudflare Durable
Objects Migrations](https://developers.cloudflare.com/durable-objects/reference/migrations/)
for details.

## Conversation Memory & Handles

- Durable Object stores up to 100 turns; LLM requests are windowed to the most recent 25.
- Response handles are persisted per machine (`ux`) and per planner session token; the worker
  may request summaries/resets via `X-Worker-Conversation-Reset` / `X-Worker-Planner-Reset`.

## Integration Notes

- The desktop bridge posts `{"query": string}` and expects a JSON body that
  matches `ReplySchema` (including the optional `planner_context` summary for MCP hand-offs).
- Planner responses must match `McpPlanResult` and are stored alongside conversation history.
- The worker surfaces LLM errors via the `X-Worker-Fallback` header and still
  returns a valid fallback payload, ensuring the desktop UI remains stable.
- Shared schema updates must be published from `packages/shared` prior to
  bumping the worker or desktop packages.
- LLM replies wrapped in Markdown fences (e.g., ```json ... ```) are unwrapped
  automatically before schema validation to tolerate chatty model behavior.
- Optional: when using Cloudflare Observability, align `wrangler.toml` with the
  Cloudflare dashboard by configuring `[observability]` / `[observability.logs]`
  (e.g., `enabled`, `head_sampling_rate`). Wrangler reads these values directly
  from the TOML file during `dev` and `deploy`.
