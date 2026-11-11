use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader as TokioBufReader};
use tokio::process::Command as TokioCommand;
use tokio::time::timeout;
use tracing::debug;

// use crate::stt::SttConfig; // Removed unused import

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarConfig {
    pub executable_path: String,
    pub python_env: Option<String>,
    pub startup_timeout_ms: u64,
    pub communication_timeout_ms: u64,
    pub access_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wakeword_keyword: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wakeword_sensitivity: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_user_model_id: Option<String>,
}

impl Default for SidecarConfig {
    fn default() -> Self {
        Self {
            executable_path: "resources/sprite_voice_sidecar.exe".to_string(),
            python_env: Some("apps/voice-service/venv".to_string()),
            startup_timeout_ms: 5000,
            communication_timeout_ms: 10000,
            access_key: "".to_string(), // Should be set in config file
            wakeword_keyword: None,
            wakeword_sensitivity: None,
            app_user_model_id: None,
        }
    }
}

// Async version for non-blocking wake word detection
pub struct AsyncPythonSidecar {
    process: Option<tokio::process::Child>,
    stdin: Option<tokio::process::ChildStdin>,
    stdout: Option<TokioBufReader<tokio::process::ChildStdout>>,
    config: SidecarConfig,
    is_ready: Arc<Mutex<bool>>,
}

trait CommandArgs {
    fn push_arg<S: AsRef<OsStr>>(&mut self, arg: S);
}

impl CommandArgs for Command {
    fn push_arg<S: AsRef<OsStr>>(&mut self, arg: S) {
        self.arg(arg);
    }
}

impl CommandArgs for TokioCommand {
    fn push_arg<S: AsRef<OsStr>>(&mut self, arg: S) {
        self.arg(arg);
    }
}

fn apply_cli_runtime_args<C>(cmd: &mut C, config: &SidecarConfig, log_dir: Option<&Path>)
where
    C: CommandArgs,
{
    if let Some(dir) = log_dir {
        cmd.push_arg("--log-dir");
        cmd.push_arg(dir.as_os_str());
        debug!("Passing --log-dir={} to sidecar", dir.display());
    }

    if let Some(keyword) = &config.wakeword_keyword {
        cmd.push_arg("--wakeword");
        cmd.push_arg(keyword);
    }
    if let Some(sense) = config.wakeword_sensitivity {
        cmd.push_arg("--sensitivity");
        cmd.push_arg(format!("{}", sense));
    }
    if let Some(app_id) = config
        .app_user_model_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        cmd.push_arg("--app-user-model-id");
        cmd.push_arg(app_id);
        debug!("Passing --app-user-model-id={} to sidecar", app_id);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeWordEvent {
    Detected,
    Timeout,
}

#[derive(Debug, Deserialize)]
struct VoiceProcessorFatal {
    error: String,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    code: Option<i32>,
}

fn parse_voice_processor_fatal(line: &str) -> Option<VoiceProcessorFatal> {
    let prefix = "VOICE_PROCESSOR_FATAL ";
    if !line.starts_with(prefix) {
        return None;
    }
    let payload = line.strip_prefix(prefix)?.trim();
    serde_json::from_str(payload).ok()
}

fn fatal_as_error(fatal: VoiceProcessorFatal) -> anyhow::Error {
    let detail = fatal
        .detail
        .as_deref()
        .map(|value| format!(" ({})", value))
        .unwrap_or_default();
    let code = fatal.code.unwrap_or(1);
    anyhow!("Sidecar fatal (code {}): {}{}", code, fatal.error, detail)
}

fn fatal_error_from_line(line: &str) -> Option<anyhow::Error> {
    parse_voice_processor_fatal(line).map(fatal_as_error)
}

impl AsyncPythonSidecar {
    pub fn new(config: SidecarConfig) -> Self {
        Self {
            process: None,
            stdin: None,
            stdout: None,
            config,
            is_ready: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn start(&mut self, log_dir: Option<&std::path::Path>) -> Result<()> {
        debug!("Starting async Python sidecar...");

        // Determine if we're running a Python script or executable
        let mut cmd = if self.config.executable_path.ends_with(".py") {
            let mut python_cmd = TokioCommand::new("python");
            python_cmd.arg(&self.config.executable_path);
            python_cmd
        } else {
            TokioCommand::new(&self.config.executable_path)
        };

        // Set up environment
        if let Some(python_env) = &self.config.python_env {
            cmd.env("PYTHONPATH", python_env);
            debug!("Set PYTHONPATH to: {}", python_env);
        }

        if !self.config.access_key.is_empty() {
            cmd.env("PICOVOICE_ACCESS_KEY", &self.config.access_key);
            debug!("Set PICOVOICE_ACCESS_KEY environment variable");
        }

        let current_pid = std::process::id();
        apply_cli_runtime_args(&mut cmd, &self.config, log_dir);
        cmd.arg("--parent-pid");
        cmd.arg(current_pid.to_string());
        debug!("Passing --parent-pid {} to Python sidecar", current_pid);

        // Set up stdio
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        debug!("Spawning async sidecar process...");
        let mut child = cmd
            .spawn()
            .context("Failed to spawn async sidecar process")?;

        // Get stdin and stdout handles
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdin handle"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdout handle"))?;

        debug!("Got async stdin and stdout handles");

        // Store stdout first, then wait for ready
        let mut stdout_reader = TokioBufReader::new(stdout);
        self.wait_for_ready_async(&mut stdout_reader).await?;

        // Store handles
        self.stdin = Some(stdin);
        self.stdout = Some(stdout_reader);
        self.process = Some(child);

        debug!("Async sidecar ready successfully");
        Ok(())
    }

    async fn wait_for_ready_async(
        &mut self,
        reader: &mut TokioBufReader<tokio::process::ChildStdout>,
    ) -> Result<()> {
        debug!("Waiting for async sidecar ready signal...");

        let mut line = String::new();
        let timeout_duration = Duration::from_millis(self.config.startup_timeout_ms);

        let result = timeout(timeout_duration, async {
            while reader.read_line(&mut line).await? > 0 {
                let trimmed = line.trim();
                debug!("Async sidecar output: '{}'", trimmed);

                if trimmed == "VOICE_PROCESSOR_READY" {
                    debug!("Async sidecar ready signal received!");
                    return Ok(());
                }

                if trimmed.starts_with("VOICE_PROCESSOR_ERROR") {
                    return Err(anyhow!("Sidecar error: {}", trimmed));
                }

                if let Some(fatal_error) = fatal_error_from_line(trimmed) {
                    return Err(fatal_error);
                }

                line.clear();
            }
            Err(anyhow!("Sidecar process ended before ready"))
        })
        .await;

        match result {
            Ok(Ok(())) => {
                *self.is_ready.lock().unwrap() = true;
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow!("Sidecar startup timeout")),
        }
    }

    pub async fn wait_for_wake_word_async(&mut self) -> Result<WakeWordEvent> {
        if !*self.is_ready.lock().unwrap() {
            return Err(anyhow!("Sidecar not ready"));
        }

        debug!("Waiting for wake word detection (async)...");

        if let Some(stdout) = &mut self.stdout {
            let mut line = String::new();

            // Non-blocking read with timeout
            let timeout_duration = Duration::from_millis(self.config.communication_timeout_ms);
            let result = timeout(timeout_duration, async {
                while stdout.read_line(&mut line).await? > 0 {
                    let trimmed = line.trim();
                    debug!("Async wake word detection output: '{}'", trimmed);

                    if trimmed == "WAKE_WORD_DETECTED" {
                        debug!("Wake word detected (async)!");
                        return Ok(WakeWordEvent::Detected);
                    }

                    if trimmed.starts_with("WAKE_WORD_ERROR") {
                        return Err(anyhow!("Wake word error: {}", trimmed));
                    }

                    if let Some(fatal_error) = fatal_error_from_line(trimmed) {
                        return Err(fatal_error);
                    }

                    line.clear();
                }
                Err(anyhow!("No wake word detected"))
            })
            .await;

            match result {
                Ok(Ok(event)) => Ok(event),
                Ok(Err(e)) => Err(e),
                Err(_) => Ok(WakeWordEvent::Timeout),
            }
        } else {
            Err(anyhow!("No stdout handle available"))
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        debug!("Stopping async sidecar...");

        if let Some(mut child) = self.process.take() {
            // Force kill the process to ensure it's terminated
            // Note: We can't await in a sync method, so we just kill and let the OS clean up
            let _ = child.start_kill();
            debug!("Python process kill signal sent");
        }

        self.stdin = None;
        self.stdout = None;
        *self.is_ready.lock().unwrap() = false;

        debug!("Async sidecar stopped");
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        *self.is_ready.lock().unwrap()
    }
}

impl Drop for AsyncPythonSidecar {
    fn drop(&mut self) {
        debug!("AsyncPythonSidecar dropping - cleaning up Python process");
        let _ = self.stop();
    }
}
