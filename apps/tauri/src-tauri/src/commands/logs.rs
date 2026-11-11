//! Command helpers for exposing recent runtime logs to the panel UI.

use std::{fs, io, path::PathBuf};

use tauri::AppHandle;

use crate::config::prelude::logs_dir_from_handle;

#[derive(Debug, Clone, serde::Serialize)]
pub struct LogSnapshot {
    pub path: String,
    pub lines: Vec<String>,
}

/// Read the newest application log file and return up to `limit` lines.
#[tauri::command]
pub fn get_recent_logs(app_handle: AppHandle, limit: Option<usize>) -> Result<LogSnapshot, String> {
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    let log_dir = logs_dir_from_handle(&app_handle);

    let entries = match fs::read_dir(&log_dir) {
        Ok(entries) => entries,
        Err(err) => {
            if err.kind() == io::ErrorKind::NotFound {
                return Ok(LogSnapshot {
                    path: String::new(),
                    lines: Vec::new(),
                });
            }
            return Err(format!(
                "Failed to read log directory {}: {err}",
                log_dir.display()
            ));
        }
    };

    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in entries {
        let entry = entry.map_err(|err| format!("Failed to enumerate log directory: {err}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.starts_with("app") || !name.ends_with(".log") {
            continue;
        }

        let metadata = entry
            .metadata()
            .map_err(|err| format!("Failed to inspect log file metadata: {err}"))?;
        let modified = metadata
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        match &mut newest {
            Some((current, current_path)) => {
                if modified > *current {
                    *current = modified;
                    *current_path = path;
                }
            }
            None => newest = Some((modified, path)),
        }
    }

    let Some((_, path)) = newest else {
        return Ok(LogSnapshot {
            path: String::new(),
            lines: Vec::new(),
        });
    };

    let contents = fs::read_to_string(&path)
        .map_err(|err| format!("Failed to read latest log file {}: {err}", path.display()))?;
    let mut lines: Vec<String> = contents.lines().map(|line| line.to_string()).collect();
    if lines.len() > limit {
        lines = lines.split_off(lines.len() - limit);
    }

    Ok(LogSnapshot {
        path: path.display().to_string(),
        lines,
    })
}
