//! Brain provider configuration.
//!
//! The desktop assistant can be driven either by the legacy remote worker
//! (Cloudflare) or by the local Codex app-server. Both implementations expose
//! the same `ChatWorkerOutcome` contract so the UI and voice pipeline keep
//! working unchanged.

use serde::{Deserialize, Serialize};

/// Which implementation answers user requests.
///
/// The brain is ONE system: the local Codex app-server is the default, and
/// the legacy worker remains as an integrated fallback (never as a separate
/// selectable "second brain").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BrainProvider {
    /// Local Codex app-server (vendored in `crates/codex-main`). Default.
    #[default]
    Codex,
    /// Legacy Cloudflare worker — integrated fallback used only when the
    /// Codex brain is unavailable.
    Worker,
}

/// Codex-specific settings (only read when `provider` is `codex`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct CodexBrainSettings {
    /// Path to the `codex` binary. Empty = probe `resources/codex/codex`.
    pub binary_path: String,
    /// Model id override; `None` uses the Codex config default.
    pub model: Option<String>,
    /// Working directory for the agent thread. Empty = user home.
    pub cwd: String,
    /// Sandbox mode: `read-only`, `workspace-write` or `danger-full-access`.
    pub sandbox: String,
    /// Approval policy: `never`, `on-request`, `on-failure` or `untrusted`.
    pub approval_policy: String,
    /// When the Codex flow fails, transparently fall back to the worker brain.
    pub fallback_to_worker: bool,
}

impl Default for CodexBrainSettings {
    fn default() -> Self {
        Self {
            binary_path: String::new(),
            model: None,
            cwd: String::new(),
            sandbox: String::from("workspace-write"),
            approval_policy: String::from("never"),
            fallback_to_worker: true,
        }
    }
}

/// Root brain configuration block (`stt.config.json → "brain"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct BrainConfig {
    pub provider: BrainProvider,
    pub codex: CodexBrainSettings,
}

impl BrainConfig {
    /// Only serialize the block when it differs from the default (codex with
    /// worker fallback).
    pub fn is_default(&self) -> bool {
        self.provider == BrainProvider::default() && self.codex == CodexBrainSettings::default()
    }
}
