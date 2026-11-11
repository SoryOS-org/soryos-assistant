use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::{
    fmt, io,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstalledApp {
    pub name: String,
    pub app_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchWindowPreference {
    Foreground,
    Background,
}

impl LaunchWindowPreference {
    fn is_background(self) -> bool {
        matches!(self, LaunchWindowPreference::Background)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchStrategy {
    AppUserModelIdExact,
    AppUserModelIdFuzzy,
    AppUserModelIdDirect,
    DirectExecutablePath,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchReport {
    pub strategy: LaunchStrategy,
    pub resolved_app: Option<InstalledApp>,
    pub window_preference: LaunchWindowPreference,
}

#[derive(Debug, Clone)]
pub struct LaunchApplicationOptions {
    pub query: String,
    pub strict_matching: bool,
    pub refresh_catalog: bool,
    pub request_focus: bool,
}

impl LaunchApplicationOptions {
    pub fn trimmed_query(&self) -> &str {
        self.query.trim()
    }

    pub fn window_preference(&self) -> LaunchWindowPreference {
        if self.request_focus {
            LaunchWindowPreference::Foreground
        } else {
            LaunchWindowPreference::Background
        }
    }
}

#[derive(Default)]
pub struct ApplicationLauncher {
    catalog: Option<Vec<InstalledApp>>,
}

impl ApplicationLauncher {
    pub fn launch(
        &mut self,
        options: &LaunchApplicationOptions,
    ) -> Result<LaunchReport, ApplicationError> {
        #[cfg(target_os = "windows")]
        {
            self.launch_on_windows(options)
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = options;
            Err(ApplicationError::UnsupportedPlatform)
        }
    }

    pub fn refresh_catalog(&mut self) -> Result<(), ApplicationError> {
        #[cfg(target_os = "windows")]
        {
            let mut apps = enumerate_start_menu_apps()?;
            normalise_catalog(&mut apps);
            self.catalog = Some(apps);
            Ok(())
        }

        #[cfg(not(target_os = "windows"))]
        {
            self.catalog = None;
            Err(ApplicationError::UnsupportedPlatform)
        }
    }

    pub fn catalog_snapshot(&mut self) -> Result<Vec<InstalledApp>, ApplicationError> {
        if self.catalog.is_none() {
            let _ = self.refresh_catalog();
        }

        Ok(self.catalog.as_ref().cloned().unwrap_or_else(Vec::new))
    }

    pub fn set_cached_catalog(&mut self, mut apps: Vec<InstalledApp>) {
        normalise_catalog(&mut apps);
        self.catalog = Some(apps);
    }

    #[cfg(target_os = "windows")]
    fn launch_on_windows(
        &mut self,
        options: &LaunchApplicationOptions,
    ) -> Result<LaunchReport, ApplicationError> {
        let query = options.trimmed_query();
        let preference = options.window_preference();

        if query.is_empty() {
            return Err(ApplicationError::EmptyQuery);
        }

        if let Some(path) = detect_launchable_path(query) {
            launch_direct_path(&path, preference)?;
            return Ok(LaunchReport {
                strategy: LaunchStrategy::DirectExecutablePath,
                resolved_app: None,
                window_preference: preference,
            });
        }

        if looks_like_app_id(query) {
            launch_with_app_user_model_id(extract_app_id(query), preference)?;
            return Ok(LaunchReport {
                strategy: LaunchStrategy::AppUserModelIdDirect,
                resolved_app: None,
                window_preference: preference,
            });
        }

        if options.refresh_catalog || self.catalog.is_none() {
            self.refresh_catalog()?;
        }

        let query_lower = query.to_ascii_lowercase();
        let mut matched: Option<InstalledApp> = self.catalog.as_ref().and_then(|apps| {
            apps.iter()
                .find(|app| app.name.eq_ignore_ascii_case(query))
                .cloned()
        });

        let mut strategy = LaunchStrategy::AppUserModelIdExact;

        if matched.is_none() && !options.strict_matching {
            matched = self.catalog.as_ref().and_then(|apps| {
                // Try multiple fuzzy matching strategies
                // 1. Contains match (existing)
                apps.iter()
                    .find(|app| app.name.to_ascii_lowercase().contains(&query_lower))
                    .or_else(|| {
                        // 2. Word boundary match - app name contains query as a complete word
                        apps.iter().find(|app| {
                            let app_lower = app.name.to_ascii_lowercase();
                            app_lower.contains(&format!(" {} ", query_lower))
                                || app_lower.starts_with(&format!("{} ", query_lower))
                                || app_lower.ends_with(&format!(" {}", query_lower))
                        })
                    })
                    .cloned()
            });

            if matched.is_some() {
                strategy = LaunchStrategy::AppUserModelIdFuzzy;
            }
        }

        if let Some(app) = matched.clone() {
            launch_catalog_entry(&app, preference)?;
            return Ok(LaunchReport {
                strategy,
                resolved_app: Some(app),
                window_preference: preference,
            });
        }

        // If no app found in catalog, reject the request instead of trying Start-Process
        // This prevents launching invalid app names
        Err(ApplicationError::NoMatch(query.to_string()))
    }
}

#[derive(Debug)]
pub enum ApplicationError {
    #[cfg(not(target_os = "windows"))]
    UnsupportedPlatform,
    EmptyQuery,
    NoMatch(String),
    CatalogUnavailable(String),
    PowershellFailure(String),
    Io(io::Error),
    Parse(serde_json::Error),
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(not(target_os = "windows"))]
            ApplicationError::UnsupportedPlatform => {
                write!(f, "application launching is only supported on Windows")
            }
            ApplicationError::EmptyQuery => write!(f, "launch query cannot be empty"),
            ApplicationError::NoMatch(query) => {
                write!(f, "no installed application matched '{query}'")
            }
            ApplicationError::CatalogUnavailable(message) => {
                write!(f, "failed to load application catalog: {message}")
            }
            ApplicationError::PowershellFailure(message) => {
                write!(f, "PowerShell reported an error: {message}")
            }
            ApplicationError::Io(err) => write!(f, "I/O error: {err}"),
            ApplicationError::Parse(err) => write!(f, "failed to parse catalog data: {err}"),
        }
    }
}

impl std::error::Error for ApplicationError {}

impl From<io::Error> for ApplicationError {
    fn from(value: io::Error) -> Self {
        ApplicationError::Io(value)
    }
}

impl From<serde_json::Error> for ApplicationError {
    fn from(value: serde_json::Error) -> Self {
        ApplicationError::Parse(value)
    }
}

#[cfg(target_os = "windows")]
fn detect_launchable_path(query: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(query);
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
fn detect_launchable_path(_query: &str) -> Option<PathBuf> {
    None
}

#[cfg(target_os = "windows")]
fn launch_catalog_entry(
    app: &InstalledApp,
    preference: LaunchWindowPreference,
) -> Result<(), ApplicationError> {
    if let Some(app_id) = app.app_id.as_deref() {
        launch_with_app_user_model_id(app_id, preference)
    } else {
        launch_via_start_process(&app.name, preference)
    }
}

#[cfg(target_os = "windows")]
fn launch_direct_path(
    path: &Path,
    preference: LaunchWindowPreference,
) -> Result<(), ApplicationError> {
    let target = path.to_string_lossy();
    launch_via_start_process(&target, preference)
}

#[cfg(target_os = "windows")]
fn launch_via_start_process(
    target: &str,
    preference: LaunchWindowPreference,
) -> Result<(), ApplicationError> {
    let mut script = format!("Start-Process {}", quote_for_powershell(target));
    if preference.is_background() {
        script.push_str(" -WindowStyle Hidden");
    }

    run_powershell(&script)
        .map(|_| ())
        .map_err(|err| match err {
            PowershellExecutionError::Spawn(spawn_err) => ApplicationError::Io(spawn_err),
            PowershellExecutionError::Failure(message) => {
                ApplicationError::PowershellFailure(message)
            }
        })
}

#[cfg(target_os = "windows")]
fn launch_with_app_user_model_id(
    app_id: &str,
    preference: LaunchWindowPreference,
) -> Result<(), ApplicationError> {
    let app_id = extract_app_id(app_id);
    let shell_arg = format!("shell:AppsFolder\\{}", app_id);
    launch_via_start_process(&shell_arg, preference)
}

#[cfg(target_os = "windows")]
fn looks_like_app_id(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.contains('!') || trimmed.starts_with("shell:AppsFolder\\")
}

#[cfg(target_os = "windows")]
fn extract_app_id(value: &str) -> &str {
    value
        .trim()
        .strip_prefix("shell:AppsFolder\\")
        .unwrap_or(value.trim())
}

#[cfg(target_os = "windows")]
fn enumerate_start_menu_apps() -> Result<Vec<InstalledApp>, ApplicationError> {
    let script = "Get-StartApps | Select-Object Name, AppID | ConvertTo-Json -Depth 2";
    let output = run_powershell(script).map_err(|err| match err {
        PowershellExecutionError::Spawn(spawn_err) => ApplicationError::Io(spawn_err),
        PowershellExecutionError::Failure(message) => ApplicationError::CatalogUnavailable(message),
    })?;

    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let value: serde_json::Value = serde_json::from_str(trimmed)?;
    let entries = match value {
        serde_json::Value::Array(entries) => entries,
        serde_json::Value::Object(_) => vec![value],
        _ => return Ok(Vec::new()),
    };

    let mut results = Vec::with_capacity(entries.len());
    for entry in entries {
        let raw: RawStartApp = serde_json::from_value(entry)?;
        results.push(InstalledApp {
            name: raw.name,
            app_id: raw.app_id.filter(|app_id| !app_id.is_empty()),
        });
    }

    Ok(results)
}

#[cfg(target_os = "windows")]
#[derive(Deserialize)]
struct RawStartApp {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "AppID")]
    #[serde(default)]
    app_id: Option<String>,
}

fn normalise_catalog(apps: &mut Vec<InstalledApp>) {
    apps.sort();
    apps.dedup();
}

#[cfg(target_os = "windows")]
fn quote_for_powershell(input: &str) -> String {
    let escaped = input.replace("'", "''");
    format!("'{}'", escaped)
}

#[cfg(target_os = "windows")]
fn powershell_candidates() -> &'static [&'static str] {
    static CANDIDATES: OnceLock<Vec<&'static str>> = OnceLock::new();
    let list = CANDIDATES.get_or_init(|| vec!["powershell.exe", "pwsh.exe"]);
    list.as_slice()
}

#[cfg(target_os = "windows")]
fn run_powershell(script: &str) -> Result<String, PowershellExecutionError> {
    let mut spawn_error: Option<io::Error> = None;

    for candidate in powershell_candidates() {
        let mut command = Command::new(candidate);
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ]);
        #[cfg(target_os = "windows")]
        {
            command.creation_flags(0x08000000);
        }

        match command.output() {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    return Ok(stdout);
                }

                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                return Err(PowershellExecutionError::Failure(if stderr.is_empty() {
                    "PowerShell execution failed".to_string()
                } else {
                    stderr
                }));
            }
            Err(err) => {
                spawn_error = Some(err);
                continue;
            }
        }
    }

    Err(PowershellExecutionError::Spawn(spawn_error.unwrap_or_else(
        || {
            io::Error::new(
                io::ErrorKind::NotFound,
                "no PowerShell executable available in PATH",
            )
        },
    )))
}

#[cfg(target_os = "windows")]
enum PowershellExecutionError {
    Spawn(io::Error),
    Failure(String),
}
