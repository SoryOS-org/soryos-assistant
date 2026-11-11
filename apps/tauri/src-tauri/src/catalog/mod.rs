use crate::config::prelude::catalog_dir_from_handle;
use crate::intents::application::InstalledApp;
use crate::intents::IntentCatalogSnapshot;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tracing::{debug, warn};

pub const EVENT_NAME: &str = "intent_catalog_updated";
const SNAPSHOT_FILENAME: &str = "catalog.snapshot.json";
const SNAPSHOT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogSnapshot {
    version: u32,
    generated_at_ms: u64,
    applications: Vec<InstalledApp>,
}

impl CatalogSnapshot {
    pub fn new(mut applications: Vec<InstalledApp>) -> Self {
        applications.sort();
        applications.dedup();
        Self {
            version: SNAPSHOT_VERSION,
            generated_at_ms: current_millis(),
            applications,
        }
    }

    pub fn applications(&self) -> &[InstalledApp] {
        &self.applications
    }

    pub fn is_version_compatible(&self) -> bool {
        self.version == SNAPSHOT_VERSION
    }
}

#[derive(Debug)]
pub struct PersistOutcome {
    pub snapshot: CatalogSnapshot,
    pub changed: bool,
}

pub fn load_snapshot(app_handle: &AppHandle) -> Result<Option<CatalogSnapshot>> {
    let path = snapshot_path(app_handle);
    if !path.exists() {
        return Ok(None);
    }

    let data = fs::read_to_string(&path)
        .with_context(|| format!("failed to read catalog snapshot {}", path.display()))?;
    let snapshot: CatalogSnapshot = serde_json::from_str(&data)
        .with_context(|| format!("failed to parse catalog snapshot {}", path.display()))?;

    if snapshot.is_version_compatible() {
        debug!(
            "Loaded catalog snapshot ({} apps, generated_at_ms={})",
            snapshot.applications.len(),
            snapshot.generated_at_ms
        );
        Ok(Some(snapshot))
    } else {
        // incompatible snapshot; ignore so a fresh one will be written
        debug!(
            "Ignoring incompatible catalog snapshot version {} (expected {})",
            snapshot.version, SNAPSHOT_VERSION
        );
        Ok(None)
    }
}

pub fn persist_if_changed(app_handle: &AppHandle, apps: &[InstalledApp]) -> Result<PersistOutcome> {
    let mut normalised = apps.to_vec();
    normalised.sort();
    normalised.dedup();

    if let Some(existing) = load_snapshot(app_handle)? {
        if existing.applications == normalised {
            debug!(
                "Catalog snapshot unchanged ({} apps); skipping write",
                existing.applications.len()
            );
            return Ok(PersistOutcome {
                snapshot: existing,
                changed: false,
            });
        }
    }

    let snapshot = CatalogSnapshot::new(normalised);
    debug!(
        "Persisting catalog snapshot ({} apps, version {})",
        snapshot.applications.len(),
        snapshot.version
    );
    write_snapshot(app_handle, &snapshot)?;
    Ok(PersistOutcome {
        snapshot,
        changed: true,
    })
}

fn write_snapshot(app_handle: &AppHandle, snapshot: &CatalogSnapshot) -> Result<()> {
    let path = snapshot_path(app_handle);
    let parent = path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent)
        .with_context(|| format!("failed to create catalog cache dir {}", parent.display()))?;

    let temp_path = path.with_extension("tmp");
    let content =
        serde_json::to_string_pretty(snapshot).context("failed to serialise catalog snapshot")?;
    fs::write(&temp_path, content)
        .with_context(|| format!("failed to write temp snapshot {}", temp_path.display()))?;
    fs::rename(&temp_path, &path)
        .with_context(|| format!("failed to move snapshot into place {}", path.display()))?;
    Ok(())
}

fn snapshot_path(app_handle: &AppHandle) -> PathBuf {
    catalog_dir_from_handle(app_handle).join(SNAPSHOT_FILENAME)
}

fn current_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub fn emit_update_event(app_handle: &AppHandle, snapshot: &CatalogSnapshot) {
    let payload = IntentCatalogSnapshot {
        applications: snapshot.applications().to_vec(),
    };
    if let Err(err) = app_handle.emit(EVENT_NAME, &payload) {
        warn!("Failed to emit catalog update event: {err}");
    }
    debug!(
        "Emitted catalog update event with {} applications",
        snapshot.applications().len()
    );
}
