use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use tracing::warn;

use super::{MetricsEvent, MetricsEventKind, FLUSH_INTERVAL_EVENTS, FLUSH_INTERVAL_MS};

pub(crate) trait MetricsRecorder: Send + 'static {
    fn write_event(&mut self, event: &MetricsEvent) -> Result<()>;
    fn flush(&mut self) -> Result<()>;
    fn rotate_if_needed(&mut self) -> Result<()>;
}

pub(crate) struct JsonlRecorder {
    active_path: PathBuf,
    writer: BufWriter<File>,
    max_bytes: u64,
    retention: usize,
    current_bytes: u64,
    events_since_flush: usize,
    last_flush_time: Instant,
}

impl JsonlRecorder {
    pub(crate) fn open(active_path: PathBuf, max_bytes: u64, retention: usize) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&active_path)
            .with_context(|| format!("Failed to open metrics file {}", active_path.display()))?;

        let current_bytes = file.metadata().map(|meta| meta.len()).unwrap_or_default();

        Ok(Self {
            active_path,
            writer: BufWriter::new(file),
            max_bytes,
            retention,
            current_bytes,
            events_since_flush: 0,
            last_flush_time: Instant::now(),
        })
    }

    fn rotate(&mut self) -> Result<()> {
        // Ensure all buffered data is flushed before rotation
        self.writer
            .flush()
            .context("Failed to flush before rotation")?;
        let rotated_path = rotated_file_path(&self.active_path)?;
        fs::rename(&self.active_path, &rotated_path).with_context(|| {
            format!(
                "Failed to rotate metrics file {} -> {}",
                self.active_path.display(),
                rotated_path.display()
            )
        })?;

        self.writer = BufWriter::new(
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&self.active_path)
                .with_context(|| {
                    format!(
                        "Failed to open fresh metrics file {}",
                        self.active_path.display()
                    )
                })?,
        );
        self.current_bytes = 0;
        self.events_since_flush = 0;
        self.last_flush_time = Instant::now();
        self.cleanup_retention()?;
        Ok(())
    }

    fn cleanup_retention(&self) -> Result<()> {
        let Some(dir) = self.active_path.parent() else {
            return Ok(());
        };

        let mut entries: Vec<_> = fs::read_dir(dir)
            .with_context(|| format!("Failed to enumerate {}", dir.display()))?
            .filter_map(|res| res.ok())
            .filter(|entry| {
                if entry.path() == self.active_path {
                    return false;
                }
                match entry.file_name().to_str() {
                    Some(name) => name.starts_with("latency") && name.ends_with(".jsonl"),
                    None => false,
                }
            })
            .collect();

        entries.sort_by_key(|entry| {
            entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        });
        entries.reverse();

        for entry in entries.into_iter().skip(self.retention.saturating_sub(1)) {
            if let Err(err) = fs::remove_file(entry.path()) {
                warn!("Failed to prune old metrics file: {err}");
            }
        }

        Ok(())
    }
}

impl MetricsRecorder for JsonlRecorder {
    fn write_event(&mut self, event: &MetricsEvent) -> Result<()> {
        self.rotate_if_needed()?;

        let payload = serde_json::to_vec(event).context("Failed to encode metrics event")?;
        let newline = b"\n";
        let required = payload.len() as u64 + newline.len() as u64;

        self.writer
            .write_all(&payload)
            .context("Failed to write metrics payload")?;
        self.writer
            .write_all(newline)
            .context("Failed to write metrics newline")?;
        self.current_bytes = self.current_bytes.saturating_add(required);
        self.events_since_flush += 1;

        // Flush after critical events (TurnSummary) or periodically
        let is_critical = matches!(event.event, MetricsEventKind::TurnSummary);
        let should_flush = is_critical
            || self.events_since_flush >= FLUSH_INTERVAL_EVENTS
            || self.last_flush_time.elapsed().as_millis() as u64 >= FLUSH_INTERVAL_MS;

        if should_flush {
            self.flush()?;
            self.events_since_flush = 0;
            self.last_flush_time = Instant::now();
        }

        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.writer
            .flush()
            .context("Failed to flush metrics writer")
    }

    fn rotate_if_needed(&mut self) -> Result<()> {
        if self.current_bytes >= self.max_bytes {
            self.rotate()
        } else {
            Ok(())
        }
    }
}

fn rotated_file_path(active_path: &Path) -> Result<PathBuf> {
    let Some(parent) = active_path.parent() else {
        return Err(anyhow!(
            "Active metrics path {} has no parent directory",
            active_path.display()
        ));
    };
    let stem = active_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("latency");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs();
    let rotated_name = format!("{stem}-{timestamp}.jsonl");
    Ok(parent.join(rotated_name))
}
