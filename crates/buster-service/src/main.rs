use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{ServerOptions, NamedPipeServer};
use tokio::sync::{Mutex, Notify};
use tracing::{error, info, warn};

use buster_core::models::ConnectionStatus;
use buster_windows::{
    add_server_bypass_route, apply_dns_servers, dismantle_wintun_adapter, find_physical_gateway,
    flush_dns_cache, remove_server_bypass_route, remove_virtual_tun_routes,
    resolve_server_ipv4, CoreProcessManager, FirewallKillSwitch, LogLine, SafeJobObject,
    SessionJournal, TunBypassRoute,
};
use buster_windows::ipc::{
    ConnectTunnelParams, IpcRequest, IpcResponse, ServiceConfig, ServiceStatusReport,
    PIPE_NAME, SERVICE_NAME,
};

#[derive(Default)]
struct ServiceState {
    status: ConnectionStatus,
    active_profile: Option<String>,
    connected_at: Option<chrono::DateTime<Utc>>,
    core_process: Option<CoreProcessManager>,
    sidecar_process: Option<CoreProcessManager>,
    active_bypass_route: Option<TunBypassRoute>,
    recent_logs: Vec<LogLine>,
    tun_enabled: bool,
    kill_switch_enabled: bool,
    auto_connect_on_boot: bool,
    last_connect_params: Option<ConnectTunnelParams>,
}

struct ServiceContext {
    program_data_dir: PathBuf,
    state: Arc<Mutex<ServiceState>>,
    kill_switch: Arc<FirewallKillSwitch>,
    session_journal: Arc<SessionJournal>,
    stop_notify: Arc<Notify>,
}

impl ServiceContext {
    fn new(program_data_dir: PathBuf) -> Self {
        let kill_switch = Arc::new(FirewallKillSwitch::new(
            program_data_dir.join("killswitch_journal.json"),
        ));
        let session_journal = Arc::new(SessionJournal::new(
            program_data_dir.join("session.json"),
        ));

        Self {
            program_data_dir,
            state: Arc::new(Mutex::new(ServiceState::default())),
            kill_switch,
            session_journal,
            stop_notify: Arc::new(Notify::new()),
        }
    }

    fn config_file(&self) -> PathBuf {
        self.program_data_dir.join("service_config.json")
    }

    async fn load_config(&self) -> ServiceConfig {
        let path = self.config_file();
        if path.is_file() {
            if let Ok(bytes) = tokio::fs::read(&path).await {
                if let Ok(cfg) = serde_json::from_slice::<ServiceConfig>(&bytes) {
                    return cfg;
                }
            }
        }
        ServiceConfig::default()
    }

    async fn save_config(&self, cfg: &ServiceConfig) -> Result<()> {
        let path = self.config_file();
        let json = serde_json::to_string_pretty(cfg)?;
        tokio::fs::write(&path, json).await?;
        Ok(())
    }

    async fn startup_recovery(&self) {
        info!("Running startup recovery for BusterVPNService...");
        let _ = SessionJournal::recover_all_crash_state(&self.program_data_dir);
        remove_virtual_tun_routes();

        let runtime_wintun = self.program_data_dir.join("runtime").join("wintun.dll");
        let wintun_path = if runtime_wintun.is_file() {
            Some(runtime_wintun.as_path())
        } else {
            None
        };
        let _ = dismantle_wintun_adapter(wintun_path);
        flush_dns_cache();

        let cfg = self.load_config().await;
        let mut st = self.state.lock().await;
        st.auto_connect_on_boot = cfg.auto_connect_on_boot;
        st.last_connect_params = cfg.last_connect_params.clone();
        drop(st);

        if cfg.auto_connect_on_boot {
            if let Some(params) = cfg.last_connect_params {
                info!("Auto-connect on boot is enabled; bringing up tunnel for profile '{}'", params.profile_name);
                let ctx_clone = self.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    if let Err(e) = ctx_clone.do_connect(params).await {
                        error!("Auto-connect on boot failed: {}", e);
                    }
                });
            }
        }
    }

    async fn do_connect(&self, params: ConnectTunnelParams) -> Result<()> {
        let mut st = self.state.lock().await;
        if st.status == ConnectionStatus::Connected {
            drop(st);
            self.do_disconnect().await;
            st = self.state.lock().await;
        }

        st.status = ConnectionStatus::Connecting;
        st.active_profile = Some(params.profile_name.clone());
        drop(st);

        let res = self.execute_connect(&params).await;
        match res {
            Ok((core, sidecar)) => {
                let mut st = self.state.lock().await;
                st.status = ConnectionStatus::Connected;
                st.connected_at = Some(Utc::now());
                st.tun_enabled = params.tun_enabled;
                st.kill_switch_enabled = params.kill_switch;
                st.core_process = Some(core);
                st.sidecar_process = sidecar;

                let cfg = ServiceConfig {
                    auto_connect_on_boot: st.auto_connect_on_boot,
                    last_connect_params: Some(params),
                };
                drop(st);
                let _ = self.save_config(&cfg).await;
                info!("Tunnel successfully established by BusterVPNService");
                Ok(())
            }
            Err(e) => {
                error!("Connect tunnel execution failed: {}", e);
                self.do_disconnect().await;
                let mut st = self.state.lock().await;
                st.status = ConnectionStatus::Disconnected;
                st.active_profile = None;
                st.connected_at = None;
                Err(e)
            }
        }
    }

    async fn execute_connect(&self, params: &ConnectTunnelParams) -> Result<(CoreProcessManager, Option<CoreProcessManager>)> {
        let runtime_dir = self.program_data_dir.join("runtime");
        tokio::fs::create_dir_all(&runtime_dir).await?;

        // 1. Resolve server host & bypass routes
        let server_host = &params.server_host;
        let mut server_ips = params.server_ips.clone();
        if server_ips.is_empty() {
            if let Ok(resolved) = resolve_server_ipv4(server_host) {
                server_ips = resolved;
            }
        }

        if params.tun_enabled {
            let gateway = find_physical_gateway()
                .context("Failed to find physical gateway for TUN bypass route")?;
            let route = add_server_bypass_route(server_host, Some(&gateway))
                .context("Failed to add server bypass route")?;
            let mut st = self.state.lock().await;
            st.active_bypass_route = Some(route);
        }

        // 2. Write core config
        let config_path = runtime_dir.join(&params.config_filename);
        tokio::fs::write(&config_path, &params.config_content).await?;
        let abs_config = config_path.canonicalize().unwrap_or(config_path);
        let abs_config_str = {
            let s = abs_config.to_string_lossy().to_string();
            s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
        };

        // 3. Locate core binary and working directory
        let candidate_cores_dirs = [
            params.cores_dir.as_deref().map(PathBuf::from),
            std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("cores"))),
            std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())),
            Some(PathBuf::from(r"C:\Program Files\BusterVPN\cores")),
            Some(PathBuf::from(r"C:\BUSTERVPN\antivpnv2\crates\buster-app\cores")),
            Some(PathBuf::from(r"C:\BUSTERVPN\cores")),
        ];

        // Ensure active assets (sing-box.exe, xray.exe, wintun.dll, geosite-ru.srs, geoip-ru.srs)
        // are copied into runtime_dir (C:\ProgramData\BusterVPN\runtime) if missing or outdated
        for asset in &["sing-box.exe", "xray.exe", "wintun.dll", "geosite-ru.srs", "geoip-ru.srs"] {
            let dst = runtime_dir.join(asset);
            let dst_len = dst.metadata().map(|m| m.len()).unwrap_or(0);
            for cand in candidate_cores_dirs.iter().flatten() {
                let src = cand.join(asset);
                if src.is_file() {
                    let src_len = src.metadata().map(|m| m.len()).unwrap_or(0);
                    if dst_len == 0 || dst_len != src_len {
                        if let Ok(_) = tokio::fs::copy(&src, &dst).await {
                            info!("Synchronized runtime asset '{}' ({} bytes) from {}", asset, src_len, src.display());
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        // Purge legacy core files from runtime directory if present
        for legacy in &[
            "hysteria.exe",
            "geoip.dat",
            "geosite.dat",
            "hysteria_config.yaml",
        ] {
            let legacy_file = runtime_dir.join(legacy);
            if legacy_file.exists() {
                let _ = tokio::fs::remove_file(&legacy_file).await;
            }
        }

        let core_exe = if runtime_dir.join("sing-box.exe").is_file() {
            runtime_dir.join("sing-box.exe")
        } else if let Some(cand) = candidate_cores_dirs.iter().flatten().find(|d| d.join("sing-box.exe").is_file()) {
            cand.join("sing-box.exe")
        } else {
            runtime_dir.join("sing-box.exe")
        };

        let working_dir = if runtime_dir.join("geosite-ru.srs").is_file() {
            runtime_dir.clone()
        } else if let Some(cand) = candidate_cores_dirs.iter().flatten().find(|d| d.join("geosite-ru.srs").is_file()) {
            cand.clone()
        } else {
            runtime_dir.clone()
        };

        if !core_exe.is_file() {
            bail!("Core binary 'sing-box.exe' not found at {}", core_exe.display());
        }

        // 4. Dismantle stale Wintun if TUN enabled
        if params.tun_enabled {
            let target_wintun = if runtime_dir.join("wintun.dll").is_file() {
                Some(runtime_dir.join("wintun.dll"))
            } else if working_dir.join("wintun.dll").is_file() {
                Some(working_dir.join("wintun.dll"))
            } else {
                None
            };
            let _ = dismantle_wintun_adapter(target_wintun.as_deref());
        }

        // Ensure config path passed via args is canonical absolute path
        let mut core_args = params.core_args.clone();
        if let Some(pos) = core_args.iter().position(|a| a == "-c") {
            if pos + 1 < core_args.len() {
                core_args[pos + 1] = abs_config_str;
            }
        }

        // 5. Spawn core bound to SafeJobObject with working_dir set to cores_dir
        let job = Arc::new(SafeJobObject::new().context("Failed to create SafeJobObject")?);

        // 5a. If sidecar config is present, spawn sidecar core (xray.exe) bound to the same SafeJobObject
        let mut sidecar_opt: Option<CoreProcessManager> = None;
        if let Some(ref sidecar_cfg) = params.sidecar_config {
            let sidecar_filename = params
                .sidecar_filename
                .as_deref()
                .unwrap_or("xray_detour_config.json");
            let sidecar_config_path = runtime_dir.join(sidecar_filename);
            tokio::fs::write(&sidecar_config_path, sidecar_cfg)
                .await
                .with_context(|| format!("Failed to write sidecar config to {}", sidecar_config_path.display()))?;

            let abs_sidecar_cfg = sidecar_config_path.canonicalize().unwrap_or(sidecar_config_path.clone());
            let abs_sidecar_str = {
                let s = abs_sidecar_cfg.to_string_lossy().to_string();
                s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
            };

            let xray_exe = if runtime_dir.join("xray.exe").is_file() {
                runtime_dir.join("xray.exe")
            } else if let Some(cand) = candidate_cores_dirs.iter().flatten().find(|d| d.join("xray.exe").is_file()) {
                cand.join("xray.exe")
            } else {
                runtime_dir.join("xray.exe")
            };

            if xray_exe.is_file() {
                let sidecar_args = params.sidecar_args.clone().unwrap_or_else(|| {
                    vec!["run".to_string(), "-c".to_string(), abs_sidecar_str]
                });
                match CoreProcessManager::spawn_core_with_workdir(&xray_exe, sidecar_args, job.clone(), &runtime_dir) {
                    Ok(mut sc) => {
                        let sc_pid = sc.pid();
                        let _ = self.session_journal.record(sc_pid, &xray_exe);
                        let (sc_tx, mut sc_rx) = tokio::sync::mpsc::channel::<LogLine>(200);
                        sc.start_log_streaming(sc_tx);

                        let state_clone = self.state.clone();
                        tokio::spawn(async move {
                            while let Some(line) = sc_rx.recv().await {
                                let mut st = state_clone.lock().await;
                                if st.recent_logs.len() >= 400 {
                                    st.recent_logs.remove(0);
                                }
                                st.recent_logs.push(line);
                            }
                        });
                        info!("Spawned xray.exe sidecar detour (PID: {}) bound to SafeJobObject", sc_pid);
                        sidecar_opt = Some(sc);
                    }
                    Err(e) => {
                        warn!("Failed to spawn xray.exe sidecar detour: {}", e);
                    }
                }
            } else {
                warn!("xray.exe binary not found at {}", xray_exe.display());
            }
        }

        let mut core = CoreProcessManager::spawn_core_with_workdir(&core_exe, core_args, job, &working_dir)
            .with_context(|| format!("Failed to spawn core process at {}", core_exe.display()))?;

        let pid = core.pid();
        self.session_journal.record(pid, &core_exe)?;

        // 6. Firewall Kill Switch
        if params.kill_switch {
            self.kill_switch
                .enable(Some(&core_exe), &server_ips)
                .context("Failed to enable Windows Firewall Kill Switch")?;
        }

        // 7. TUN DNS configuration
        if params.tun_enabled && !params.dns_servers.is_empty() {
            let dns_list = params.dns_servers.clone();
            tokio::spawn(async move {
                for attempt in 1..=5 {
                    tokio::time::sleep(Duration::from_millis(400)).await;
                    match apply_dns_servers("BusterVPN_TUN", &dns_list) {
                        Ok(true) => {
                            info!("Configured DNS on BusterVPN_TUN in background service (attempt {})", attempt);
                            break;
                        }
                        Ok(false) => {}
                        Err(e) => {
                            warn!("DNS config attempt {}: {}", attempt, e);
                        }
                    }
                }
            });
        }

        // 8. Stream logs into ring buffer
        let (tx, mut rx) = tokio::sync::mpsc::channel::<LogLine>(200);
        core.start_log_streaming(tx);

        let state_clone = self.state.clone();
        tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                let mut st = state_clone.lock().await;
                if st.recent_logs.len() >= 400 {
                    st.recent_logs.remove(0);
                }
                st.recent_logs.push(line);
            }
        });

        Ok((core, sidecar_opt))
    }

    async fn do_disconnect(&self) {
        info!("BusterVPNService disconnecting active session...");
        let _ = self.kill_switch.disable();

        remove_virtual_tun_routes();

        let mut st = self.state.lock().await;
        if let Some(ref route) = st.active_bypass_route {
            remove_server_bypass_route(route);
        }
        st.active_bypass_route = None;

        if let Some(mut sidecar) = st.sidecar_process.take() {
            let _ = sidecar.terminate();
        }
        if let Some(mut core) = st.core_process.take() {
            let _ = core.terminate();
        }
        st.status = ConnectionStatus::Disconnected;
        st.active_profile = None;
        st.connected_at = None;
        st.tun_enabled = false;
        st.kill_switch_enabled = false;
        drop(st);

        let runtime_wintun = self.program_data_dir.join("runtime").join("wintun.dll");
        let _ = dismantle_wintun_adapter(if runtime_wintun.is_file() { Some(&runtime_wintun) } else { None });

        flush_dns_cache();
        self.session_journal.clear();
        info!("BusterVPNService disconnect teardown complete");
    }

    async fn handle_ipc(&self, req: IpcRequest) -> IpcResponse {
        match req.method.as_str() {
            "Ping" => IpcResponse {
                id: req.id,
                result: Some(json!("Pong")),
                error: None,
            },
            "GetStatus" => {
                let st = self.state.lock().await;
                let report = ServiceStatusReport {
                    status: st.status,
                    active_profile: st.active_profile.clone(),
                    connected_at: st.connected_at,
                    core_pid: st.core_process.as_ref().map(|c| c.pid()),
                    tun_enabled: st.tun_enabled,
                    kill_switch: st.kill_switch_enabled,
                    auto_connect_on_boot: st.auto_connect_on_boot,
                };
                IpcResponse {
                    id: req.id,
                    result: serde_json::to_value(report).ok(),
                    error: None,
                }
            }
            "ConnectTunnel" => {
                let params = match req.params {
                    Some(val) => match serde_json::from_value::<ConnectTunnelParams>(val) {
                        Ok(p) => p,
                        Err(e) => {
                            return IpcResponse {
                                id: req.id,
                                result: None,
                                error: Some(format!("Invalid connect params: {}", e)),
                            };
                        }
                    },
                    None => {
                        return IpcResponse {
                            id: req.id,
                            result: None,
                            error: Some("Missing connect params".to_string()),
                        };
                    }
                };

                match self.do_connect(params).await {
                    Ok(_) => IpcResponse {
                        id: req.id,
                        result: Some(json!({"success": true})),
                        error: None,
                    },
                    Err(e) => IpcResponse {
                        id: req.id,
                        result: None,
                        error: Some(e.to_string()),
                    },
                }
            }
            "DisconnectTunnel" => {
                self.do_disconnect().await;
                IpcResponse {
                    id: req.id,
                    result: Some(json!({"success": true})),
                    error: None,
                }
            }
            "GetLogs" => {
                let st = self.state.lock().await;
                let logs = st.recent_logs.clone();
                IpcResponse {
                    id: req.id,
                    result: serde_json::to_value(logs).ok(),
                    error: None,
                }
            }
            "SetAutoConnectOnBoot" => {
                let enabled = req.params.and_then(|v| v.as_bool()).unwrap_or(false);
                let mut st = self.state.lock().await;
                st.auto_connect_on_boot = enabled;
                let cfg = ServiceConfig {
                    auto_connect_on_boot: enabled,
                    last_connect_params: st.last_connect_params.clone(),
                };
                drop(st);
                let _ = self.save_config(&cfg).await;

                IpcResponse {
                    id: req.id,
                    result: Some(json!({"auto_connect_on_boot": enabled})),
                    error: None,
                }
            }
            "GetServiceConfig" => {
                let cfg = self.load_config().await;
                IpcResponse {
                    id: req.id,
                    result: serde_json::to_value(cfg).ok(),
                    error: None,
                }
            }
            "StopService" => {
                info!("IPC request: StopService received");
                self.stop_notify.notify_waiters();
                IpcResponse {
                    id: req.id,
                    result: Some(json!({"stopping": true})),
                    error: None,
                }
            }
            unknown => IpcResponse {
                id: req.id,
                result: None,
                error: Some(format!("Unknown method: {}", unknown)),
            },
        }
    }
}

impl Clone for ServiceContext {
    fn clone(&self) -> Self {
        Self {
            program_data_dir: self.program_data_dir.clone(),
            state: self.state.clone(),
            kill_switch: self.kill_switch.clone(),
            session_journal: self.session_journal.clone(),
            stop_notify: self.stop_notify.clone(),
        }
    }
}

async fn run_ipc_server(ctx: ServiceContext) -> Result<()> {
    info!("Starting IPC server on named pipe {}", PIPE_NAME);

    let mut first = true;
    loop {
        let server = match ServerOptions::new()
            .first_pipe_instance(first)
            .create(PIPE_NAME)
        {
            Ok(s) => {
                first = false;
                s
            }
            Err(e) => {
                error!("Failed to create named pipe {}: {}", PIPE_NAME, e);
                tokio::select! {
                    _ = ctx.stop_notify.notified() => {
                        info!("IPC server stopping on service shutdown");
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(500)) => continue,
                }
            }
        };

        tokio::select! {
            _ = ctx.stop_notify.notified() => {
                info!("IPC server stopping on service shutdown");
                break;
            }
            res = server.connect() => {
                match res {
                    Ok(_) => {
                        let client_ctx = ctx.clone();
                        tokio::spawn(async move {
                            handle_client_connection(server, client_ctx).await;
                        });
                    }
                    Err(e) => {
                        warn!("Pipe connection failed: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_client_connection(server: NamedPipeServer, ctx: ServiceContext) {
    let (reader, mut writer) = tokio::io::split(server);
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();

    while let Ok(n) = buf_reader.read_line(&mut line).await {
        if n == 0 {
            break;
        }

        let trimmed = line.trim();
        if !trimmed.is_empty() {
            let req: Result<IpcRequest, _> = serde_json::from_str(trimmed);
            let resp = match req {
                Ok(r) => ctx.handle_ipc(r).await,
                Err(e) => IpcResponse {
                    id: 0,
                    result: None,
                    error: Some(format!("Invalid JSON request: {}", e)),
                },
            };

            if let Ok(mut resp_str) = serde_json::to_string(&resp) {
                resp_str.push('\n');
                if let Err(e) = writer.write_all(resp_str.as_bytes()).await {
                    warn!("Failed to write IPC response: {}", e);
                    break;
                }
                let _ = writer.flush().await;
            }
        }
        line.clear();
    }
}

fn program_data_path() -> PathBuf {
    if let Ok(pd) = std::env::var("ProgramData") {
        PathBuf::from(pd).join("BusterVPN")
    } else {
        PathBuf::from(r"C:\ProgramData\BusterVPN")
    }
}

async fn async_main(stop_notify: Arc<Notify>, is_console: bool) -> Result<()> {
    if is_console {
        let stop_clone = stop_notify.clone();
        tokio::spawn(async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                info!("Ctrl-C received, stopping service...");
                stop_clone.notify_waiters();
            }
        });
    }

    let base_dir = program_data_path();
    let _ = tokio::fs::create_dir_all(&base_dir).await;

    let mut ctx = ServiceContext::new(base_dir);
    ctx.stop_notify = stop_notify.clone();
    ctx.startup_recovery().await;

    let ipc_ctx = ctx.clone();
    let ipc_handle = tokio::spawn(async move {
        if let Err(e) = run_ipc_server(ipc_ctx).await {
            error!("IPC server error: {}", e);
        }
    });

    stop_notify.notified().await;
    info!("Service stop signal received, shutting down active sessions...");
    ctx.do_disconnect().await;
    let _ = tokio::time::timeout(Duration::from_secs(3), ipc_handle).await;

    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let is_console = args.iter().any(|a| a == "--console");

    let _guard = if is_console {
        let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stdout());
        tracing_subscriber::fmt()
            .with_env_filter("info")
            .with_writer(non_blocking)
            .init();
        Some(guard)
    } else {
        let log_dir = program_data_path().join("logs");
        let _ = std::fs::create_dir_all(&log_dir);
        let file_appender = tracing_appender::rolling::daily(&log_dir, "service.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        tracing_subscriber::fmt()
            .with_env_filter("info")
            .with_writer(non_blocking)
            .init();
        Some(guard)
    };

    info!("BusterVPNService starting (version {})", env!("CARGO_PKG_VERSION"));

    if is_console {
        info!("Running in interactive console mode (--console)");
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("Failed to build Tokio multi-threaded runtime for console mode")?;
        let stop_notify = Arc::new(Notify::new());

        rt.block_on(async {
            let _ = async_main(stop_notify, true).await;
        });
        return Ok(());
    }

    // Attempt to run as Windows Service via service_dispatcher
    let run_res = windows_service::service_dispatcher::start(SERVICE_NAME, ffi_service_main);
    if let Err(e) = run_res {
        // If 1063 (ERROR_FAILED_SERVICE_CONTROLLER_CONNECT), fallback to console mode
        warn!("Service dispatcher failed (not launched by SCM): {}. Falling back to console mode.", e);
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("Failed to build Tokio multi-threaded runtime for fallback mode")?;
        let stop_notify = Arc::new(Notify::new());
        rt.block_on(async {
            let _ = async_main(stop_notify, true).await;
        });
    }

    Ok(())
}

windows_service::define_windows_service!(ffi_service_main, my_service_main);

fn my_service_main(_arguments: Vec<OsString>) {
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

    let stop_notify = Arc::new(Notify::new());
    let stop_notify_clone = stop_notify.clone();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                info!("Windows Service stop/shutdown event received");
                stop_notify_clone.notify_waiters();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = match service_control_handler::register(SERVICE_NAME, event_handler) {
        Ok(h) => h,
        Err(e) => {
            error!("Failed to register service control handler: {}", e);
            return;
        }
    };

    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    });

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to create Tokio multi-threaded runtime: {}", e);
            let _ = status_handle.set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::Stopped,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code: ServiceExitCode::Win32(1),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            });
            return;
        }
    };

    rt.block_on(async {
        let _ = async_main(stop_notify, false).await;
    });

    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    });
}
