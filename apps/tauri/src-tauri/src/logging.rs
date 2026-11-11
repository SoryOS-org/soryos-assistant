use std::sync::{Once, OnceLock};

use anyhow::anyhow;
use tauri::AppHandle;
use tracing::level_filters::LevelFilter;
use tracing_log::LogTracer;
use tracing_subscriber::{
    filter::Directive, fmt, layer::SubscriberExt, registry::Registry, reload,
    util::SubscriberInitExt, EnvFilter,
};

#[cfg(not(debug_assertions))]
use tracing_appender::rolling::{RollingFileAppender, Rotation};

use crate::config::AppConfig;

static LOG_INIT: Once = Once::new();
static LOG_FILTER_HANDLE: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

fn level_filter_from_str(value: &str) -> Option<LevelFilter> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "trace" => Some(LevelFilter::TRACE),
        "debug" => Some(LevelFilter::DEBUG),
        "info" => Some(LevelFilter::INFO),
        "warn" | "warning" | "warnings" => Some(LevelFilter::WARN),
        "error" | "errors" => Some(LevelFilter::ERROR),
        _ => None,
    }
}

fn level_filter_str(level: LevelFilter) -> &'static str {
    match level {
        LevelFilter::OFF => "off",
        LevelFilter::ERROR => "error",
        LevelFilter::WARN => "warn",
        LevelFilter::INFO => "info",
        LevelFilter::DEBUG => "debug",
        LevelFilter::TRACE => "trace",
    }
}

fn build_env_filter(default_level: LevelFilter) -> EnvFilter {
    let mut filter = EnvFilter::new(level_filter_str(default_level));

    for directive in ["reqwest=warn", "hf_hub=warn"] {
        if let Ok(parsed) = directive.parse::<Directive>() {
            filter = filter.add_directive(parsed);
        }
    }

    if let Ok(parsed) =
        format!("whisper_rs={}", level_filter_str(default_level)).parse::<Directive>()
    {
        filter = filter.add_directive(parsed);
    }
    if let Ok(parsed) = format!("ggml={}", level_filter_str(default_level)).parse::<Directive>() {
        filter = filter.add_directive(parsed);
    }
    if let Ok(parsed) = format!("mcp_host={}", level_filter_str(default_level)).parse::<Directive>()
    {
        filter = filter.add_directive(parsed);
    }

    filter
}

pub(crate) fn init_logging(app_handle: &AppHandle) {
    LOG_INIT.call_once({
        let handle_clone = app_handle.clone();
        move || {
            if let Err(err) = initialise_subscriber(&handle_clone) {
                eprintln!("Failed to initialise logging: {err:?}");
            }
        }
    });
}

fn initialise_subscriber(app_handle: &AppHandle) -> anyhow::Result<()> {
    let filter = build_env_filter(LevelFilter::INFO);
    let (filter_layer, reload_handle) = reload::Layer::new(filter);

    #[cfg(debug_assertions)]
    let _ = app_handle;

    #[cfg(debug_assertions)]
    {
        let fmt_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .with_target(false)
            .with_ansi(true);

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .try_init()
            .map_err(|err| anyhow!("Failed to install debug log subscriber: {err}"))?;
    }

    #[cfg(not(debug_assertions))]
    {
        let log_dir = crate::config::prelude::logs_dir_from_handle(app_handle);
        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("app")
            .filename_suffix("log")
            .build(&log_dir)
            .map_err(|err| {
                anyhow!(
                    "Failed to create rolling file appender in {}: {err}",
                    log_dir.display()
                )
            })?;

        let file_layer = fmt::layer()
            .with_writer(file_appender)
            .with_ansi(false)
            .with_timer(fmt::time::UtcTime::rfc_3339())
            .with_target(false);

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(file_layer)
            .try_init()
            .map_err(|err| anyhow!("Failed to initialize tracing subscriber: {err}"))?;
    }

    let _ = LOG_FILTER_HANDLE.set(reload_handle);

    if LogTracer::init().is_err() {
        eprintln!("Tracing already configured for log crate; continuing");
    }

    Ok(())
}

pub(crate) fn update_runtime_log_filter(level: LevelFilter) {
    if let Some(handle) = LOG_FILTER_HANDLE.get() {
        if let Err(err) = handle.reload(build_env_filter(level)) {
            eprintln!("Failed to apply telemetry log level {level:?}: {err}");
        }
    }
}

pub(crate) fn telemetry_level_from_state(app_config: &AppConfig) -> Option<LevelFilter> {
    let stt = app_config.stt();
    stt.read()
        .ok()
        .and_then(|cfg| level_filter_from_str(&cfg.telemetry.level))
}
