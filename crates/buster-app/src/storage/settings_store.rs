//! Persistent application settings store.
//!
//! Stores `AppSettings` as human-readable JSON (`settings.json`).
//! Uses atomic writes (temp file + rename) to avoid half-written files on sudden power loss.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::models::AppSettings;
use anyhow::{Context, Result};

pub struct SettingsStore {
    path: PathBuf,
    gate: Arc<Mutex<()>>,
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            gate: Arc::new(Mutex::new(())),
        }
    }

    /// Loads application settings. If file does not exist, returns defaults.
    pub async fn load(&self) -> Result<AppSettings> {
        let _guard = self.gate.lock().await;

        if !self.path.exists() {
            return Ok(AppSettings::default());
        }

        let content = tokio::fs::read_to_string(&self.path)
            .await
            .with_context(|| format!("Failed to read settings file at {}", self.path.display()))?;

        let settings: AppSettings = serde_json::from_str(&content)
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to deserialize settings, reverting to defaults: {}", e);
                AppSettings::default()
            });

        Ok(settings)
    }

    /// Atomically persists settings to JSON.
    pub async fn save(&self, settings: &AppSettings) -> Result<()> {
        let _guard = self.gate.lock().await;

        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        let json = serde_json::to_string_pretty(settings)
            .context("Failed to serialize AppSettings to JSON")?;

        let temp_path = self.path.with_extension("tmp");
        tokio::fs::write(&temp_path, json.as_bytes())
            .await
            .with_context(|| format!("Failed to write temp file {}", temp_path.display()))?;

        tokio::fs::rename(&temp_path, &self.path)
            .await
            .with_context(|| format!("Failed to rename temp file to {}", self.path.display()))?;

        tracing::debug!("Persisted application settings to {}", self.path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_settings_save_and_load() {
        let dir = std::env::temp_dir().join(format!("buster_test_settings_{}", uuid::Uuid::new_v4()));
        let store = SettingsStore::new(dir.join("settings.json"));

        let settings = AppSettings {
            is_tun_enabled: true,
            kill_switch: true,
            ..Default::default()
        };

        store.save(&settings).await.expect("save settings");

        let loaded = store.load().await.expect("load settings");
        assert!(loaded.is_tun_enabled);
        assert!(loaded.kill_switch);

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
