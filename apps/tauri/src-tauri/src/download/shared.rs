use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use hf_hub::api::sync::ApiBuilder;
use hf_hub::{Repo, RepoType};
use reqwest::blocking::Client;

pub fn download_file_http<F>(
    url: &str,
    target_path: &Path,
    mut progress: Option<&mut F>,
) -> Result<()>
where
    F: FnMut(u64, Option<u64>),
{
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create directory for download target: {}",
                parent.display()
            )
        })?;
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .context("Failed to initialise HTTP client")?;

    let mut response = client
        .get(url)
        .send()
        .with_context(|| format!("Failed to send download request to {}", url))?;

    response
        .error_for_status_ref()
        .map_err(|e| anyhow!("Download failed with status: {:?}", e.status()))?;

    let total = response.content_length();
    if let Some(cb) = progress.as_deref_mut() {
        cb(0, total);
    }

    let mut file = fs::File::create(target_path)
        .with_context(|| format!("Failed to create {}", target_path.display()))?;

    let mut downloaded = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = response.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        downloaded += read as u64;
        if let Some(cb) = progress.as_deref_mut() {
            cb(downloaded, total);
        }
    }

    file.flush()
        .with_context(|| format!("Failed to flush {}", target_path.display()))?;

    Ok(())
}

pub fn download_file_hf<F>(
    repo_id: &str,
    repo_type: RepoType,
    revision: Option<&str>,
    repo_path: &str,
    target_path: &Path,
    mut progress: Option<&mut F>,
) -> Result<()>
where
    F: FnMut(u64, Option<u64>),
{
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create directory for download target: {}",
                parent.display()
            )
        })?;
    }

    let api = ApiBuilder::default()
        .build()
        .context("Failed to initialise hf-hub client")?;
    let repo_spec = if let Some(rev) = revision {
        Repo::with_revision(repo_id.to_string(), repo_type, rev.to_string())
    } else {
        Repo::new(repo_id.to_string(), repo_type)
    };
    let repo = api.repo(repo_spec);
    let cached_path = repo
        .get(repo_path)
        .with_context(|| format!("Failed to download {repo_path} from hf-hub"))?;

    let total = fs::metadata(&cached_path)
        .with_context(|| format!("Failed to inspect cached asset {}", cached_path.display()))?
        .len();

    if let Some(cb) = progress.as_deref_mut() {
        cb(0, Some(total));
    }

    let mut source = fs::File::open(&cached_path).with_context(|| {
        format!(
            "Failed to open cached asset for reading: {}",
            cached_path.display()
        )
    })?;
    let mut dest = fs::File::create(target_path).with_context(|| {
        format!(
            "Failed to create destination file: {}",
            target_path.display()
        )
    })?;

    let mut copied = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        dest.write_all(&buffer[..read])?;
        copied += read as u64;
        if let Some(cb) = progress.as_deref_mut() {
            cb(copied.min(total), Some(total));
        }
    }
    dest.flush()
        .context("Failed to flush copied asset to destination")?;

    Ok(())
}
