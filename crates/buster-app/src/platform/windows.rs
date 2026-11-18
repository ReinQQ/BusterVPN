//! Windows-specific platform layer delegating to buster-windows.

#![cfg(target_os = "windows")]

pub use buster_windows::{
    add_server_bypass_route, apply_dns_servers, dismantle_wintun_adapter, find_physical_gateway,
    flush_dns_cache, is_elevated, is_pid_running, proxy::SystemProxySettings,
    remove_server_bypass_route, remove_virtual_tun_routes, CoreProcessManager, FirewallKillSwitch,
    LogLine, LogLevel, LogStream, NetworkMonitor, SafeJobObject, SessionJournal,
    SystemProxyManager, TunBypassRoute,
};
pub use buster_windows::proxy::disable_system_proxy;
pub use buster_windows::autostart::{configure_autostart, is_autostart_enabled};
