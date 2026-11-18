//! Loopback proxy port allocator for SOCKS5 and HTTP inbound listeners.
//!
//! Tries preferred ports 10808/10809 first; if occupied, finds the next available
//! port pair in the 10808..65000 range. Tests both TCP and UDP binding to ensure
//! conflict-free operation before launching the core.

use std::net::{SocketAddrV4, Ipv4Addr, TcpListener, UdpSocket};
use crate::error::WindowsError;

pub const PREFERRED_SOCKS_PORT: u16 = 10808;
pub const PREFERRED_HTTP_PORT: u16 = 10809;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProxyPortPair {
    pub socks_port: u16,
    pub http_port: u16,
}

/// Checks if both TCP and UDP can bind to `127.0.0.1:port`.
pub fn is_port_free(port: u16) -> bool {
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    // Test TCP
    let tcp_ok = TcpListener::bind(addr).is_ok();
    if !tcp_ok {
        return false;
    }
    // Test UDP
    UdpSocket::bind(addr).is_ok()
}

/// Allocates a pair of free loopback ports for SOCKS5 and HTTP inbounds.
pub fn allocate_proxy_pair() -> Result<ProxyPortPair, WindowsError> {
    if is_port_free(PREFERRED_SOCKS_PORT) && is_port_free(PREFERRED_HTTP_PORT) {
        return Ok(ProxyPortPair {
            socks_port: PREFERRED_SOCKS_PORT,
            http_port: PREFERRED_HTTP_PORT,
        });
    }

    // Scan for available ports
    let mut free_socks = None;
    let mut free_http = None;

    for port in 10810..=65000 {
        if is_port_free(port) {
            if free_socks.is_none() {
                free_socks = Some(port);
            } else if free_http.is_none() {
                free_http = Some(port);
                break;
            }
        }
    }

    match (free_socks, free_http) {
        (Some(socks), Some(http)) => Ok(ProxyPortPair {
            socks_port: socks,
            http_port: http,
        }),
        _ => Err(WindowsError::ProcessStart(
            "No free local TCP/UDP port pair available for proxy listeners".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_proxy_pair() {
        let pair = allocate_proxy_pair().expect("should find free ports");
        assert_ne!(pair.socks_port, pair.http_port);
        assert!(pair.socks_port > 0);
        assert!(pair.http_port > 0);
    }
}
