use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Host;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoutingMatchType {
    DomainSuffix,
    DomainKeyword,
    IpCidr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RoutingAction {
    #[default]
    Direct,
    Proxy,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: Uuid,
    pub value: String,
    pub match_type: RoutingMatchType,
    pub action: RoutingAction,
    pub enabled: bool,
    pub created_at_utc: DateTime<Utc>,
}

impl RoutingRule {
    pub fn new(value: impl Into<String>, match_type: RoutingMatchType, action: RoutingAction) -> Self {
        Self {
            id: Uuid::new_v4(),
            value: value.into(),
            match_type,
            action,
            enabled: true,
            created_at_utc: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingOptions {
    pub bypass_local_networks: bool,
    pub rules: Vec<RoutingRule>,
    #[serde(default)]
    pub split_tunneling: SplitTunnelingConfig,
    #[serde(default)]
    pub dns_profile: DnsProfile,
}

impl Default for RoutingOptions {
    fn default() -> Self {
        Self {
            bypass_local_networks: true,
            rules: Vec::new(),
            split_tunneling: SplitTunnelingConfig::default(),
            dns_profile: DnsProfile::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SplitTunnelingConfig {
    pub enabled: bool,
    #[serde(default)]
    pub mode: SplitTunnelMode, // "proxy_selected" | "bypass_selected"
    #[serde(default)]
    pub apps: Vec<String>,      // For backward compatibility
    #[serde(default)]
    pub bypass_apps: Vec<String>, // Applications that bypass the VPN (direct)
    #[serde(default)]
    pub proxy_apps: Vec<String>,  // Applications that route strictly through VPN (proxy)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum SplitTunnelMode {
    #[serde(rename = "proxy_selected")]
    ProxySelected,
    #[default]
    #[serde(rename = "bypass_selected")]
    BypassSelected,
}

impl Default for SplitTunnelingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: SplitTunnelMode::BypassSelected,
            apps: Vec::new(),
            bypass_apps: Vec::new(),
            proxy_apps: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DnsProfile {
    #[default]
    Standard,        // Cloudflare 1.1.1.1, Google 8.8.8.8
    AdGuardAdBlock,  // 94.140.14.14, 94.140.15.15
    CloudflareSafe,  // 1.1.1.2, 1.0.0.2 (Malware blocking)
    FamilyShield,    // 1.1.1.3, 1.0.0.3 (Malware + Adult content)
}

impl DnsProfile {
    pub fn dns_servers(&self) -> Vec<String> {
        match self {
            Self::Standard => vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()],
            Self::AdGuardAdBlock => vec!["94.140.14.14".to_string(), "94.140.15.15".to_string()],
            Self::CloudflareSafe => vec!["1.1.1.2".to_string(), "1.0.0.2".to_string()],
            Self::FamilyShield => vec!["1.1.1.3".to_string(), "1.0.0.3".to_string()],
        }
    }
}

/// Converts Unicode domains to ASCII/IDNA (punycode) format.
pub fn to_idn_ascii(domain: &str) -> String {
    let trimmed = domain.trim().trim_end_matches('.');
    if trimmed.is_empty() || trimmed.is_ascii() {
        return trimmed.to_string();
    }

    match Host::parse(trimmed) {
        Ok(Host::Domain(ascii)) => ascii,
        _ => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_idn_ascii() {
        assert_eq!(to_idn_ascii("example.com"), "example.com");
        assert_eq!(to_idn_ascii("рф"), "xn--p1ai");
        assert_eq!(to_idn_ascii("тест.рф"), "xn--e1aybc.xn--p1ai");
    }
}
