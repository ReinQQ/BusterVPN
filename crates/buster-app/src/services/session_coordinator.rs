//! Central connection lifecycle orchestrator for BusterVPN.
//!
//! Manages:
//! - Connection state transitions (Disconnected -> Connecting -> Connected -> Disconnecting)
//! - Process isolation & crash management (Win32 Job Object)
//! - Physical gateway discovery & host bypass route pinning
//! - Windows Firewall Kill Switch activation/rollback
//! - System proxy switcher (WinINET registry)
//! - Real-time stdout/stderr log capturing
//! - Crash journal recording & startup recovery

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::{DateTime, Utc};
use anyhow::{bail, Context, Result};

use buster_core::config::singbox_builder::{
    build_singbox_config, build_xray_vless_detour_config_from_parsed, SingboxBuildParams,
};
use buster_core::models::{ConnectionStatus, ServerProfile, XrayProtocol};
use buster_core::parsing::parse_share_link;

use crate::models::AppSettings;
use crate::services::UpdaterService;

use buster_windows::{
    add_server_bypass_route, allocate_proxy_pair, apply_dns_servers, find_physical_gateway,
    is_elevated, proxy::SystemProxySettings, remove_server_bypass_route, resolve_server_ipv4,
    sync_runtime_assets, wait_for_sidecar_ready, CoreProcessManager, FirewallKillSwitch,
    LogLevel, LogLine, LogStream, NetworkMonitor, SafeJobObject, SessionJournal,
    SystemProxyManager, TunBypassRoute,
    ipc::{BusterServiceClient, ConnectTunnelParams},
};

pub struct CoordinatorState {
    pub status: ConnectionStatus,
    pub active_profile: Option<ServerProfile>,
    pub connected_at: Option<DateTime<Utc>>,
    pub active_core: Option<CoreProcessManager>,
    pub sidecar_core: Option<CoreProcessManager>,
    pub active_bypass_route: Option<TunBypassRoute>,
    pub recent_logs: Vec<LogLine>,
    pub using_service: bool,
}

pub struct SessionCoordinator {
    app_data_dir: PathBuf,
    state: Arc<Mutex<CoordinatorState>>,
    proxy_manager: Arc<SystemProxyManager>,
    kill_switch: Arc<FirewallKillSwitch>,
    session_journal: Arc<SessionJournal>,
    network_monitor: Arc<Mutex<Option<NetworkMonitor>>>,
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
            sidecar_core: None,
            active_bypass_route: None,
            recent_logs: Vec::with_capacity(400),
            using_service: false,
        }));

        Self {
            app_data_dir,
            state,
            proxy_manager,
            kill_switch,
            session_journal,
            network_monitor: Arc::new(Mutex::new(None)),
        }
    }

    /// Recovers from abnormal shutdown on application startup.
    pub fn startup_recovery(&self) {
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

    /// Prepares and synchronizes all core executables and routing database assets into runtime_dir.
    pub async fn prepare_runtime_assets(
        &self,
        cores_dir: &Path,
        runtime_dir: &Path,
    ) -> Result<()> {
        let extra_dirs = vec![
            cores_dir.to_path_buf(),
            self.app_data_dir.join("cores"),
            PathBuf::from(r"C:\Program Files\BusterVPN\cores"),
            PathBuf::from(r"C:\BUSTERVPN\antivpnv2\crates\buster-app\cores"),
            PathBuf::from(r"C:\BUSTERVPN\cores"),
            PathBuf::from(r"C:\BUSTERVPN"),
        ];
        sync_runtime_assets(runtime_dir, &extra_dirs)
            .context("Failed to synchronize runtime assets")?;

        let critical_missing = !runtime_dir.join("sing-box.exe").is_file()
            || !runtime_dir.join("xray.exe").is_file()
            || !runtime_dir.join("wintun.dll").is_file()
            || !runtime_dir.join("geosite-ru.srs").is_file()
            || !runtime_dir.join("geoip-ru.srs").is_file();

        if critical_missing {
            tracing::info!("Critical core assets missing in runtime dir; triggering updater sync...");
            let updater = UpdaterService::new();
            let _ = updater.update_cores(cores_dir, Some(runtime_dir)).await;
        }

        Ok(())
    }

    /// Ensures that the background Windows Service (BusterVPNService) is running.
    pub async fn ensure_service_running() -> Result<()> {
        use buster_windows::ipc::{BusterServiceClient, WindowsServiceManager, WindowsServiceState};

        if BusterServiceClient::is_service_available().await {
            return Ok(());
        }

        let scm_status = WindowsServiceManager::query_status();
        match scm_status {
            WindowsServiceState::Running => {
                for _ in 0..15 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    if BusterServiceClient::is_service_available().await {
                        return Ok(());
                    }
                }
            }
            WindowsServiceState::Stopped
            | WindowsServiceState::StartPending
            | WindowsServiceState::StopPending
            | WindowsServiceState::Other(_) => {
                tracing::info!("BusterVPNService is stopped in SCM. Attempting to start...");
                if WindowsServiceManager::start_service().is_err() {
                    let _ = std::process::Command::new("sc.exe")
                        .args(["start", "BusterVPNService"])
                        .output();
                }
                for _ in 0..15 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    if BusterServiceClient::is_service_available().await {
                        tracing::info!("BusterVPNService started successfully");
                        return Ok(());
                    }
                }
            }
            WindowsServiceState::NotInstalled => {
                tracing::info!("BusterVPNService is not installed in SCM. Attempting registration...");
                let cur_exe = std::env::current_exe().unwrap_or_default();
                let cur_dir = cur_exe.parent().unwrap_or_else(|| Path::new("."));

                let service_candidates = [
                    cur_dir.join("buster-service.exe"),
                    PathBuf::from(r"C:\Program Files\BusterVPN\buster-service.exe"),
                    PathBuf::from(r"C:\BUSTERVPN\antivpnv2\target\release\buster-service.exe"),
                ];
                let service_exe = service_candidates.into_iter().find(|p| p.is_file());

                let ctl_candidates = [
                    cur_dir.join("busterctl.exe"),
                    PathBuf::from(r"C:\Program Files\BusterVPN\busterctl.exe"),
                    PathBuf::from(r"C:\BUSTERVPN\antivpnv2\target\release\busterctl.exe"),
                ];
                let ctl_exe = ctl_candidates.into_iter().find(|p| p.is_file());

                if let Some(ref svc) = service_exe {
                    if WindowsServiceManager::install_service(svc).is_ok() {
                        let _ = WindowsServiceManager::start_service();
                    } else if let Some(ref ctl) = ctl_exe {
                        let _ = std::process::Command::new("powershell.exe")
                            .args([
                                "-NoProfile",
                                "-Command",
                                &format!("Start-Process -FilePath '{}' -ArgumentList 'service install' -Verb RunAs -Wait", ctl.display())
                            ])
                            .output();
                        let _ = std::process::Command::new("powershell.exe")
                            .args([
                                "-NoProfile",
                                "-Command",
                                &format!("Start-Process -FilePath '{}' -ArgumentList 'service start' -Verb RunAs -Wait", ctl.display())
                            ])
                            .output();
                    }
                } else if let Some(ref ctl) = ctl_exe {
                    let _ = std::process::Command::new("powershell.exe")
                        .args([
                            "-NoProfile",
                            "-Command",
                            &format!("Start-Process -FilePath '{}' -ArgumentList 'service install' -Verb RunAs -Wait", ctl.display())
                        ])
                        .output();
                    let _ = std::process::Command::new("powershell.exe")
                        .args([
                            "-NoProfile",
                            "-Command",
                            &format!("Start-Process -FilePath '{}' -ArgumentList 'service start' -Verb RunAs -Wait", ctl.display())
                        ])
                        .output();
                }

                for _ in 0..15 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    if BusterServiceClient::is_service_available().await {
                        tracing::info!("BusterVPNService installed and running");
                        return Ok(());
                    }
                }
            }
        }

        if BusterServiceClient::is_service_available().await {
            Ok(())
        } else {
            tracing::warn!("Could not automatically start BusterVPNService Named Pipe server");
            Ok(())
        }
    }

    /// Gets the current connection status.
    pub async fn get_status(&self) -> ConnectionStatus {
        if self.state.lock().await.using_service {
            if let Ok(rep) = BusterServiceClient::get_status().await {
                let mut st = self.state.lock().await;
                st.status = rep.status;
                return rep.status;
            }
        }
        self.state.lock().await.status
    }

    /// Gets the active profile name, if connected.
    pub async fn get_active_profile_name(&self) -> Option<String> {
        self.state.lock().await.active_profile.as_ref().map(|p| p.name.clone())
    }

    /// Gets recent log entries.
    pub async fn get_logs(&self) -> Vec<LogLine> {
        if self.state.lock().await.using_service {
            if let Ok(logs) = BusterServiceClient::get_logs().await {
                return logs;
            }
        }
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

        if st.status == ConnectionStatus::Error {
            drop(st);
            self.do_disconnect().await;
            st = self.state.lock().await;
        } else if st.status != ConnectionStatus::Disconnected {
            bail!("Cannot connect: current status is {:?}", st.status);
        }

        st.status = ConnectionStatus::Connecting;
        st.active_profile = Some(profile.clone());
        drop(st);

        let _ = Self::ensure_service_running().await;
        let service_available = BusterServiceClient::is_service_available().await;

        let res = if service_available {
            self.connect_via_service(profile, settings, cores_dir).await
        } else {
            self.do_connect(profile, settings, cores_dir).await
        };

        if let Err(err) = res {
            tracing::error!("Connection attempt failed: {}", err);
            self.do_disconnect().await;
            let mut st = self.state.lock().await;
            st.status = ConnectionStatus::Disconnected;
            st.active_profile = None;
            st.connected_at = None;
            st.using_service = false;
            return Err(err);
        }

        let mut st = self.state.lock().await;
        st.status = ConnectionStatus::Connected;
        st.connected_at = Some(Utc::now());
        tracing::info!("VPN connection established to {}", profile.name);

        Ok(())
    }

    async fn connect_via_service(
        &self,
        profile: &ServerProfile,
        settings: &AppSettings,
        cores_dir: &Path,
    ) -> Result<()> {
        let routing_mode = settings.effective_routing_mode();

        let raw_link = profile
            .raw_link
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Profile '{}' has no raw share link payload", profile.name))?;
        let parsed_server = parse_share_link(raw_link)
            .context("Failed to parse server profile share link")?;

        let ports = allocate_proxy_pair()
            .context("Failed to allocate local proxy ports")?;

        let server_host = profile
            .host
            .as_deref()
            .or(parsed_server.host.as_deref())
            .unwrap_or("127.0.0.1");

        let server_ips = resolve_server_ipv4(server_host).unwrap_or_default();

        let runtime_dir = self.app_data_dir.join("runtime");
        self.prepare_runtime_assets(cores_dir, &runtime_dir).await?;

        let is_vless = parsed_server
            .xray
            .as_ref()
            .is_some_and(|x| x.protocol == XrayProtocol::Vless);

        let mut sidecar_config: Option<String> = None;
        let mut sidecar_filename: Option<String> = None;
        let mut sidecar_args: Option<Vec<String>> = None;

        let mut params = SingboxBuildParams::new(&parsed_server, routing_mode);
        params.socks_port = ports.socks_port;
        params.http_port = ports.http_port;
        params.bypass_local_networks = settings.bypass_local_networks;
        params.block_quic = true;
        params.routing_rules = &settings.routing_rules;
        params.working_dir = Some(cores_dir);
        params.split_tunneling = Some(settings.split_tunneling.clone());
        params.dns_servers = Some(settings.dns_profile.dns_servers());
        params.resolved_server_ips = Some(server_ips.clone());

        if is_vless {
            if !runtime_dir.join("xray.exe").is_file() {
                bail!("xray.exe missing in runtime directory");
            }
            let detour_port = std::net::TcpListener::bind("127.0.0.1:0")
                .and_then(|l| l.local_addr().map(|a| a.port()))
                .context("Failed to allocate local port for xray.exe SOCKS5 sidecar")?;
            let xray_cfg = build_xray_vless_detour_config_from_parsed(&parsed_server, detour_port)
                .map_err(|e| anyhow::anyhow!("Failed to build Xray VLESS detour config: {}", e))?;
            params.vless_socks_detour_port = Some(detour_port);
            sidecar_config = Some(xray_cfg);
            sidecar_filename = Some("xray_detour_config.json".to_string());
            sidecar_args = Some(vec![
                "run".to_string(),
                "-c".to_string(),
                "xray_detour_config.json".to_string(),
            ]);
            tracing::info!("Configured Xray SOCKS5 detour on 127.0.0.1:{}", detour_port);
        }

        let config_json = build_singbox_config(&params)
            .context("Failed to build Sing-box JSON configuration")?;

        let config_file = runtime_dir.join("singbox_config.json");
        tokio::fs::write(&config_file, &config_json).await?;
        let abs_config = config_file.canonicalize().unwrap_or(config_file);
        let abs_config_str = {
            let s = abs_config.to_string_lossy().to_string();
            s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
        };

        let core_kind = "sing-box".to_string();
        let config_filename = "singbox_config.json".to_string();
        let core_args = vec![
            "run".to_string(),
            "-c".to_string(),
            abs_config_str,
        ];

        let params = ConnectTunnelParams {
            profile_name: profile.name.clone(),
            core_kind,
            config_content: config_json,
            config_filename,
            core_args,
            server_host: server_host.to_string(),
            server_ips,
            dns_servers: settings.dns_profile.dns_servers(),
            kill_switch: settings.kill_switch,
            tun_enabled: routing_mode.has_tun(),
            cores_dir: Some(cores_dir.to_string_lossy().to_string()),
            sidecar_config,
            sidecar_filename,
            sidecar_args,
        };

        BusterServiceClient::connect_tunnel(params).await
            .context("Failed to connect tunnel via BusterVPNService")?;

        if routing_mode.has_system_proxy() {
            self.proxy_manager
                .enable(&SystemProxySettings {
                    socks_port: Some(ports.socks_port),
                    http_port: Some(ports.http_port),
                    bypass_entries: vec![],
                })
                .context("Failed to configure Windows System Proxy in user session")?;
        }

        let mut st = self.state.lock().await;
        st.using_service = true;
        st.status = ConnectionStatus::Connected;
        st.connected_at = Some(Utc::now());
        drop(st);
        tracing::info!("VPN connection established via service to {}", profile.name);

        Ok(())
    }

    async fn do_connect(
        &self,
        profile: &ServerProfile,
        settings: &AppSettings,
        cores_dir: &Path,
    ) -> Result<()> {
        let routing_mode = settings.effective_routing_mode();

        // 1. Verify elevation if TUN mode or Kill Switch is enabled
        if (routing_mode.has_tun() || settings.kill_switch) && !is_elevated() {
            bail!("Фоновая служба BusterVPNService не запущена. Для работы TUN и Kill Switch без прав администратора запустите службу BusterVPNService в Настройках или перезапустите приложение от имени администратора.");
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
            .or(parsed_server.host.as_deref())
            .unwrap_or("127.0.0.1");

        let server_ips = resolve_server_ipv4(server_host)
            .with_context(|| format!("Failed to resolve server hostname '{}'", server_host))?;

        // 5. Install TUN server bypass route if TUN is enabled
        let mut bypass_route = None;
        if routing_mode.has_tun() {
            let gateway = find_physical_gateway()
                .context("Failed to discover physical default gateway for TUN bypass route")?;

            let route = add_server_bypass_route(server_host, Some(&gateway))
                .context("Failed to install server bypass route")?;

            bypass_route = Some(route);
        }

        // 6. Prepare runtime directory and synchronize engine assets
        let runtime_dir = self.app_data_dir.join("runtime");
        self.prepare_runtime_assets(cores_dir, &runtime_dir).await?;

        // 6b. Ensure no stale orphaned adapter exists before spawning core
        let runtime_wintun = runtime_dir.join("wintun.dll");
        let _ = buster_windows::dismantle_wintun_adapter(Some(&runtime_wintun));

        let singbox_name = "sing-box.exe";

        let is_vless = parsed_server
            .xray
            .as_ref()
            .is_some_and(|x| x.protocol == XrayProtocol::Vless);

        let xray_exe = runtime_dir.join("xray.exe");

        let mut sidecar_opt: Option<CoreProcessManager> = None;
        let job = Arc::new(SafeJobObject::new().context("Failed to create SafeJobObject")?);

        let mut params = SingboxBuildParams::new(&parsed_server, routing_mode);
        params.socks_port = ports.socks_port;
        params.http_port = ports.http_port;
        params.bypass_local_networks = settings.bypass_local_networks;
        params.block_quic = true;
        params.routing_rules = &settings.routing_rules;
        params.working_dir = Some(cores_dir);
        params.split_tunneling = Some(settings.split_tunneling.clone());
        params.dns_servers = Some(settings.dns_profile.dns_servers());
        params.resolved_server_ips = Some(server_ips.clone());

        if is_vless {
            if !xray_exe.is_file() {
                bail!("xray.exe missing in runtime directory");
            }
            let detour_port = std::net::TcpListener::bind("127.0.0.1:0")
                .and_then(|l| l.local_addr().map(|a| a.port()))
                .context("Failed to allocate local port for xray.exe SOCKS5 sidecar")?;
            let xray_cfg = build_xray_vless_detour_config_from_parsed(&parsed_server, detour_port)
                .map_err(|e| anyhow::anyhow!("Failed to build Xray VLESS detour config: {}", e))?;

            params.vless_socks_detour_port = Some(detour_port);
            let xray_cfg_file = runtime_dir.join("xray_detour_config.json");
            tokio::fs::write(&xray_cfg_file, &xray_cfg)
                .await
                .with_context(|| format!("Failed to write Xray detour config to {}", xray_cfg_file.display()))?;
            let abs_xray_cfg = xray_cfg_file.canonicalize().unwrap_or(xray_cfg_file);
            let abs_xray_str = {
                let s = abs_xray_cfg.to_string_lossy().to_string();
                s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
            };
            let sc_args = vec!["run".to_string(), "-c".to_string(), abs_xray_str];

            let mut sc = CoreProcessManager::spawn_core_with_workdir(
                &xray_exe,
                sc_args,
                job.clone(),
                &runtime_dir,
            )
            .context("Failed to spawn xray.exe sidecar detour")?;

            let sc_pid = sc.pid();
            let _ = self.session_journal.record(sc_pid, &xray_exe);
            let (sc_tx, mut sc_rx) = tokio::sync::mpsc::channel::<LogLine>(200);
            sc.start_log_streaming(sc_tx);

            let logs_state = self.state.clone();
            tokio::spawn(async move {
                while let Some(line) = sc_rx.recv().await {
                    let mut st = logs_state.lock().await;
                    if st.recent_logs.len() >= 400 {
                        st.recent_logs.remove(0);
                    }
                    st.recent_logs.push(line);
                }
            });

            wait_for_sidecar_ready(&mut sc, detour_port)
                .await
                .context("xray.exe sidecar failed readiness check")?;

            tracing::info!(
                "Spawned Xray sidecar detour (PID: {}) listening on 127.0.0.1:{}",
                sc_pid,
                detour_port
            );
            sidecar_opt = Some(sc);
        }

        let config_json = build_singbox_config(&params)
            .context("Failed to build Sing-box JSON configuration")?;

        let config_file = runtime_dir.join("singbox_config.json");
        tokio::fs::write(&config_file, config_json).await?;
        let abs_config = config_file.canonicalize().unwrap_or(config_file);
        let abs_config_str = {
            let s = abs_config.to_string_lossy().to_string();
            s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
        };

        let runtime_sb = runtime_dir.join(singbox_name);
        let core_exe = if runtime_sb.is_file() {
            runtime_sb
        } else {
            cores_dir.join(singbox_name)
        };

        let args = vec![
            "run".to_string(),
            "-c".to_string(),
            abs_config_str,
        ];

        // 7. Spawn core process bound to Win32 Job Object
        let mut core = CoreProcessManager::spawn_core_with_workdir(&core_exe, args, job, cores_dir)
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

        // 13. Crash Watchdog: monitors both sing-box.exe and xray.exe to prevent unencrypted leaks under active Kill Switch
        {
            let watchdog_state = self.state.clone();
            let watchdog_killswitch_active = settings.kill_switch;
            let watchdog_app_data = self.app_data_dir.clone();
            let watchdog_proxy = self.proxy_manager.clone();
            let watchdog_journal = self.session_journal.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    let st = watchdog_state.lock().await;
                    match st.status {
                        ConnectionStatus::Connecting => continue,
                        ConnectionStatus::Connected => break,
                        _ => return,
                    }
                }

                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                    let mut st = watchdog_state.lock().await;
                    if st.status != ConnectionStatus::Connected {
                        break;
                    }

                    let mut crashed: Option<(String, String)> = None;
                    if let Some(ref mut c) = st.active_core {
                        match c.try_wait() {
                            Ok(Some(status)) => {
                                crashed = Some((
                                    format!("sing-box.exe (PID {}) exited unexpectedly with {}", c.pid(), status),
                                    c.recent_stderr(),
                                ));
                            }
                            Ok(None) => {}
                            Err(e) => {
                                crashed = Some((
                                    format!("sing-box.exe (PID {}) handle check failed: {}", c.pid(), e),
                                    c.recent_stderr(),
                                ));
                            }
                        }
                    }
                    if crashed.is_none() {
                        if let Some(ref mut sc) = st.sidecar_core {
                            match sc.try_wait() {
                                Ok(Some(status)) => {
                                    crashed = Some((
                                        format!("xray.exe (PID {}) exited unexpectedly with {}", sc.pid(), status),
                                        sc.recent_stderr(),
                                    ));
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    crashed = Some((
                                        format!("xray.exe (PID {}) handle check failed: {}", sc.pid(), e),
                                        sc.recent_stderr(),
                                    ));
                                }
                            }
                        }
                    }

                    if let Some((reason, stderr)) = crashed {
                        let msg = if stderr.is_empty() {
                            reason.clone()
                        } else {
                            format!("{} | stderr: {}", reason, stderr)
                        };
                        tracing::error!("{}", msg);
                        if st.recent_logs.len() >= 400 {
                            st.recent_logs.remove(0);
                        }
                        st.recent_logs.push(LogLine {
                            timestamp: Utc::now(),
                            level: LogLevel::Error,
                            stream: LogStream::Stderr,
                            text: msg,
                        });
                        if let Some(ref route) = st.active_bypass_route {
                            remove_server_bypass_route(route);
                        }
                        st.active_bypass_route = None;
                        if let Some(mut sc) = st.sidecar_core.take() {
                            let _ = sc.terminate();
                        }
                        if let Some(mut c) = st.active_core.take() {
                            let _ = c.terminate();
                        }
                        st.status = ConnectionStatus::Error;
                        st.active_profile = None;
                        st.connected_at = None;
                        drop(st);

                        let proxy_journal = watchdog_app_data.join("proxy_journal.json");
                        let _ = buster_windows::disable_system_proxy(Some(&proxy_journal));
                        let _ = watchdog_proxy.disable();
                        buster_windows::remove_virtual_tun_routes();
                        let runtime_wintun = watchdog_app_data.join("runtime").join("wintun.dll");
                        let _ = buster_windows::dismantle_wintun_adapter(
                            if runtime_wintun.is_file() { Some(&runtime_wintun) } else { None },
                        );
                        buster_windows::flush_dns_cache();
                        watchdog_journal.clear();

                        if watchdog_killswitch_active {
                            tracing::warn!(
                                "Fail-Closed Kill Switch is ACTIVE: preserving firewall block rules to prevent traffic leak!"
                            );
                        }
                        break;
                    }
                }
            });
        }

        // 14. Network Monitor: auto-reconnect routes on network interface / physical gateway change
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
        st.sidecar_core = sidecar_opt;
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
        let was_using_service = st.using_service;
        drop(st);

        if was_using_service {
            let proxy_journal = self.app_data_dir.join("proxy_journal.json");
            let _ = buster_windows::disable_system_proxy(Some(&proxy_journal));
            let _ = self.proxy_manager.disable();
            let _ = BusterServiceClient::disconnect_tunnel().await;
            let mut st = self.state.lock().await;
            st.status = ConnectionStatus::Disconnected;
            st.active_profile = None;
            st.connected_at = None;
            st.using_service = false;
            tracing::info!("VPN session disconnected via BusterVPNService");
            return Ok(());
        }

        self.do_disconnect().await;

        let mut st = self.state.lock().await;
        st.status = ConnectionStatus::Disconnected;
        st.active_profile = None;
        st.connected_at = None;
        st.using_service = false;
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
        let _ = buster_windows::disable_system_proxy(Some(&proxy_journal));
        let _ = self.proxy_manager.disable();
        let _ = self.kill_switch.disable();

        // 2. Virtual Routes removal (virtual TUN routes & server host bypass routes)
        buster_windows::remove_virtual_tun_routes();

        let mut st = self.state.lock().await;
        if let Some(ref route) = st.active_bypass_route {
            remove_server_bypass_route(route);
        }
        st.active_bypass_route = None;

        // 3. Core process termination (sing-box.exe / xray.exe bound to SafeJobObject)
        if let Some(mut sidecar) = st.sidecar_core.take() {
            let _ = sidecar.terminate();
        }
        if let Some(mut core) = st.active_core.take() {
            let _ = core.terminate();
        }
        drop(st);

        // 4. Wintun dismantle and release driver handles
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

        // 5. DNS cache flush
        buster_windows::flush_dns_cache();

        // Clear crash journal
        self.session_journal.clear();
        tracing::info!("Disconnect teardown sequence completed instantaneously.");
    }
}
