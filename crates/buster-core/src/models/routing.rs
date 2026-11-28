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
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: SplitTunnelMode, // "proxy_selected" | "bypass_selected"
    #[serde(default)]
    pub apps: Vec<String>,
    #[serde(default)]
    pub bypass_apps: Vec<String>, // Legacy compatibility for older settings.json
    #[serde(default)]
    pub proxy_apps: Vec<String>,  // Legacy compatibility for older settings.json
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
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

/// Extracts a clean executable basename from any Windows/Unix path or raw filename.
pub fn normalize_process_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_matches('"').trim_matches('\'').trim();
    if trimmed.is_empty() {
        return None;
    }
    let basename = trimmed
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(trimmed)
        .trim();
    if basename.is_empty() {
        return None;
    }
    Some(basename.to_string())
}

fn escape_regex_literal(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len() + 8);
    for ch in input.chars() {
        if matches!(
            ch,
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
        ) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

/// Expands each process basename into standard Windows casing variants
/// (`original`, `lowercase`, `Capitalized`, `UPPERCASE`) without duplicates.
pub fn expand_process_name_variants(names: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut expanded = Vec::with_capacity(names.len() * 3);

    for name in names {
        let lower = name.to_ascii_lowercase();
        let upper = name.to_ascii_uppercase();
        let mut chars = lower.chars();
        let capitalized = match chars.next() {
            Some(first) => {
                let mut s = String::with_capacity(lower.len());
                s.push(first.to_ascii_uppercase());
                s.push_str(chars.as_str());
                s
            }
            None => String::new(),
        };

        for candidate in [name.clone(), lower, capitalized, upper] {
            if !candidate.is_empty() && seen.insert(candidate.clone()) {
                expanded.push(candidate);
            }
        }
    }

    expanded
}

/// Builds Go/RE2 case-insensitive `process_path_regex` patterns matching each executable basename.
pub fn build_process_path_regexes(names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|name| format!(r"(?i)(?:^|[\\/]){}$", escape_regex_literal(name)))
        .collect()
}

impl SplitTunnelingConfig {
    /// Returns deduplicated, basename-normalized process names for the active split-tunnel mode.
    /// Falls back to legacy `bypass_apps`/`proxy_apps` when `apps` is empty.
    pub fn normalized_process_names(&self) -> Vec<String> {
        let source: &[String] = if !self.apps.is_empty() {
            &self.apps
        } else {
            match self.mode {
                SplitTunnelMode::BypassSelected => &self.bypass_apps,
                SplitTunnelMode::ProxySelected => &self.proxy_apps,
            }
        };

        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::with_capacity(source.len());
        for entry in source {
            if let Some(name) = normalize_process_name(entry) {
                let key = name.to_ascii_lowercase();
                if seen.insert(key) {
                    result.push(name);
                }
            }
        }
        result
    }

    /// Returns expanded casing variants for `sing-box` `"process_name"` rules.
    pub fn expanded_process_names(&self) -> Vec<String> {
        expand_process_name_variants(&self.normalized_process_names())
    }

    /// Returns Go/RE2 case-insensitive regex patterns for `sing-box` `"process_path_regex"` rules.
    pub fn process_path_regexes(&self) -> Vec<String> {
        build_process_path_regexes(&self.normalized_process_names())
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
