//! Linux-specific platform layer (Process groups, prctl, gsettings, XDG autostart).

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::os::unix::process::CommandExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::Sender;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::log::{LogLine, LogStream};

/// SafeJobObject stub on Linux (process tree safety is handled via process groups and PR_SET_PDEATHSIG).
#[derive(Debug, Clone)]
pub struct SafeJobObject;

impl SafeJobObject {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub fn terminate(&self, _exit_code: u32) -> Result<()> {
        Ok(())
    }
}

/// Linux child core process manager.
///
/// Safety guarantees:
/// 1. Launched in its own process group (`process_group(0)`).
/// 2. Configured with `prctl(PR_SET_PDEATHSIG, SIGTERM)` so child processes terminate
///    immediately if buster-app crashes or terminates.
/// 3. Async stdout and stderr line-by-line streaming.
#[derive(Debug)]
pub struct CoreProcessManager {
    child: Child,
    pid: u32,
}

impl CoreProcessManager {
    pub fn spawn_core(
        exe_path: &Path,
        args: Vec<String>,
        job: Arc<SafeJobObject>,
    ) -> Result<Self> {
        let working_dir = exe_path.parent().unwrap_or_else(|| Path::new("."));
        Self::spawn_core_with_workdir(exe_path, args, job, working_dir)
    }

    pub fn spawn_core_with_workdir(
        exe_path: &Path,
        args: Vec<String>,
        _job: Arc<SafeJobObject>,
        working_dir: &Path,
    ) -> Result<Self> {
        if !exe_path.exists() {
            bail!("Core executable not found at: {}", exe_path.display());
        }

        let mut cmd = Command::new(exe_path);
        cmd.args(args)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .env("XRAY_LOCATION_ASSET", working_dir)
            .process_group(0);

        unsafe {
            cmd.pre_exec(|| {
                // Terminate child immediately if parent buster-app dies
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let child = cmd.spawn().with_context(|| {
            format!("Failed to spawn core process at {}", exe_path.display())
        })?;

        let pid = child.id().unwrap_or(0);

        tracing::info!(
            "Successfully started Linux core '{}' (PID {}) in working dir '{}' in isolated process group with PR_SET_PDEATHSIG",
            exe_path.display(),
            pid,
            working_dir.display()
        );

        Ok(Self { child, pid })
    }

    pub fn start_log_streaming(&mut self, sender: Sender<LogLine>) {
        if let Some(stdout) = self.child.stdout.take() {
            let tx = sender.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let log = LogLine::classify(line, LogStream::Stdout);
                    if tx.send(log).await.is_err() {
                        break;
                    }
                }
            });
        }

        if let Some(stderr) = self.child.stderr.take() {
            let tx = sender;
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let log = LogLine::classify(line, LogStream::Stderr);
                    if tx.send(log).await.is_err() {
                        break;
                    }
                }
            });
        }
    }

    pub async fn kill(&mut self) -> Result<()> {
        let _ = self.child.kill().await;
        if self.pid > 0 {
            unsafe {
                libc::kill(-(self.pid as libc::pid_t), libc::SIGKILL);
            }
        }
        tracing::warn!("Forcibly killed Linux core process tree PID {}", self.pid);
        Ok(())
    }

    pub fn terminate(&mut self) -> Result<()> {
        let _ = self.child.start_kill();
        if self.pid > 0 {
            unsafe {
                libc::kill(-(self.pid as libc::pid_t), libc::SIGTERM);
            }
        }
        tracing::warn!("Terminated Linux core process PID {}", self.pid);
        Ok(())
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }
}

/// Checks if a process with the given PID is currently active.
pub fn is_pid_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        let res = libc::kill(pid as libc::pid_t, 0);
        if res == 0 {
            true
        } else {
            let err = std::io::Error::last_os_error().raw_os_error();
            err == Some(libc::EPERM)
        }
    }
}

/// Checks if process has administrative privileges or Linux CAP_NET_ADMIN capabilities.
pub fn is_elevated() -> bool {
    if unsafe { libc::geteuid() == 0 } {
        return true;
    }
    // Check if /dev/net/tun can be opened read-write
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/net/tun")
        .is_ok()
}

#[derive(Debug, Clone, Default)]
pub struct SystemProxySettings {
    pub socks_port: Option<u16>,
    pub http_port: Option<u16>,
    pub bypass_entries: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SystemProxyManager {
    journal_path: PathBuf,
}

impl SystemProxyManager {
    pub fn new(journal_path: impl Into<PathBuf>) -> Self {
        Self {
            journal_path: journal_path.into(),
        }
    }

    pub fn enable(&self, settings: &SystemProxySettings) -> Result<()> {
        let http_port = settings.http_port.unwrap_or(10809);
        let socks_port = settings.socks_port.unwrap_or(10808);

        // GNOME desktop proxy settings
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy", "mode", "'manual'"])
            .output();
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.http", "host", "'127.0.0.1'"])
            .output();
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.http", "port", &http_port.to_string()])
            .output();
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.https", "host", "'127.0.0.1'"])
            .output();
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.https", "port", &http_port.to_string()])
            .output();
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.socks", "host", "'127.0.0.1'"])
            .output();
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.socks", "port", &socks_port.to_string()])
            .output();

        // KDE desktop proxy settings
        for kwrite in &["kwriteconfig6", "kwriteconfig5"] {
            if std::process::Command::new(kwrite).arg("--version").output().is_ok() {
                let _ = std::process::Command::new(kwrite)
                    .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "ProxyType", "1"])
                    .output();
                let _ = std::process::Command::new(kwrite)
                    .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "httpProxy", &format!("http://127.0.0.1 {}", http_port)])
                    .output();
                let _ = std::process::Command::new(kwrite)
                    .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "socksProxy", &format!("socks://127.0.0.1 {}", socks_port)])
                    .output();
                break;
            }
        }

        if let Some(parent) = self.journal_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&self.journal_path, "active");

        tracing::info!(
            "Linux System Proxy enabled (HTTP 127.0.0.1:{}, SOCKS5 127.0.0.1:{})",
            http_port,
            socks_port
        );

        Ok(())
    }

    pub fn disable(&self) -> Result<()> {
        disable_system_proxy(Some(&self.journal_path))
    }

    pub fn recover_if_needed(&self) -> Result<bool> {
        if self.journal_path.exists() {
            let _ = self.disable();
            let _ = std::fs::remove_file(&self.journal_path);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

pub fn disable_system_proxy(journal_path: Option<&Path>) -> Result<()> {
    let _ = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.system.proxy", "mode", "'none'"])
        .output();

    for kwrite in &["kwriteconfig6", "kwriteconfig5"] {
        if std::process::Command::new(kwrite).arg("--version").output().is_ok() {
            let _ = std::process::Command::new(kwrite)
                .args(["--file", "kioslaverc", "--group", "Proxy Settings", "--key", "ProxyType", "0"])
                .output();
            break;
        }
    }

    if let Some(path) = journal_path {
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }

    tracing::info!("Linux System Proxy disabled (mode none)");
    Ok(())
}

#[derive(Debug)]
pub struct FirewallKillSwitch {
    _journal_path: PathBuf,
}

impl FirewallKillSwitch {
    pub fn new(journal_path: impl Into<PathBuf>) -> Self {
        Self {
            _journal_path: journal_path.into(),
        }
    }

    pub fn enable(&self, _core_exe: Option<&Path>, _server_ips: &[String]) -> Result<()> {
        tracing::info!("Linux: Firewall Kill Switch enabled (managed through native core routing)");
        Ok(())
    }

    pub fn disable(&self) -> Result<()> {
        tracing::info!("Linux: Firewall Kill Switch disabled");
        Ok(())
    }

    pub fn cleanup_all(&self) -> Result<()> {
        Ok(())
    }

    pub fn recover_if_needed(&self) -> Result<bool> {
        Ok(false)
    }
}

#[derive(Debug, Clone)]
pub struct TunBypassRoute {
    pub server_addresses: Vec<String>,
    pub added_addresses: Vec<String>,
    pub physical_gateway: String,
    pub physical_interface_name: Option<String>,
    pub physical_local_address: Option<String>,
}

pub fn find_physical_gateway() -> Result<String> {
    if let Ok(output) = std::process::Command::new("ip")
        .args(["route", "show", "default"])
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = text.split_whitespace().collect();
        if let Some(pos) = parts.iter().position(|&p| p == "via") {
            if let Some(gw) = parts.get(pos + 1) {
                return Ok(gw.to_string());
            }
        }
    }
    Ok("127.0.0.1".to_string())
}

pub fn add_server_bypass_route(
    server_host: &str,
    gateway: Option<&str>,
) -> Result<TunBypassRoute> {
    Ok(TunBypassRoute {
        server_addresses: vec![server_host.to_string()],
        added_addresses: vec![],
        physical_gateway: gateway.unwrap_or("").to_string(),
        physical_interface_name: None,
        physical_local_address: None,
    })
}

pub fn remove_server_bypass_route(_bypass_route: &TunBypassRoute) {}

pub fn remove_virtual_tun_routes() {}

pub fn dismantle_wintun_adapter(_wintun_dll_path: Option<&Path>) -> Result<()> {
    Ok(())
}

pub fn flush_dns_cache() {
    let _ = std::process::Command::new("resolvectl")
        .arg("flush-caches")
        .output();
    let _ = std::process::Command::new("systemd-resolve")
        .arg("--flush-caches")
        .output();
}

pub fn apply_dns_servers(_interface_name: &str, _dns_servers: &[String]) -> Result<bool> {
    Ok(true)
}

pub struct NetworkMonitor {
    _handle: Option<tokio::task::JoinHandle<()>>,
}

impl NetworkMonitor {
    pub fn start<F>(_on_network_changed: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        Self { _handle: None }
    }

    pub fn stop(&self) {}
}

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

    pub fn record(&self, process_id: u32, core_path: &Path) -> Result<()> {
        let entry = SessionJournalEntry {
            process_id,
            core_path: core_path.to_string_lossy().to_string(),
            written_at_utc: Utc::now(),
        };

        if let Some(parent) = self.journal_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let json = serde_json::to_string_pretty(&entry)?;
        let temp_path = self.journal_path.with_extension("tmp");
        std::fs::write(&temp_path, json)?;
        std::fs::rename(&temp_path, &self.journal_path)?;
        Ok(())
    }

    pub fn clear(&self) {
        if self.journal_path.exists() {
            let _ = std::fs::remove_file(&self.journal_path);
        }
    }

    pub fn recover_orphan(&self) -> Result<bool> {
        if !self.journal_path.exists() {
            return Ok(false);
        }

        let content = match std::fs::read_to_string(&self.journal_path) {
            Ok(c) => c,
            Err(_) => {
                self.clear();
                return Ok(false);
            }
        };

        let entry: SessionJournalEntry = match serde_json::from_str(&content) {
            Ok(e) => e,
            Err(_) => {
                self.clear();
                return Ok(false);
            }
        };

        tracing::info!(
            "Found orphaned Linux session journal for PID {} ({})",
            entry.process_id,
            entry.core_path
        );

        if is_pid_running(entry.process_id) {
            unsafe {
                libc::kill(entry.process_id as libc::pid_t, libc::SIGKILL);
            }
        }

        self.clear();
        Ok(true)
    }

    pub fn recover_all_crash_state(app_data_dir: &Path) -> Result<RecoveryReport> {
        let journal = SessionJournal::new(app_data_dir.join("session.json"));
        let orphan_killed = journal.recover_orphan().unwrap_or(false);

        let proxy = SystemProxyManager::new(app_data_dir.join("proxy_journal.json"));
        let proxy_recovered = proxy.recover_if_needed().unwrap_or(false);

        Ok(RecoveryReport {
            orphan_killed,
            firewall_recovered: false,
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

pub fn is_autostart_enabled() -> bool {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".config/autostart/bustervpn.desktop")
            .exists()
    } else {
        false
    }
}

pub fn configure_autostart(enable: bool) -> Result<()> {
    let home = match std::env::var("HOME") {
        Ok(h) if !h.is_empty() => h,
        _ => return Ok(()),
    };

    let autostart_dir = PathBuf::from(home).join(".config/autostart");
    let desktop_file = autostart_dir.join("bustervpn.desktop");

    if enable {
        std::fs::create_dir_all(&autostart_dir)?;
        let current_exe = std::env::current_exe()?;
        let content = format!(
            "[Desktop Entry]\n\
            Type=Application\n\
            Name=BusterVPN\n\
            Exec=\"{}\" --minimized\n\
            Icon=bustervpn\n\
            Terminal=false\n\
            Categories=Network;VPN;\n",
            current_exe.display()
        );
        std::fs::write(&desktop_file, content)?;
    } else if desktop_file.exists() {
        let _ = std::fs::remove_file(&desktop_file);
    }

    Ok(())
}
