use thiserror::Error;
use crate::models::{BackendKind, ParsedServer, TuicClientConfig};
use crate::parsing::encoding::{parse_bool, parse_query, unescape_query_value};
use crate::parsing::validation::{is_valid_host, is_valid_port};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TuicParseError {
    #[error("Link is missing scheme delimiter '://'")]
    MissingSchemeDelimiter,
    #[error("Unsupported scheme: '{0}' (expected tuic://)")]
    UnsupportedScheme(String),
    #[error("TUIC link has an invalid endpoint")]
    MalformedEndpoint,
    #[error("TUIC link has an invalid host")]
    InvalidHost,
    #[error("TUIC link has an invalid port")]
    InvalidPort,
    #[error("TUIC link is missing credentials (UUID or password)")]
    MissingCredentials,
}

pub fn parse_tuic(link: &str) -> Result<ParsedServer, TuicParseError> {
    let trimmed = link.trim().trim_start_matches('\u{FEFF}');
    let scheme_end = trimmed
        .find("://")
        .ok_or(TuicParseError::MissingSchemeDelimiter)?;

    let scheme = trimmed[..scheme_end].to_ascii_lowercase();
    if scheme != "tuic" {
        return Err(TuicParseError::UnsupportedScheme(scheme));
    }

    let mut body = &trimmed[scheme_end + 3..];

    let mut name = String::new();
    if let Some(hash_idx) = body.find('#') {
        name = unescape_query_value(&body[hash_idx + 1..]).trim().to_string();
        body = &body[..hash_idx];
    }

    let mut query = std::collections::HashMap::new();
    if let Some(question_idx) = body.find('?') {
        query = parse_query(&body[question_idx..]);
        body = &body[..question_idx];
    }

    // Body is [uuid:password@]host:port
    let (credentials, endpoint) = if let Some(at_idx) = body.rfind('@') {
        (&body[..at_idx], &body[at_idx + 1..])
    } else {
        ("", body)
    };

    let mut uuid = String::new();
    let mut password = String::new();

    if !credentials.is_empty() {
        if let Some(colon_idx) = credentials.find(':') {
            uuid = unescape_query_value(&credentials[..colon_idx]);
            password = unescape_query_value(&credentials[colon_idx + 1..]);
        } else {
            uuid = unescape_query_value(credentials);
        }
    }

    if let Some(q_uuid) = query.get("uuid") {
        if uuid.is_empty() {
            uuid = q_uuid.clone();
        }
    }
    if let Some(q_pass) = query.get("password").or_else(|| query.get("pass")) {
        if password.is_empty() {
            password = q_pass.clone();
        }
    }

    if uuid.is_empty() && password.is_empty() {
        return Err(TuicParseError::MissingCredentials);
    }

    // Endpoint: host:port or [ipv6]:port
    let (host, port) = parse_host_port(endpoint)?;

    let congestion_control = query
        .get("congestion_control")
        .or_else(|| query.get("congestion"))
        .cloned()
        .or_else(|| Some("bbr".to_string()));

    let alpn: Vec<String> = query
        .get("alpn")
        .map(|s| s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect())
        .unwrap_or_else(|| vec!["h3".to_string()]);

    let sni = query.get("sni").cloned().or_else(|| Some(host.clone()));
    let allow_insecure = query.get("allow_insecure").map(|v| parse_bool(Some(v.as_str()))).unwrap_or(false)
        || query.get("insecure").map(|v| parse_bool(Some(v.as_str()))).unwrap_or(false);
    let disable_sni = query.get("disable_sni").map(|v| parse_bool(Some(v.as_str()))).unwrap_or(false);

    let display_name = if name.is_empty() {
        format!("TUIC-{}:{}", host, port)
    } else {
        name
    };

    let config = TuicClientConfig {
        uuid,
        password,
        host: host.clone(),
        port,
        congestion_control,
        alpn,
        sni,
        allow_insecure,
        disable_sni,
    };

    Ok(ParsedServer {
        backend: BackendKind::SingBox,
        name: display_name,
        host: Some(host),
        port,
        xray: None,
        hysteria2: None,
        tuic: Some(config),
        wireguard: None,
        shadowtls: None,
        anytls: None,
    })
}

fn parse_host_port(endpoint: &str) -> Result<(String, u16), TuicParseError> {
    let ep = endpoint.trim().trim_end_matches('/');
    if ep.is_empty() {
        return Err(TuicParseError::MalformedEndpoint);
    }

    let (host, port_str) = if ep.starts_with('[') {
        let bracket_end = ep.find(']').ok_or(TuicParseError::MalformedEndpoint)?;
        let host = &ep[1..bracket_end];
        let rest = &ep[bracket_end + 1..];
        let colon = rest.find(':').ok_or(TuicParseError::MalformedEndpoint)?;
        (host, &rest[colon + 1..])
    } else {
        let colon = ep.rfind(':').ok_or(TuicParseError::MalformedEndpoint)?;
        (&ep[..colon], &ep[colon + 1..])
    };

    if !is_valid_host(host) {
        return Err(TuicParseError::InvalidHost);
    }

    let port: u16 = port_str.parse().map_err(|_| TuicParseError::InvalidPort)?;
    if !is_valid_port(port as u32) {
        return Err(TuicParseError::InvalidPort);
    }

    Ok((host.to_string(), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tuic_standard() {
        let link = "tuic://b53494e0-9e66-41ea-9c02-e20ecdd39ec2:mysecret@example.com:8443/?congestion_control=bbr&alpn=h3&sni=example.com#MyTUIC";
        let parsed = parse_tuic(link).expect("Valid TUIC link");
        assert_eq!(parsed.name, "MyTUIC");
        assert_eq!(parsed.host.as_deref(), Some("example.com"));
        assert_eq!(parsed.port, 8443);
        assert_eq!(parsed.backend, BackendKind::SingBox);

        let cfg = parsed.tuic.expect("TuicClientConfig exists");
        assert_eq!(cfg.uuid, "b53494e0-9e66-41ea-9c02-e20ecdd39ec2");
        assert_eq!(cfg.password, "mysecret");
        assert_eq!(cfg.congestion_control.as_deref(), Some("bbr"));
        assert_eq!(cfg.alpn, vec!["h3"]);
        assert_eq!(cfg.sni.as_deref(), Some("example.com"));
    }
}
