# MCP Response Pagination Plan

## Goals

1. Prevent oversized MCP tool outputs (e.g., browser snapshots) from blowing past LLM token limits or HTTP payload caps.
2. Give the planner a deterministic way to review large outputs incrementally (`page -> request next page`) without crashing or hallucinating.
3. Keep SRP: worker handles payload shaping/storage, while the Tauri host focuses on executing tools and relaying concise summaries.

## Constraints & Observations

- `McpActionResult.output` currently carries raw JSON from the MCP host straight into planner history (`apps/tauri/src-tauri/src/machine.rs`). Large snapshots therefore traverse the full loop.
- The worker already formats planner history (`apps/worker/src/gateway.ts::formatPlannerHistory`). This is the chokepoint where we can truncate, elide, or replace payloads with references.
- We can introduce an internal “pagination tool” (e.g., `sprite_internal_fetch_page`) that the LLM can call when it needs the rest of a response. This mirrors how intents/apps are exposed today.
- We must not rely on environment variables for runtime configuration (per AGENTS.md); pagination state should live alongside the MCP snapshots or a bounded cache keyed by `session_token` + `step_index`.
- Front-end indicators must reflect long-running/page fetch steps, so pagination commands should emit the usual `assistant://mcp-progress` events.

## Proposed Architecture

### 1. Chunking Handler (Rust side)

- When a tool result exceeds a configurable byte budget (e.g., 32 KB), the MCP orchestration layer **walks the JSON structure** and only paginates the specific top-level fields (strings/arrays/objects) that cross the threshold. Everything else stays inline so the worker retains context.
- Oversized fields are replaced with lightweight metadata:
  ```json
  {
    "chunked": true,
    "chunk_id": "browser_snapshot::0::result.content[0]",
    "field_path": "result.content[0]",
    "preview": "Captured https://mail.google.com…",
    "total_bytes": 1048576,
    "page_size": 8192,
    "total_pages": 128,
    "next_page": 0,
    "hint": "Call sprite_internal.fetch_chunk with this chunk_id to see more."
  }
  ```
- The chunk store keeps the original field values keyed by `(session_token, step_index, field_path)` so subsequent chunk fetches can return just that slice. Multiple oversized fields in the same response each get their own chunk metadata.
- Because the rest of the JSON is untouched, downstream consumers (panel, worker) still see headers, status flags, etc., without needing to page in the heavy bits.

### 2. Internal Pagination Tool

- Define a virtual MCP server `sprite_internal` with a tool `fetch_chunk`:
  ```json
  {
    "name": "fetch_chunk",
    "description": "Retrieve the next chunk of a truncated MCP result.",
    "input_schema": {
      "type": "object",
      "required": ["session_token", "chunk_id"],
      "properties": {
        "session_token": {"type": "string"},
        "chunk_id": {"type": "string", "description": "Opaque handle returned in history.chunks[].chunk_id"}
      }
    }
  }
  ```
- The host implementation:
  1. Validates that the chunk belongs to the requesting `session_token`.
  2. Streams the stored data (optionally compress + base64 if binary).
  3. Marks `chunk_id` as consumed if we want single-use semantics.

### 3. Worker / LLM Prompt Changes

- Update `formatPlannerHistory` (worker) so when it sees `mcp_response.chunks`, it emits:
  ```
  Step 2: browsermcp/browser_snapshot | status=completed | summary=Captured https://… | chunks=2 (use sprite_internal.fetch_chunk to view)
  ```
- Extend the MCP tools prompt to include the new `sprite_internal` server so the planner knows how to fetch more data.
- Optionally surface a helper hint in the planner system prompt: “Large tool responses may be paginated. Use `sprite_internal.fetch_chunk` to retrieve additional data when `chunks` metadata is present.”

### 4. Lifecycle & Cleanup

- Store chunk data for the lifetime of the planner run (session token). When the session completes or cancels, drop the corresponding entries to avoid leaks.
- Expose a background sweeper that clears stale sessions after a timeout (e.g., 10 minutes) in case the planner crashes mid-run.

### 5. Telemetry & UI

- Emit structured logs when chunking occurs (`chunked_bytes`, `chunk_count`, `session_token`).
- The panel can show “(truncated)” badges and optionally request chunks for operator debugging.
- `assistant://mcp-progress` events should include a `status_note` like “Chunk 1/3 ready (fetch via sprite_internal.fetch_chunk).”

## Next Steps

1. Implement `ChunkStore` + history shaping on the Rust side.
2. Define the internal tool + host handler.
3. Wire worker prompt updates + tests ensuring planner history references the pagination tool.
4. Add cleanup/telemetry + document the new flow for agents/operators.
5. (Upcoming) Layer in optional helpers like `sprite_internal.search_chunk` and lightweight per-chunk header metadata so the planner can jump directly to relevant sections without iterating.
