use tauri::App;

use super::{defaults, stt};
#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
    max_attempts: u32,
    base_delay_ms: u64,
}

impl RetryPolicy {
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts.max(1)
    }

    pub fn base_delay_ms(&self) -> u64 {
        self.base_delay_ms.max(1)
    }
}

#[derive(Clone, Debug)]
pub struct WorkerBridgeConfig {
    base_url: String,
    retry_policy: RetryPolicy,
}

impl WorkerBridgeConfig {
    pub(crate) fn load(_app: &App, worker_settings: &stt::WorkerConfig) -> Self {
        let base_candidate = Self::derive_base_from_chat(&worker_settings.url)
            .unwrap_or_else(|| worker_settings.url.clone());

        let mut config = Self::new(base_candidate);
        config.apply_worker_settings(worker_settings);
        config
    }

    pub(crate) fn new(base_url: String) -> Self {
        Self {
            base_url,
            retry_policy: RetryPolicy {
                max_attempts: defaults::WORKER_RETRY_MAX_ATTEMPTS,
                base_delay_ms: defaults::WORKER_RETRY_BASE_DELAY_MS,
            },
        }
    }

    fn derive_base_from_chat(chat_url: &str) -> Option<String> {
        let trimmed = chat_url.trim();
        if trimmed.is_empty() {
            return None;
        }

        let without_slash = trimmed.trim_end_matches('/');
        if let Some((base, tail)) = without_slash.rsplit_once('/') {
            if tail.eq_ignore_ascii_case("chat") {
                return Some(base.to_string());
            }
        }

        Some(without_slash.to_string())
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn retry_policy(&self) -> RetryPolicy {
        self.retry_policy
    }

    pub fn apply_stt_worker_config(&mut self, config: &stt::WorkerConfig) {
        let candidate = Self::derive_base_from_chat(&config.url)
            .unwrap_or_else(|| config.url.trim().to_string());
        self.base_url = candidate;
        self.apply_worker_settings(config);
    }

    fn apply_worker_settings(&mut self, config: &stt::WorkerConfig) {
        self.retry_policy = RetryPolicy {
            max_attempts: config.retry.max.max(1),
            base_delay_ms: config.retry.base_ms.max(1),
        };
    }
}
