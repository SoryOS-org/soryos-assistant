pub mod application;

use application::{ApplicationLauncher, InstalledApp, LaunchApplicationOptions, LaunchReport};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct IntentState {
    launcher: Arc<Mutex<ApplicationLauncher>>,
}

impl IntentState {
    pub fn hydrate_catalog(&self, apps: Vec<InstalledApp>) {
        if let Ok(mut launcher) = self.launcher.lock() {
            launcher.set_cached_catalog(apps);
        }
    }

    pub fn process_intent(&self, request: IntentRequest) -> Result<IntentResponse, IntentError> {
        let mut launcher = self
            .launcher
            .lock()
            .map_err(|_| IntentError::StatePoisoned)?;

        match request {
            IntentRequest::LaunchApplication {
                query,
                strict,
                refresh_catalog,
                focus,
            } => {
                let options = LaunchApplicationOptions {
                    query,
                    strict_matching: strict,
                    refresh_catalog,
                    request_focus: focus,
                };

                let report = launcher.launch(&options)?;
                Ok(IntentResponse::LaunchApplication { report })
            }
        }
    }

    pub fn process_intents(&self, requests: Vec<IntentRequest>) -> IntentBatchResult {
        let mut results = Vec::with_capacity(requests.len());

        for (index, request) in requests.into_iter().enumerate() {
            match self.process_intent(request) {
                Ok(response) => results.push(IntentBatchEntry {
                    index,
                    response: Some(response),
                    error: None,
                }),
                Err(err) => results.push(IntentBatchEntry {
                    index,
                    response: None,
                    error: Some(err.to_string()),
                }),
            }
        }

        IntentBatchResult { results }
    }

    pub fn refresh_catalog(&self) -> Result<IntentCatalogSnapshot, IntentError> {
        let mut launcher = self
            .launcher
            .lock()
            .map_err(|_| IntentError::StatePoisoned)?;

        launcher.refresh_catalog()?;
        let apps = launcher.catalog_snapshot()?;
        Ok(IntentCatalogSnapshot { applications: apps })
    }

    pub fn catalog_snapshot(&self) -> Result<IntentCatalogSnapshot, IntentError> {
        let mut launcher = self
            .launcher
            .lock()
            .map_err(|_| IntentError::StatePoisoned)?;

        let apps = launcher.catalog_snapshot()?;
        Ok(IntentCatalogSnapshot { applications: apps })
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IntentRequest {
    LaunchApplication {
        query: String,
        #[serde(default)]
        strict: bool,
        #[serde(default)]
        refresh_catalog: bool,
        #[serde(default)]
        focus: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IntentResponse {
    LaunchApplication { report: LaunchReport },
}

#[derive(Debug, Serialize)]
pub struct IntentCatalogSnapshot {
    pub applications: Vec<InstalledApp>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntentBatchResult {
    pub results: Vec<IntentBatchEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntentBatchEntry {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<IntentResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug)]
pub enum IntentError {
    StatePoisoned,
    Application(application::ApplicationError),
}

impl fmt::Display for IntentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntentError::StatePoisoned => write!(f, "intent state is unavailable"),
            IntentError::Application(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for IntentError {}

impl From<application::ApplicationError> for IntentError {
    fn from(value: application::ApplicationError) -> Self {
        IntentError::Application(value)
    }
}
