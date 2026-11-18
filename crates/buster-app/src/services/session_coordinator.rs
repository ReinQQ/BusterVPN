//! Central connection lifecycle orchestrator for BusterVPN.
//!
//! Manages:
//! - Connection state transitions (Disconnected -> Connecting -> Connected -> Disconnecting)
//! - Process isolation & crash management (Win32 Job Object on Windows, Process groups + PR_SET_PDEATHSIG on Linux)
//! - Physical gateway discovery & host bypass route pinning
//! - Windows Firewall Kill Switch activation/rollback
//! - System proxy switcher (WinINET registry on Windows, GNOME/KDE on Linux)
//! - Real-time stdout/stderr log capturing
//! - Crash journal recording & startup recovery

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::{DateTime, Utc};
use anyhow::{bail, Context, Result};

use buster_core::config::hysteria_builder::{build_hysteria_config, HysteriaBuildParams};
use buster_core::config::xray_builder::{build_xray_config_json, XrayBuildParams};
use buster_core::models::{BackendKind, ConnectionStatus, ServerProfile};
use buster_core::parsing::parse_share_link;

use crate::models::AppSettings;
use crate::services::UpdaterService;

#[cfg(target_os = "windows")]
use buster_windows::{
    add_server_bypass_route, allocate_proxy_pair, apply_dns_servers, find_physical_gateway,
    is_elevated, is_pid_running, proxy::SystemProxySettings, remove_server_bypass_route,
    resolve_server_ipv4, CoreProcessManager, FirewallKillSwitch, LogLine, NetworkMonitor,
    SafeJobObject, SessionJournal, SystemProxyManager, TunBypassRoute,
};

#[cfg(not(target_os = "windows"))]
use crate::platform::{
    add_server_bypass_route, allocate_proxy_pair, apply_dns_servers, find_physical_gateway,
    is_elevated, is_pid_running, remove_server_bypass_route, resolve_server_ipv4,
    CoreProcessManager, FirewallKillSwitch, LogLine, NetworkMonitor, SafeJobObject,
    SessionJournal, SystemProxyManager, SystemProxySettings, TunBypassRoute,
};

pub struct CoordinatorState {
    pub status: ConnectionStatus,
    pub active_profile: Option<ServerProfile>,
    pub connected_at: Option<DateTime<Utc>>,
    pub active_core: Option<CoreProcessManager>,
    pub active_bypass_route: Option<TunBypassRoute>,
    pub recent_logs: Vec<LogLine>,
}

pub struct SessionCoordinator {
    app_data_dir: PathBuf,
    state: Arc<Mutex<CoordinatorState>>,
    proxy_manager: Arc<SystemProxyManager>,
    kill_switch: Arc<FirewallKillSwitch>,
    session_journal: Arc<SessionJournal>,
    network_monitor: Arc<Mutex<Option<NetworkMonitor>>>,
    #[allow(dead_code)]
    active_settings: Arc<Mutex<Option<AppSettings>>>,
}

impl SessionCoordinator {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let proxy_manager = Arc::new(SystemProxyManager::new(
            app_data_dir.join("proxy_journal.json"),
        ));
        let kill_switch = Arc::new(FirewallKillSwitch::new(
            app_data_dir.join("killswitch_journal.json"),
        ));
        let session_journal = Arc::new(SessionJournal::new(
            app_data_dir.join("session.json"),
        ));

        let state = Arc::new(Mutex::new(CoordinatorState {
            status: ConnectionStatus::Disconnected,
            active_profile: None,
            connected_at: None,
            active_core: None,
            active_bypass_route: None,
            recent_logs: Vec::with_capacity(400),
        }));

        Self {
            app_data_dir,
            state,
            proxy_manager,
            kill_switch,
            session_journal,
            network_monitor: Arc::new(Mutex::new(None)),
            active_settings: Arc::new(Mutex::new(None)),
        }
    }

    /// Recovers from abnormal shutdown on application startup.
    pub fn startup_recovery(&self) {
        #[cfg(target_os = "windows")]
        {
            if let Err(e) = SessionJournal::recover_all_crash_state(&self.app_data_dir) {
                tracing::warn!("Startup crash state recovery failed: {}", e);
            }

            // 1. Guaranteed Windows System Proxy disable & WinINet / WinHTTP reset
            let proxy_journal = self.app_data_dir.join("proxy_journal.json");
            let _ = buster_windows::disable_system_proxy(Some(&proxy_journal));

            // 2. Remove any lingering virtual TUN routes (0.0.0.0/1, 128.0.0.0/1)
            buster_windows::remove_virtual_tun_routes();

            // 3. Dismantle any orphaned Wintun adapter
            let runtime_wintun = self.app_data_dir.join("runtime").join("wintun.dll");
            let cores_wintun = self.app_data_dir.join("cores").join("wintun.dll");
            let wintun_path = if runtime_wintun.is_file() {
                Some(runtime_wintun.as_path())
            } else if cores_wintun.is_file() {
                Some(cores_wintun.as_path())
            } else {
                None
            };
            let _ = buster_windows::dismantle_wintun_adapter(wintun_path);

            // 4. Flush DNS cache
            buster_windows::flush_dns_cache();
        }

        #[cfg(not(target_os = "windows"))]
        {
            if let Err(e) = SessionJournal::recover_all_crash_state(&self.app_data_dir) {
                tracing::warn!("Startup crash state recovery failed: {}", e);
            }
            let proxy_journal = self.app_data_dir.join("proxy_journal.json");
            let _ = crate::platform::disable_system_proxy(Some(&proxy_journal));
            crate::platform::flush_dns_cache();
        }
    }

    /// Prepares and synchronizes all core executables and routing database assets into runtime_dir.
    pub async fn prepare_runtime_assets(
        &self,
        cores_dir: &Path,
        runtime_dir: &Path,
    ) -> Result<()> {
        tokio::fs::create_dir_all(runtime_dir).await?;

        let candidate_dirs = [
            cores_dir.to_path_buf(),
            self.app_data_dir.join("cores"),
            PathBuf::from(r"C:\Program Files\BusterVPN\cores"),
            PathBuf::from(r"C:\BUSTERVPN\cores"),
            PathBuf::from(r"C:\BUSTERVPN"),
            PathBuf::from("/usr/lib/bustervpn/cores"),
            PathBuf::from("/opt/bustervpn/cores"),
        ];

        #[cfg(target_os = "windows")]
        let required_files = [
            "xray.exe",
            "hysteria.exe",
            "wintun.dll",
            "geoip.dat",
            "geosite.dat",
        ];

        #[cfg(not(target_os = "windows"))]
        let required_files = [
            "xray",
            "hysteria",
            "geoip.dat",
            "geosite.dat",
        ];

        // Runtime Cache Cleaning: If geosite.dat in runtime_dir is smaller than 10MB or corrupted, remove it
        let runtime_geosite = runtime_dir.join("geosite.dat");
        if runtime_geosite.is_file() {
            if let Ok(meta) = tokio::fs::metadata(&runtime_geosite).await {
                if meta.len() < 10 * 1024 * 1024 {
                    tracing::warn!(
                        "Runtime geosite.dat is smaller than 10MB ({} bytes); removing corrupted/truncated cache",
                        meta.len()
                    );
                    let _ = tokio::fs::remove_file(&runtime_geosite).await;
                }
            }
        }

        for &file_name in &required_files {
            let target = runtime_dir.join(file_name);
            let mut target_valid = target.is_file();

            // Verify geosite.dat size constraint
            if file_name == "geosite.dat" && target_valid {
                if let Ok(meta) = tokio::fs::metadata(&target).await {
                    if meta.len() < 10 * 1024 * 1024 {
                        target_valid = false;
                        let _ = tokio::fs::remove_file(&target).await;
                    }
                }
            }

            // If target does not exist or is invalid, search candidate directories
            if !target_valid {
                for cand in &candidate_dirs {
                    let src = cand.join(file_name);
                    if src.is_file() {
                        if file_name == "geosite.dat" {
                            if let Ok(meta) = tokio::fs::metadata(&src).await {
                                if meta.len() < 10 * 1024 * 1024 {
                                    continue; // Skip small/truncated geosite.dat candidates
                                }
                            }
                        }

                        if let Ok(_) = tokio::fs::copy(&src, &target).await {
                            tracing::info!(
                                "Copied runtime asset '{}' from {}",
                                file_name,
                                src.display()
                            );

                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                if file_name == "xray" || file_name == "hysteria" {
                                    if let Ok(meta) = std::fs::metadata(&target) {
                                        let mut perms = meta.permissions();
                                        perms.set_mode(0o755);
                                        let _ = std::fs::set_permissions(&target, perms);
                                    }
                                }
                            }

                            target_valid = true;
                            break;
                        }
                    }
                }
            }
        }

        // Check if any critical engine component is still missing in runtime_dir
        #[cfg(target_os = "windows")]
        let critical_missing = !runtime_dir.join("xray.exe").is_file()
            || !runtime_dir.join("wintun.dll").is_file()
            || !runtime_dir.join("geoip.dat").is_file()
            || !runtime_dir.join("geosite.dat").is_file();

        #[cfg(not(target_os = "windows"))]
        let critical_missing = !runtime_dir.join("xray").is_file()
            || !runtime_dir.join("geoip.dat").is_file()
            || !runtime_dir.join("geosite.dat").is_file();

        if critical_missing {
            tracing::info!("Critical core assets missing in runtime dir; triggering updater sync...");
            let updater = UpdaterService::new();
            let _ = updater.update_cores(cores_dir, Some(runtime_dir)).await;
        }

        Ok(())
    }

    /// Gets the current connection status.
    pub async fn get_status(&self) -> ConnectionStatus {
        self.state.lock().await.status
    }

    /// Gets the active profile name, if connected.
    pub async fn get_active_profile_name(&self) -> Option<String> {
        self.state.lock().await.active_profile.as_ref().map(|p| p.name.clone())
    }

    /// Gets recent log entries.
    pub async fn get_logs(&self) -> Vec<LogLine> {
        self.state.lock().await.recent_logs.clone()
    }

    /// Initiates a VPN connection with the given server profile and settings.
    pub async fn connect(
        &self,
        profile: &ServerProfile,
        settings: &AppSettings,
        cores_dir: &Path,
    ) -> Result<()> {
        let mut st = self.state.lock().await;

        if st.status != ConnectionStatus::Disconnected {
            bail!("Cannot connect: current status is {:?}", st.status);
        }

        st.status = ConnectionStatus::Connecting;
        st.active_profile = Some(profile.clone());
        drop(st);

        if let Err(err) = self.do_connect(profile, settings, cores_dir).await {
            tracing::error!("Connection attempt failed: {}", err);
            // Safe cleanup on failure
            self.do_disconnect().await;
            let mut st = self.state.lock().await;
            st.status = ConnectionStatus::Disconnected;
            st.active_profile = None;
            st.connected_at = None;
            return Err(err);
        }

        let mut st = self.state.lock().await;
        st.status = ConnectionStatus::Connected;
        st.connected_at = Some(Utc::now());
        tracing::info!("VPN connection established to {}", profile.name);

        Ok(())
    }

    async fn do_connect(
        &self,
        profile: &ServerProfile,
        settings: &AppSettings,
        cores_dir: &Path,
    ) -> Result<()> {
        let routing_mode = settings.effective_routing_mode();

        // 1. Verify elevation if TUN mode is enabled
        #[cfg(target_os = "windows")]
        if routing_mode.has_tun() && !is_elevated() {
            bail!("TUN mode requires administrative privileges. Please run BusterVPN as Administrator.");
        }
        #[cfg(not(target_os = "windows"))]
        if routing_mode.has_tun() && !is_elevated() {
            bail!("TUN mode requires administrative privileges or network capabilities (CAP_NET_ADMIN). Please run as root or grant capabilities: sudo setcap cap_net_admin,cap_net_bind_service=+ep /path/to/binary");
        }

        // 2. Parse raw share link to extract core protocol configuration
        let raw_link = profile
            .raw_link
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Profile '{}' has no raw share link payload", profile.name))?;
        let parsed_server = parse_share_link(raw_link)
            .context("Failed to parse server profile share link")?;

        // 3. Allocate free local ports for SOCKS5 / HTTP inbounds
        let ports = allocate_proxy_pair()
            .context("Failed to allocate local proxy ports")?;

        // 4. Resolve server IPv4 addresses
        let server_host = profile
            .host
            .as_deref()
            .or_else(|| parsed_server.host.as_deref())
            .unwrap_or("127.0.0.1");

        let server_ips = resolve_server_ipv4(server_host)
            .with_context(|| format!("Failed to resolve server hostname '{}'", server_host))?;

        let resolved_ip: Ipv4Addr = server_ips
            .first()
            .and_then(|s| s.parse().ok())
            .unwrap_or(Ipv4Addr::LOCALHOST);

        // 5. Install TUN server bypass route if TUN is enabled
        let mut bypass_route = None;
        if routing_mode.has_tun() {
            #[cfg(target_os = "windows")]
            {
                let gateway = find_physical_gateway()
                    .context("Failed to discover physical default gateway for TUN bypass route")?;

                let route = add_server_bypass_route(server_host, Some(&gateway))
                    .context("Failed to install server bypass route")?;

                bypass_route = Some(route);
            }
            #[cfg(not(target_os = "windows"))]
            {
                let route = add_server_bypass_route(server_host, None)
                    .context("Failed to install server bypass route")?;
                bypass_route = Some(route);
            }
        }

        // 6. Prepare runtime directory and synchronize engine assets
        let runtime_dir = self.app_data_dir.join("runtime");
        self.prepare_runtime_assets(cores_dir, &runtime_dir).await?;

        // 6b. Ensure no stale orphaned adapter exists before spawning core
        #[cfg(target_os = "windows")]
        {
            let runtime_wintun = runtime_dir.join("wintun.dll");
            let _ = buster_windows::dismantle_wintun_adapter(Some(&runtime_wintun));
        }

        let xray_name = if cfg!(target_os = "windows") { "xray.exe" } else { "xray" };
        let hysteria_name = if cfg!(target_os = "windows") { "hysteria.exe" } else { "hysteria" };

        let (core_exe, args) = match profile.backend {
            BackendKind::Xray => {
                let mut params = XrayBuildParams::new(&parsed_server, routing_mode, resolved_ip);
                params.socks_port = ports.socks_port;
                params.http_port = ports.http_port;
                params.bypass_local_networks = settings.bypass_local_networks;
                params.routing_rules = &settings.routing_rules;
                params.working_dir = Some(&runtime_dir);
                params.split_tunneling = Some(settings.split_tunneling.clone());
                params.dns_servers = Some(settings.dns_profile.dns_servers());

                let config_json = build_xray_config_json(&params)
                    .context("Failed to build Xray JSON configuration")?;

                let config_file = runtime_dir.join("xray_config.json");
                tokio::fs::write(&config_file, config_json).await?;

                let runtime_xray = runtime_dir.join(xray_name);
                let exe = if runtime_xray.is_file() {
                    runtime_xray
                } else {
                    cores_dir.join(xray_name)
                };
                let args = vec![
                    "run".to_string(),
                    "-c".to_string(),
                    config_file.to_string_lossy().to_string(),
                ];
                (exe, args)
            }
            BackendKind::Hysteria2 => {
                let mut params = HysteriaBuildParams::new(&parsed_server, routing_mode, &server_ips);
                params.socks_port = ports.socks_port;
                params.http_port = ports.http_port;
                params.bypass_local_networks = settings.bypass_local_networks;
                params.routing_rules = &settings.routing_rules;

                let config_yaml = build_hysteria_config(&params)
                    .context("Failed to build Hysteria2 YAML configuration")?;

                let config_file = runtime_dir.join("hysteria_config.yaml");
                tokio::fs::write(&config_file, config_yaml).await?;

                let runtime_hysteria = runtime_dir.join(hysteria_name);
                let exe = if runtime_hysteria.is_file() {
                    runtime_hysteria
                } else {
                    cores_dir.join(hysteria_name)
                };
                let args = vec![
                    "client".to_string(),
                    "-c".to_string(),
                    config_file.to_string_lossy().to_string(),
                ];
                (exe, args)
            }
        };

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&core_exe) {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&core_exe, perms);
            }
        }

        // 7. Spawn core process bound to Win32 Job Object (Windows) or Process Group (Linux)
        let job = Arc::new(SafeJobObject::new().context("Failed to create SafeJobObject")?);
        let mut core = CoreProcessManager::spawn_core(&core_exe, args, job)
            .with_context(|| format!("Failed to spawn core process at {}", core_exe.display()))?;

        // 8. Record active PID in crash journal
        let pid = core.pid();
        self.session_journal.record(pid, &core_exe)?;

        // 9. If Kill Switch enabled, configure firewall rules
        if settings.kill_switch {
            self.kill_switch
                .enable(Some(&core_exe), &server_ips)
                .context("Failed to activate Windows Firewall Kill Switch")?;
        }

        // 10. If System Proxy enabled, configure system proxy settings
        if routing_mode.has_system_proxy() {
            self.proxy_manager
                .enable(&SystemProxySettings {
                    socks_port: Some(ports.socks_port),
                    http_port: Some(ports.http_port),
                    bypass_entries: vec![],
                })
                .context("Failed to configure Windows System Proxy")?;
        }

        // 11. If TUN is enabled, apply DNS with retry while Wintun interface initializes
        #[cfg(target_os = "windows")]
        if routing_mode.has_tun() {
            let dns_list = settings.dns_profile.dns_servers();
            tokio::spawn(async move {
                for attempt in 1..=5 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
                    match apply_dns_servers("BusterVPN_TUN", &dns_list) {
                        Ok(true) => {
                            tracing::info!("Successfully configured DNS on BusterVPN_TUN (attempt {})", attempt);
                            break;
                        }
                        Ok(false) => {
                            tracing::debug!("BusterVPN_TUN adapter not ready yet for DNS (attempt {})", attempt);
                        }
                        Err(e) => {
                            tracing::warn!("Error configuring DNS on BusterVPN_TUN (attempt {}): {}", attempt, e);
                        }
                    }
                }
            });
        }

        // 12. Start log streaming in background
        let (tx, mut rx) = tokio::sync::mpsc::channel::<LogLine>(200);
        core.start_log_streaming(tx);

        let logs_state = self.state.clone();
        tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                let mut st = logs_state.lock().await;
                if st.recent_logs.len() >= 400 {
                    st.recent_logs.remove(0);
                }
                st.recent_logs.push(line);
            }
        });

        // 13. Crash Watchdog: monitors PID to prevent unencrypted leaks under active Kill Switch
        let watchdog_state = self.state.clone();
        let watchdog_killswitch_active = settings.kill_switch;
        let core_pid = pid;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                let mut st = watchdog_state.lock().await;
                if st.status != ConnectionStatus::Connected {
                    break;
                }
                if !is_pid_running(core_pid) {
                    tracing::error!(
                        "Core process (PID {}) terminated unexpectedly while session is Connected!",
                        core_pid
                    );
                    st.status = ConnectionStatus::Disconnected;
                    if watchdog_killswitch_active {
                        tracing::warn!(
                            "Fail-Closed Kill Switch is ACTIVE: preserving firewall block rules to prevent traffic leak!"
                        );
                    }
                    break;
                }
            }
        });

        // 14. Network Monitor: auto-reconnect routes on network interface / physical gateway change
        #[cfg(target_os = "windows")]
        {
            let coord_state = self.state.clone();
            let monitor = NetworkMonitor::start(move || {
                let c_state = coord_state.clone();
                tokio::spawn(async move {
                    let mut st = c_state.lock().await;
                    if st.status != ConnectionStatus::Connected {
                        return;
                    }

                    tracing::info!("Auto-reconnecting routes on physical network gateway switch...");
                    if let Ok(new_gw) = find_physical_gateway() {
                        if let Some(ref old_route) = st.active_bypass_route {
                            remove_server_bypass_route(old_route);
                        }

                        if let Some(ref profile) = st.active_profile {
                            let server_host = profile.host.as_deref().unwrap_or("127.0.0.1");
                            if let Ok(new_route) = add_server_bypass_route(server_host, Some(&new_gw)) {
                                tracing::info!(
                                    "Applied updated server bypass route for {} via gateway {}",
                                    server_host,
                                    new_gw
                                );
                                st.active_bypass_route = Some(new_route);
                            }
                        }
                    }
                    buster_windows::flush_dns_cache();
                });
            });
            *self.network_monitor.lock().await = Some(monitor);
        }

        // Store active core and bypass route in state
        let mut st = self.state.lock().await;
        st.active_core = Some(core);
        st.active_bypass_route = bypass_route;

        Ok(())
    }

    /// Disconnects the active VPN session and restores all system settings.
    pub async fn disconnect(&self) -> Result<()> {
        let mut st = self.state.lock().await;
        if st.status == ConnectionStatus::Disconnected {
            return Ok(());
        }

        st.status = ConnectionStatus::Disconnecting;
        drop(st);

        self.do_disconnect().await;

        let mut st = self.state.lock().await;
        st.status = ConnectionStatus::Disconnected;
        st.active_profile = None;
        st.connected_at = None;
        tracing::info!("VPN session disconnected and system settings restored");

        Ok(())
    }

    async fn do_disconnect(&self) {
        tracing::info!("Starting streamlined disconnect teardown sequence in strict order...");

        // 0. Stop network monitor
        if let Some(monitor) = self.network_monitor.lock().await.take() {
            monitor.stop();
        }

        // 1. System Proxy reset (ProxyEnable = 0, clear ProxyServer, WinINet refresh & Kill Switch disable)
        let proxy_journal = self.app_data_dir.join("proxy_journal.json");
        #[cfg(target_os = "windows")]
        let _ = buster_windows::disable_system_proxy(Some(&proxy_journal));
        #[cfg(not(target_os = "windows"))]
        let _ = crate::platform::disable_system_proxy(Some(&proxy_journal));
        let _ = self.proxy_manager.disable();
        let _ = self.kill_switch.disable();

        // 2. Virtual Routes removal (virtual TUN routes & server host bypass routes)
        #[cfg(target_os = "windows")]
        buster_windows::remove_virtual_tun_routes();

        let mut st = self.state.lock().await;
        if let Some(ref route) = st.active_bypass_route {
            remove_server_bypass_route(route);
        }
        st.active_bypass_route = None;

        // 3. Core process termination (xray.exe / hysteria.exe bound to SafeJobObject)
        if let Some(mut core) = st.active_core.take() {
            let _ = core.terminate();
        }
        drop(st);

        // 4. Wintun dismantle and release driver handles
        #[cfg(target_os = "windows")]
        {
            let runtime_wintun = self.app_data_dir.join("runtime").join("wintun.dll");
            let cores_wintun = self.app_data_dir.join("cores").join("wintun.dll");
            let wintun_path = if runtime_wintun.is_file() {
                Some(runtime_wintun.as_path())
            } else if cores_wintun.is_file() {
                Some(cores_wintun.as_path())
            } else {
                None
            };
            let _ = buster_windows::dismantle_wintun_adapter(wintun_path);
        }

        // 5. DNS cache flush
        #[cfg(target_os = "windows")]
        buster_windows::flush_dns_cache();
        #[cfg(not(target_os = "windows"))]
        crate::platform::flush_dns_cache();

        // Clear crash journal
        self.session_journal.clear();
        tracing::info!("Disconnect teardown sequence completed instantaneously.");
    }
}
