use thiserror::Error;
use crate::models::{
    BackendKind, ParsedServer, XrayClientConfig, XrayProtocol, XraySecurity, XrayTransport,
};
use crate::parsing::encoding::{parse_query, try_decode_base64, unescape_query_value};
use crate::parsing::validation::{is_valid_host, is_valid_port};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ShadowsocksParseError {
    #[error("Missing scheme delimiter '://'")]
    MissingSchemeDelimiter,
    #[error("Expected 'ss://' scheme, got '{0}'")]
    InvalidScheme(String),
    #[error("Shadowsocks plugins (SIP003) are not supported by this build")]
    UnsupportedPlugin,
    #[error("Shadowsocks userinfo is invalid base64 or missing credentials")]
    InvalidUserInfo,
    #[error("Legacy Shadowsocks link is not valid base64")]
    InvalidLegacyBase64,
    #[error("Legacy Shadowsocks payload is malformed")]
    MalformedLegacyPayload,
    #[error("Shadowsocks credentials are malformed (expected method:password)")]
    MalformedCredentials,
    #[error("Share link has a malformed IPv6 endpoint")]
    MalformedIpv6Endpoint,
    #[error("Share link has no port")]
    MissingPort,
    #[error("Share link has an invalid port")]
    InvalidPort,
    #[error("Share link has an invalid host")]
    InvalidHost,
}

pub fn parse_shadowsocks(link: &str) -> Result<ParsedServer, ShadowsocksParseError> {
    let trimmed = link.trim().trim_start_matches('\u{FEFF}');
    let scheme_end = trimmed
        .find("://")
        .ok_or(ShadowsocksParseError::MissingSchemeDelimiter)?;

    let scheme = trimmed[..scheme_end].to_ascii_lowercase();
    if scheme != "ss" {
        return Err(ShadowsocksParseError::InvalidScheme(scheme));
    }

    let mut payload = trimmed[scheme_end + 3..].trim();

    // Extract fragment name (#Name)
    let mut name = String::new();
    if let Some(hash_idx) = payload.find('#') {
        name = unescape_query_value(&payload[hash_idx + 1..]).trim().to_string();
        payload = &payload[..hash_idx];
    }

    // Extract query parameters (?plugin=...)
    if let Some(query_idx) = payload.find('?') {
        let query = parse_query(&payload[query_idx..]);
        if query.contains_key("plugin") {
            return Err(ShadowsocksParseError::UnsupportedPlugin);
        }
        payload = &payload[..query_idx];
    }

    let payload = payload.trim_end_matches('/');

    let (method, password, host, port) = if payload.contains('@') {
        // SIP002 Format: [userInfo]@[host]:[port]
        let at_idx = payload.rfind('@').ok_or(ShadowsocksParseError::InvalidUserInfo)?;
        let user_info_raw = &payload[..at_idx];
        let host_port_raw = &payload[at_idx + 1..];

        let user_info = unescape_query_value(user_info_raw);
        let credentials = if user_info.contains(':') {
            user_info
        } else {
            try_decode_base64(user_info_raw).ok_or(ShadowsocksParseError::InvalidUserInfo)?
        };

        let (method, password) = split_credentials(&credentials)?;
        let (host, port) = parse_host_port(host_port_raw)?;
        (method, password, host, port)
    } else {
        // Legacy Base64 Format: BASE64(method:password@host:port)
        let decoded = try_decode_base64(payload).ok_or(ShadowsocksParseError::InvalidLegacyBase64)?;
        let at_idx = decoded.rfind('@').ok_or(ShadowsocksParseError::MalformedLegacyPayload)?;
        if at_idx == 0 {
            return Err(ShadowsocksParseError::MalformedLegacyPayload);
        }

        let credentials = &decoded[..at_idx];
        let host_port_raw = &decoded[at_idx + 1..];

        let (method, password) = split_credentials(credentials)?;
        let (host, port) = parse_host_port(host_port_raw)?;
        (method, password, host, port)
    };

    if method.trim().is_empty() {
        return Err(ShadowsocksParseError::MalformedCredentials);
    }

    let final_name = if name.is_empty() {
        format!("Shadowsocks {}", host)
    } else {
        name
    };

    let config = XrayClientConfig {
        protocol: XrayProtocol::Shadowsocks,
        host: host.clone(),
        port,
        credential: password,
        method: Some(method.to_ascii_lowercase()),
        alter_id: 0,
        flow: None,
        transport: XrayTransport::Tcp,
        security: XraySecurity::None,
        sni: None,
        host_header: None,
        path: None,
        service_name: None,
        fingerprint: None,
        reality_public_key: None,
        reality_short_id: None,
        spider_x: None,
        alpn: Vec::new(),
        allow_insecure: false,
    };

    Ok(ParsedServer {
        backend: BackendKind::Xray,
        name: final_name,
        host: Some(host),
        port,
        xray: Some(config),
        hysteria2: None,
        ..Default::default()
    })
}

fn split_credentials(value: &str) -> Result<(String, String), ShadowsocksParseError> {
    let colon = value.find(':').ok_or(ShadowsocksParseError::MalformedCredentials)?;
    if colon == 0 || colon == value.len() - 1 {
        return Err(ShadowsocksParseError::MalformedCredentials);
    }
    Ok((value[..colon].to_string(), value[colon + 1..].to_string()))
}

fn parse_host_port(value: &str) -> Result<(String, u16), ShadowsocksParseError> {
    let host_port = value.split('/').next().unwrap_or(value);
    let (host, port_text) = if host_port.starts_with('[') {
        let close = host_port.find(']').ok_or(ShadowsocksParseError::MalformedIpv6Endpoint)?;
        if close + 2 > host_port.len() || host_port.as_bytes()[close + 1] != b':' {
            return Err(ShadowsocksParseError::MalformedIpv6Endpoint);
        }
        (&host_port[1..close], &host_port[close + 2..])
    } else {
        let colon = host_port.rfind(':').ok_or(ShadowsocksParseError::MissingPort)?;
        if colon == 0 {
            return Err(ShadowsocksParseError::MissingPort);
        }
        (&host_port[..colon], &host_port[colon + 1..])
    };

    let port: u32 = port_text.parse().map_err(|_| ShadowsocksParseError::InvalidPort)?;
    if !is_valid_port(port) {
        return Err(ShadowsocksParseError::InvalidPort);
    }

    if !is_valid_host(host) {
        return Err(ShadowsocksParseError::InvalidHost);
    }

    Ok((host.to_string(), port as u16))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    #[test]
    fn test_parses_shadowsocks_sip002_base64() {
        let user_info = STANDARD.encode("aes-256-gcm:shh-secret");
        let link = format!("ss://{}@ss.example.com:8388#SS%20Node", user_info);

        let parsed = parse_shadowsocks(&link).expect("SIP002 parse should succeed");

        assert_eq!(parsed.backend, BackendKind::Xray);
        assert_eq!(parsed.name, "SS Node");
        assert_eq!(parsed.host.as_deref(), Some("ss.example.com"));
        assert_eq!(parsed.port, 8388);

        let xray = parsed.xray.unwrap();
        assert_eq!(xray.protocol, XrayProtocol::Shadowsocks);
        assert_eq!(xray.method.as_deref(), Some("aes-256-gcm"));
        assert_eq!(xray.credential, "shh-secret");
    }

    #[test]
    fn test_parses_shadowsocks_plain_text() {
        let link = "ss://chacha20-ietf-poly1305:my-password@192.168.1.1:8388#PlainSS";
        let parsed = parse_shadowsocks(link).expect("Plaintext parse should succeed");

        assert_eq!(parsed.name, "PlainSS");
        assert_eq!(parsed.host.as_deref(), Some("192.168.1.1"));
        assert_eq!(parsed.port, 8388);

        let xray = parsed.xray.unwrap();
        assert_eq!(xray.method.as_deref(), Some("chacha20-ietf-poly1305"));
        assert_eq!(xray.credential, "my-password");
    }

    #[test]
    fn test_parses_shadowsocks_legacy_base64() {
        let payload = STANDARD.encode("aes-128-cfb:legacy-pass@legacy.example.com:8388");
        let link = format!("ss://{}#Legacy%20Node", payload);
        let parsed = parse_shadowsocks(&link).expect("Legacy parse should succeed");

        assert_eq!(parsed.name, "Legacy Node");
        let xray = parsed.xray.unwrap();
        assert_eq!(xray.method.as_deref(), Some("aes-128-cfb"));
        assert_eq!(xray.credential, "legacy-pass");
    }

    #[test]
    fn test_shadowsocks_plugin_rejected() {
        let user_info = STANDARD.encode("aes-128-gcm:secret");
        let link = format!("ss://{}@ss.example.com:8388?plugin=v2ray-plugin#node", user_info);
        let err = parse_shadowsocks(&link).unwrap_err();
        assert_eq!(err, ShadowsocksParseError::UnsupportedPlugin);
    }
}
