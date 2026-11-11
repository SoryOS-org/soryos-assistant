use anyhow::{anyhow, Result};
use tauri::App;

#[derive(Clone)]
pub struct IdentityConfig {
    app_user_model_id: String,
}

impl IdentityConfig {
    pub(crate) fn load(app: &App) -> Result<Self> {
        let manifest_id = app.config().identifier.trim().to_owned();

        if manifest_id.is_empty() {
            return Err(anyhow!("tauri.conf.json identifier is missing or empty"));
        }

        Ok(Self {
            app_user_model_id: manifest_id,
        })
    }

    pub fn ensure_runtime_contract(&self) -> Result<()> {
        if self.app_user_model_id.trim().is_empty() {
            Err(anyhow!("AppUserModelId cannot be empty"))
        } else {
            Ok(())
        }
    }

    pub fn app_user_model_id(&self) -> &str {
        &self.app_user_model_id
    }
}
