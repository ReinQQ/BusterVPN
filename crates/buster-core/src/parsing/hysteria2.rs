use std::sync::OnceLock;
use regex::Regex;
use thiserror::Error;
use crate::models::{BackendKind, Hysteria2ClientConfig, ParsedServer};
use crate::parsing::encoding::{
    parse_bool, parse_query, unescape_query_value,
};
use crate::parsing::validation::{is_valid_host, is_valid_port};

static PORT_SPEC_REGEX: OnceLock<Regex> = OnceLock::new();

fn port_spec_regex() -> &'static Regex {
    PORT_SPEC_REGEX.get_or_init(|| {
        Regex::new(r"^[1-9][0-9]{0,4}(?:-[1-9][0-9]{0,4})?(?:,[1-9][0-9]{0,4}(?:-[1-9][0-9]{0,4})?)*$")
            .expect("Invalid port spec regex")
    })
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Hysteria2ParseError {
    #[error("Link is missing scheme delimiter '://'")]
    MissingSchemeDelimiter,
    #[error("Unsupported scheme: '{0}' (expected hy2:// or hysteria2://)")]
    UnsupportedScheme(String),
    #[error("Hysteria2 link is missing authentication value")]
    MissingAuth,
    #[error("Hysteria2 authentication value is invalid")]
    InvalidAuth,
    #[error("Hysteria2 link has an invalid host")]
    InvalidHost,
    #[error("Hysteria2 link has malformed IPv6 brackets")]
    MalformedIpv6Brackets,
    #[error("Hysteria2 link has a malformed endpoint")]
    MalformedEndpoint,
    #[error("Hysteria2 port or port-hopping range is invalid")]
    InvalidPortSpec,
    #[error("Unsupported Hysteria2 obfuscation '{0}'")]
    UnsupportedObfs(String),
}

pub fn parse_hysteria2(link: &str) -> Result<ParsedServer, Hysteria2ParseError> {
    let trimmed = link.trim().trim_start_matches('\u{FEFF}');
    let scheme_end = trimmed
        .find("://")
        .ok_or(Hysteria2ParseError::MissingSchemeDelimiter)?;

    let scheme = trimmed[..scheme_end].to_ascii_lowercase();
    if !matches!(
        scheme.as_str(),
        "hy2" | "hysteria2" | "hy2+realm" | "hysteria2+realm"
    ) {
        return Err(Hysteria2ParseError::UnsupportedScheme(scheme));
    }

    let mut body = &trimmed[scheme_end + 3..];

    // Extract fragment name (#Name)
    let mut name = String::new();
    if let Some(hash_idx) = body.find('#') {
        name = unescape_query_value(&body[hash_idx + 1..]).trim().to_string();
        body = &body[..hash_idx];
    }

    // Extract query parameters (?key=val)
    let mut query = std::collections::HashMap::new();
    if let Some(question_idx) = body.find('?') {
        query = parse_query(&body[question_idx..]);
        body = &body[..question_idx];
    }

    let body = body.trim_end_matches('/');

    // Extract auth (password) before '@'
    let at_idx = body.find('@').ok_or(Hysteria2ParseError::MissingAuth)?;
    if at_idx == 0 {
        return Err(Hysteria2ParseError::MissingAuth);
    }

    let auth = unescape_query_value(&body[..at_idx]);
    if auth.is_empty() || auth.len() > 1024 || auth.chars().any(char::is_control) {
        return Err(Hysteria2ParseError::InvalidAuth);
    }

    let host_port_raw = &body[at_idx + 1..];
    let (host, mut port_spec) = parse_host_port_spec(host_port_raw)?;

    // If query has "mport", allow overriding or appending port-hopping specification
    if let Some(mport) = query.get("mport").filter(|s| !s.trim().is_empty()) {
        if port_spec == "443" && !mport.trim().is_empty() {
            port_spec = mport.trim().to_string();
            validate_port_spec(&port_spec)?;
        }
    }

    let first_port = extract_first_port(&port_spec)?;
    let server_address = format_server_address(&host, &port_spec);

    let obfs = query.get("obfs").map(|s| s.to_ascii_lowercase()).unwrap_or_default();
    if !obfs.is_empty() && obfs != "salamander" && obfs != "gecko" {
        return Err(Hysteria2ParseError::UnsupportedObfs(obfs));
    }

    let sni = query.get("sni").filter(|s| !s.trim().is_empty()).cloned();
    let insecure = parse_bool(query.get("insecure").map(|s| s.as_str()));
    let pin_sha256 = query.get("pinsha256").filter(|s| !s.trim().is_empty()).cloned();
    let ech = query.get("ech").filter(|s| !s.trim().is_empty()).cloned();
    let obfs_type = if obfs.is_empty() { None } else { Some(obfs) };
    let obfs_password = query
        .get("obfs-password")
        .filter(|s| !s.trim().is_empty())
        .cloned();

    let up_mbps = query
        .get("upmbps")
        .or_else(|| query.get("up"))
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0);

    let down_mbps = query
        .get("downmbps")
        .or_else(|| query.get("down"))
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0);

    let final_name = if name.is_empty() {
        format!("Hysteria2 · {}:{}", host, port_spec)
    } else {
        name
    };

    let config = Hysteria2ClientConfig {
        host: host.clone(),
        port: first_port,
        server_address,
        auth,
        sni,
        insecure,
        pin_sha256,
        ech,
        obfs_type,
        obfs_password,
        up_mbps,
        down_mbps,
    };

    Ok(ParsedServer {
        backend: BackendKind::Hysteria2,
        name: final_name,
        host: Some(host),
        port: first_port,
        xray: None,
        hysteria2: Some(config),
    })
}

fn parse_host_port_spec(value: &str) -> Result<(String, String), Hysteria2ParseError> {
    let (host, port_spec) = if value.starts_with('[') {
        let close = value.find(']').ok_or(Hysteria2ParseError::MalformedIpv6Brackets)?;
        let host = &value[1..close];
        let rest = &value[close + 1..];
        if !rest.is_empty() && !rest.starts_with(':') {
            return Err(Hysteria2ParseError::MalformedEndpoint);
        }
        let port_spec = if rest.len() > 1 { &rest[1..] } else { "443" };
        (host.to_string(), port_spec.to_string())
    } else {
        match value.rfind(':') {
            Some(colon) => {
                let host = &value[..colon];
                let port_spec = &value[colon + 1..];
                (host.to_string(), port_spec.to_string())
            }
            None => (value.to_string(), "443".to_string()),
        }
    };

    if !is_valid_host(&host) {
        return Err(Hysteria2ParseError::InvalidHost);
    }

    let trimmed_spec = port_spec.trim();
    let final_spec = if trimmed_spec.is_empty() {
        "443".to_string()
    } else {
        trimmed_spec.to_string()
    };

    validate_port_spec(&final_spec)?;

    Ok((host, final_spec))
}

fn validate_port_spec(spec: &str) -> Result<(), Hysteria2ParseError> {
    if !port_spec_regex().is_match(spec) {
        return Err(Hysteria2ParseError::InvalidPortSpec);
    }

    for part in spec.split(',') {
        if part.is_empty() {
            continue;
        }
        for bound in part.split('-') {
            let port = bound
                .parse::<u32>()
                .map_err(|_| Hysteria2ParseError::InvalidPortSpec)?;
            if !is_valid_port(port) {
                return Err(Hysteria2ParseError::InvalidPortSpec);
            }
        }
    }

    Ok(())
}

fn extract_first_port(spec: &str) -> Result<u16, Hysteria2ParseError> {
    let first_part = spec.split(',').next().unwrap_or(spec);
    let first_bound = first_part.split('-').next().unwrap_or(first_part);
    first_bound
        .parse::<u16>()
        .map_err(|_| Hysteria2ParseError::InvalidPortSpec)
}

fn format_server_address(host: &str, port_spec: &str) -> String {
    if host.contains(':') {
        format!("[{}]:{}", host, port_spec)
    } else {
        format!("{}:{}", host, port_spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hysteria2_standard() {
        let uri = "hysteria2://my-secret-password@example.com:443?sni=example.com&insecure=1#Production%20HY2";
        let parsed = parse_hysteria2(uri).expect("Should parse standard Hysteria2");

        assert_eq!(parsed.backend, BackendKind::Hysteria2);
        assert_eq!(parsed.name, "Production HY2");
        assert_eq!(parsed.host.as_deref(), Some("example.com"));
        assert_eq!(parsed.port, 443);

        let hy2 = parsed.hysteria2.expect("Config must be present");
        assert_eq!(hy2.auth, "my-secret-password");
        assert_eq!(hy2.sni.as_deref(), Some("example.com"));
        assert!(hy2.insecure);
        assert_eq!(hy2.server_address, "example.com:443");
    }

    #[test]
    fn test_hysteria2_port_hopping_and_obfs() {
        let uri = "hysteria2://auth-secret@example.com:443,5000-6000/?sni=real.example.com&insecure=0&obfs=salamander&obfs-password=obfs-secret&upmbps=40&downmbps=120#HY";
        let parsed = parse_hysteria2(uri).expect("Should parse port-hopping Hysteria2");

        assert_eq!(parsed.name, "HY");
        assert_eq!(parsed.port, 443);

        let hy2 = parsed.hysteria2.unwrap();
        assert_eq!(hy2.auth, "auth-secret");
        assert_eq!(hy2.server_address, "example.com:443,5000-6000");
        assert_eq!(hy2.sni.as_deref(), Some("real.example.com"));
        assert!(!hy2.insecure);
        assert_eq!(hy2.obfs_type.as_deref(), Some("salamander"));
        assert_eq!(hy2.obfs_password.as_deref(), Some("obfs-secret"));
        assert_eq!(hy2.up_mbps, Some(40));
        assert_eq!(hy2.down_mbps, Some(120));
    }

    #[test]
    fn test_hysteria2_ipv6_brackets() {
        let uri = "hy2://secret@[2001:db8::1]:8443#IPv6";
        let parsed = parse_hysteria2(uri).expect("Should parse IPv6 Hysteria2");

        assert_eq!(parsed.name, "IPv6");
        assert_eq!(parsed.host.as_deref(), Some("2001:db8::1"));
        assert_eq!(parsed.port, 8443);
        let hy2 = parsed.hysteria2.unwrap();
        assert_eq!(hy2.server_address, "[2001:db8::1]:8443");
    }

    #[test]
    fn test_hysteria2_invalid_obfs() {
        let uri = "hysteria2://secret@example.com:443?obfs=unknown";
        let err = parse_hysteria2(uri).unwrap_err();
        assert!(matches!(err, Hysteria2ParseError::UnsupportedObfs(_)));
    }

    #[test]
    fn test_hysteria2_invalid_port_spec() {
        let uri = "hysteria2://secret@example.com:999999";
        let err = parse_hysteria2(uri).unwrap_err();
        assert_eq!(err, Hysteria2ParseError::InvalidPortSpec);
    }
}
