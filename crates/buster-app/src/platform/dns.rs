//! DNS resolution utilities for server profiles.

use std::net::ToSocketAddrs;
use anyhow::{bail, Context, Result};

/// Resolves a server hostname or IP string to one or more IPv4 address strings.
pub fn resolve_server_ipv4(server_host: &str) -> Result<Vec<String>> {
    if server_host.parse::<std::net::Ipv4Addr>().is_ok() {
        return Ok(vec![server_host.to_string()]);
    }

    #[cfg(target_os = "windows")]
    buster_windows::flush_dns_cache();
    #[cfg(target_os = "linux")]
    crate::platform::flush_dns_cache();

    let addrs: Vec<String> = format!("{}:0", server_host)
        .to_socket_addrs()
        .with_context(|| format!("Failed to resolve server hostname '{}'", server_host))?
        .filter_map(|addr| match addr {
            std::net::SocketAddr::V4(v4) => Some(v4.ip().to_string()),
            _ => None,
        })
        .collect();

    if addrs.is_empty() {
        if server_host.parse::<std::net::Ipv4Addr>().is_ok() {
            return Ok(vec![server_host.to_string()]);
        }
        bail!("No IPv4 address resolved for host '{}'", server_host);
    }

    Ok(addrs)
}
