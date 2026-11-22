use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;
use crate::models::{
    BackendKind, ParsedServer, XrayClientConfig, XrayProtocol, XraySecurity, XrayTransport,
};
use crate::parsing::encoding::{split_list, try_decode_base64, unescape_query_value};
use crate::parsing::validation::{is_valid_host, is_valid_port};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VmessParseError {
    #[error("Missing scheme delimiter '://'")]
    MissingSchemeDelimiter,
    #[error("Expected 'vmess://' scheme, got '{0}'")]
    InvalidScheme(String),
    #[error("VMess link is not valid base64")]
    InvalidBase64,
    #[error("VMess payload is not a valid JSON object: {0}")]
    InvalidJson(String),
    #[error("VMess payload has an invalid address, port, or UUID")]
    InvalidAddressOrPortOrUuid,
    #[error("VMess TCP+HTTP header obfuscation is not supported by this build")]
    UnsupportedTcpHttpObfuscation,
    #[error("Unsupported VMess transport: '{0}'")]
    UnsupportedTransport(String),
}

pub fn parse_vmess(link: &str) -> Result<ParsedServer, VmessParseError> {
    let trimmed = link.trim().trim_start_matches('\u{FEFF}');
    let scheme_end = trimmed
        .find("://")
        .ok_or(VmessParseError::MissingSchemeDelimiter)?;

    let scheme = trimmed[..scheme_end].to_ascii_lowercase();
    if scheme != "vmess" {
        return Err(VmessParseError::InvalidScheme(scheme));
    }

    let mut payload = trimmed[scheme_end + 3..].trim();

    // Check for fragment name (#Fragment)
    let mut fragment_name = String::new();
    if let Some(hash_idx) = payload.find('#') {
        fragment_name = unescape_query_value(&payload[hash_idx + 1..]).trim().to_string();
        payload = &payload[..hash_idx];
    }

    let json_text = try_decode_base64(payload).ok_or(VmessParseError::InvalidBase64)?;
    let root: Value = serde_json::from_str(&json_text)
        .map_err(|e| VmessParseError::InvalidJson(e.to_string()))?;

    if !root.is_object() {
        return Err(VmessParseError::InvalidJson("Root is not an object".to_string()));
    }

    let host = get_str_field(&root, "add").unwrap_or_default();
    let port_u32 = match root.get("port") {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0) as u32,
        Some(Value::String(s)) => s.parse::<u32>().unwrap_or(0),
        _ => 0,
    };
    let id_str = get_str_field(&root, "id").unwrap_or_default();

    if !is_valid_host(&host) || !is_valid_port(port_u32) || Uuid::parse_str(&id_str).is_err() {
        return Err(VmessParseError::InvalidAddressOrPortOrUuid);
    }
    let port = port_u32 as u16;

    let transport_text = get_str_field(&root, "net").unwrap_or_default();
    let transport = match transport_text.to_ascii_lowercase().as_str() {
        "" | "tcp" | "raw" => XrayTransport::Tcp,
        "ws" | "websocket" => XrayTransport::WebSocket,
        "grpc" | "gun" => XrayTransport::Grpc,
        "httpupgrade" => XrayTransport::HttpUpgrade,
        other => return Err(VmessParseError::UnsupportedTransport(other.to_string())),
    };

    let header_type = get_str_field(&root, "type").unwrap_or_default();
    if header_type.eq_ignore_ascii_case("http") && transport == XrayTransport::Tcp {
        return Err(VmessParseError::UnsupportedTcpHttpObfuscation);
    }

    let tls_val = get_str_field(&root, "tls").unwrap_or_default();
    let security = if tls_val.eq_ignore_ascii_case("tls") || tls_val == "1" {
        XraySecurity::Tls
    } else {
        XraySecurity::None
    };

    let mut sni = get_str_field(&root, "sni").filter(|s| !s.trim().is_empty());
    if sni.is_none() && security == XraySecurity::Tls {
        sni = Some(host.clone());
    }

    let stream_host = get_str_field(&root, "host").filter(|s| !s.trim().is_empty());
    let mut path = get_str_field(&root, "path").filter(|s| !s.trim().is_empty());
    if transport == XrayTransport::WebSocket && path.is_none() {
        path = Some("/".to_string());
    }

    let alter_id = match root.get("aid") {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0) as i32,
        Some(Value::String(s)) => s.parse::<i32>().unwrap_or(0),
        _ => 0,
    };

    let method = get_str_field(&root, "scy")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "auto".to_string());

    let fingerprint = get_str_field(&root, "fp").filter(|s| !s.trim().is_empty());
    let alpn = get_str_field(&root, "alpn")
        .map(|s| split_list(&s))
        .unwrap_or_default();

    let ps_name = get_str_field(&root, "ps").unwrap_or_default();
    let name = if !fragment_name.is_empty() {
        fragment_name
    } else if !ps_name.trim().is_empty() {
        ps_name.trim().to_string()
    } else {
        format!("VMess {}", host)
    };

    let config = XrayClientConfig {
        protocol: XrayProtocol::Vmess,
        host: host.clone(),
        port,
        credential: id_str,
        method: Some(method),
        alter_id,
        flow: None,
        transport,
        security,
        sni,
        host_header: stream_host,
        path,
        service_name: None,
        fingerprint,
        reality_public_key: None,
        reality_short_id: None,
        spider_x: None,
        alpn,
        allow_insecure: false,
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

fn get_str_field(val: &Value, name: &str) -> Option<String> {
    val.as_object()?.get(name).and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    #[test]
    fn test_parses_vmess_json_base64() {
        let json = r#"{
            "v": "2",
            "ps": "VMess Edge",
            "add": "vmess.example.com",
            "port": "8443",
            "id": "22222222-2222-4222-8222-222222222222",
            "aid": "0",
            "scy": "auto",
            "net": "ws",
            "host": "cdn.example.com",
            "path": "/ws",
            "tls": "tls",
            "sni": "cdn.example.com",
            "alpn": "h2,http/1.1"
        }"#;

        let link = format!("vmess://{}", STANDARD.encode(json));
        let parsed = parse_vmess(&link).expect("VMess parse should succeed");

        assert_eq!(parsed.name, "VMess Edge");
        assert_eq!(parsed.host.as_deref(), Some("vmess.example.com"));
        assert_eq!(parsed.port, 8443);

        let xray = parsed.xray.unwrap();
        assert_eq!(xray.protocol, XrayProtocol::Vmess);
        assert_eq!(xray.transport, XrayTransport::WebSocket);
        assert_eq!(xray.security, XraySecurity::Tls);
        assert_eq!(xray.path.as_deref(), Some("/ws"));
        assert_eq!(xray.sni.as_deref(), Some("cdn.example.com"));
        assert_eq!(xray.alpn, vec!["h2".to_string(), "http/1.1".to_string()]);
    }

    #[test]
    fn test_vmess_fragment_overrides_ps() {
        let json = r#"{"add": "1.1.1.1", "port": 443, "id": "11111111-1111-1111-1111-111111111111", "ps": "Internal"}"#;
        let link = format!("vmess://{}#External%20Name", STANDARD.encode(json));
        let parsed = parse_vmess(&link).expect("Parse with fragment should succeed");
        assert_eq!(parsed.name, "External Name");
    }

    #[test]
    fn test_invalid_base64_fails() {
        let err = parse_vmess("vmess://not-valid-base64!").unwrap_err();
        assert_eq!(err, VmessParseError::InvalidBase64);
    }
}
