use anyhow::Context;
use keyring::{Entry, Error as KeyringError};
use tracing::warn;

const SERVICE_NAME: &str = "sprite_assistant";
const PICOVOICE_USERNAME: &str = "picovoice_access_key";
const WORKER_AUTH_USERNAME: &str = "worker_auth_token";

#[derive(Debug, Clone, Copy)]
pub enum SecretKey {
    Picovoice,
    WorkerAuth,
}

impl SecretKey {
    fn username(self) -> &'static str {
        match self {
            SecretKey::Picovoice => PICOVOICE_USERNAME,
            SecretKey::WorkerAuth => WORKER_AUTH_USERNAME,
        }
    }
}

fn load_secret(key: SecretKey) -> Option<String> {
    Entry::new(SERVICE_NAME, key.username())
        .ok()
        .and_then(|entry| match entry.get_password() {
            Ok(value) if !value.trim().is_empty() => Some(value),
            Ok(_) => None,
            Err(KeyringError::NoEntry) => None,
            Err(err) => {
                warn!("Failed to read {:?} from secure storage: {err}", key);
                None
            }
        })
}

fn has_secret(key: SecretKey) -> bool {
    load_secret(key).is_some()
}

fn store_secret(key: SecretKey, value: &str) -> anyhow::Result<()> {
    Entry::new(SERVICE_NAME, key.username())
        .context("failed to create keyring entry")?
        .set_password(value)
        .context("failed to write secret to secure storage")
}

fn clear_secret(key: SecretKey) -> anyhow::Result<()> {
    Entry::new(SERVICE_NAME, key.username())
        .context("failed to create keyring entry")?
        .delete_password()
        .or_else(|err| {
            // If it's already gone, treat as success
            if matches!(err, keyring::Error::NoEntry) {
                Ok(())
            } else {
                Err(err)
            }
        })
        .context("failed to clear secret from secure storage")
}

pub fn masked_hint(value: &str) -> String {
    if value.len() <= 4 {
        "****".to_string()
    } else {
        let tail_len = 4usize.min(value.len());
        let tail = &value[value.len() - tail_len..];
        let mask_len = value.len().saturating_sub(tail_len);
        let mask = "*".repeat(mask_len);
        format!("{mask}{tail}")
    }
}

pub fn load_picovoice_key() -> Option<String> {
    load_secret(SecretKey::Picovoice)
}

pub fn has_picovoice_key() -> bool {
    has_secret(SecretKey::Picovoice)
}

pub fn store_picovoice_key(value: &str) -> anyhow::Result<()> {
    store_secret(SecretKey::Picovoice, value)
}

pub fn clear_picovoice_key() -> anyhow::Result<()> {
    clear_secret(SecretKey::Picovoice)
}

pub fn load_worker_auth_token() -> Option<String> {
    load_secret(SecretKey::WorkerAuth)
}

pub fn store_worker_auth_token(value: &str) -> anyhow::Result<()> {
    store_secret(SecretKey::WorkerAuth, value)
}

pub fn clear_worker_auth_token() -> anyhow::Result<()> {
    clear_secret(SecretKey::WorkerAuth)
}
