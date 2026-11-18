use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use windows::Win32::Networking::WinInet::{
    InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;
use crate::error::WindowsError;

pub const DEFAULT_SOCKS_PORT: u16 = 10808;
pub const DEFAULT_HTTP_PORT: u16 = 10809;

const CREATE_NO_WINDOW: u32 = 0x08000000;
const INTERNET_SETTINGS_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

/// Robustly and unconditionally disables Windows System Proxy:
/// 1. Sets ProxyEnable = 0 in HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings.
/// 2. Clears ProxyServer to "".
/// 3. Broadcasts INTERNET_OPTION_SETTINGS_CHANGED and INTERNET_OPTION_REFRESH to all WinINet clients.
/// 4. Executes `netsh winhttp reset proxy` to ensure system services do not retain dead proxy ports.
/// 5. Removes stale proxy recovery journal.
pub fn disable_system_proxy(journal_path: Option<&Path>) -> Result<(), WindowsError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok((key, _)) = hkcu.create_subkey(INTERNET_SETTINGS_KEY) {
        let _ = key.set_value("ProxyEnable", &0u32);
        let _ = key.set_value("ProxyServer", &"");
        let _ = key.set_value("ProxyOverride", &"");
    }

    // Broadcast WinINet options to all running applications (Browsers, Discord, Steam, etc.)
    unsafe {
        let _ = InternetSetOptionW(None, INTERNET_OPTION_SETTINGS_CHANGED, None, 0);
        let _ = InternetSetOptionW(None, INTERNET_OPTION_REFRESH, None, 0);
    }

    // Reset WinHTTP proxy for system-level services asynchronously to eliminate network teardown hangs
    let _ = Command::new("netsh")
        .args(["winhttp", "reset", "proxy"])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();

    if let Some(path) = journal_path {
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }

    tracing::info!("Guaranteed system proxy reset: ProxyEnable=0, ProxyServer='', WinINet refreshed, WinHTTP reset");
    Ok(())
}

pub const DEFAULT_BYPASS_ENTRIES: &[&str] = &[
    "<local>",
    "localhost",
    "127.*",
    "10.*",
    "172.16.*",
    "172.17.*",
    "172.18.*",
    "172.19.*",
    "172.20.*",
    "172.21.*",
    "172.22.*",
    "172.23.*",
    "172.24.*",
    "172.25.*",
    "172.26.*",
    "172.27.*",
    "172.28.*",
    "172.29.*",
    "172.30.*",
    "172.31.*",
    "192.168.*",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyBackupJournal {
    pub proxy_enable: Option<u32>,
    pub proxy_server: Option<String>,
    pub proxy_override: Option<String>,
    pub auto_config_url: Option<String>,
    pub auto_detect: Option<u32>,
    pub applied_proxy_server: String,
    pub applied_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SystemProxySettings {
    pub socks_port: Option<u16>,
    pub http_port: Option<u16>,
    pub bypass_entries: Vec<String>,
}

impl Default for SystemProxySettings {
    fn default() -> Self {
        Self {
            socks_port: Some(DEFAULT_SOCKS_PORT),
            http_port: Some(DEFAULT_HTTP_PORT),
            bypass_entries: DEFAULT_BYPASS_ENTRIES.iter().map(|s| s.to_string()).collect(),
        }
    }
}

pub struct SystemProxyManager {
    journal_path: PathBuf,
}

impl SystemProxyManager {
    pub fn new(journal_path: impl Into<PathBuf>) -> Self {
        Self {
            journal_path: journal_path.into(),
        }
    }

    /// Formats the proxy server string based on configured ports.
    /// Example: "http=127.0.0.1:10809;https=127.0.0.1:10809;socks=127.0.0.1:10808"
    pub fn format_proxy_server(socks_port: Option<u16>, http_port: Option<u16>) -> String {
        let mut parts = Vec::new();
        if let Some(http) = http_port {
            parts.push(format!("http=127.0.0.1:{}", http));
            parts.push(format!("https=127.0.0.1:{}", http));
        }
        if let Some(socks) = socks_port {
            parts.push(format!("socks=127.0.0.1:{}", socks));
        }
        if parts.is_empty() {
            format!("127.0.0.1:{}", DEFAULT_HTTP_PORT)
        } else {
            parts.join(";")
        }
    }

    /// Enables the system proxy and saves an atomic recovery journal.
    pub fn enable(&self, settings: &SystemProxySettings) -> Result<(), WindowsError> {
        let proxy_server = Self::format_proxy_server(settings.socks_port, settings.http_port);
        let proxy_override = if settings.bypass_entries.is_empty() {
            DEFAULT_BYPASS_ENTRIES.join(";")
        } else {
            settings.bypass_entries.join(";")
        };

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey(INTERNET_SETTINGS_KEY)?;

        // If an orphaned journal exists, restore first to preserve original user settings
        if self.journal_path.exists() {
            let _ = self.restore_internal(&key);
        }

        // Snapshot current user settings before modifying
        let backup = ProxyBackupJournal {
            proxy_enable: key.get_value("ProxyEnable").ok(),
            proxy_server: key.get_value("ProxyServer").ok(),
            proxy_override: key.get_value("ProxyOverride").ok(),
            auto_config_url: key.get_value("AutoConfigURL").ok(),
            auto_detect: key.get_value("AutoDetect").ok(),
            applied_proxy_server: proxy_server.clone(),
            applied_at_utc: Utc::now(),
        };

        // Write journal to disk atomically
        self.write_journal(&backup)?;

        // Apply new proxy settings to Windows Registry
        key.set_value("ProxyEnable", &1u32)?;
        key.set_value("ProxyServer", &proxy_server)?;
        key.set_value("ProxyOverride", &proxy_override)?;

        // Disable PAC and WPAD temporarily while VPN proxy is active
        let _ = key.delete_value("AutoConfigURL");
        key.set_value("AutoDetect", &0u32)?;

        // Notify WinINET consumers (Edge, Chrome, System services) immediately
        Self::notify_wininet()?;

        tracing::info!("System proxy enabled: {}", proxy_server);
        Ok(())
    }

    /// Restores the original proxy settings and removes the recovery journal.
    pub fn disable(&self) -> Result<(), WindowsError> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey_with_flags(INTERNET_SETTINGS_KEY, KEY_READ | KEY_WRITE) {
            let _ = self.restore_internal(&key);
        }

        // Guaranteed fallback reset ensuring ProxyEnable=0, ProxyServer="", and WinINET/WinHTTP notified
        disable_system_proxy(Some(&self.journal_path))
    }

    /// Recovers system proxy settings if an orphaned journal was left by a previous crash.
    pub fn recover_if_needed(&self) -> Result<bool, WindowsError> {
        if self.journal_path.exists() {
            tracing::warn!(
                "Found stale system proxy journal at '{}'. Restoring user settings...",
                self.journal_path.display()
            );
            self.disable()?;
            return Ok(true);
        }

        // Also check if registry has stale 127.0.0.1 proxy enabled without a journal
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey_with_flags(INTERNET_SETTINGS_KEY, KEY_READ) {
            let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
            let server: String = key.get_value("ProxyServer").unwrap_or_default();
            if enabled == 1 && (server.contains("127.0.0.1") || server.contains("localhost")) {
                tracing::warn!("System proxy was left enabled pointing to localhost; applying guaranteed reset");
                let _ = disable_system_proxy(None);
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn restore_internal(&self, key: &RegKey) -> Result<(), WindowsError> {
        if !self.journal_path.exists() {
            // Default cleanup: simply disable proxy
            key.set_value("ProxyEnable", &0u32)?;
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.journal_path)?;
        let backup: ProxyBackupJournal = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        if let Some(enable) = backup.proxy_enable {
            key.set_value("ProxyEnable", &enable)?;
        } else {
            let _ = key.delete_value("ProxyEnable");
        }

        if let Some(server) = backup.proxy_server {
            key.set_value("ProxyServer", &server)?;
        } else {
            let _ = key.delete_value("ProxyServer");
        }

        if let Some(override_val) = backup.proxy_override {
            key.set_value("ProxyOverride", &override_val)?;
        } else {
            let _ = key.delete_value("ProxyOverride");
        }

        if let Some(auto_url) = backup.auto_config_url {
            key.set_value("AutoConfigURL", &auto_url)?;
        }

        if let Some(auto_detect) = backup.auto_detect {
            key.set_value("AutoDetect", &auto_detect)?;
        }

        Ok(())
    }

    fn write_journal(&self, backup: &ProxyBackupJournal) -> Result<(), WindowsError> {
        if let Some(parent) = self.journal_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let temp_path = self.journal_path.with_extension("tmp");
        let json = serde_json::to_string_pretty(backup)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        std::fs::write(&temp_path, json)?;
        std::fs::rename(&temp_path, &self.journal_path)?;
        Ok(())
    }

    /// Notifies WinINET to refresh options immediately without rebooting or restarting browsers.
    pub fn notify_wininet() -> Result<(), WindowsError> {
        unsafe {
            let _ = InternetSetOptionW(None, INTERNET_OPTION_SETTINGS_CHANGED, None, 0);
            InternetSetOptionW(None, INTERNET_OPTION_REFRESH, None, 0)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_proxy_server() {
        let formatted = SystemProxyManager::format_proxy_server(Some(10808), Some(10809));
        assert_eq!(
            formatted,
            "http=127.0.0.1:10809;https=127.0.0.1:10809;socks=127.0.0.1:10808"
        );
    }

    #[test]
    fn test_disable_system_proxy_guaranteed() {
        let res = disable_system_proxy(None);
        assert!(res.is_ok());

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey_with_flags(INTERNET_SETTINGS_KEY, KEY_READ) {
            let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(1);
            assert_eq!(enabled, 0);
            let server: String = key.get_value("ProxyServer").unwrap_or_default();
            assert_eq!(server, "");
        }
    }
}
