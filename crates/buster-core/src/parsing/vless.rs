use thiserror::Error;
use url::Url;
use uuid::Uuid;
use crate::models::{
    BackendKind, ParsedServer, XrayClientConfig, XrayProtocol, XraySecurity, XrayTransport,
};
use crate::parsing::encoding::{
    parse_bool, parse_query, split_list, unescape_query_value,
};
use crate::parsing::validation::{is_valid_host, is_valid_port};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VlessParseError {
    #[error("Malformed URI: {0}")]
    MalformedUri(String),
    #[error("Expected scheme '{expected}', got '{actual}'")]
    InvalidScheme {
        expected: &'static str,
        actual: String,
    },
    #[error("Missing or invalid server host")]
    InvalidHost,
    #[error("Missing or invalid server port")]
    InvalidPort,
    #[error("VLESS link does not contain a valid credential/UUID: {0}")]
    InvalidUuid(String),
    #[error("Unsupported transport: '{0}'")]
    UnsupportedTransport(String),
    #[error("Unsupported security mode: '{0}'")]
    UnsupportedSecurity(String),
    #[error("Reality link is missing its public key (pbk)")]
    MissingRealityPublicKey,
    #[error("Reality is supported only with TCP/gRPC transports")]
    RealityUnsupportedTransport,
}

pub fn parse_vless(link: &str) -> Result<ParsedServer, VlessParseError> {
    let trimmed = link.trim().trim_start_matches('\u{FEFF}');
    let url = Url::parse(trimmed).map_err(|e| VlessParseError::MalformedUri(e.to_string()))?;

    if url.scheme() != "vless" {
        return Err(VlessParseError::InvalidScheme {
            expected: "vless",
            actual: url.scheme().to_string(),
        });
    }

    let host = url
        .host_str()
        .filter(|h| is_valid_host(h))
        .ok_or(VlessParseError::InvalidHost)?
        .to_string();

    let port = url
        .port()
        .filter(|p| is_valid_port(*p as u32))
        .ok_or(VlessParseError::InvalidPort)?;

    let credential_raw = unescape_query_value(url.username());
    let uuid = Uuid::parse_str(&credential_raw)
        .map_err(|_| VlessParseError::InvalidUuid(credential_raw.clone()))?;

    let query = parse_query(url.query().unwrap_or(""));

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
        other => return Err(VlessParseError::UnsupportedTransport(other.to_string())),
    };

    // Security resolution
    let security_raw = query.get("security").map(|s| s.as_str()).unwrap_or("none");
    let security = match security_raw {
        "reality" => XraySecurity::Reality,
        "tls" => XraySecurity::Tls,
        "none" | "" => XraySecurity::None,
        other => return Err(VlessParseError::UnsupportedSecurity(other.to_string())),
    };

    // SNI resolution
    let mut sni = query
        .get("sni")
        .or_else(|| query.get("peer"))
        .or_else(|| query.get("servername"))
        .or_else(|| query.get("server_name"))
        .filter(|s| !s.trim().is_empty())
        .cloned();

    if sni.is_none() && (security == XraySecurity::Reality || transport == XrayTransport::Tcp) {
        if let Some(h) = query.get("host").filter(|s| !s.trim().is_empty()) {
            sni = Some(h.clone());
        }
    }

    if sni.is_none() && security != XraySecurity::None {
        sni = Some(host.clone());
    }

    // Fingerprint resolution
    let mut fingerprint = query
        .get("fp")
        .or_else(|| query.get("fingerprint"))
        .filter(|s| !s.trim().is_empty())
        .cloned();

    if security == XraySecurity::Reality && fingerprint.is_none() {
        fingerprint = Some("chrome".to_string());
    }

    // WebSocket / gRPC paths
    let stream_host = query.get("host").filter(|s| !s.trim().is_empty()).cloned();
    let mut path = query.get("path").filter(|s| !s.trim().is_empty()).cloned();
    let service_name = query
        .get("servicename")
        .filter(|s| !s.trim().is_empty())
        .cloned();

    if transport == XrayTransport::WebSocket && path.is_none() {
        path = Some("/".to_string());
    }

    // Reality parameters
    let reality_key = query
        .get("pbk")
        .or_else(|| query.get("publickey"))
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().replace(' ', "+"));

    let reality_short_id = query
        .get("sid")
        .or_else(|| query.get("shortid"))
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string());

    let spider_x = query
        .get("spx")
        .or_else(|| query.get("spiderx"))
        .filter(|s| !s.trim().is_empty())
        .cloned();

    if security == XraySecurity::Reality {
        if reality_key.is_none() {
            return Err(VlessParseError::MissingRealityPublicKey);
        }
        if !matches!(transport, XrayTransport::Tcp | XrayTransport::Grpc) {
            return Err(VlessParseError::RealityUnsupportedTransport);
        }
    }

    let flow = query.get("flow").filter(|s| !s.trim().is_empty()).cloned();
    let alpn = query
        .get("alpn")
        .map(|s| split_list(s))
        .unwrap_or_default();

    let allow_insecure = parse_bool(query.get("allowinsecure").map(|s| s.as_str()))
        || parse_bool(query.get("insecure").map(|s| s.as_str()));

    // Display Name
    let name = match url.fragment() {
        Some(frag) if !frag.trim().is_empty() => unescape_query_value(frag).trim().to_string(),
        _ => {
            let label = if security == XraySecurity::Reality {
                "VLESS Reality"
            } else {
                "VLESS"
            };
            format!("{} {:?} · {}:{}", label, transport, host, port)
        }
    };

    let config = XrayClientConfig {
        protocol: XrayProtocol::Vless,
        host: host.clone(),
        port,
        credential: uuid.to_string(),
        method: None,
        alter_id: 0,
        flow,
        transport,
        security,
        sni,
        host_header: stream_host,
        path,
        service_name,
        fingerprint,
        reality_public_key: reality_key,
        reality_short_id,
        spider_x,
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

    const VLESS_REALITY: &str = "vless://11111111-1111-4111-8111-111111111111@example.com:443?encryption=none&security=reality&sni=www.example.com&fp=chrome&pbk=PUBLIC_KEY&sid=1234abcd&spx=%2F&type=tcp&flow=xtls-rprx-vision#Amsterdam%20Edge";

    #[test]
    fn test_parses_vless_reality_vision() {
        let parsed = parse_vless(VLESS_REALITY).expect("VLESS parsing must succeed");

        assert_eq!(parsed.backend, BackendKind::Xray);
        assert_eq!(parsed.name, "Amsterdam Edge");
        assert_eq!(parsed.host.as_deref(), Some("example.com"));
        assert_eq!(parsed.port, 443);

        let xray = parsed.xray.expect("Xray config must be present");
        assert_eq!(xray.protocol, XrayProtocol::Vless);
        assert_eq!(xray.transport, XrayTransport::Tcp);
        assert_eq!(xray.security, XraySecurity::Reality);
        assert_eq!(xray.flow.as_deref(), Some("xtls-rprx-vision"));
        assert_eq!(xray.reality_public_key.as_deref(), Some("PUBLIC_KEY"));
        assert_eq!(xray.reality_short_id.as_deref(), Some("1234abcd"));
        assert_eq!(xray.fingerprint.as_deref(), Some("chrome"));
        assert_eq!(xray.spider_x.as_deref(), Some("/"));
        assert_eq!(xray.sni.as_deref(), Some("www.example.com"));
    }

    #[test]
    fn test_vless_websocket_tls() {
        let uri = "vless://22222222-2222-4222-8222-222222222222@ws.example.com:8443?type=ws&security=tls&path=%2Fcustom-ws&host=ws.example.com#WS%20Node";
        let parsed = parse_vless(uri).expect("VLESS WS parsing must succeed");

        assert_eq!(parsed.name, "WS Node");
        let xray = parsed.xray.unwrap();
        assert_eq!(xray.transport, XrayTransport::WebSocket);
        assert_eq!(xray.security, XraySecurity::Tls);
        assert_eq!(xray.path.as_deref(), Some("/custom-ws"));
        assert_eq!(xray.host_header.as_deref(), Some("ws.example.com"));
    }

    #[test]
    fn test_invalid_uuid_rejected() {
        let uri = "vless://not-a-valid-uuid@example.com:443?security=none";
        let err = parse_vless(uri).unwrap_err();
        assert!(matches!(err, VlessParseError::InvalidUuid(_)));
    }

    #[test]
    fn test_reality_without_pbk_rejected() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@example.com:443?security=reality&type=tcp";
        let err = parse_vless(uri).unwrap_err();
        assert_eq!(err, VlessParseError::MissingRealityPublicKey);
    }
}
