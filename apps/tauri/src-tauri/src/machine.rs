//! Support modules shared by the single brain (Codex) and the UI.
//!
//! The legacy worker chat/planner engine was removed during the brain merge:
//! the Codex engine handles chat and tool iteration natively. What remains
//! here is everything Codex does *not* provide and exploits instead:
//! - `worker_client`: HTTP helpers still used by machine-state sync/config/memory.
//! - `chunking`: long-result chunking exposed as `sprite_internal` tools.
//! - `sync`: machine-state and preference synchronization.
//! - `types`: response/event contracts shared with the UI and voice pipeline.

pub mod worker_client;

pub(crate) mod chunking;
mod sync;
mod types;

pub const EVENT_WORKER_AUTH_FAILED: &str = "worker_auth_failed";
pub const EVENT_WORKER_AUTH_OK: &str = "worker_auth_ok";

pub use sync::{
    snapshot_preferences, sync_catalog, sync_mcp_config, sync_mcp_snapshot, sync_preferences,
};
pub use types::ChatWorkerOutcome;

pub use types::mcp;
