pub mod worker_client;

mod chunking;
mod planner_flow;
mod sync;
mod types;
mod worker_dispatch;
mod worker_flow;

pub const EVENT_WORKER_AUTH_FAILED: &str = "worker_auth_failed";
pub const EVENT_WORKER_AUTH_OK: &str = "worker_auth_ok";

pub use sync::{
    snapshot_preferences, sync_catalog, sync_mcp_config, sync_mcp_snapshot, sync_preferences,
};
pub use types::ChatWorkerOutcome;
pub use worker_flow::{run_planner_flow, run_worker_flow};

pub use types::mcp;
