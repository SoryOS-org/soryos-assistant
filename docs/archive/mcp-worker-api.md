# MCP Worker API

## 1. Goals

- Provide a minimal contract for handing a user turn to the worker and letting the worker return speech plus high-level intents.
- Allow the worker to plan and supervise MCP tool calls in a closed loop without a separate result callback surface.
- Keep the desktop app responsible for executing tools and relaying tool transcripts back to the worker when new planning is required.

## 2. Endpoints

### 2.1 `POST /chat` (existing)

- **Request**: unchanged payload shape, but the **desktop generates** a `session_token` for the turn and includes it alongside the user query, machine ID, and schema fingerprints. The worker must echo this token in subsequent planner responses.
- **Response**:
  ```json
  {
    "intents": [
      { "type": "converse" },
      {
        "type": "mcp_plan",
        "mcp_server": "browsermcp",
        "instruction": "Check the user's email inbox for new messages"
      }
    ]
  }
  ```

The chat turn is purely conversational: it returns a single reply string (used for both transcript and TTS) and a list of intents that the desktop app should act on. `mcp_plan` is **not** embedded here any longer; the intent simply signals that the desktop should enter the planner flow.

#### Intent types

| Type | Purpose |
|------|---------|
| `converse` | Pure chat response. |
| `open_app` | User explicitly asked to open/launch an application. Includes `app_name`. |
| `open_app_not_found` | Application unavailable; includes `query` and optional suggestions. |
| `mcp_plan` | Worker wants the MCP planner to take over. Includes `mcp_server` (string) and `instruction` (short natural-language objective). |

### 2.2 `POST /planner` (new)

- **Purpose**: request or refine the next batch of MCP tool actions. The desktop app calls this endpoint whenever it needs fresh guidance—initially after the chat turn, and subsequently when it has new tool results.
- **Request**:
  ```json
  {
    "query": "open the release notes",
    "machine_id": "machine-123",
    "session_token": "abc123",
    "instruction": "Locate the latest release notes in the browser",
    "mcp_server": "browsermcp",
    "conversation_memory": { "type": "turns", "turns": [ ... ] },
    "allowlist": [
      {"app_id": "browsermcp", "enabled": true, "display_name": "Browser"}
    ],
    "history": [
      {
        "step_index": 0,
        "action": {
          "server_id": "browsermcp",
          "tool_name": "browser_navigate",
          "arguments": {"url": "https://mail.google.com"}
        },
        "status": "transport_error",
        "mcp_response": {
          "result": {
            "content": [
              {
                "type": "text",
                "text": "Error: No connection to browser extension."
              }
            ],
            "isError": true
          }
        },
        "error": "Navigation failed with HTTP 301",
        "latency_ms": 732
      }
    ]
  }
  ```
- **Response**:
  ```json
  {
    "mcp_plan": {
      "session_token": "abc123",
      "actions": [
        {
          "server_id": "browsermcp",
          "tool_name": "browser_navigate",
          "arguments": {"url": "https://mail.google.com"},
          "speak": true,
          "status_note": "Trying Gmail over HTTPS"
        },
        {
          "server_id": "browsermcp",
          "tool_name": "browser_snapshot",
          "arguments": {"wait_ms": 1500}
        }
      ],
      "reply": "Navigating to Gmail and capturing the inbox view.",
    }
  }
  ```

Key fields:

| Field | Description |
|-------|-------------|
| `history` | Optional array summarising previously attempted actions and their outcomes. Each entry includes the raw tool output, any `error`, and `latency_ms`. The planner must consult this history to avoid repeating failing steps. |
| `mcp_plan.actions[].speak` | Optional boolean. When true, the desktop should surface the associated `reply` immediately (e.g., "Trying HTTPS instead"). |
| `mcp_plan.reply` | Text update that should appear in the MCP activity log / panel. |
| `mcp_plan.summary` | Optional narrative (<= 3 sentences) summarizing planner state when `SUMMARY REQUIRED` is triggered. |

If the planner cannot make progress, it may return an empty `actions` array along with a `reply` explaining the failure so the desktop can escalate to the user.

### 2.3 Tool execution feedback

The worker no longer exposes a separate callback such as `/mcp_result`. Instead, the desktop app simply re-invokes `/planner` with an updated `history` payload whenever it needs the worker to react to the latest tool outputs. The desktop records each command/response pair locally (including `completed` vs. `transport_error` status) before sending the summarised history back to the worker.

Typical loop:

1. Desktop posts `/planner` with empty `history` → receives `actions = [step0, step1]`.
2. Desktop executes `step0`, captures the raw result, marks the transport outcome (`completed` or `transport_error`), appends it to `history`, and calls `/planner` again.
3. Planner returns follow-up actions (maybe `step1` with tweaked arguments) and optional status narration. The LLM is expected to take the supplied history into account before retrying a failed step.
4. Repeat until the planner returns `actions: []` with a concluding `reply` (done) or an explicit escalation instruction.

### 2.4 `POST /mcp_cancel` (optional)

- Used when the desktop or user cancels the workflow.
- Payload: `{ "session_token": "abc123", "reason": "user_cancelled" }`
- Worker cleans up memory/state.

## 3. Authentication & Transport

- Same auth mechanism as existing worker endpoints (bearer token / mTLS).
- Rate limits per session (e.g., ≤5 outstanding actions) to prevent runaway loops.

## 4. State & Correlation

- The desktop generates the `session_token`, includes it in every `/chat` and `/planner` call, and attaches it to UI/telemetry events so downstream systems can correlate tool executions.
- The worker mirrors the evolving `history` while the desktop keeps the authoritative per-step transcript (action, raw payload, error, latency).
- Conversation memory shared with the planner should stay lightweight; the desktop currently trims turn-based memories to the most recent 10 entries before sending them.

## 5. Error Semantics

| Scenario | Planner expectation | Desktop behaviour |
|----------|---------------------|-------------------|
| Tool call completed | Append raw output to `history`, decide on follow-up actions if needed. | Record `status: "completed"`, include `latency_ms`/payload in history, and re-post `/planner` if more work remains. |
| Transport failure (e.g., tool unavailable, timeout) | Inspect the failure in history and decide whether to retry with adjustments, branch, or escalate. | Record `status: "transport_error"` with the error string, append to history, and await new guidance before retrying. |
| Planner returns empty `actions` | Worker has finished (either success or graceful failure). | Stop calling `/planner`. Speak/surface the concluding `reply`. |
| User cancels | Desktop calls `/mcp_cancel`. | Worker tears down any planner state associated with the session token. |

If a planner response is malformed (e.g., missing `mcp_plan`), the desktop should surface the failure to the user and halt the workflow.

## 6. Telemetry

- Every request/response includes `trace_id` header (UUID) for correlation.
- Desktop logs each planner action execution and its transport outcome (`completed`/`transport_error`), along with latency and payload size.
- Worker exposes per-session metrics (actions attempted, success rate, total duration) using the same `session_token` key.

## 7. Open Questions / TODO

- Worker memory eviction: when to drop old `session_token` entries.
- Handling of large tool outputs (consider upload to blob store with reference IDs).
- Streaming updates (SSE/WebSocket) for high-frequency steps (future work).

