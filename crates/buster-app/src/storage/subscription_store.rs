//! Secure Subscription Registry Store.
//!
//! Features:
//! - Windows DPAPI encryption with purpose `"SubscriptionStore"` and magic header `b"BVPN2"`.
//! - Atomic persistence (write to `.tmp` then rename).
//! - Non-destructive quarantine on errors.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::Utc;
use crate::models::SubscriptionRecord;
use anyhow::{Context, Result};
use buster_windows::dpapi;

const MAGIC: &[u8] = b"BVPN2";
const PURPOSE: &str = "SubscriptionStore";

pub struct SubscriptionStore {
    path: PathBuf,
    gate: Arc<Mutex<()>>,
}

impl SubscriptionStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            gate: Arc::new(Mutex::new(())),
        }
    }

    /// Loads all subscription records.
    pub async fn load(&self) -> Result<Vec<SubscriptionRecord>> {
        let _guard = self.gate.lock().await;

        if !self.path.exists() {
            let json_path = self.path.with_file_name("subscriptions.json");
            if json_path.exists() {
                if let Ok(content) = tokio::fs::read_to_string(&json_path).await {
                    if let Ok(records) = serde_json::from_str::<Vec<SubscriptionRecord>>(&content) {
                        tracing::info!("Loaded {} subscription record(s) from JSON fallback", records.len());
                        return Ok(records);
                    }
                }
            }
            return Ok(Vec::new());
        }

        let content = tokio::fs::read(&self.path)
            .await
            .with_context(|| format!("Failed to read subscription store at {}", self.path.display()))?;

        if content.len() <= MAGIC.len() || !content.starts_with(MAGIC) {
            tracing::warn!("Invalid magic header in subscription store, quarantining");
            self.quarantine("invalid_header").await?;
            return Ok(Vec::new());
        }

        let ciphertext = &content[MAGIC.len()..];
        let plaintext = match dpapi::unprotect(ciphertext, PURPOSE) {
            Ok(pt) => pt,
            Err(e) => {
                tracing::error!("DPAPI decryption failed for subscription store: {}", e);
                self.quarantine("decryption_failed").await?;
                return Ok(Vec::new());
            }
        };

        let records: Vec<SubscriptionRecord> = match serde_json::from_slice(&plaintext) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to parse subscription JSON: {}", e);
                self.quarantine("json_parse_failed").await?;
                return Ok(Vec::new());
            }
        };

        tracing::info!("Loaded {} subscription record(s)", records.len());
        Ok(records)
    }

    /// Atomically persists subscription records.
    pub async fn save(&self, records: &[SubscriptionRecord]) -> Result<()> {
        let _guard = self.gate.lock().await;

        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        let plaintext = serde_json::to_vec(records)
            .context("Failed to serialize subscription records to JSON")?;

        let ciphertext = dpapi::protect(&plaintext, PURPOSE)
            .context("Failed to encrypt subscription records with DPAPI")?;

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

        let json_path = self.path.with_file_name("subscriptions.json");
        if let Ok(json_str) = serde_json::to_string_pretty(records) {
            let _ = tokio::fs::write(&json_path, json_str).await;
        }

        tracing::debug!("Persisted {} subscription record(s)", records.len());
        Ok(())
    }

    async fn quarantine(&self, reason: &str) -> Result<()> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let quarantined = self.path.with_extension(format!("quarantine_{}_{}.bak", reason, timestamp));
        tracing::warn!("Quarantining corrupted subscription store to {}", quarantined.display());
        let _ = tokio::fs::rename(&self.path, &quarantined).await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_subscription_store_dpapi_save_and_load() {
        let dir = std::env::temp_dir().join(format!("buster_test_subs_{}", uuid::Uuid::new_v4()));
        let store = SubscriptionStore::new(dir.join("subscriptions.dat"));

        let record = SubscriptionRecord::new("https://example.com/sub".to_string(), "My Sub".to_string());

        store.save(std::slice::from_ref(&record)).await.expect("save subscriptions");

        let loaded = store.load().await.expect("load subscriptions");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].url, "https://example.com/sub");
        assert_eq!(loaded[0].title, "My Sub");

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
