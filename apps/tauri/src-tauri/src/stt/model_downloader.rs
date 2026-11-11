use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::config::paths;
use crate::stt::download;
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use tracing::info;

/// Whisper model information
#[derive(Debug, Clone)]
pub struct WhisperModelInfo {
    pub name: String,
    pub filename: String,
    pub size_mb: u32,
}

const MODEL_CATALOG: &[(&str, &str, u32)] = &[
    ("tiny", "ggml-tiny.bin", 75),
    ("tiny-q5_1", "ggml-tiny-q5_1.bin", 31),
    ("tiny-q8_0", "ggml-tiny-q8_0.bin", 42),
    ("tiny.en", "ggml-tiny.en.bin", 75),
    ("tiny.en-q5_1", "ggml-tiny.en-q5_1.bin", 31),
    ("tiny.en-q8_0", "ggml-tiny.en-q8_0.bin", 42),
    ("base", "ggml-base.bin", 142),
    ("base-q5_1", "ggml-base-q5_1.bin", 57),
    ("base-q8_0", "ggml-base-q8_0.bin", 78),
    ("base.en", "ggml-base.en.bin", 142),
    ("base.en-q5_1", "ggml-base.en-q5_1.bin", 57),
    ("base.en-q8_0", "ggml-base.en-q8_0.bin", 78),
    ("small", "ggml-small.bin", 466),
    ("small-q5_1", "ggml-small-q5_1.bin", 181),
    ("small-q8_0", "ggml-small-q8_0.bin", 252),
    ("small.en", "ggml-small.en.bin", 466),
    ("small.en-q5_1", "ggml-small.en-q5_1.bin", 181),
    ("small.en-q8_0", "ggml-small.en-q8_0.bin", 252),
    ("small.en-tdrz", "ggml-small.en-tdrz.bin", 465),
    ("medium", "ggml-medium.bin", 1536),
    ("medium-q5_0", "ggml-medium-q5_0.bin", 514),
    ("medium-q8_0", "ggml-medium-q8_0.bin", 785),
    ("medium.en", "ggml-medium.en.bin", 1536),
    ("medium.en-q5_0", "ggml-medium.en-q5_0.bin", 514),
    ("medium.en-q8_0", "ggml-medium.en-q8_0.bin", 785),
    ("large-v1", "ggml-large-v1.bin", 2900),
    ("large-v2", "ggml-large-v2.bin", 2900),
    ("large-v2-q5_0", "ggml-large-v2-q5_0.bin", 1090),
    ("large-v2-q8_0", "ggml-large-v2-q8_0.bin", 1500),
    ("large-v3", "ggml-large-v3.bin", 2900),
    ("large-v3-q5_0", "ggml-large-v3-q5_0.bin", 1100),
    ("large-v3-q8_0", "ggml-large-v3-q8_0.bin", 1600),
    ("large-v3-turbo", "ggml-large-v3-turbo.bin", 1536),
    ("large-v3-turbo-q5_0", "ggml-large-v3-turbo-q5_0.bin", 547),
    ("large-v3-turbo-q8_0", "ggml-large-v3-turbo-q8_0.bin", 834),
];

impl WhisperModelInfo {
    pub fn get_model_info(model_name: &str) -> Option<Self> {
        MODEL_CATALOG
            .iter()
            .find(|entry| entry.0 == model_name)
            .map(|(name, filename, size_mb)| Self {
                name: (*name).to_string(),
                filename: (*filename).to_string(),
                size_mb: *size_mb,
            })
    }

    pub fn get_model_info_by_filename(filename: &str) -> Option<Self> {
        MODEL_CATALOG
            .iter()
            .find(|entry| entry.1 == filename)
            .map(|(name, filename, size_mb)| Self {
                name: (*name).to_string(),
                filename: (*filename).to_string(),
                size_mb: *size_mb,
            })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelCatalogEntry {
    pub id: String,
    pub filename: String,
    pub disk_mb: u32,
    pub label: String,
    pub path: String,
}

pub fn catalog_entries() -> Vec<ModelCatalogEntry> {
    let base_dir = paths::default_models_dir();
    MODEL_CATALOG
        .iter()
        .map(|(id, filename, size)| ModelCatalogEntry {
            id: (*id).to_string(),
            filename: (*filename).to_string(),
            disk_mb: *size,
            label: friendly_label(id, *size),
            path: base_dir.join(filename).display().to_string(),
        })
        .collect()
}

fn friendly_label(id: &str, size_mb: u32) -> String {
    let tokens: Vec<String> = id
        .split(['-', '.'])
        .filter(|segment| !segment.is_empty())
        .map(readable_token)
        .collect();

    let label = if tokens.is_empty() {
        id.to_string()
    } else {
        tokens.join(" · ")
    };

    format!("{label} ({size_mb} MB)")
}

fn readable_token(token: &str) -> String {
    match token.to_ascii_lowercase().as_str() {
        "en" => "English".to_string(),
        "tdrz" => "TDRZ".to_string(),
        "v1" => "V1".to_string(),
        "v2" => "V2".to_string(),
        "v3" => "V3".to_string(),
        other if other.starts_with('q') => other.to_uppercase(),
        other if other.len() <= 3 => other.to_uppercase(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => other.to_string(),
            }
        }
    }
}

pub struct ModelDownloader {
    models_dir: PathBuf,
}

impl ModelDownloader {
    pub fn new(models_dir: impl AsRef<Path>) -> Result<Self> {
        let models_dir = models_dir.as_ref().to_path_buf();
        fs::create_dir_all(&models_dir).with_context(|| {
            format!(
                "Failed to create models directory: {}",
                models_dir.display()
            )
        })?;

        Ok(Self { models_dir })
    }

    pub fn get_model_path(&self, model_name: &str) -> Result<PathBuf> {
        let info = WhisperModelInfo::get_model_info(model_name)
            .ok_or_else(|| anyhow!("Unknown model: {}", model_name))?;

        Ok(self.models_dir.join(&info.filename))
    }

    pub fn is_model_downloaded(&self, model_name: &str) -> Result<bool> {
        let model_path = self.get_model_path(model_name)?;

        if let Ok(metadata) = fs::metadata(&model_path) {
            const MIN_MODEL_BYTES: u64 = 1_000_000; // 1 MB minimum
            Ok(metadata.len() >= MIN_MODEL_BYTES)
        } else {
            Ok(false)
        }
    }

    pub fn download_model_with_progress<F>(
        &self,
        model_name: &str,
        mut progress: Option<F>,
    ) -> Result<PathBuf>
    where
        F: FnMut(u64, Option<u64>),
    {
        let info = WhisperModelInfo::get_model_info(model_name)
            .ok_or_else(|| anyhow!("Unknown model: {}", model_name))?;

        let model_path = self.models_dir.join(&info.filename);

        // Check if already downloaded
        if self.is_model_downloaded(model_name)? {
            info!("Model already downloaded: {}", model_path.display());
            return Ok(model_path);
        }

        info!(
            "Downloading Whisper model '{}' (~{}MB)...",
            info.name, info.size_mb
        );
        let url_override = std::env::var("WHISPER_MODEL_URL").ok();
        download::download_model(
            &info.filename,
            info.size_mb,
            &model_path,
            url_override.as_deref(),
            progress.as_mut(),
        )
        .with_context(|| format!("Failed to download {}", info.filename))?;

        info!("Model downloaded successfully: {}", model_path.display());

        Ok(model_path)
    }

    pub fn ensure_model_with_progress<F>(
        &self,
        model_name: &str,
        progress: Option<F>,
    ) -> Result<PathBuf>
    where
        F: FnMut(u64, Option<u64>),
    {
        if self.is_model_downloaded(model_name)? {
            self.get_model_path(model_name)
        } else {
            self.download_model_with_progress(model_name, progress)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info() {
        let tiny = WhisperModelInfo::get_model_info("tiny.en").unwrap();
        assert_eq!(tiny.name, "tiny.en");
        assert_eq!(tiny.filename, "ggml-tiny.en.bin");
        let base = WhisperModelInfo::get_model_info("base").unwrap();
        assert_eq!(base.name, "base");
        assert_eq!(base.filename, "ggml-base.bin");

        assert!(WhisperModelInfo::get_model_info("invalid").is_none());
    }
}
