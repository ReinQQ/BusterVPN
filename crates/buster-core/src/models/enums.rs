use serde::{Deserialize, Serialize};

/// Backend core that owns the tunnel connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    #[default]
    Xray,
    Hysteria2,
    SingBox,
}

impl BackendKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Xray => "Xray",
            Self::Hysteria2 => "Hysteria 2",
            Self::SingBox => "Sing-box",
        }
    }
}

/// Selected engine architecture mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum EngineMode {
    #[default]
    SingBox,
    Classic,
}

/// How BusterVPN exposes the tunnel to Windows applications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RoutingMode {
    /// Configure WinINET/WinHTTP system proxy pointing at local listeners.
    #[default]
    SystemProxy,

    /// Route the machine through a Wintun adapter owned by the core.
    Tun,

    /// Start the core with both a TUN inbound and local HTTP/SOCKS listeners,
    /// then point WinINET at the local listeners while the rest of the machine
    /// follows the TUN default route.
    TunAndSystemProxy,

    /// Keep local HTTP/SOCKS listeners but leave Windows proxy settings and the
    /// routing table untouched. Applications opt in by using the local ports.
    LocalOnly,
}

impl RoutingMode {
    /// True when the core must create a Wintun adapter and default route.
    pub fn has_tun(&self) -> bool {
        matches!(self, Self::Tun | Self::TunAndSystemProxy)
    }

    /// True when WinINET/WinHTTP settings must point at the local listeners.
    pub fn has_system_proxy(&self) -> bool {
        matches!(self, Self::SystemProxy | Self::TunAndSystemProxy)
    }

    /// True when the core exposes local HTTP/SOCKS listeners.
    pub fn has_local_proxy(&self) -> bool {
        matches!(
            self,
            Self::SystemProxy | Self::Tun | Self::TunAndSystemProxy | Self::LocalOnly
        )
    }

    /// Combines the two independent exposure switches into a routing mode.
    pub fn from_switches(tun_enabled: bool, system_proxy_enabled: bool) -> Self {
        match (tun_enabled, system_proxy_enabled) {
            (true, true) => Self::TunAndSystemProxy,
            (true, false) => Self::Tun,
            (false, true) => Self::SystemProxy,
            (false, false) => Self::LocalOnly,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Faulted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerProtocol {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
    Hysteria2,
    Tuic,
    WireGuard,
    ShadowTls,
    AnyTls,
}

/// Selectable accent gradient for the dark obsidian shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum AccentTheme {
    /// Cyber indigo to cyan (default).
    #[default]
    Default = 0,
    /// Pastel pink.
    Piglet = 1,
    /// Deep violet to bright purple.
    Purplin = 2,
    /// Warm red to gold.
    Sunset = 3,
}
