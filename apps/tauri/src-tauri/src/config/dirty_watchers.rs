use std::sync::{Arc, RwLock};

use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{async_runtime, AppHandle, Emitter};
use tokio::time::{sleep, Duration};
use tracing::{debug, error, warn};

use crate::config::{io, mcp, paths, stt, tts};
use crate::stt::events::ConfigDirtyEvent;
use crate::stt::EVENT_CONFIG_DIRTY;

const CONFIG_DEBOUNCE_MS: u64 = 800;

pub struct ConfigDirtyWatchers {
    _stt: RecommendedWatcher,
    _tts: RecommendedWatcher,
    _mcp: RecommendedWatcher,
}

pub fn start_config_dirty_watchers(
    app_handle: AppHandle,
    stt_config: Arc<RwLock<stt::SttConfig>>,
    tts_config: Arc<RwLock<tts::TtsConfig>>,
    mcp_config: Arc<RwLock<mcp::McpConfig>>,
) -> Result<ConfigDirtyWatchers> {
    let stt_watcher = spawn_config_dirty_watcher(
        app_handle.clone(),
        "stt.config.json",
        "stt",
        stt_config,
        stt::parse_config,
        stt::config_file_fingerprint,
    )?;
    let tts_watcher = spawn_config_dirty_watcher(
        app_handle.clone(),
        "tts.config.json",
        "tts",
        tts_config,
        tts::parse_config,
        tts::config_file_fingerprint,
    )?;
    let mcp_watcher = spawn_config_dirty_watcher(
        app_handle,
        "mcp.config.json",
        "mcp",
        mcp_config,
        mcp::parse_config,
        mcp::config_file_fingerprint,
    )?;

    Ok(ConfigDirtyWatchers {
        _stt: stt_watcher,
        _tts: tts_watcher,
        _mcp: mcp_watcher,
    })
}

fn spawn_config_dirty_watcher<T, FRead, FFingerprint>(
    app_handle: AppHandle,
    filename: &'static str,
    label: &'static str,
    config_state: Arc<RwLock<T>>,
    read_file: FRead,
    fingerprint: FFingerprint,
) -> Result<RecommendedWatcher>
where
    T: Clone + Send + Sync + 'static,
    FRead: Fn(&str) -> Result<T> + Send + Sync + Copy + 'static,
    FFingerprint: Fn(&T) -> Option<String> + Send + Sync + Copy + 'static,
{
    let config_path = paths::config_path(filename);
    let watched_path = config_path.clone();
    let (debounce_tx, debounce_rx) = async_channel::bounded(1);
    let debounce_handle = app_handle.clone();
    let debounce_state = Arc::clone(&config_state);
    let debounce_path = watched_path.clone();

    async_runtime::spawn(async move {
        while debounce_rx.recv().await.is_ok() {
            loop {
                let delay = sleep(Duration::from_millis(CONFIG_DEBOUNCE_MS));
                tokio::pin!(delay);
                tokio::select! {
                    _ = &mut delay => break,
                    recv = debounce_rx.recv() => {
                        if recv.is_err() {
                            return;
                        }
                        continue;
                    }
                }
            }

            let file_config = match read_config_from_disk(&debounce_path, read_file) {
                Ok(config) => config,
                Err(err) => {
                    warn!("Failed to read {label} config after change: {err}");
                    let _ = debounce_handle.emit(
                        EVENT_CONFIG_DIRTY,
                        ConfigDirtyEvent {
                            path: debounce_path.display().to_string(),
                            kind: format!("{label}:ReadError"),
                        },
                    );
                    continue;
                }
            };

            if emit_dirty_if_changed(
                &debounce_handle,
                label,
                &debounce_path,
                &file_config,
                &debounce_state,
                fingerprint,
            ) {
                continue;
            }
        }
    });

    let mut watcher = notify::recommended_watcher({
        let debounce_tx = debounce_tx.clone();
        move |res: Result<notify::Event, notify::Error>| match res {
            Ok(event) => {
                let should_reload = matches!(
                    event.kind,
                    notify::EventKind::Modify(_)
                        | notify::EventKind::Create(_)
                        | notify::EventKind::Remove(_)
                        | notify::EventKind::Any
                );

                if should_reload {
                    debug!(
                        "{label} config watcher observed change kind={:?} path={}",
                        event.kind,
                        watched_path.display()
                    );
                    let _ = debounce_tx.try_send(());
                } else {
                    debug!(
                        "{label} config watcher ignored event kind={:?} path={}",
                        event.kind,
                        watched_path.display()
                    );
                }
            }
            Err(e) => error!("{label} config watcher error: {e}"),
        }
    })?;

    watcher.watch(&config_path, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

fn read_config_from_disk<T, FRead>(path: &std::path::Path, parse: FRead) -> Result<T>
where
    FRead: Fn(&str) -> Result<T>,
{
    let content = io::read_config_string(path)?;
    parse(&content)
}

fn emit_dirty_if_changed<T, FFingerprint>(
    app_handle: &AppHandle,
    label: &str,
    path: &std::path::Path,
    file_config: &T,
    config_state: &Arc<RwLock<T>>,
    fingerprint: FFingerprint,
) -> bool
where
    FFingerprint: Fn(&T) -> Option<String>,
{
    let file_fingerprint = fingerprint(file_config);
    let memory_fingerprint = config_state
        .read()
        .ok()
        .and_then(|config| fingerprint(&config));

    if file_fingerprint.is_none() {
        warn!("Failed to serialize {label} config for comparison");
        let _ = app_handle.emit(
            EVENT_CONFIG_DIRTY,
            ConfigDirtyEvent {
                path: path.display().to_string(),
                kind: format!("{label}:SerializeError"),
            },
        );
        return true;
    }

    if file_fingerprint != memory_fingerprint {
        let _ = app_handle.emit(
            EVENT_CONFIG_DIRTY,
            ConfigDirtyEvent {
                path: path.display().to_string(),
                kind: format!("{label}:Changed"),
            },
        );
        return true;
    }

    debug!(
        "{label} config watcher observed change but content matches memory: {}",
        path.display()
    );
    false
}
