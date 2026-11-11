use std::collections::BTreeMap;
use std::env;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, warn};
use which::which;

use crate::config::{HeartbeatConfig, TransportConfig, TransportTimeouts};

use super::{Transport, TransportError};

const DEFAULT_NEWLINE: &[u8] = b"\n";

pub struct StdioTransport {
    server_id: String,
    child: Arc<Mutex<Child>>,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    stdout: Arc<Mutex<BufReader<tokio::process::ChildStdout>>>,
    timeouts: TimeoutDurations,
    #[allow(dead_code)]
    heartbeat: Option<HeartbeatConfig>,
}

impl std::fmt::Debug for StdioTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdioTransport")
            .field("server_id", &self.server_id)
            .finish_non_exhaustive()
    }
}

impl StdioTransport {
    pub async fn spawn(
        server_id: String,
        config: &TransportConfig,
    ) -> Result<Self, TransportError> {
        let settings = match config {
            TransportConfig::Stdio {
                command,
                args,
                env,
                current_dir,
                timeouts,
                heartbeat,
            } => StdioSettings {
                command: command.clone(),
                args: args.clone(),
                env: env.clone(),
                current_dir: current_dir.clone(),
                timeouts: timeouts.clone(),
                heartbeat: heartbeat.clone(),
            },
            _ => {
                return Err(TransportError::Unsupported(format!(
                    "server {server_id} configured with non-stdio transport"
                )))
            }
        };

        if settings.command.trim().is_empty() {
            return Err(TransportError::MissingCommand);
        }

        let resolved_command =
            which(&settings.command).map_err(|_| TransportError::CommandNotFound {
                command: settings.command.clone(),
            })?;
        let resolved_command_str = resolved_command.to_string_lossy().to_string();
        debug!(
            server_id = %server_id,
            requested_command = %settings.command,
            resolved_command = %resolved_command_str,
            "Resolved stdio transport command"
        );

        let mut command = Command::new(&resolved_command);

        if !settings.args.is_empty() {
            debug!(server_id = %server_id, args = ?settings.args, "Applying transport arguments");
            command.args(&settings.args);
        }

        #[cfg(debug_assertions)]
        let mut env_map = settings.env.clone();
        #[cfg(not(debug_assertions))]
        let env_map = settings.env.clone();
        if !env_map.is_empty() {
            debug!(server_id = %server_id, env_overrides = ?env_map, "Applying transport environment overrides");
        }

        #[cfg(debug_assertions)]
        {
            use std::collections::HashSet;
            use std::path::PathBuf;

            use tracing::info;

            let mut sanitized_paths: Vec<String> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();

            let mut push_path = |entry: &str| {
                let trimmed = entry.trim();
                if trimmed.is_empty() {
                    return;
                }
                let canonical =
                    Path::new(trimmed)
                        .components()
                        .fold(PathBuf::new(), |mut acc, comp| {
                            acc.push(comp);
                            acc
                        });
                let canonical_str = canonical.to_string_lossy().to_string();
                let key = canonical_str.to_ascii_lowercase();
                if seen.insert(key) {
                    sanitized_paths.push(canonical_str);
                }
            };

            if let Some(parent) = resolved_command.parent() {
                push_path(&parent.to_string_lossy());
            }

            if let Ok(npm_prefix) = env::var("npm_config_prefix") {
                push_path(&npm_prefix);
                push_path(&format!(r"{}\node_modules\.bin", npm_prefix));
            }

            if let Ok(appdata) = env::var("APPDATA") {
                push_path(&format!(r"{}\npm", appdata));
                push_path(&format!(r"{}\npm\node_modules\.bin", appdata));
            }

            if let Ok(system_root) = env::var("SystemRoot").or_else(|_| env::var("WINDIR")) {
                push_path(&system_root);
                push_path(&format!(r"{}\System32", system_root));
                push_path(&format!(r"{}\System32\Wbem", system_root));
                push_path(&format!(r"{}\System32\WindowsPowerShell\v1.0", system_root));
            }

            if let Ok(existing_path) = env::var("PATH") {
                for entry in existing_path.split(';') {
                    let trimmed = entry.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let lower = trimmed.to_ascii_lowercase();
                    if lower.contains("target\\debug")
                        || lower.contains("target/debug")
                        || lower.contains("cmakefiles")
                        || lower.contains("\\build\\")
                        || lower.contains("/build/")
                        || lower.contains("_deps")
                    {
                        continue;
                    }

                    if lower.contains("nodejs")
                        || lower.contains("system32")
                        || lower.contains("windows")
                        || lower.contains("powershell")
                        || lower.contains("npm")
                        || lower.contains("git\\cmd")
                    {
                        push_path(trimmed);
                    }
                }
            }

            if !sanitized_paths.is_empty() {
                let sanitized = sanitized_paths.join(";");
                let sanitized_count = sanitized_paths.len();
                let sanitized_len = sanitized.len();
                info!(
                    server_id = %server_id,
                    sanitized_path_count = sanitized_count,
                    sanitized_path_len = sanitized_len,
                    "Applying sanitized PATH for debug spawn"
                );
                if tracing::enabled!(tracing::Level::DEBUG) {
                    debug!(server_id = %server_id, sanitized_path = %sanitized);
                }
                env_map.insert("PATH".into(), sanitized);
            }
        }

        if !env_map.is_empty() {
            command.envs(&env_map);
        }

        if let Some(dir) = settings.current_dir.as_deref() {
            if !dir.trim().is_empty() {
                debug!(server_id = %server_id, current_dir = %dir, "Setting transport working directory");
                command.current_dir(Path::new(dir));
            }
        }

        if let Ok(current_dir) = env::current_dir() {
            debug!(
                server_id = %server_id,
                host_cwd = %current_dir.display(),
                "Host process current working directory"
            );
        }

        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let timeouts = TimeoutDurations::from(settings.timeouts);

        let mut child = command.spawn().map_err(TransportError::Spawn)?;
        if let Some(status) = child.try_wait().map_err(TransportError::Spawn)? {
            warn!(
                server_id = %server_id,
                status = ?status,
                "Transport process reported exit immediately after spawn; continuing to allow handshake"
            );
        }
        let server_id_for_logs = server_id.clone();
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => {
                            let message = line.trim_end_matches(['\r', '\n']);
                            if message.is_empty() {
                                continue;
                            }

                            warn!(server_id = %server_id_for_logs, stderr = %message);
                        }
                        Err(err) => {
                            warn!(
                                server_id = %server_id_for_logs,
                                error = %err,
                                "Failed to read stdio transport stderr"
                            );
                            break;
                        }
                    }
                }
            });
        }

        let stdin = child.stdin.take().ok_or(TransportError::NotInitialised)?;
        let stdout = child.stdout.take().ok_or(TransportError::NotInitialised)?;

        Ok(Self {
            server_id,
            child: Arc::new(Mutex::new(child)),
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(BufReader::new(stdout))),
            timeouts,
            heartbeat: settings.heartbeat,
        })
    }

    async fn ensure_child_alive(&self) -> Result<(), TransportError> {
        let mut guard = self.child.lock().await;
        match guard.try_wait() {
            Ok(Some(status)) => {
                warn!(server_id = %self.server_id, status = ?status, "Transport child reported exit");
                Err(TransportError::process_exited(self.server_id.clone()))
            }
            Ok(None) => Ok(()),
            Err(err) => {
                warn!(server_id = %self.server_id, error = %err, "try_wait failed while checking transport child");
                Err(TransportError::Spawn(err))
            }
        }
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn send(&self, payload: &[u8]) -> Result<(), TransportError> {
        self.ensure_child_alive().await?;
        let timeout = self.timeouts.write;
        let stdin = Arc::clone(&self.stdin);

        let write_future = async move {
            let mut guard = stdin.lock().await;
            guard.write_all(payload).await?;
            guard.write_all(DEFAULT_NEWLINE).await?;
            guard.flush().await?;
            Ok::<_, TransportError>(())
        };

        match timeout {
            Some(duration) if duration.as_millis() > 0 => {
                tokio::time::timeout(duration, write_future)
                    .await
                    .map_err(|_| TransportError::timeout("write", duration))??
            }
            _ => write_future.await?,
        }

        Ok(())
    }

    async fn recv(&self) -> Result<Vec<u8>, TransportError> {
        self.ensure_child_alive().await?;
        let timeout = self.timeouts.read;
        let stdout = Arc::clone(&self.stdout);

        let read_future = async move {
            let mut buffer = Vec::new();
            let mut guard = stdout.lock().await;
            let bytes = guard.read_until(b'\n', &mut buffer).await?;
            if bytes == 0 {
                return Err(TransportError::process_exited(self.server_id.clone()));
            }
            // Trim trailing newline for convenience.
            if buffer.last() == Some(&b'\n') {
                buffer.pop();
            }
            let text = String::from_utf8_lossy(&buffer);
            debug!(server_id = %self.server_id, message = %text, "Transport stdout frame received");
            Ok::<_, TransportError>(buffer)
        };

        match timeout {
            Some(duration) if duration.as_millis() > 0 => {
                tokio::time::timeout(duration, read_future)
                    .await
                    .map_err(|_| TransportError::timeout("read", duration))?
            }
            _ => read_future.await,
        }
    }

    async fn shutdown(&self) -> Result<(), TransportError> {
        let mut child = self.child.lock().await;
        if child.id().is_none() {
            return Ok(());
        }
        child.start_kill().map_err(TransportError::Spawn)?;
        let wait_future = child.wait();
        match self.timeouts.connect {
            Some(duration) if duration.as_millis() > 0 => {
                tokio::time::timeout(duration, wait_future)
                    .await
                    .map_err(|_| TransportError::timeout("shutdown", duration))?
                    .map_err(TransportError::Spawn)?;
            }
            _ => {
                wait_future.await.map_err(TransportError::Spawn)?;
            }
        }
        Ok(())
    }
}

struct StdioSettings {
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    current_dir: Option<String>,
    timeouts: TransportTimeouts,
    heartbeat: Option<HeartbeatConfig>,
}

#[derive(Clone)]
struct TimeoutDurations {
    connect: Option<Duration>,
    read: Option<Duration>,
    write: Option<Duration>,
}

impl TimeoutDurations {
    fn from(config: TransportTimeouts) -> Self {
        Self {
            connect: duration_from_ms(config.connect_timeout_ms),
            read: duration_from_ms(config.read_timeout_ms),
            write: duration_from_ms(config.write_timeout_ms),
        }
    }
}

fn duration_from_ms(ms: u64) -> Option<Duration> {
    if ms == 0 {
        None
    } else {
        Some(Duration::from_millis(ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TransportConfig;
    use std::collections::BTreeMap;
    use std::time::Duration;

    #[tokio::test]
    async fn spawn_requires_command() {
        let config = TransportConfig::default();
        let result = StdioTransport::spawn("test".into(), &config).await;
        assert!(matches!(result, Err(TransportError::MissingCommand)));
    }

    #[tokio::test]
    async fn spawn_requires_command_to_exist() {
        let config = TransportConfig::Stdio {
            command: "__clippy_transport_missing__".into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            current_dir: None,
            timeouts: TransportTimeouts::default(),
            heartbeat: None,
        };

        let result = StdioTransport::spawn("test".into(), &config).await;
        let err = match result {
            Ok(_) => panic!("expected CommandNotFound, spawn succeeded"),
            Err(err) => err,
        };

        match err {
            TransportError::CommandNotFound { command } => {
                assert_eq!(command, "__clippy_transport_missing__");
            }
            other => panic!("expected CommandNotFound, got {other}"),
        }
    }

    #[test]
    fn duration_helper_handles_zero() {
        assert!(duration_from_ms(0).is_none());
        assert_eq!(duration_from_ms(1500).unwrap(), Duration::from_millis(1500));
    }
}
