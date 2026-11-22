use thiserror::Error;
use url::Url;
use crate::models::{
    BackendKind, ParsedServer, XrayClientConfig, XrayProtocol, XraySecurity, XrayTransport,
};
use crate::parsing::encoding::{
    parse_bool, parse_query, split_list, unescape_query_value,
};
use crate::parsing::validation::{is_valid_host, is_valid_port};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TrojanParseError {
    #[error("Malformed Trojan link: {0}")]
    MalformedUri(String),
    #[error("Expected 'trojan://' scheme, got '{0}'")]
    InvalidScheme(String),
    #[error("Missing or invalid server host")]
    InvalidHost,
    #[error("Missing or invalid server port")]
    InvalidPort,
    #[error("Trojan link has no password credential")]
    MissingPassword,
    #[error("Trojan requires TLS")]
    TrojanRequiresTls,
    #[error("Unsupported transport: '{0}'")]
    UnsupportedTransport(String),
}

pub fn parse_trojan(link: &str) -> Result<ParsedServer, TrojanParseError> {
    let trimmed = link.trim().trim_start_matches('\u{FEFF}');
    let url = Url::parse(trimmed).map_err(|e| TrojanParseError::MalformedUri(e.to_string()))?;

    if url.scheme() != "trojan" {
        return Err(TrojanParseError::InvalidScheme(url.scheme().to_string()));
    }

    let host = url
        .host_str()
        .filter(|h| is_valid_host(h))
        .ok_or(TrojanParseError::InvalidHost)?
        .to_string();

    let port = url
        .port()
        .filter(|p| is_valid_port(*p as u32))
        .ok_or(TrojanParseError::InvalidPort)?;

    let password = unescape_query_value(url.username());
    if password.trim().is_empty() {
        return Err(TrojanParseError::MissingPassword);
    }

    let query = parse_query(url.query().unwrap_or(""));

    // Security check: Trojan strictly requires TLS
    let security_text = query.get("security").map(|s| s.to_ascii_lowercase()).unwrap_or_default();
    if !security_text.is_empty() && security_text != "tls" {
        return Err(TrojanParseError::TrojanRequiresTls);
    }
    let security = XraySecurity::Tls;

    // Transport resolution
    let transport_raw = query
        .get("type")
        .or_else(|| query.get("network"))
        .map(|s| s.as_str())
        .unwrap_or("tcp");

    let transport = match transport_raw {
        "" | "tcp" | "raw" => XrayTransport::Tcp,
        "ws" | "websocket" => XrayTransport::WebSocket,
        "grpc" | "gun" => XrayTransport::Grpc,
        "httpupgrade" => XrayTransport::HttpUpgrade,
        other => return Err(TrojanParseError::UnsupportedTransport(other.to_string())),
    };

    let mut sni = query
        .get("sni")
        .or_else(|| query.get("peer"))
        .filter(|s| !s.trim().is_empty())
        .cloned();

    if sni.is_none() {
        sni = Some(host.clone());
    }

    let stream_host = query.get("host").filter(|s| !s.trim().is_empty()).cloned();
    let mut path = query.get("path").filter(|s| !s.trim().is_empty()).cloned();
    let service_name = query
        .get("servicename")
        .filter(|s| !s.trim().is_empty())
        .cloned();

    if transport == XrayTransport::WebSocket && path.is_none() {
        path = Some("/".to_string());
    }

    let fingerprint = query
        .get("fp")
        .or_else(|| query.get("fingerprint"))
        .filter(|s| !s.trim().is_empty())
        .cloned();

    let alpn = query
        .get("alpn")
        .map(|s| split_list(s))
        .unwrap_or_default();

    let allow_insecure = parse_bool(query.get("allowinsecure").map(|s| s.as_str()))
        || parse_bool(query.get("insecure").map(|s| s.as_str()));

    let name = match url.fragment() {
        Some(frag) if !frag.trim().is_empty() => unescape_query_value(frag).trim().to_string(),
        _ => format!("Trojan {:?} · {}:{}", transport, host, port),
    };

    let config = XrayClientConfig {
        protocol: XrayProtocol::Trojan,
        host: host.clone(),
        port,
        credential: password,
        method: None,
        alter_id: 0,
        flow: None,
        transport,
        security,
        sni,
        host_header: stream_host,
        path,
        service_name,
        fingerprint,
        reality_public_key: None,
        reality_short_id: None,
        spider_x: None,
        alpn,
        allow_insecure,
    };

    Ok(ParsedServer {
        backend: BackendKind::Xray,
        name,
        host: Some(host),
        port,
        xray: Some(config),
        hysteria2: None,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parses_trojan_standard() {
        let uri = "trojan://top-secret@trojan.example.com:443?sni=trojan.example.com&alpn=h2,http/1.1#Trojan%20Node";
        let parsed = parse_trojan(uri).expect("Trojan parsing must succeed");

        assert_eq!(parsed.name, "Trojan Node");
        assert_eq!(parsed.host.as_deref(), Some("trojan.example.com"));
        assert_eq!(parsed.port, 443);

        let xray = parsed.xray.unwrap();
        assert_eq!(xray.protocol, XrayProtocol::Trojan);
        assert_eq!(xray.credential, "top-secret");
        assert_eq!(xray.security, XraySecurity::Tls);
        assert_eq!(xray.sni.as_deref(), Some("trojan.example.com"));
        assert_eq!(xray.alpn, vec!["h2".to_string(), "http/1.1".to_string()]);
    }

    #[test]
    fn test_trojan_without_tls_rejected() {
        let uri = "trojan://password@example.com:443?security=none#bad";
        let err = parse_trojan(uri).unwrap_err();
        assert_eq!(err, TrojanParseError::TrojanRequiresTls);
    }

    #[test]
    fn test_trojan_websocket() {
        let uri = "trojan://pass@example.com:443?type=ws&path=%2Ftrojan-ws#WS";
        let parsed = parse_trojan(uri).expect("Trojan WS should parse");

        let xray = parsed.xray.unwrap();
        assert_eq!(xray.transport, XrayTransport::WebSocket);
        assert_eq!(xray.path.as_deref(), Some("/trojan-ws"));
    }
}
