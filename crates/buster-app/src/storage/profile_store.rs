//! Secure Server Profile Store.
//!
//! Features:
//! - Windows DPAPI encryption (`CryptProtectData`) with magic header `b"BVPN1"`.
//! - Atomic persistence (write to `.tmp` then rename).
//! - Non-destructive quarantine on corruption.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::Utc;
use buster_core::models::ServerProfile;
use anyhow::{Context, Result};
use buster_windows::dpapi;

const MAGIC: &[u8] = b"BVPN1";
const PURPOSE: &str = "ProfileStore";

pub struct ProfileStore {
    path: PathBuf,
    gate: Arc<Mutex<()>>,
}

impl ProfileStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            gate: Arc::new(Mutex::new(())),
        }
    }

    /// Loads all server profiles from the store. Returns an empty vector if the file does not exist.
    pub async fn load(&self) -> Result<Vec<ServerProfile>> {
        let _guard = self.gate.lock().await;

        if !self.path.exists() {
            let json_path = self.path.with_file_name("profiles.json");
            if json_path.exists() {
                if let Ok(content) = tokio::fs::read_to_string(&json_path).await {
                    if let Ok(profiles) = serde_json::from_str::<Vec<ServerProfile>>(&content) {
                        tracing::info!("Loaded {} server profile(s) from JSON fallback", profiles.len());
                        return Ok(profiles);
                    }
                }
            }
            return Ok(Vec::new());
        }

        let content = tokio::fs::read(&self.path)
            .await
            .with_context(|| format!("Failed to read profile store at {}", self.path.display()))?;

        if content.len() <= MAGIC.len() || !content.starts_with(MAGIC) {
            tracing::warn!("Invalid magic header in profile store, quarantining file");
            self.quarantine("invalid_header").await?;
            return Ok(Vec::new());
        }

        let ciphertext = &content[MAGIC.len()..];
        let plaintext = match dpapi::unprotect(ciphertext, PURPOSE) {
            Ok(pt) => pt,
            Err(e) => {
                tracing::error!("DPAPI decryption failed for profile store: {}", e);
                self.quarantine("decryption_failed").await?;
                return Ok(Vec::new());
            }
        };

        let profiles: Vec<ServerProfile> = match serde_json::from_slice(&plaintext) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Failed to parse profiles JSON: {}", e);
                self.quarantine("json_parse_failed").await?;
                return Ok(Vec::new());
            }
        };

        let valid_profiles: Vec<ServerProfile> = profiles
            .into_iter()
            .filter(|p| {
                if let Err(err) = p.validate() {
                    tracing::warn!("Skipping invalid profile '{}': {}", p.name, err);
                    false
                } else {
                    true
                }
            })
            .collect();

        tracing::info!("Loaded {} valid server profile(s)", valid_profiles.len());
        Ok(valid_profiles)
    }

    /// Atomically persists server profiles.
    pub async fn save(&self, profiles: &[ServerProfile]) -> Result<()> {
        let _guard = self.gate.lock().await;

        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        let plaintext = serde_json::to_vec(profiles)
            .context("Failed to serialize server profiles to JSON")?;

        let ciphertext = dpapi::protect(&plaintext, PURPOSE)
            .context("Failed to encrypt server profiles with DPAPI")?;

        let mut payload = Vec::with_capacity(MAGIC.len() + ciphertext.len());
        payload.extend_from_slice(MAGIC);
        payload.extend_from_slice(&ciphertext);

        let temp_path = self.path.with_extension("tmp");
        tokio::fs::write(&temp_path, &payload)
            .await
            .with_context(|| format!("Failed to write temp file {}", temp_path.display()))?;

        tokio::fs::rename(&temp_path, &self.path)
            .await
            .with_context(|| format!("Failed to rename temp file to {}", self.path.display()))?;

        let json_path = self.path.with_file_name("profiles.json");
        if let Ok(json_str) = serde_json::to_string_pretty(profiles) {
            let _ = tokio::fs::write(&json_path, json_str).await;
        }

        tracing::debug!("Persisted {} profile(s) securely", profiles.len());
        Ok(())
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    async fn quarantine(&self, reason: &str) -> Result<()> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let quarantined = self.path.with_extension(format!("quarantine_{}_{}.bak", reason, timestamp));
        tracing::warn!("Quarantining corrupted profile store to {}", quarantined.display());
        let _ = tokio::fs::rename(&self.path, &quarantined).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use buster_core::models::BackendKind;

    #[tokio::test]
    async fn test_profile_store_dpapi_save_and_load() {
        let dir = std::env::temp_dir().join(format!("buster_test_profiles_{}", uuid::Uuid::new_v4()));
        let store = ProfileStore::new(dir.join("profiles.dat"));

        let mut profile = ServerProfile::new("Test Node", BackendKind::Xray, "example.com", 443);
        profile.raw_link = Some("vless://test@example.com:443?security=none#Test".to_string());

        store.save(&[profile.clone()]).await.expect("save profiles");

        let loaded = store.load().await.expect("load profiles");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Test Node");
        assert_eq!(loaded[0].port, 443);

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
