mod handle;
mod recorder;
mod time;
mod types;
mod worker;

const METRICS_FILENAME: &str = "latency.jsonl";
const DEFAULT_MAX_BYTES: u64 = 15 * 1024 * 1024;
const DEFAULT_RETENTION: usize = 5;
const SUMMARY_CACHE_CAPACITY: usize = 200;
const QUEUE_CAPACITY: usize = 1024;
// Flush after this many events to ensure data persistence
const FLUSH_INTERVAL_EVENTS: usize = 10;
// Flush at least every N milliseconds to ensure data persistence
const FLUSH_INTERVAL_MS: u64 = 5000;

pub use handle::MetricsHandle;
pub use time::monotonic_now_ms;
pub use types::{MetricsEvent, MetricsEventKind, MetricsPhase, MetricsStats, TurnTrigger};
