use thiserror::Error;
use crate::models::{BackendKind, ParsedServer, WireGuardClientConfig};
use crate::parsing::encoding::{parse_query, unescape_query_value};
use crate::parsing::validation::{is_valid_host, is_valid_port};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WireGuardParseError {
    #[error("Empty WireGuard configuration or URI")]
    EmptyInput,
    #[error("WireGuard URI missing scheme delimiter '://'")]
    MissingSchemeDelimiter,
    #[error("Unsupported scheme: '{0}' (expected wg:// or wireguard://)")]
    UnsupportedScheme(String),
    #[error("WireGuard missing private key")]
    MissingPrivateKey,
    #[error("WireGuard missing peer public key")]
    MissingPublicKey,
    #[error("WireGuard missing endpoint")]
    MissingEndpoint,
    #[error("WireGuard invalid endpoint host or port")]
    InvalidEndpoint,
}

pub fn is_wireguard_ini(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    lower.contains("[interface]") && (lower.contains("privatekey") || lower.contains("private_key"))
}

pub fn parse_wireguard(input: &str) -> Result<ParsedServer, WireGuardParseError> {
    let trimmed = input.trim().trim_start_matches('\u{FEFF}');
    if trimmed.is_empty() {
        return Err(WireGuardParseError::EmptyInput);
    }

    if is_wireguard_ini(trimmed) {
        parse_wireguard_ini(trimmed)
    } else {
        parse_wireguard_uri(trimmed)
    }
}

fn parse_wireguard_uri(link: &str) -> Result<ParsedServer, WireGuardParseError> {
    let scheme_end = link
        .find("://")
        .ok_or(WireGuardParseError::MissingSchemeDelimiter)?;

    let scheme = link[..scheme_end].to_ascii_lowercase();
    if scheme != "wg" && scheme != "wireguard" {
        return Err(WireGuardParseError::UnsupportedScheme(scheme));
    }

    let mut body = &link[scheme_end + 3..];

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

    let (private_key, endpoint) = if let Some(at_idx) = body.rfind('@') {
        (unescape_query_value(&body[..at_idx]), &body[at_idx + 1..])
    } else {
        (String::new(), body)
    };

    let private_key = if private_key.is_empty() {
        query
            .get("private_key")
            .or_else(|| query.get("privatekey"))
            .cloned()
            .ok_or(WireGuardParseError::MissingPrivateKey)?
    } else {
        private_key
    };

    let peer_public_key = query
        .get("public_key")
        .or_else(|| query.get("publickey"))
        .or_else(|| query.get("peer_public_key"))
        .cloned()
        .ok_or(WireGuardParseError::MissingPublicKey)?;

    let preshared_key = query
        .get("preshared_key")
        .or_else(|| query.get("presharedkey"))
        .or_else(|| query.get("psk"))
        .cloned();

    let local_address: Vec<String> = query
        .get("ip")
        .or_else(|| query.get("address"))
        .map(|s| s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect())
        .unwrap_or_else(|| vec!["10.0.0.2/32".to_string()]);

    let allowed_ips: Vec<String> = query
        .get("allowed_ips")
        .or_else(|| query.get("allowedips"))
        .map(|s| s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect())
        .unwrap_or_else(|| vec!["0.0.0.0/0".to_string()]);

    let mtu = query.get("mtu").and_then(|m| m.parse::<u16>().ok());

    let reserved = query.get("reserved").map(|r| {
        r.split(',')
            .filter_map(|b| b.trim().parse::<u8>().ok())
            .collect::<Vec<u8>>()
    });

    let (host, port) = parse_host_port(endpoint)?;

    let display_name = if name.is_empty() {
        format!("WireGuard-{}:{}", host, port)
    } else {
        name
    };

    let config = WireGuardClientConfig {
        host: host.clone(),
        port,
        private_key,
        peer_public_key,
        preshared_key,
        local_address,
        allowed_ips,
        mtu,
        reserved,
    };

    Ok(ParsedServer {
        backend: BackendKind::SingBox,
        name: display_name,
        host: Some(host),
        port,
        xray: None,
        hysteria2: None,
        tuic: None,
        wireguard: Some(config),
        shadowtls: None,
        anytls: None,
    })
}

fn parse_wireguard_ini(ini: &str) -> Result<ParsedServer, WireGuardParseError> {
    let mut private_key = None;
    let mut peer_public_key = None;
    let mut preshared_key = None;
    let mut endpoint = None;
    let mut local_address = Vec::new();
    let mut allowed_ips = Vec::new();
    let mut mtu = None;
    let mut reserved = None;

    for line in ini.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.starts_with(';') || trimmed.is_empty() {
            continue;
        }

        if let Some(eq_idx) = trimmed.find('=') {
            let key = trimmed[..eq_idx].trim().to_ascii_lowercase();
            let val = trimmed[eq_idx + 1..].trim();

            match key.as_str() {
                "privatekey" | "private_key" => private_key = Some(val.to_string()),
                "publickey" | "public_key" => peer_public_key = Some(val.to_string()),
                "presharedkey" | "preshared_key" => preshared_key = Some(val.to_string()),
                "endpoint" => endpoint = Some(val.to_string()),
                "address" => {
                    local_address = val.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                }
                "allowedips" | "allowed_ips" => {
                    allowed_ips = val.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                }
                "mtu" => mtu = val.parse::<u16>().ok(),
                "reserved" => {
                    reserved = Some(
                        val.split(',')
                            .filter_map(|s| s.trim().parse::<u8>().ok())
                            .collect(),
                    );
                }
                _ => {}
            }
        }
    }

    let priv_key = private_key.ok_or(WireGuardParseError::MissingPrivateKey)?;
    let pub_key = peer_public_key.ok_or(WireGuardParseError::MissingPublicKey)?;
    let ep = endpoint.ok_or(WireGuardParseError::MissingEndpoint)?;

    let (host, port) = parse_host_port(&ep)?;

    if local_address.is_empty() {
        local_address.push("10.0.0.2/32".to_string());
    }
    if allowed_ips.is_empty() {
        allowed_ips.push("0.0.0.0/0".to_string());
    }

    let display_name = format!("WireGuard-{}:{}", host, port);

    let config = WireGuardClientConfig {
        host: host.clone(),
        port,
        private_key: priv_key,
        peer_public_key: pub_key,
        preshared_key,
        local_address,
        allowed_ips,
        mtu,
        reserved,
    };

    Ok(ParsedServer {
        backend: BackendKind::SingBox,
        name: display_name,
        host: Some(host),
        port,
        xray: None,
        hysteria2: None,
        tuic: None,
        wireguard: Some(config),
        shadowtls: None,
        anytls: None,
    })
}

fn parse_host_port(endpoint: &str) -> Result<(String, u16), WireGuardParseError> {
    let ep = endpoint.trim().trim_end_matches('/');
    if ep.is_empty() {
        return Err(WireGuardParseError::InvalidEndpoint);
    }

    let (host, port_str) = if ep.starts_with('[') {
        let bracket_end = ep.find(']').ok_or(WireGuardParseError::InvalidEndpoint)?;
        let host = &ep[1..bracket_end];
        let rest = &ep[bracket_end + 1..];
        let colon = rest.find(':').ok_or(WireGuardParseError::InvalidEndpoint)?;
        (host, &rest[colon + 1..])
    } else {
        let colon = ep.rfind(':').ok_or(WireGuardParseError::InvalidEndpoint)?;
        (&ep[..colon], &ep[colon + 1..])
    };

    if !is_valid_host(host) {
        return Err(WireGuardParseError::InvalidEndpoint);
    }

    let port: u16 = port_str.parse().map_err(|_| WireGuardParseError::InvalidEndpoint)?;
    if !is_valid_port(port as u32) {
        return Err(WireGuardParseError::InvalidEndpoint);
    }

    Ok((host.to_string(), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_wireguard_uri() {
        let link = "wg://aGVsbG93b3JsZDExMTExMTExMTExMTExMTExMTExMTEx@198.51.100.1:51820?public_key=YnllZ29vZGJ5ZTIyMjIyMjIyMjIyMjIyMjIyMjIyMjIy&preshared_key=c2VjcmV0MzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzM=&ip=10.0.0.5/32&mtu=1420&reserved=1,2,3#MyWG";
        let parsed = parse_wireguard(link).expect("Valid WireGuard URI");
        assert_eq!(parsed.name, "MyWG");
        assert_eq!(parsed.host.as_deref(), Some("198.51.100.1"));
        assert_eq!(parsed.port, 51820);
        assert_eq!(parsed.backend, BackendKind::SingBox);

        let cfg = parsed.wireguard.expect("WireGuard config exists");
        assert_eq!(cfg.private_key, "aGVsbG93b3JsZDExMTExMTExMTExMTExMTExMTExMTEx");
        assert_eq!(cfg.peer_public_key, "YnllZ29vZGJ5ZTIyMjIyMjIyMjIyMjIyMjIyMjIyMjIy");
        assert_eq!(cfg.preshared_key.as_deref(), Some("c2VjcmV0MzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzM="));
        assert_eq!(cfg.local_address, vec!["10.0.0.5/32"]);
        assert_eq!(cfg.mtu, Some(1420));
        assert_eq!(cfg.reserved, Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_parse_wireguard_ini() {
        let ini = r#"
[Interface]
PrivateKey = aGVsbG93b3JsZDExMTExMTExMTExMTExMTExMTExMTEx
Address = 10.0.0.2/32
MTU = 1380

[Peer]
PublicKey = YnllZ29vZGJ5ZTIyMjIyMjIyMjIyMjIyMjIyMjIyMjIy
Endpoint = 198.51.100.2:51820
AllowedIPs = 0.0.0.0/0
"#;
        let parsed = parse_wireguard(ini).expect("Valid WireGuard INI");
        assert_eq!(parsed.host.as_deref(), Some("198.51.100.2"));
        assert_eq!(parsed.port, 51820);
        let cfg = parsed.wireguard.expect("WireGuard config exists");
        assert_eq!(cfg.private_key, "aGVsbG93b3JsZDExMTExMTExMTExMTExMTExMTExMTEx");
        assert_eq!(cfg.peer_public_key, "YnllZ29vZGJ5ZTIyMjIyMjIyMjIyMjIyMjIyMjIyMjIy");
        assert_eq!(cfg.mtu, Some(1380));
    }
}
