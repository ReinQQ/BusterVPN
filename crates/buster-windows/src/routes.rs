//! Network routing, physical gateway discovery, and TUN bypass route management.
//!
//! Provides utilities to:
//! - Check process administrator elevation (required for TUN/Wintun & route modification).
//! - Discover the physical default gateway via `route print -4` or IP Helper table.
//! - Add/remove host bypass routes (`route ADD <ip> MASK 255.255.255.255 <gateway> METRIC 1`)
//!   to prevent encrypted VPN core transport from routing into the TUN adapter (DNS/transport loop).
//! - Apply static DNS resolvers to the TUN interface via `netsh.exe`.

use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use std::path::Path;
use std::process::Command;
use std::os::windows::process::CommandExt;
use crate::error::WindowsError;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone)]
pub struct TunBypassRoute {
    pub server_addresses: Vec<String>,
    pub added_addresses: Vec<String>,
    pub physical_gateway: String,
    pub physical_interface_name: Option<String>,
    pub physical_local_address: Option<String>,
}

/// Checks if the current process is running with administrative privileges (elevated token).
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = windows::Win32::Foundation::HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut return_length = 0u32;
        let success = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut return_length,
        );

        let _ = CloseHandle(token);

        success.is_ok() && elevation.TokenIsElevated != 0
    }
}

/// Verifies that `wintun.dll` exists and has a plausible file size.
pub fn verify_wintun(wintun_path: &Path) -> Result<(), WindowsError> {
    if !wintun_path.exists() {
        return Err(WindowsError::ProcessStart(format!(
            "wintun.dll not found at '{}'. Please place wintun.dll in the core directory",
            wintun_path.display()
        )));
    }

    let metadata = std::fs::metadata(wintun_path).map_err(|e| {
        WindowsError::ProcessStart(format!("Failed to read wintun.dll metadata: {}", e))
    })?;

    if metadata.len() < 16 * 1024 {
        return Err(WindowsError::ProcessStart(
            "wintun.dll is present but suspiciously small (<16 KB). Please use official Wintun runtime".to_string(),
        ));
    }

    Ok(())
}

/// Resolves a server host name (or IP string) to distinct IPv4 addresses.
pub fn resolve_server_ipv4(server_host: &str) -> Result<Vec<String>, WindowsError> {
    let host = server_host.trim().trim_matches('[').trim_matches(']');
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Ok(vec![ip.to_string()]);
    }

    // Try resolving host:443 to get socket addresses
    let socket_str = format!("{}:443", host);
    if let Ok(addrs) = socket_str.to_socket_addrs() {
        let mut ips = Vec::new();
        for addr in addrs {
            if let IpAddr::V4(v4) = addr.ip() {
                let s = v4.to_string();
                if !ips.contains(&s) {
                    ips.push(s);
                }
            }
        }
        if !ips.is_empty() {
            return Ok(ips);
        }
    }

    Err(WindowsError::ProcessStart(format!(
        "Failed to resolve server hostname '{}' to any IPv4 address",
        server_host
    )))
}

/// Discovers the physical default gateway by parsing `route PRINT -4`.
pub fn find_physical_gateway() -> Result<String, WindowsError> {
    let output = Command::new("route")
        .args(["PRINT", "-4"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| WindowsError::ProcessStart(format!("Failed to execute 'route PRINT -4': {}", e)))?;

    if !output.status.success() {
        return Err(WindowsError::ProcessStart(
            "route PRINT command returned non-zero exit code".to_string(),
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    parse_default_gateway(&text).ok_or_else(|| {
        WindowsError::ProcessStart("No physical IPv4 default gateway found in routing table".to_string())
    })
}

/// Parses the default IPv4 gateway (`0.0.0.0` mask `0.0.0.0`) from `route PRINT -4` output.
pub fn parse_default_gateway(route_output: &str) -> Option<String> {
    // Looking for lines like:
    // Network Destination        Netmask          Gateway       Interface  Metric
    //           0.0.0.0          0.0.0.0      192.168.1.1    192.168.1.50      25
    let mut best_gateway = None;
    let mut lowest_metric = u32::MAX;

    for line in route_output.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() >= 5
            && tokens[0] == "0.0.0.0"
            && tokens[1] == "0.0.0.0"
        {
            let gateway = tokens[2];
            let metric: u32 = tokens.last().and_then(|m| m.parse().ok()).unwrap_or(u32::MAX);

            // Filter out non-routable gateways or loopback
            if gateway != "On-link" && gateway != "127.0.0.1" && gateway != "0.0.0.0" {
                if metric < lowest_metric {
                    lowest_metric = metric;
                    best_gateway = Some(gateway.to_string());
                }
            }
        }
    }

    best_gateway
}

/// Installs host bypass routes for the VPN server IP(s) via the physical gateway.
/// This prevents core connection packets from being caught in the TUN default route.
pub fn add_server_bypass_route(
    server_host: &str,
    physical_gateway: Option<&str>,
) -> Result<TunBypassRoute, WindowsError> {
    let addresses = resolve_server_ipv4(server_host)?;
    let gateway = match physical_gateway {
        Some(gw) => gw.to_string(),
        None => find_physical_gateway()?,
    };

    let mut added = Vec::new();

    for addr in &addresses {
        let add_out = Command::new("route")
            .args(["ADD", addr, "MASK", "255.255.255.255", &gateway, "METRIC", "1"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        match add_out {
            Ok(output) if output.status.success() => {
                added.push(addr.clone());
            }
            Ok(output) => {
                let err_text = format!("{} {}", String::from_utf8_lossy(&output.stderr), String::from_utf8_lossy(&output.stdout));
                if err_text.contains("already exists") || err_text.contains("уже существует") {
                    // Try to CHANGE the existing route
                    let change_out = Command::new("route")
                        .args(["CHANGE", addr, "MASK", "255.255.255.255", &gateway, "METRIC", "1"])
                        .creation_flags(CREATE_NO_WINDOW)
                        .output();

                    if let Ok(c_out) = change_out {
                        if c_out.status.success() {
                            added.push(addr.clone());
                            continue;
                        }
                    }
                }
                tracing::warn!("Could not add bypass route for {}: {}", addr, err_text);
            }
            Err(e) => {
                tracing::warn!("Failed to execute route ADD for {}: {}", addr, e);
            }
        }
    }

    tracing::info!("VPN server bypass routes installed via {}: {:?}", gateway, added);

    Ok(TunBypassRoute {
        server_addresses: addresses,
        added_addresses: added,
        physical_gateway: gateway,
        physical_interface_name: None,
        physical_local_address: None,
    })
}

/// Removes all host bypass routes previously installed by BusterVPN.
pub fn remove_server_bypass_route(bypass_route: &TunBypassRoute) {
    for addr in &bypass_route.added_addresses {
        let _ = Command::new("route")
            .args(["DELETE", addr])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
    tracing::info!("VPN server bypass routes removed: {:?}", bypass_route.added_addresses);
}

/// Applies static DNS servers to a network interface using `netsh.exe`.
pub fn apply_dns_servers(interface_name: &str, dns_servers: &[String]) -> Result<bool, WindowsError> {
    let ipv4_dns: Vec<&String> = dns_servers
        .iter()
        .filter(|ip| ip.parse::<Ipv4Addr>().is_ok())
        .collect();

    if ipv4_dns.is_empty() {
        return Ok(false);
    }

    // Set primary DNS: netsh interface ip set dns name="<interface>" static <primary> validate=no
    let primary = ipv4_dns[0];
    let output = Command::new("netsh")
        .args([
            "interface",
            "ip",
            "set",
            "dns",
            &format!("name={}", interface_name),
            "static",
            primary,
            "validate=no",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| WindowsError::ProcessStart(format!("Failed to run netsh primary DNS: {}", e)))?;

    if !output.status.success() {
        let err = format!("{} {}", String::from_utf8_lossy(&output.stderr), String::from_utf8_lossy(&output.stdout));
        tracing::warn!("Failed to set primary DNS on {}: {}", interface_name, err);
        return Ok(false);
    }

    // Add secondary DNS servers: netsh interface ip add dns name="<interface>" <sec> index=<idx> validate=no
    for (idx, sec) in ipv4_dns.iter().skip(1).enumerate() {
        let _ = Command::new("netsh")
            .args([
                "interface",
                "ip",
                "add",
                "dns",
                &format!("name={}", interface_name),
                sec,
                &format!("index={}", idx + 2),
                "validate=no",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }

    tracing::info!("Applied DNS servers to {}: {:?}", interface_name, ipv4_dns);
    Ok(true)
}

type WintunOpenAdapterFn = unsafe extern "system" fn(pool: *const u16, name: *const u16) -> *mut std::ffi::c_void;
type WintunCloseAdapterFn = unsafe extern "system" fn(adapter: *mut std::ffi::c_void);

/// Removes split default virtual routes installed during TUN session.
/// 0.0.0.0/1 and 128.0.0.0/1 take priority over the physical 0.0.0.0/0 route;
/// deleting them un-blackholes all IPv4 traffic back to the physical gateway.
pub fn remove_virtual_tun_routes() {
    let _ = Command::new("route")
        .args(["DELETE", "0.0.0.0", "MASK", "128.0.0.0"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    let _ = Command::new("route")
        .args(["DELETE", "128.0.0.0", "MASK", "128.0.0.0"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    // Delete split IPv6 routes if present
    let _ = Command::new("netsh")
        .args(["interface", "ipv6", "delete", "route", "::/1", "interface=BusterVPN_TUN"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    let _ = Command::new("netsh")
        .args(["interface", "ipv6", "delete", "route", "8000::/1", "interface=BusterVPN_TUN"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    tracing::info!("Removed virtual TUN routes (0.0.0.0/1, 128.0.0.0/1)");
}

/// Flushes the Windows DNS resolver cache to clear stale tunnel DNS entries.
pub fn flush_dns_cache() {
    use windows::core::s;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    let mut flushed_api = false;
    let wide_dns: Vec<u16> = "dnsapi.dll".encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if let Ok(module) = LoadLibraryW(PCWSTR(wide_dns.as_ptr())) {
            if let Some(proc) = GetProcAddress(module, s!("DnsFlushResolverCache")) {
                type DnsFlushFn = unsafe extern "system" fn() -> i32;
                let flush_fn: DnsFlushFn = std::mem::transmute(proc);
                let _ = flush_fn();
                flushed_api = true;
            }
            let _ = FreeLibrary(module);
        }
    }

    let _ = Command::new("ipconfig")
        .arg("/flushdns")
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    tracing::info!("DNS cache flushed (DnsFlushResolverCache: {})", flushed_api);
}

/// Dismantles and cleanly uninstalls the Wintun adapter ("BusterVPN_TUN" / "BusterVPN").
///
/// 1. Removes custom virtual routes (0.0.0.0/1, 128.0.0.0/1) FIRST to prevent traffic blackholing.
/// 2. Clears static DNS from the TUN adapter.
/// 3. If wintun.dll is present, dynamically calls WintunOpenAdapter and WintunCloseAdapter
///    to release driver handles and trigger adapter deletion.
/// 4. Executes netsh interface shutdown commands on "BusterVPN_TUN" and "BusterVPN".
/// 5. If an orphaned PnP device exists (SWD\Wintun\...), removes it via pnputil.
/// 6. Flushes the DNS resolver cache.
/// 7. Resets WinHTTP proxy to prevent system services targeting dead ports.
pub fn dismantle_wintun_adapter(wintun_dll_path: Option<&Path>) -> Result<(), WindowsError> {
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    // 1. Remove virtual split routes FIRST so network traffic is unblocked
    remove_virtual_tun_routes();

    // 2. Delete static DNS on TUN adapters
    for iface in &["BusterVPN_TUN", "BusterVPN"] {
        let _ = Command::new("netsh")
            .args(["interface", "ip", "delete", "dns", &format!("name={}", iface), "all"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }

    // 3. Dismantle Wintun adapter via wintun.dll if available
    let candidate_paths = [
        wintun_dll_path.map(|p| p.to_path_buf()),
        Some(PathBuf::from(r"C:\Program Files\BusterVPN\cores\wintun.dll")),
        Some(PathBuf::from(r"C:\BUSTERVPN\cores\wintun.dll")),
    ];

    let mut dll_loaded = false;
    for candidate in candidate_paths.into_iter().flatten() {
        if candidate.is_file() {
            let wide_path: Vec<u16> = candidate.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
            unsafe {
                if let Ok(module) = LoadLibraryW(PCWSTR(wide_path.as_ptr())) {
                    if let Some(open_proc) = GetProcAddress(module, windows::core::s!("WintunOpenAdapter")) {
                        if let Some(close_proc) = GetProcAddress(module, windows::core::s!("WintunCloseAdapter")) {
                            let open_fn: WintunOpenAdapterFn = std::mem::transmute(open_proc);
                            let close_fn: WintunCloseAdapterFn = std::mem::transmute(close_proc);

                            for pool in &["BusterVPN", "Wintun"] {
                                for name in &["BusterVPN_TUN", "BusterVPN"] {
                                    let pool_w: Vec<u16> = pool.encode_utf16().chain(std::iter::once(0)).collect();
                                    let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
                                    let handle = open_fn(pool_w.as_ptr(), name_w.as_ptr());
                                    if !handle.is_null() {
                                        tracing::info!("Dismantling active/orphaned Wintun adapter '{}' in pool '{}'", name, pool);
                                        close_fn(handle);
                                    }
                                }
                            }
                        }
                    }
                    let _ = FreeLibrary(module);
                    dll_loaded = true;
                    break;
                }
            }
        }
    }

    if !dll_loaded {
        tracing::debug!("wintun.dll not directly loaded for adapter teardown; proceeding with system cleanup");
    }

    // 4. Disable interfaces via netsh
    for iface in &["BusterVPN_TUN", "BusterVPN"] {
        let _ = Command::new("netsh")
            .args(["interface", "set", "interface", &format!("name={}", iface), "admin=disable"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }

    // 5. Remove orphaned PnP Wintun devices via pnputil if any are left
    cleanup_orphaned_pnp_adapters();

    // 6. Flush DNS resolver cache
    flush_dns_cache();

    // 7. Reset WinHTTP proxy
    let _ = Command::new("netsh")
        .args(["winhttp", "reset", "proxy"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    Ok(())
}

fn cleanup_orphaned_pnp_adapters() {
    let output = Command::new("pnputil")
        .args(["/enum-devices", "/class", "Net"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        let mut current_instance: Option<String> = None;
        let mut is_wintun = false;

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.contains("SWD\\Wintun\\") || trimmed.contains("SWD\\WINTUN\\") {
                if let Some(pos) = trimmed.find("SWD\\") {
                    current_instance = Some(trimmed[pos..].trim().to_string());
                    is_wintun = true;
                }
            } else if trimmed.to_lowercase().contains("bustervpn") {
                if is_wintun {
                    if let Some(ref inst) = current_instance {
                        tracing::info!("Removing orphaned Wintun PnP device: {}", inst);
                        let _ = Command::new("pnputil")
                            .args(["/remove-device", inst])
                            .creation_flags(CREATE_NO_WINDOW)
                            .output();
                    }
                }
            } else if trimmed.is_empty() {
                current_instance = None;
                is_wintun = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_default_gateway() {
        let sample_output = r#"
===========================================================================
Interface List
 10...00 15 5d 12 34 56 ......Hyper-V Virtual Ethernet Adapter
  1...........................Software Loopback Interface 1
===========================================================================

IPv4 Route Table
===========================================================================
Active Routes:
Network Destination        Netmask          Gateway       Interface  Metric
          0.0.0.0          0.0.0.0      192.168.1.1     192.168.1.42     15
        127.0.0.0        255.0.0.0         On-link         127.0.0.1    331
      192.168.1.0    255.255.255.0         On-link      192.168.1.42    271
===========================================================================
"#;
        let gw = parse_default_gateway(sample_output);
        assert_eq!(gw, Some("192.168.1.1".to_string()));
    }

    #[test]
    fn test_dismantle_wintun_adapter_graceful() {
        let res = dismantle_wintun_adapter(None);
        assert!(res.is_ok());
    }

    #[test]
    fn test_remove_virtual_tun_routes() {
        remove_virtual_tun_routes();
    }

    #[test]
    fn test_flush_dns_cache() {
        flush_dns_cache();
    }
}
