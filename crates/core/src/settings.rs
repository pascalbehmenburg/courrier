//! Runtime settings sourced from environment variables.
//!
//! Replaces the old Config.toml: per-account settings now live in the
//! database, so all that's left here is process-wide infrastructure
//! (DB path, storage path, encryption key, web bind).

use anyhow::{Context, Result};
use std::path::PathBuf;

const ENV_DB_PATH: &str = "COURRIER_DB_PATH";
const ENV_STORAGE_PATH: &str = "COURRIER_STORAGE_PATH";
const ENV_ENCRYPTION_KEY: &str = "COURRIER_ENCRYPTION_KEY";
const ENV_BIND_ADDR: &str = "COURRIER_BIND_ADDR";
const ENV_FETCH_ON_STARTUP: &str = "COURRIER_FETCH_ON_STARTUP";

const DEFAULT_DB_PATH: &str = "courrier.db";
const DEFAULT_STORAGE_PATH: &str = "emails";
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";

#[derive(Debug, Clone)]
pub struct Settings {
    pub db_path: PathBuf,
    pub storage_path: PathBuf,
    /// 32 raw bytes, base64-decoded from `COURRIER_ENCRYPTION_KEY`.
    pub encryption_key: [u8; 32],
    pub bind_addr: String,
    pub fetch_on_startup: bool,
}

impl Settings {
    pub fn from_env() -> Result<Self> {
        let db_path = std::env::var(ENV_DB_PATH)
            .unwrap_or_else(|_| DEFAULT_DB_PATH.to_string())
            .into();
        let storage_path = std::env::var(ENV_STORAGE_PATH)
            .unwrap_or_else(|_| DEFAULT_STORAGE_PATH.to_string())
            .into();
        let bind_addr =
            std::env::var(ENV_BIND_ADDR).unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string());
        let fetch_on_startup = std::env::var(ENV_FETCH_ON_STARTUP)
            .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(true);
        let encryption_key = load_encryption_key()?;

        Ok(Self {
            db_path,
            storage_path,
            encryption_key,
            bind_addr,
            fetch_on_startup,
        })
    }
}

fn load_encryption_key() -> Result<[u8; 32]> {
    use base64::Engine;
    let raw = std::env::var(ENV_ENCRYPTION_KEY).with_context(|| {
        format!(
            "{ENV_ENCRYPTION_KEY} is required.\n\
             Generate one with:\n  \
             head -c 32 /dev/urandom | base64\n\
             then export it before starting the server."
        )
    })?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .with_context(|| format!("{ENV_ENCRYPTION_KEY} must be valid base64"))?;
    if decoded.len() != 32 {
        anyhow::bail!(
            "{ENV_ENCRYPTION_KEY} must decode to exactly 32 bytes (got {})",
            decoded.len()
        );
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded);
    Ok(key)
}
