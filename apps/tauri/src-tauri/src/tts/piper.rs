use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use tokio::{io::AsyncWriteExt, process::Command as TokioCommand};
use tracing::{debug, info, warn};

use crate::config::tts::PiperConfig;

#[derive(Debug, Clone)]
pub struct PiperRuntime {
    executable: PathBuf,
    model: PathBuf,
    metadata: PathBuf,
}

impl PiperRuntime {
    pub fn from_config(config: &PiperConfig) -> Self {
        Self {
            executable: PathBuf::from(&config.executable_path),
            model: PathBuf::from(&config.model_path),
            metadata: PathBuf::from(&config.metadata_path),
        }
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn model(&self) -> &Path {
        &self.model
    }

    pub fn metadata(&self) -> &Path {
        &self.metadata
    }
}

pub async fn synthesize_to_file(
    runtime: &PiperRuntime,
    text: &str,
    output_path: &Path,
) -> Result<()> {
    let mut cmd = TokioCommand::new(runtime.executable());
    cmd.arg("--model").arg(runtime.model());
    cmd.arg("--config").arg(runtime.metadata());
    cmd.arg("--output_file").arg(output_path);
    if let Some(model_dir) = runtime.model().parent() {
        let espeak_dir = model_dir.join("espeak-ng-data");
        if espeak_dir.exists() {
            cmd.arg("--espeak_data").arg(espeak_dir);
        }

        let tashkeel_model = model_dir.join("libtashkeel_model.ort");
        if tashkeel_model.exists() {
            cmd.arg("--tashkeel_model").arg(tashkeel_model);
        }
    }
    cmd.kill_on_drop(true);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    info!("Piper synth start (chars={})", text.chars().count());
    debug!(
        "Piper synth args: exec={} model={} metadata={} output={}",
        runtime.executable().to_string_lossy(),
        runtime.model().display(),
        runtime.metadata().display(),
        output_path.display()
    );

    let started_at = Instant::now();
    let mut child = cmd.spawn().with_context(|| {
        format!(
            "Failed to execute Piper at {}",
            runtime.executable().display()
        )
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .await
            .with_context(|| "Failed to write text to Piper stdin".to_string())?;
        stdin
            .write_all(b"\n")
            .await
            .with_context(|| "Failed to terminate Piper stdin".to_string())?;
        stdin
            .shutdown()
            .await
            .with_context(|| "Failed to close Piper stdin".to_string())?;
    }

    let output = child.wait_with_output().await.with_context(|| {
        format!(
            "Failed to wait for Piper process at {}",
            runtime.executable().display()
        )
    })?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        warn!(
            "Piper exited with error status={} stdout='{}' stderr='{}'",
            output.status.code().unwrap_or(-1),
            stdout,
            stderr
        );
        return Err(anyhow!(
            "Piper exited with status {}",
            output.status.code().unwrap_or(-1)
        ));
    }

    let elapsed = started_at.elapsed();
    let metadata = fs::metadata(output_path).with_context(|| {
        format!(
            "Piper reported success but output file is missing: {}",
            output_path.display()
        )
    })?;

    if metadata.len() == 0 {
        warn!(
            "Piper produced empty output file ({} bytes): {}",
            metadata.len(),
            output_path.display()
        );
    } else {
        debug!(
            "Piper wrote {} bytes to {} in {:.2?}",
            metadata.len(),
            output_path.display(),
            elapsed
        );
    }

    Ok(())
}
