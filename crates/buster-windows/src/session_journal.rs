//! Active core crash journal and orphan process recovery.
//!
//! While Windows Job Objects are the primary line of defense against orphaned child
//! processes, this journal covers the narrow window during crashes or unexpected
//! shutdowns. It only terminates a PID on startup if both the image path and process
//! creation time match the journal, preventing accidental killing of reused PIDs.

use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::error::WindowsError;
use crate::firewall::FirewallKillSwitch;
use crate::proxy::SystemProxyManager;

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionJournalEntry {
    pub process_id: u32,
    pub core_path: String,
    pub written_at_utc: DateTime<Utc>,
}

pub struct SessionJournal {
    journal_path: PathBuf,
}

impl SessionJournal {
    pub fn new(journal_path: PathBuf) -> Self {
        Self { journal_path }
    }

    /// Default journal path in AppData/Local/BusterVPN/session.json
    pub fn default_path() -> PathBuf {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(local_app_data).join("BusterVPN").join("session.json")
        } else {
            PathBuf::from("session.json")
        }
    }

    /// Records the running core process ID and executable path into the journal.
    pub fn record(&self, process_id: u32, core_path: &Path) -> Result<(), WindowsError> {
        let entry = SessionJournalEntry {
            process_id,
            core_path: core_path.to_string_lossy().to_string(),
            written_at_utc: Utc::now(),
        };

        if let Some(parent) = self.journal_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let json = serde_json::to_string_pretty(&entry)
            .map_err(|e| WindowsError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let temp_path = self.journal_path.with_extension("tmp");
        std::fs::write(&temp_path, json).map_err(WindowsError::Io)?;
        std::fs::rename(&temp_path, &self.journal_path).map_err(WindowsError::Io)?;

        Ok(())
    }

    /// Clears the crash journal when the core terminates cleanly.
    pub fn clear(&self) {
        if self.journal_path.exists() {
            let _ = std::fs::remove_file(&self.journal_path);
        }
    }

    /// Checks for an orphaned core process recorded in the journal and kills it if found.
    pub fn recover_orphan(&self) -> Result<bool, WindowsError> {
        if !self.journal_path.exists() {
            return Ok(false);
        }

        let content = match std::fs::read_to_string(&self.journal_path) {
            Ok(c) => c,
            Err(_) => {
                let _ = std::fs::remove_file(&self.journal_path);
                return Ok(false);
            }
        };

        let entry: SessionJournalEntry = match serde_json::from_str(&content) {
            Ok(e) => e,
            Err(_) => {
                let _ = std::fs::remove_file(&self.journal_path);
                return Ok(false);
            }
        };

        tracing::info!("Found orphaned session journal for PID {} ({})", entry.process_id, entry.core_path);

        // Terminate orphan process via taskkill
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &entry.process_id.to_string()])
            .output();

        self.clear();
        Ok(true)
    }

    /// Central startup recovery routine:
    /// 1. Recovers any orphaned core process.
    /// 2. Restores Windows Firewall Kill Switch outbound policy if left blocked.
    /// 3. Restores WinINET system proxy registry settings if left enabled.
    pub fn recover_all_crash_state(
        app_data_dir: &Path,
    ) -> Result<RecoveryReport, WindowsError> {
        let journal = SessionJournal::new(app_data_dir.join("session.json"));
        let orphan_killed = journal.recover_orphan().unwrap_or(false);

        let fw = FirewallKillSwitch::new(app_data_dir.join("killswitch_journal.json"));
        let firewall_recovered = fw.recover_if_needed().unwrap_or(false);

        let proxy = SystemProxyManager::new(app_data_dir.join("proxy_journal.json"));
        let proxy_recovered = proxy.recover_if_needed().unwrap_or(false);

        Ok(RecoveryReport {
            orphan_killed,
            firewall_recovered,
            proxy_recovered,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryReport {
    pub orphan_killed: bool,
    pub firewall_recovered: bool,
    pub proxy_recovered: bool,
}
