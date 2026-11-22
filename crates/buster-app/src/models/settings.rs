//! Application settings models and defaults.

use buster_core::models::{
    DnsProfile, EngineMode, RoutingMode, RoutingOptions, RoutingRule, SplitTunnelingConfig,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_routing_mode")]
    pub last_routing_mode: RoutingMode,

    #[serde(default)]
    pub is_tun_enabled: bool,

    #[serde(default = "default_true")]
    pub is_system_proxy_enabled: bool,

    #[serde(default = "default_true")]
    pub bypass_local_networks: bool,

    #[serde(default)]
    pub routing_rules: Vec<RoutingRule>,

    #[serde(default)]
    pub split_tunneling: SplitTunnelingConfig,

    #[serde(default)]
    pub dns_profile: DnsProfile,

    #[serde(default)]
    pub kill_switch: bool,

    #[serde(default)]
    pub start_with_windows: bool,

    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,

    #[serde(default = "default_true")]
    pub enable_ip_lookup: bool,

    #[serde(default = "default_language")]
    pub language: String,

    #[serde(default = "default_accent_theme")]
    pub accent_theme: String,

    #[serde(default)]
    pub last_profile_id: Option<Uuid>,

    #[serde(default)]
    pub engine_mode: EngineMode,

    #[serde(default)]
    pub auto_connect_service_boot: bool,

    #[serde(default = "Utc::now")]
    pub updated_at_utc: DateTime<Utc>,
}

fn default_routing_mode() -> RoutingMode {
    RoutingMode::SystemProxy
}

fn default_true() -> bool {
    true
}

fn default_language() -> String {
    "ru".to_string()
}

fn default_accent_theme() -> String {
    "cyber-indigo".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            last_routing_mode: RoutingMode::SystemProxy,
            is_tun_enabled: false,
            is_system_proxy_enabled: true,
            bypass_local_networks: true,
            routing_rules: Vec::new(),
            split_tunneling: SplitTunnelingConfig::default(),
            dns_profile: DnsProfile::default(),
            kill_switch: false,
            start_with_windows: false,
            minimize_to_tray: true,
            enable_ip_lookup: true,
            language: "ru".to_string(),
            accent_theme: "cyber-indigo".to_string(),
            last_profile_id: None,
            engine_mode: EngineMode::SingBox,
            auto_connect_service_boot: false,
            updated_at_utc: Utc::now(),
        }
    }
}

impl AppSettings {
    /// Computes the effective routing mode based on independent switches.
    pub fn effective_routing_mode(&self) -> RoutingMode {
        RoutingMode::from_switches(self.is_tun_enabled, self.is_system_proxy_enabled)
    }

    /// Converts settings to `RoutingOptions` required by config builders.
    pub fn to_routing_options(&self) -> RoutingOptions {
        RoutingOptions {
            bypass_local_networks: self.bypass_local_networks,
            rules: self.routing_rules.iter().filter(|r| r.enabled).cloned().collect(),
            split_tunneling: self.split_tunneling.clone(),
            dns_profile: self.dns_profile,
        }
    }
}
