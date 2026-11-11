use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[allow(dead_code)]
#[path = "src/config/defaults.rs"]
mod defaults;
#[path = "src/download/shared.rs"]
mod download_helpers;

fn main() {
    if let Err(err) = ensure_whisper_model() {
        println!("cargo:warning=Failed to prepare Whisper model: {err}");
    }

    if let Err(err) = ensure_webview_favicon() {
        println!("cargo:warning=Failed to stage favicon for webview: {err}");
    }

    emit_build_metadata();

    tauri_build::build()
}

fn ensure_whisper_model() -> Result<(), Box<dyn std::error::Error>> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let model_rel_path = PathBuf::from("resources/whisper").join(defaults::STT_MODEL_FILENAME);
    let model_path = project_root.join(&model_rel_path);

    if let Ok(metadata) = fs::metadata(&model_path) {
        const MIN_MODEL_BYTES: u64 = 1_000_000;
        if metadata.len() >= MIN_MODEL_BYTES {
            return Ok(());
        } else {
            println!(
                "cargo:warning=Existing Whisper model is only {} bytes; re-downloading",
                metadata.len()
            );
        }
    }

    if std::env::var("SKIP_WHISPER_MODEL_DOWNLOAD").is_ok() {
        println!(
            "cargo:warning=Skipping Whisper model download because SKIP_WHISPER_MODEL_DOWNLOAD is set"
        );
        return Ok(());
    }

    let model_meta = resolve_model_metadata(&model_rel_path)?;
    let url_override = std::env::var("WHISPER_MODEL_URL").ok();

    println!(
        "cargo:warning=Ensuring Whisper model {} (~{}MB)",
        model_meta.filename, model_meta.size_mb
    );
    download_whisper_model(
        &model_meta.filename,
        model_meta.size_mb,
        &model_path,
        url_override.as_deref(),
    )?;
    println!(
        "cargo:warning=Whisper model ready at {}",
        model_path.display()
    );

    Ok(())
}

fn download_whisper_model(
    model_filename: &str,
    size_mb: u32,
    target_path: &Path,
    url_override: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    const HF_REPO_ID: &str = "ggerganov/whisper.cpp";

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create directory for model {}: {err}",
                parent.display()
            )
        })?;
    }

    if let Some(url) = url_override {
        download_helpers::download_file_http::<fn(u64, Option<u64>)>(url, target_path, None)
            .map_err(|err| format!("failed to download Whisper model via HTTP: {err}"))?;
    } else {
        download_helpers::download_file_hf::<fn(u64, Option<u64>)>(
            HF_REPO_ID,
            hf_hub::RepoType::Model,
            None,
            model_filename,
            target_path,
            None,
        )
        .map_err(|err| format!("failed to download Whisper model via hf-hub: {err}"))?;
    }

    let bytes = fs::metadata(target_path)
        .map_err(|err| format!("failed to inspect downloaded model: {err}"))?
        .len();
    if bytes < 1_000_000 {
        return Err(format!(
            "downloaded Whisper model is unexpectedly small ({} bytes)",
            bytes
        )
        .into());
    }

    println!(
        "cargo:warning=Model saved to {} (~{} MB, expected ~{} MB)",
        target_path.display(),
        bytes / 1_000_000,
        size_mb
    );

    Ok(())
}

fn ensure_webview_favicon() -> Result<(), Box<dyn std::error::Error>> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let Some(workspace_root) = project_root.parent() else {
        return Err("unable to resolve workspace root for favicon copy".into());
    };

    let source = project_root.join("icons").join("icon.ico");
    if !source.exists() {
        return Err(format!("missing source favicon at {}", source.display()).into());
    }

    let webview_dir = workspace_root.join("webview");
    if !webview_dir.exists() {
        fs::create_dir_all(&webview_dir)
            .map_err(|err| format!("failed to create webview directory: {err}"))?;
    }

    let dest = webview_dir.join("favicon.ico");
    fs::copy(&source, &dest).map_err(|err| {
        format!(
            "failed to copy favicon from {} to {}: {err}",
            source.display(),
            dest.display()
        )
    })?;

    Ok(())
}

fn emit_build_metadata() {
    println!(
        "cargo:rustc-env=APP_BUILD_VERSION={}",
        env!("CARGO_PKG_VERSION")
    );

    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
    {
        if output.status.success() {
            if let Ok(hash) = String::from_utf8(output.stdout) {
                let trimmed = hash.trim();
                if !trimmed.is_empty() {
                    println!("cargo:rustc-env=APP_BUILD_COMMIT={trimmed}");
                }
            }
        }
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=APP_BUILD_TIMESTAMP={timestamp}");
}

struct ModelMetadata {
    filename: String,
    size_mb: u32,
}

fn resolve_model_metadata(model_path: &Path) -> Result<ModelMetadata, Box<dyn std::error::Error>> {
    let filename = model_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("Invalid model filename: {}", model_path.display()))?;

    let model_key = filename
        .strip_prefix("ggml-")
        .and_then(|name| name.strip_suffix(".bin"))
        .unwrap_or("base.en");

    let size_mb = match model_key {
        "tiny" | "tiny.en" => 75,
        "base" | "base.en" => 142,
        "small" | "small.en" => 466,
        "medium" | "medium.en" => 1500,
        other => {
            return Err(format!(
                "Unrecognized Whisper model filename '{}'; update build.rs to support it",
                other
            )
            .into())
        }
    };

    Ok(ModelMetadata {
        filename: filename.to_string(),
        size_mb,
    })
}
