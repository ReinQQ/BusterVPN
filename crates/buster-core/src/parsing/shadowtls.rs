use thiserror::Error;
use crate::models::{AnyTlsClientConfig, BackendKind, ParsedServer, ShadowTlsClientConfig};
use crate::parsing::encoding::{parse_query, unescape_query_value};
use crate::parsing::validation::{is_valid_host, is_valid_port};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ShadowTlsParseError {
    #[error("Link is missing scheme delimiter '://'")]
    MissingSchemeDelimiter,
    #[error("Unsupported scheme: '{0}' (expected shadowtls:// or anytls://)")]
    UnsupportedScheme(String),
    #[error("Link has an invalid endpoint")]
    MalformedEndpoint,
    #[error("Link has an invalid host")]
    InvalidHost,
    #[error("Link has an invalid port")]
    InvalidPort,
    #[error("Link is missing password")]
    MissingPassword,
}

pub fn parse_shadowtls_or_anytls(link: &str) -> Result<ParsedServer, ShadowTlsParseError> {
    let trimmed = link.trim().trim_start_matches('\u{FEFF}');
    let scheme_end = trimmed
        .find("://")
        .ok_or(ShadowTlsParseError::MissingSchemeDelimiter)?;

    let scheme = trimmed[..scheme_end].to_ascii_lowercase();
    let is_shadowtls = scheme == "shadowtls";
    let is_anytls = scheme == "anytls";

    if !is_shadowtls && !is_anytls {
        return Err(ShadowTlsParseError::UnsupportedScheme(scheme));
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

    let (password_raw, endpoint) = if let Some(at_idx) = body.rfind('@') {
        (&body[..at_idx], &body[at_idx + 1..])
    } else {
        ("", body)
    };

    let mut password = unescape_query_value(password_raw);
    if password.is_empty() {
        if let Some(q_pass) = query.get("password").or_else(|| query.get("pass")) {
            password = q_pass.clone();
        }
    }

    if password.is_empty() {
        return Err(ShadowTlsParseError::MissingPassword);
    }

    let (host, port) = parse_host_port(endpoint)?;
    let sni = query.get("sni").or_else(|| query.get("host")).cloned();

    if is_shadowtls {
        let version = query
            .get("version")
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or(3);

        let display_name = if name.is_empty() {
            format!("ShadowTLS-{}:{}", host, port)
        } else {
            name
        };

        let config = ShadowTlsClientConfig {
            host: host.clone(),
            port,
            password,
            sni,
            version,
        };

        Ok(ParsedServer {
            backend: BackendKind::SingBox,
            name: display_name,
            host: Some(host),
            port,
            xray: None,
            hysteria2: None,
            tuic: None,
            wireguard: None,
            shadowtls: Some(config),
            anytls: None,
        })
    } else {
        let alpn = query
            .get("alpn")
            .map(|s| s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect())
            .unwrap_or_else(|| vec!["h2".to_string(), "http/1.1".to_string()]);

        let display_name = if name.is_empty() {
            format!("AnyTLS-{}:{}", host, port)
        } else {
            name
        };

        let config = AnyTlsClientConfig {
            host: host.clone(),
            port,
            password,
            sni,
            alpn,
        };

        Ok(ParsedServer {
            backend: BackendKind::SingBox,
            name: display_name,
            host: Some(host),
            port,
            xray: None,
            hysteria2: None,
            tuic: None,
            wireguard: None,
            shadowtls: None,
            anytls: Some(config),
        })
    }
}

fn parse_host_port(endpoint: &str) -> Result<(String, u16), ShadowTlsParseError> {
    let ep = endpoint.trim().trim_end_matches('/');
    if ep.is_empty() {
        return Err(ShadowTlsParseError::MalformedEndpoint);
    }

    let (host, port_str) = if ep.starts_with('[') {
        let bracket_end = ep.find(']').ok_or(ShadowTlsParseError::MalformedEndpoint)?;
        let host = &ep[1..bracket_end];
        let rest = &ep[bracket_end + 1..];
        let colon = rest.find(':').ok_or(ShadowTlsParseError::MalformedEndpoint)?;
        (host, &rest[colon + 1..])
    } else {
        let colon = ep.rfind(':').ok_or(ShadowTlsParseError::MalformedEndpoint)?;
        (&ep[..colon], &ep[colon + 1..])
    };

    if !is_valid_host(host) {
        return Err(ShadowTlsParseError::InvalidHost);
    }

    let port: u16 = port_str.parse().map_err(|_| ShadowTlsParseError::InvalidPort)?;
    if !is_valid_port(port as u32) {
        return Err(ShadowTlsParseError::InvalidPort);
    }

    Ok((host.to_string(), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_shadowtls() {
        let link = "shadowtls://secretpassword@example.com:8443?sni=portal.example.com&version=3#MyShadowTLS";
        let parsed = parse_shadowtls_or_anytls(link).expect("Valid ShadowTLS");
        assert_eq!(parsed.name, "MyShadowTLS");
        assert_eq!(parsed.host.as_deref(), Some("example.com"));
        assert_eq!(parsed.port, 8443);
        assert_eq!(parsed.backend, BackendKind::SingBox);

        let cfg = parsed.shadowtls.expect("ShadowTLS config");
        assert_eq!(cfg.password, "secretpassword");
        assert_eq!(cfg.sni.as_deref(), Some("portal.example.com"));
        assert_eq!(cfg.version, 3);
    }

    #[test]
    fn test_parse_anytls() {
        let link = "anytls://anysecret@example.com:443?sni=any.example.com&alpn=h2,http/1.1#MyAnyTLS";
        let parsed = parse_shadowtls_or_anytls(link).expect("Valid AnyTLS");
        assert_eq!(parsed.name, "MyAnyTLS");
        assert_eq!(parsed.backend, BackendKind::SingBox);

        let cfg = parsed.anytls.expect("AnyTLS config");
        assert_eq!(cfg.password, "anysecret");
        assert_eq!(cfg.sni.as_deref(), Some("any.example.com"));
        assert_eq!(cfg.alpn, vec!["h2", "http/1.1"]);
    }
}
