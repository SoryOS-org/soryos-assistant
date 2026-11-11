use std::sync::OnceLock;
use std::time::Instant;

static MONOTONIC_START: OnceLock<Instant> = OnceLock::new();

pub fn monotonic_now_ms() -> u64 {
    let start = MONOTONIC_START.get_or_init(Instant::now);
    start.elapsed().as_millis() as u64
}
