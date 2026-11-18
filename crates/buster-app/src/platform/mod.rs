//! Cross-platform abstraction layer for BusterVPN.

pub mod port;
pub mod dns;
pub mod log;

pub use port::{allocate_proxy_pair, is_port_free, ProxyPortPair, PREFERRED_HTTP_PORT, PREFERRED_SOCKS_PORT};
pub use dns::resolve_server_ipv4;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "linux")]
pub use log::*;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub use log::*;
