# /v1/responses Migration Plan

## Goals

1. Move both the chat UX model and the MCP planner model off `v1/chat/completions` and onto the stateful `v1/responses` API.
2. Send the full system prompt, MCP catalog, and allowlist **only once per model** and rely on `previous_response_id` for the remainder of the turn to shrink tokens and avoid prompt drift.
3. Keep the existing orchestration split intact: desktop executes MCP tools, the worker handles UX + planner prompting, and MCP telemetry continues to emit `assistant://mcp-status` / `assistant://mcp-progress` events.
4. Preserve SRP: provider plumbing handles HTTP payloads, the worker orchestrates prompts/state, durable objects persist long-lived identifiers, and the desktop simply calls `/chat` / `/planner`.

## Current Implementation Snapshot

- `apps/worker/src/gateway.ts` already constructs `/v1/responses` payloads with strict `response_format` schemas. It still inlines the entire system prompt, conversation memory, MCP catalog string, and planner history on the **initial** request because continuation turns only send deltas alongside `previous_response_id`.
- Providers under `apps/worker/src/providers` normalize legacy `/chat/completions` URLs to `/responses`; both OpenRouter and Cloudflare compat clients now POST `/responses` payloads and surface the returned `response.id` for reuse.
- `MachineStateDO` (`apps/worker/src/machine-state-do.ts`) now persists UX and planner response handles per machine/session so the worker can reuse `previous_response_id` and reset stale ones deterministically.
- Planner orchestration (desktop `apps/tauri/src-tauri/src/machine.rs`) still assumes the worker owns the planning loop; desktop calls `/planner` with history snapshots while the worker manages long-lived response threads internally.

## Target Interaction Model

The worker owns two long-lived `/v1/responses` threads per machine/session:

1. **UX model thread**: First `/responses` call includes the full system prompt, allowlist summary, MCP catalog text, and base instructions as `input_text`. Subsequent chat refinements reuse the returned `response_id` via `previous_response_id` and pass only new user utterances or MCP summaries.
2. **Planner model thread**: Initialized after the worker emits an `mcp_plan` intent. The first planner request bundles the planner-specific system prompt, tool schemas for the nominated server, the session token, and current MCP allowlist. Later planner turns (history updates, retries) reuse the stored planner `response_id` and feed a single `input_text` object summarizing the newest MCP result.

Both threads supply `response_format` schemas so the LLM emits strict JSON without our current prompt gymnastics.

## Architecture Changes

### 1. Provider / HTTP Layer

- Introduce a `ResponsesClient` abstraction alongside (or replacing) the current `GatewayProviderClient`.
  - Methods: `create_response(initialPayload)` and `continue_response({ previousResponseId, input })`.
  - Surface model-agnostic error codes + raw bodies for telemetry.
- Update the Cloudflare compat and OpenRouter providers to hit `/v1/responses`, pass `response_format`, and read `response.id`, `output_text`, and tool calls (if auto-run later).
- Preserve BYOK rules (`AGENTS.md`): tokens stay in the worker config module; no new env vars at runtime.
- Support a dedicated planner model via `AI_GATEWAY_PLANNER_MODEL`; fall back to `AI_GATEWAY_MODEL` when unspecified so deployments can tune UX vs planner capacity independently. Planner requests also pin `reasoning.effort` to `minimal` on GPT-5/O-series models so they stay responsive and within token budgets.

### 2. Worker Prompt Builder (`apps/worker/src/gateway.ts`)

- Split today’s `buildMessages()` into two flows:
  - **Initial turn**: compile system prompt, allowlist summary, MCP catalog (`buildMcpToolsPrompt()`), and planner instructions into an `input` array that matches the `/responses` contract. Attach the corresponding JSON schema as `response_format`.
  - **Continuation**: skip all of the above and submit only `{ "role": "user", "content": [{ "type": "input_text", "text": "<history delta>" }] }` plus the `previous_response_id`.
- Define and reuse strict JSON schemas for:
- `ux_reply`: `{ reply: string; planner_context?: string; intents: Intent[] }`
- `planner_plan`: `{ mcp_plan: { session_token: string; reply?: string; actions: Action[] } }`
- Continue to pass tool/result history, but only when it changes; store a hash to avoid re-sending identical history blobs.

### 3. Durable Object State (`apps/worker/src/machine-state-do.ts`)

- Extend the stored machine state with a `responses` map keyed by `{ machine_id, model_role, session_token }` that includes:
  - `ux_response_id`
  - `planner_response_id`
  - Model metadata (name, timestamp) so we can reset when configs change.
- Add helper RPCs to read/write these IDs so `/chat` and `/planner` handlers can fetch/store them before calling the provider.
- On config/model changes (e.g., `AI_GATEWAY_MODEL` updates), invalidate cached response IDs to force a new initial prompt.

### 4. Worker Orchestration (`apps/worker/src/index.ts`)

- `/chat`: before calling `requestGatewayReply`, fetch the cached UX `response_id`. Choose between `create` vs `continue` requests based on whether one exists. When the provider returns a `response_id`, store it along with the current personality/model fingerprint.
- `/planner`: derive a unique key per `session_token`. Use the cached planner `response_id` to send follow-up history summaries; reset when the planner declares completion or when the desktop sends `/mcp_cancel`.
- Ensure planner history deltas re-enter the UX model as needed (e.g., final summary) by posting another continuation turn that references the same `response_id`.
- Schedule periodic UX updates while the planner loop runs (e.g., every N tool actions or every ~30 seconds). These updates carry the planner’s `reply` narration through the chat thread so the sprite UI has text + speech feedback beyond the current “listening” animation, enabling future richer state machines.

**Continuation payloads must only include deltas.** Once a `response_id` exists, the worker should stop re-sending:

- Full system prompt or MCP catalog text
- Allowlist summaries or conversation history except for the latest update
- Original user query / planner instruction (unless they changed)

Each follow-up call should consist of the new MCP result or UX narration only. If the provider rejects the cached `response_id`, the worker starts a new thread which replays the full prompt exactly once.
To prevent cached threads from ballooning, the worker now tracks token usage per conversation. Once a UX thread consumes roughly 9k tokens (planner threads ~12k), the next response must include a `summary` field so the desktop can reset the session; after the summary is delivered the cached `response_id` is cleared and a fresh thread begins.
 To enforce this we treat response threads with no recorded `last_synced_turn_ts` as having zero deltas—continuations skip conversation history until at least one turn has been synced.

**Planner narration handoff**

1. Every planner `reply` must be persisted into `MachineStateDO` as an assistant `conversation_turn` so the next `/chat` call (or transcript viewer) can see that narration.
2. We do **not** call the chat `/responses` thread immediately after each planner update. Instead, the worker tracks the last synced conversation index, and when the user initiates the next `/chat` request it gathers only the new turns since that index and submits them alongside the fresh user query. If we reuse the cached `response_id`, only the delta is sent; if the fingerprint forces a new thread, the full prompt (latest transcript, allowlist, MCP catalog, etc.) is replayed once.
3. This keeps the UX thread consistent without firing extra LLM calls mid-plan, and it guarantees that follow-up chat turns have access to the planner’s final status without replaying the entire prompt.

> **Current UX limitation:** the desktop UI renders the same string that the TTS engine speaks. We now treat `reply` as the canonical transcript text for future improvements, so planner narration and spoken output always match.

> **Input payload requirements:** On continuation turns we now prepend cached assistant/user deltas back into the `/responses` call. Each delta must use the content type originally returned by the model (e.g., `output_text`). If a stored turn predates this metadata, default assistant entries to `output_text` before sending them. Newly cached turns should always record the provider’s `content[].type` so the worker can echo it faithfully when replaying history.

### 5. Desktop + Machine Bridge

- Desktop continues to call `/chat` and `/planner` the same way, but we should:
  - Add a `model_session_generation` field to the desktop payload so the worker can detect when local cache invalidation is required (e.g., after schema resync).
  - Confirm MCP schema sync (emitted via `get_mcp_registry`) still happens before the worker’s initial `/responses` call so the first prompt is accurate.
- Update docs/UI to note that MCP catalog/system prompt updates require resetting `/responses` threads (provide a kill switch command or endpoint).

### 6. Telemetry & Error Handling

- Log the `response_id`, `previous_response_id`, and `session_token` triplet at debug level only, ensuring we redact user text per `AGENTS.md`.
- Surface metrics around “initial response vs continuation” latencies to confirm the new flow actually shrinks token usage.
- Handle stale IDs by catching 409/404 responses from the provider, clearing the cached ID, and retrying once with a fresh initial payload.

## Work Breakdown

1. **Schema groundwork**
   - Define TypeScript interfaces + zod schemas for the UX/planner response formats in `@sprite/shared`.
   - Update worker tests (`apps/worker/test/gateway.test.ts`) to validate the new JSON shapes.
2. **Provider layer**
   - Implement `ResponsesClient` for both OpenRouter and Cloudflare compat paths, including strict error serialization.
   - Gate the new code behind a feature flag for gradual rollout.
3. **Worker orchestration**
   - Refactor `requestGatewayReply` to accept a `conversationHandle` (cached response ID + metadata) and return updated handles.
   - Thread the handle through `/chat` and `/planner` routes, ensuring retries clear stale IDs.
4. **Durable object persistence**
   - Extend `MachineStateDO` storage schema + migrations to keep response IDs per machine/session.
   - Add helper endpoints so other services (e.g., desktop cleanup) can clear cached threads if needed.
5. **Desktop coordination**
   - Document how/when the desktop should tell the worker that tool catalogs changed (e.g., piggyback on the existing `/sync` call).
   - Verify MCP schema sync still happens before the worker seeds the planner prompt.
6. **Testing & rollout**
   - Add integration tests that simulate turn 1 + 2+ for both `/chat` and `/planner`, including failure retries.
   - Capture latency metrics before/after to confirm the migration benefit.
   - Once stable, delete the legacy `chat.completions` provider code paths.

## Open Questions

- How do we scope cached `response_id`s when multiple personalities/models are in play? Proposed answer: include `{ model, personality, prompt_version }` in the cache key so a config change forces a new initial turn.
- Do we need to expose `/responses` streaming (`response.output[0].content`) for planner multi-step updates, or will plain blocking calls suffice? Start with blocking responses; streaming can be layered later once the base migration settles.
- Should the desktop receive the raw `response_id` for observability? Probably no—treat them as worker-internal and only surface via debug logs to minimize UI coupling.

---

Deliverables for this plan: updated provider client, worker orchestration that understands `responses`, DO persistence for conversation handles, doc updates, and an incremental rollout strategy with telemetry hooks.
