# Worker ↔ MCP Host Orchestration Roadmap

> **Status (2025-11-05):** Planner-centric orchestration is underway. The desktop now replays MCP history back to `/planner`, the legacy `/mcp_result` callback is being retired, and UI work remains to surface planner status cleanly.

## 1. Current Behaviour / Gaps

1. **User → Worker**: Desktop app forwards the user transcript and context to `/chat`. The worker replies with a single conversational string plus intents (including the new `mcp_plan` intent when MCP tooling is needed).
2. **Schemas**: Desktop syncs MCP tool metadata to the worker. The worker summarises this catalog and feeds it into the planner prompt.
3. **Planner loop**: The worker now owns the planning loop. The desktop executes tools but does not make independent tool decisions.
4. **Status separation**: Chat replies should live in the chat pane, while MCP progress text should appear in the MCP panel. This needs tighter guidance.
5. **History sharing**: The planner requires a concise transcript of executed actions/results (raw payloads + transport status) so it can avoid repeats and reason about retries.

## 2. Target Interaction Model

High-level sequence for one user request:

1. **User speaks / types** → Desktop streams audio/text to worker `/chat` along with latest MCP schema snapshot hash and conversation ID.
2. **Worker LLM planning**
   - Worker prompts the LLM with conversation history + MCP schema summary.
   - LLM responds with:
    - `planner_context`: short “session summary” that the planner can reuse instead of the raw chat history.
     - `intents[]`: includes conversation/app intents and may include an `mcp_plan` intent with `mcp_server`/`instruction` when MCP tooling is required.
3. **Desktop immediate feedback**
  - Play the chat response via TTS.
   - Generate a new `session_token` for the turn. If an `mcp_plan` intent is present, call `/planner` with the token to obtain the first set of MCP actions.
4. **Planner loop**
   - Desktop sends `/planner` with the user instruction, preferred server, session token, trimmed conversation context (last ~10 turns), MCP allowlist, and the current execution history.
   - Planner returns an `mcp_plan` containing:
     - `actions[]`: ordered tool invocations (each may include hints such as `speak` or `status_note`).
     - Optional `reply`: human-readable status for the MCP panel/log.
    - Optional `reply`: mid-plan narration that should also be voiced.
5. **Desktop executes plan incrementally**
   - For each action in order:
     1. Call MCP host `Router::call_tool` with correlation metadata.
     2. Capture the raw payload, transport outcome (`completed` vs `transport_error`), latency, and warnings; append the structured result to the planner history.
     3. Emit `assistant://mcp-progress` events for the panel (statuses: `queued`, `running`, `completed`, `crashed`).
   - If a step fails or more guidance is needed, the desktop calls `/planner` again with the updated history. The LLM is expected to inspect the history before retrying a failed action. No `/mcp_result` callback is required.
6. **Completion**
  - When the planner returns an empty `actions[]` set, the workflow is complete. Any concluding `reply` is delivered to the user and logged.
   - Desktop persists the turn transcript (chat + MCP panel) and the tool execution log for telemetry.

## 3. Interfaces & Contracts

| Component | API/Method | Purpose | Key Fields |
|-----------|------------|---------|------------|
| Worker | `/chat` (existing) | Entry point for user turns | `reply`, `intents[]`, echoes desktop-supplied `session_token` |
| Worker | `/planner` (new) | Produces next action batch and optional status narration | `actions[]`, optional `reply`, reuses `session_token` |
| Desktop → MCP host | `HostHandle::call_tool_async` | Executes MCP actions with correlation IDs | `server_id`, `tool_name`, `arguments`, `session_token` |
| Desktop → Worker | `/mcp_cancel` (optional) | Aborts an in-flight workflow | `session_token`, `reason` |
| Events (Desktop) | `assistant://mcp-progress` | Surface per-step status (`queued`, `running`, `completed`, `crashed`) and textual updates | Consumed by MCP panel |
| Logs | Tracing spans | Attach `session_token`, `step_index`, `tool_name` to logs across all layers |

### MCP handshake expectations

For every MCP server connection (stdio, HTTP, WS) the desktop must complete the MCP handshake before calling any other method:

1. Send an `initialize` request (declaring `protocolVersion`, capabilities, `clientInfo`).
2. Read the server's `initialize` result (capabilities/instructions).
3. Emit the `notifications/initialized` notification (no `id`, typically empty `params`) immediately after step 2.
4. Only after the notification succeeds may the client invoke RPCs such as `tools/list`, `resources/list`, or other methods.

Some servers (e.g., Windows MCP) enforce this strictly and will reject follow-up calls until the notification arrives, so the host must issue it automatically as part of the standard startup flow.

### Action Schema (worker → desktop)

```json
{
  "server_id": "browsermcp",
  "tool_name": "browser_navigate",
  "arguments": { "url": "https://example.com" },
  "speak": false,
  "status_note": "Opening example.com"
}
```

## 4. State & Memory Handling

- **Correlation IDs**: Desktop generates the `session_token` per user turn and includes it in every planner/tool execution. Worker responses must reuse the same token.
- **History**: The desktop keeps a chronological list of `{action, result}` pairs (raw payload, `completed`/`transport_error` status, latency) and shares a summarised form in each `/planner` call. The worker mirrors this to inform subsequent planning decisions and must use it before retrying a failing step.
- **Worker memory**: Worker stores final turn summaries (conversation + successful tool outcomes) for future turns.
- **Concurrency**: Multiple sessions may run simultaneously. Router clients are keyed by `session_token`, and planner history is isolated per token.
- **Timeouts**: Worker may specify per-step or global timeouts (e.g., via `status_note` or action metadata); desktop honours them and records the outcome for planner reconsideration.

## 5. Error Handling & Fallbacks

| Scenario | Desktop behaviour | Worker expectation | User feedback |
|----------|-------------------|--------------------|---------------|
| MCP server unavailable | Emit `assistant://mcp-status` failure, stop executing actions, and surface error to user | Worker expects the next `/planner` call to include the transport error in history | “I can’t reach the browser helper right now.” |
| Transport failure / timeout | Append `transport_error` to history, call `/planner` for revised steps | Worker decides to retry, branch, or ask for help after reviewing history | “Still working on it—I’ll try a different approach.” |
| Worker stops responding | Desktop sets watchdog; if `/planner` does not reply, surface error to user and stop executing actions | Worker should reply within SLA or the desktop cancels | “I’m having trouble coordinating the tools; let’s try again later.” |
| User cancels | Desktop calls `/mcp_cancel` and clears outstanding actions | Worker tears down planner state | “No problem, I’ve stopped the task.” |
| Non-deterministic tool output | Desktop provides the payload in planner history | Worker sanitises or asks the user for guidance | “The page content looks different—could you confirm what you expect?” |

## 6. Operational Considerations

- **Logging**: Use structured spans (`session_token`, `step_index`, `server_id`, `tool_name`) at info/warn. Capture the raw payload plus transport status for replay/debugging.
- **Telemetry**: Emit MCP execution metrics keyed by status (`completed` vs `transport_error`), latency, and retry counts to existing metrics pipeline.
- **UI/UX**: MCP panel shows action list with live status (queued/running/completed/crashed) and textual updates (planner `reply`, `status_note`). Provide cancel controls, surface history snippets when actions fail repeatedly, and expose per-server enable/disable toggles (persisted via the `enabled` flag in `mcp.config.json`) plus an "Edit Config" shortcut that opens the MCP config file directly.
- **TTS cadence**: Planner uses `reply` sparingly (e.g., initial acknowledgement, major branch, completion) to avoid chatter. Desktop throttles repeated narration updates.
- **Testing**: Add integration tests using dummy MCP binary + mock worker endpoints to simulate multi-step plans.
- **Rollout**: Feature flag the new loop (enable per environment) and retain legacy behaviour as fallback.

## 7. Next Steps

1. **Phase 2 design review** *(ongoing)*: Socialise API proposal (see `docs/mcp-worker-api.md`) with worker/backend teams and UI leads; confirm payloads, auth, and UX expectations.
2. **Planner state management** *(in progress)*: Implement shared history tracking (desktop + worker) so `/planner` can reason about retries without separate callbacks.
3. **Desktop orchestration loop** *(in progress)*: Update the MCP executor to re-post `/planner` after each step and surface planner replies correctly.
4. **UI/UX** *(in progress)*: Iterate on the MCP progress view (filtering, grouping, cancel controls) and wire feature flag toggles.
5. **Resilience & telemetry** *(next)*: Emit metrics for planner iterations, action latency buckets, and provide a cancel endpoint bridging to the worker.
6. **Testing & rollout** *(next)*: Build integration tests with a dummy worker and mocked MCP host, plan staging rollout, and ensure observability dashboards align with the `MCP Integration Layer` plan.

