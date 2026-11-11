# Machine State Management Architecture

## Overview

The system uses Cloudflare Durable Objects to maintain per-machine state across sessions. Each machine is identified by a hardware-based machine ID that persists across app reinstalls and Windows updates.

## Architecture Layers

```
┌─────────────────────────────────────────────────────────────┐
│                     Rust Desktop App                        │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  MachineStateManager                                  │  │
│  │  - Generates/stores hardware-based machine_id        │  │
│  │  - Caches allowlist (refreshes every 5 min)          │  │
│  │  - Provides machine_id to worker                     │  │
│  └──────────────────────────────────────────────────────┘  │
│                           │                                 │
│                           │ HTTP POST /chat                 │
│                           │ { query, machine_id,             │
│                           │   personality, allowlist }      │
│                           ▼                                 │
└─────────────────────────────────────────────────────────────┘
                           │
                           │
┌─────────────────────────────────────────────────────────────┐
│                  Cloudflare Worker                          │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  /chat endpoint                                        │  │
│  │  1. Sync user turn to DO                              │  │
│  │  2. Fetch conversation memory from DO                 │  │
│  │  3. Fetch allowlist from DO (or use provided)         │  │
│  │  4. Fetch personality from DO config                  │  │
│  │  5. Call LLM with context                             │  │
│  │  6. Sync assistant turn to DO                         │  │
│  │  7. Return response                                   │  │
│  └──────────────────────────────────────────────────────┘  │
│                           │                                 │
│                           │ DO Stub Calls                   │
│                           ▼                                 │
└─────────────────────────────────────────────────────────────┘
                           │
                           │
┌─────────────────────────────────────────────────────────────┐
│            Durable Object (MachineStateDO)                  │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  Per-machine state storage                           │  │
│  │  - conversation memory (last 25 turns)               │  │
│  │  - allowlist (installed apps)                        │  │
│  │  - config (personality, preferences)                 │  │
│  │  - last_updated_ts                                   │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Component Responsibilities

### Rust Desktop App (`apps/tauri/src-tauri/src/config/machine_state.rs`)

**MachineStateManager**:
- **Machine ID Generation**: 
  - Uses `machine-id` crate to get hardware-based identifier
  - Hardware ID is deterministic - same ID every time on the same machine
  - No persistence needed - generated fresh on each app start
  - Falls back to error if hardware ID unavailable (should be rare)

- **Allowlist Management**:
  - Builds allowlist from installed apps + preferences (local state)
  - No caching - Rust is the source of truth
  - Syncs to worker when catalog refreshes or preferences change

- **Worker Communication**:
  - Sends `machine_id` with every chat request
  - Sends `personality` from preferences
  - Sends `allowlist` built from local state (installed apps + enable/disable prefs)

- **Config Sync**:

- **MCP Sync**:
  - Serialises the active MCP configuration and tool/resource snapshots when the desktop host emits `SchemasUpdated`.
  - Calls `sync_mcp_snapshot()` (async) which wraps the payload in `config_patch.mcp` and posts it to `/sync_machine_state`.
  - Syncs preferences to worker on startup and when preferences change
  - Uses `sync_config()` method (async, fire-and-forget)

- **App Catalog Sync**:
  - Syncs installed apps to worker when catalog refreshes
  - Converts `InstalledApp` to `AllowlistEntry` format
  - Includes enabled/disabled state from preferences

### Cloudflare Worker (`apps/worker/src/index.ts`)

**`/chat` Endpoint**:
1. **Receives request** with `query`, `machine_id`, `personality`, `allowlist`
2. **Syncs user turn** to DO (fire-and-forget)
   - Creates `ConversationTurn` with role="user", content=query
   - Calls `/sync` endpoint on DO
3. **Fetches conversation memory** from DO
   - Calls `/memory` endpoint
   - Gets last 25 turns or summary
4. **Fetches allowlist** (if not provided in request)
   - Calls `/allowlist` endpoint
   - Falls back to provided allowlist if fetch fails
5. **Fetches personality** (if not provided in request)
   - Calls `/config` endpoint
   - Falls back to provided personality or default
6. **Calls LLM Gateway** with:
   - System prompt (with personality)
   - Conversation history
   - Allowlist context
   - Current user query
7. **Syncs assistant turn** after successful LLM response
   - Creates `ConversationTurn` with role="assistant", content=reply
   - Calls `/sync` endpoint on DO (fire-and-forget)
8. **Returns response** to Rust app

**Other Endpoints**:
- `POST /sync_machine_state`: Syncs machine state updates from Rust
  - `config_patch.mcp` carries MCP configuration + tool/resource snapshots.
- `GET /allowlist?machine_id=...`: Returns allowlist for machine
- `GET /config?machine_id=...`: Returns config for machine
- `POST /config`: Updates config for machine
- `GET /memory?machine_id=...`: Returns conversation memory

### Durable Object (`apps/worker/src/machine-state-do.ts`)

**MachineStateDO**:
- **State Storage**: 
  - Stores state in Cloudflare Durable Object storage
  - Key: `"state"` (single key per DO instance)
  - Structure: `MachineState` with `machine_id`, `memory`, `allowlist`, `config`, `last_updated_ts`

- **Conversation Memory**:
  - Maintains last 25 conversation turns
  - Rotates oldest turns when limit exceeded
  - Supports "turns" mode (array of turns) or "summary" mode (rolling summary)

- **Allowlist Management**:
  - Stores full `AllowlistEntry` objects internally
  - Returns sanitized `SafeAllowlistEntry` via `/allowlist` endpoint
  - Only includes enabled, present apps
  - Extracts display names and verbs from app metadata

- **Config Storage**:
  - Stores arbitrary JSON config
  - Used for personality and user preferences
  - Stores MCP tool/resource snapshots pushed by the desktop host via `config_patch.mcp`.
  - Merges patches on update

- **Methods**:
  - `applySync()`: Merges updates (apps, turns, config)
  - `getAllowlist()`: Returns safe allowlist data
  - `getConfig()`: Returns machine config
  - `getMemory()`: Returns conversation memory
  - `updateConfig()`: Updates config with patch

## Data Flow Examples

### Chat Request Flow

```
1. User speaks → Rust app captures transcript
2. Rust app calls process_user_request()
   ├─ Fetches cached allowlist (or empty)
   ├─ Gets personality from preferences
   └─ Sends POST /chat to worker:
      {
        "query": "open chrome",
        "machine_id": "abc123...",
        "personality": "Professional and concise",
        "allowlist": [...]
      }

3. Worker receives /chat request
   ├─ Syncs user turn to DO (fire-and-forget)
   ├─ Fetches conversation memory from DO
   ├─ Fetches allowlist from DO (if not provided)
   ├─ Fetches personality from DO config (if not provided)
   ├─ Builds LLM prompt with:
   │  ├─ System prompt (with personality)
   │  ├─ Conversation history
   │  ├─ Allowlist context
   │  └─ Current user query
   ├─ Calls LLM Gateway
   ├─ Syncs assistant turn to DO (fire-and-forget)
   └─ Returns response to Rust

4. Rust app receives response
   └─ Processes intents (open app, etc.)
```

### Allowlist Refresh Flow

```
1. Rust app startup
   └─ MachineStateManager loads
      └─ Spawns async task to fetch_allowlist()
         └─ GET /allowlist?machine_id=...
            └─ Worker → DO → Returns allowlist
               └─ Cached in Rust (refresh every 5 min)
```


### MCP Schema Sync Flow

```
1. MCP host registers/updates tools
   �� Host emits `assistant://mcp-status` (`SchemasUpdated`).
2. Rust machine module serialises snapshot
   �� `sync_mcp_snapshot()` wraps config + tools/resources into `config_patch.mcp`.
3. Worker receives `/sync_machine_state`
   �� Durable Object merges patch into stored config.
   �� Worker planner can reference latest tools for follow-up calls.
```

### App Catalog Sync Flow (Pending)

```
1. User triggers catalog refresh
   └─ IntentState.refresh_catalog()
      └─ Enumerates installed apps
         └─ Converts to AllowlistEntry format
            └─ POST /sync_machine_state
               {
                 "machine_id": "...",
                 "installed_apps": {
                   "chrome.exe": {
                     "enabled": true,
                     "present": true,
                     ...
                   }
                 }
               }
               └─ DO merges into allowlist
```

## State Synchronization Strategy

### Conversation Turns
- **Synced in Worker**: User turn synced before LLM call, assistant turn after
- **Rationale**: Worker already has DO access, closer to conversation logic
- **Timing**: Fire-and-forget (non-blocking)

### Allowlist
- **Prefetch**: Rust app fetches on startup
- **Cache**: Rust caches for 5 minutes
- **Refresh**: Automatic refresh or manual refresh
- **Fallback**: Worker can fetch if Rust cache unavailable

### Config/Preferences
- **Sync Direction**: Rust → Worker → DO
- **Trigger**: On config changes (pending implementation)
- **Storage**: Both local (Rust config files) and remote (DO)

## Implementation Status

### ✅ Completed

1. **Machine ID Generation**
   - Hardware-based ID generation
   - Persistent storage
   - Consistent across reinstalls

2. **Durable Object Setup**
   - MachineStateDO class
   - State storage and retrieval
   - Conversation turn rotation

3. **Worker Integration**
   - Conversation turn syncing
   - Memory fetching
   - Allowlist fetching
   - Personality retrieval
   - LLM context building

4. **Rust Integration**
   - MachineStateManager initialization
   - Allowlist prefetching
   - Personality passing
   - Machine ID management

### ⚠️ Pending

1. **App Enable/Disable UI**
   - Panel UI to manage allowlist preferences
   - Currently preferences are stored but no UI to edit them

2. **Conversation Memory Fetching**
   - Currently worker fetches from DO
   - Could optionally pass to Rust if needed for UI

## Configuration

### Machine ID Storage
- **Location**: Generated on-demand (not persisted)
- **Format**: Hardware-based identifier from `machine-id` crate
- **Persistence**: Not needed - hardware ID is deterministic and consistent
- **Stability**: Same ID across app restarts, reinstalls, and Windows updates

### Preferences Config Storage
- **Location**: `{AppData}/com.sprite.validation/config/preferences.config.json`
- **Fields**: 
  - `personality` (string)
  - `allowlist.app_states` (HashMap<String, bool>) - app enable/disable preferences
- **Default**: `personality: "Professional and concise"`, empty allowlist (all apps enabled by default)
- **Sync**: Saved locally, synced to DO on startup and when preferences change

## Error Handling

- **Machine ID Generation Failure**: Falls back to UUID-based ID
- **Worker Sync Failure**: Logged but doesn't block chat
- **DO Fetch Failure**: Worker continues without context
- **Allowlist Fetch Failure**: Uses cached version or empty list
- **All errors**: Logged with appropriate severity, never crash app

## Future Enhancements

1. **Conversation Summarization**: Auto-summarize old turns when limit reached
2. **App Enable/Disable UI**: Panel UI to manage allowlist
3. **Multi-device Sync**: User-based sync across machines
4. **License Tracking**: Use machine_id for device counting
5. **Analytics**: Track usage patterns per machine

