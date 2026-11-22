use std::path::PathBuf;
use anyhow::{bail, Context, Result};
use uuid::Uuid;

use buster_core::config::singbox_builder::{
    build_singbox_config, build_xray_vless_detour_config_from_parsed, SingboxBuildParams,
};
use buster_core::models::{RoutingMode, ServerProfile, XrayProtocol};
use buster_core::parsing::parse_share_link;

use buster_windows::dpapi::unprotect;
use buster_windows::ipc::{BusterServiceClient, ConnectTunnelParams, WindowsServiceManager};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        print_usage();
        return Ok(());
    }

    match args[0].as_str() {
        "status" => cmd_status().await?,
        "connect" => {
            let profile_arg = parse_flag_value(&args[1..], "--profile");
            cmd_connect(profile_arg.as_deref()).await?;
        }
        "disconnect" => cmd_disconnect().await?,
        "logs" => {
            let tail_str = parse_flag_value(&args[1..], "--tail");
            let tail = tail_str.and_then(|s| s.parse::<usize>().ok()).unwrap_or(25);
            cmd_logs(tail).await?;
        }
        "service" => {
            if args.len() < 2 {
                println!("Usage: busterctl service <install | uninstall | start | stop | status>");
                return Ok(());
            }
            cmd_service(&args[1])?;
        }
        "help" | "--help" | "-h" => print_usage(),
        other => {
            println!("Unknown command: '{}'\n", other);
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    println!(
        r#"BusterVPN Command-Line Controller (v{})

USAGE:
    busterctl <COMMAND> [OPTIONS]

COMMANDS:
    status                          Show current connection and service status
    connect [--profile <name|uuid>] Connect to VPN tunnel using specified or last profile
    disconnect                      Disconnect active VPN tunnel
    logs [--tail <count>]           Fetch recent engine log lines (default: 25)
    service <action>                Manage Windows Service (install, uninstall, start, stop, status)
    help                            Print this help message

EXAMPLES:
    busterctl status
    busterctl connect --profile "Germany-01"
    busterctl disconnect
    busterctl logs --tail 50
    busterctl service start
"#,
        env!("CARGO_PKG_VERSION")
    );
}

fn parse_flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == flag && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
        if args[i].starts_with(&format!("{}=", flag)) {
            return Some(args[i].trim_start_matches(&format!("{}=", flag)).to_string());
        }
        i += 1;
    }
    None
}

async fn cmd_status() -> Result<()> {
    let svc_state = WindowsServiceManager::query_status();
    println!("Windows Service: {:?}", svc_state);

    if !BusterServiceClient::is_service_available().await {
        println!("\nBusterVPNService is not currently reachable over named pipe.");
        return Ok(());
    }

    match BusterServiceClient::get_status().await {
        Ok(st) => {
            println!("\nTunnel Connection Status:");
            println!("  State:                {:?}", st.status);
            println!("  Active Profile:       {}", st.active_profile.as_deref().unwrap_or("None"));
            println!(
                "  Connected At:         {}",
                st.connected_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| "N/A".to_string())
            );
            println!("  Core Process PID:     {}", st.core_pid.map(|p| p.to_string()).unwrap_or_else(|| "N/A".to_string()));
            println!("  TUN Virtual Adapter:  {}", if st.tun_enabled { "Active" } else { "Inactive" });
            println!("  Kill Switch Firewall: {}", if st.kill_switch { "Active" } else { "Inactive" });
            println!("  Auto-Connect on Boot: {}", if st.auto_connect_on_boot { "Enabled" } else { "Disabled" });
        }
        Err(e) => {
            println!("Error querying status from service: {}", e);
        }
    }

    Ok(())
}

async fn cmd_disconnect() -> Result<()> {
    if !BusterServiceClient::is_service_available().await {
        bail!("BusterVPNService is not running. Please start the service first ('busterctl service start').");
    }

    println!("Sending disconnect signal to BusterVPNService...");
    BusterServiceClient::disconnect_tunnel().await?;
    println!("VPN tunnel disconnected successfully.");
    Ok(())
}

async fn cmd_logs(tail: usize) -> Result<()> {
    if !BusterServiceClient::is_service_available().await {
        bail!("BusterVPNService is not running. Cannot retrieve logs.");
    }

    let logs = BusterServiceClient::get_logs().await?;
    if logs.is_empty() {
        println!("No logs recorded in current session.");
        return Ok(());
    }

    let start = if logs.len() > tail { logs.len() - tail } else { 0 };
    for line in &logs[start..] {
        println!(
            "[{}] [{:?}] [{:?}] {}",
            line.timestamp.format("%H:%M:%S"),
            line.level,
            line.stream,
            line.text
        );
    }

    Ok(())
}

async fn cmd_connect(profile_target: Option<&str>) -> Result<()> {
    if !BusterServiceClient::is_service_available().await {
        bail!("BusterVPNService is not running. Please start it with 'busterctl service start'.");
    }

    let profiles = load_profiles()?;
    if profiles.is_empty() {
        bail!("No server profiles found in BusterVPN profile storage.");
    }

    let profile = if let Some(target) = profile_target {
        if let Ok(target_uuid) = Uuid::parse_str(target) {
            profiles.iter().find(|p| p.id == target_uuid)
        } else {
            profiles.iter().find(|p| {
                p.name.eq_ignore_ascii_case(target) || p.name.to_lowercase().contains(&target.to_lowercase())
            })
        }
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Profile '{}' not found", target))?
    } else {
        profiles
            .iter()
            .find(|p| p.is_favorite)
            .cloned()
            .unwrap_or_else(|| profiles[0].clone())
    };

    println!("Connecting to profile: {} ({})", profile.name, profile.endpoint());

    let raw_link = profile
        .raw_link
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Profile has no raw share link"))?;
    let parsed = parse_share_link(raw_link)
        .context("Failed to parse profile link payload")?;

    let host = parsed.host.clone().unwrap_or_else(|| "127.0.0.1".to_string());
    let server_ips = buster_windows::resolve_server_ipv4(&host).unwrap_or_default();

    let candidate_cores_dirs = [
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("cores"))),
        Some(std::path::PathBuf::from(r"C:\Program Files\BusterVPN\cores")),
        Some(std::path::PathBuf::from(r"C:\BUSTERVPN\antivpnv2\crates\buster-app\cores")),
        Some(std::path::PathBuf::from(r"C:\ProgramData\BusterVPN\runtime")),
    ];
    let found_cores = candidate_cores_dirs.into_iter().flatten().find(|d| d.join("sing-box.exe").is_file());

    let mut params = SingboxBuildParams::new(&parsed, RoutingMode::Tun);
    if let Some(ref cd) = found_cores {
        params.working_dir = Some(cd);
    }
    params.resolved_server_ips = Some(server_ips.clone());
    let is_vless = parsed
        .xray
        .as_ref()
        .map_or(false, |x| x.protocol == XrayProtocol::Vless);

    let mut sidecar_config: Option<String> = None;
    let mut sidecar_filename: Option<String> = None;
    let mut sidecar_args: Option<Vec<String>> = None;

    if is_vless {
        let detour_port = 10810;
        if let Ok(xray_cfg) = build_xray_vless_detour_config_from_parsed(&parsed, detour_port) {
            params.vless_socks_detour_port = Some(detour_port);
            sidecar_config = Some(xray_cfg);
            sidecar_filename = Some("xray_detour_config.json".to_string());
            sidecar_args = Some(vec!["run".to_string(), "-c".to_string(), "xray_detour_config.json".to_string()]);
        }
    }

    let config_content = build_singbox_config(&params)
        .context("Failed to build Sing-box configuration")?;

    let core_kind = "sing-box".to_string();
    let config_filename = "singbox_config.json".to_string();
    let core_args = vec!["run".to_string(), "-c".to_string(), "singbox_config.json".to_string()];

    let params = ConnectTunnelParams {
        profile_name: profile.name.clone(),
        core_kind,
        config_content,
        config_filename,
        core_args,
        server_host: host,
        server_ips,
        dns_servers: vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()],
        kill_switch: true,
        tun_enabled: true,
        cores_dir: found_cores.map(|p| p.to_string_lossy().to_string()),
        sidecar_config,
        sidecar_filename,
        sidecar_args,
    };

    BusterServiceClient::connect_tunnel(params).await?;
    println!("VPN tunnel connected successfully.");
    Ok(())
}

fn cmd_service(action: &str) -> Result<()> {
    match action.to_lowercase().as_str() {
        "status" => {
            let status = WindowsServiceManager::query_status();
            println!("BusterVPNService SCM Status: {:?}", status);
        }
        "install" => {
            let exe_path = find_service_executable()?;
            println!("Installing BusterVPNService from: {}", exe_path.display());
            WindowsServiceManager::install_service(&exe_path)?;
            println!("BusterVPNService installed successfully.");
        }
        "uninstall" => {
            println!("Uninstalling BusterVPNService...");
            WindowsServiceManager::uninstall_service()?;
            println!("BusterVPNService uninstalled successfully.");
        }
        "start" => {
            println!("Starting BusterVPNService...");
            WindowsServiceManager::start_service()?;
            println!("Start signal sent to BusterVPNService.");
        }
        "stop" => {
            println!("Stopping BusterVPNService...");
            WindowsServiceManager::stop_service()?;
            println!("Stop signal sent to BusterVPNService.");
        }
        other => {
            println!("Unknown service action: '{}'. Valid actions: install, uninstall, start, stop, status", other);
        }
    }
    Ok(())
}

fn find_service_executable() -> Result<PathBuf> {
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            let cand = dir.join("buster-service.exe");
            if cand.is_file() {
                return Ok(cand);
            }
        }
    }

    let default_inst = PathBuf::from(r"C:\Program Files\BusterVPN\buster-service.exe");
    if default_inst.is_file() {
        return Ok(default_inst);
    }

    let default_inst2 = PathBuf::from(r"C:\BUSTERVPN\buster-service.exe");
    if default_inst2.is_file() {
        return Ok(default_inst2);
    }

    bail!("Could not locate buster-service.exe. Please ensure buster-service.exe is next to busterctl.exe or installed in Program Files.")
}

fn load_profiles() -> Result<Vec<ServerProfile>> {
    let candidate_paths = [
        dirs_app_data().join("profiles.dat"),
        PathBuf::from(r"C:\ProgramData\BusterVPN\profiles.dat"),
    ];

    for path in &candidate_paths {
        if path.is_file() {
            if let Ok(data) = std::fs::read(path) {
                if let Ok(decrypted) = unprotect(&data, "profile_store") {
                    if let Ok(profiles) = serde_json::from_slice::<Vec<ServerProfile>>(&decrypted) {
                        return Ok(profiles);
                    }
                }
            }
        }
    }

    Ok(Vec::new())
}

fn dirs_app_data() -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(local).join("BusterVPN")
    } else {
        PathBuf::from(".bustervpn")
    }
}
