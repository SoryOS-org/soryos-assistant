use std::{fs, path::Path};

use anyhow::{ensure, Context, Result};
use hf_hub::RepoType;
use tracing::{debug, info};

use crate::download::{download_file_hf, download_file_http};

const HF_REPO_ID: &str = "ggerganov/whisper.cpp";

/// Download the Whisper model using either an override URL or the Hugging Face hub cache.
pub fn download_model<F>(
    model_filename: &str,
    size_mb: u32,
    target_path: &Path,
    url_override: Option<&str>,
    progress: Option<&mut F>,
) -> Result<()>
where
    F: FnMut(u64, Option<u64>),
{
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create directory for model: {}", parent.display())
        })?;
    }

    if let Some(url) = url_override {
        download_file_http(url, target_path, progress)?;
    } else {
        download_file_hf(
            HF_REPO_ID,
            RepoType::Model,
            None,
            model_filename,
            target_path,
            progress,
        )?;
    }

    let bytes = fs::metadata(target_path)
        .with_context(|| {
            format!(
                "Failed to inspect downloaded model {}",
                target_path.display()
            )
        })?
        .len();

    ensure!(
        bytes >= 1_000_000,
        "Downloaded model is unexpectedly small ({} bytes)",
        bytes
    );

    info!(
        "Model saved to {} (~{} MB)",
        target_path.display(),
        bytes / 1_000_000
    );
    debug!("Expected size category ~{} MB", size_mb);

    Ok(())
}
