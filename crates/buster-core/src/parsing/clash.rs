//! Clash / Mihomo (Clash Meta) YAML subscription parser.
//!
//! Extracts VLESS, Hysteria2, Trojan, and Shadowsocks proxy nodes from `proxies:` array.

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde_yaml::Value;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crate::models::{ParsedServer, ServerProfile};
use crate::parsing::parse_share_link;

/// Checks if the input text looks like a Clash/Mihomo YAML file.
pub fn is_clash_yaml(input: &str) -> bool {
    let lower = input.trim_start().to_lowercase();
    lower.starts_with("proxies:")
        || lower.contains("\nproxies:")
        || lower.contains("\r\nproxies:")
}

/// Parses a Clash YAML document and returns all successfully extracted and parsed servers.
pub fn parse_clash_yaml(yaml_str: &str) -> Vec<ParsedServer> {
    let root: Value = match serde_yaml::from_str(yaml_str) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Failed to parse YAML document: {}", e);
            return Vec::new();
        }
    };

    let proxies_val = match root.get("proxies").and_then(|p| p.as_sequence()) {
        Some(seq) => seq,
        None => return Vec::new(),
    };

    let mut parsed_servers = Vec::new();

    for item in proxies_val {
        if let Some(link) = clash_item_to_share_link(item) {
            match parse_share_link(&link) {
                Ok(mut parsed) => {
                    // If name in YAML is present, ensure it overrides
                    if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                        parsed.name = name.trim().to_string();
                    }
                    parsed_servers.push(parsed);
                }
                Err(e) => {
                    tracing::debug!("Failed to parse generated Clash link '{}': {}", link, e);
                }
            }
        }
    }

    parsed_servers
}

/// Parses a Clash YAML document directly into a list of `ServerProfile` instances,
/// ensuring each profile has a valid canonical `raw_link`.
pub fn parse_clash_profiles(yaml_str: &str, group: Option<&str>) -> Vec<ServerProfile> {
    let root: Value = match serde_yaml::from_str(yaml_str) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let proxies_val = match root.get("proxies").and_then(|p| p.as_sequence()) {
        Some(seq) => seq,
        None => return Vec::new(),
    };

    let mut profiles = Vec::new();

    for item in proxies_val {
        if let Some(link) = clash_item_to_share_link(item) {
            if let Ok(mut parsed) = parse_share_link(&link) {
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    parsed.name = name.trim().to_string();
                }
                let mut profile = parsed.to_profile(Some(link));
                if let Some(g) = group {
                    profile.group = g.to_string();
                }
                profiles.push(profile);
            }
        }
    }

    profiles
}

/// Converts a single Clash YAML mapping item into a standard share link URI.
pub fn clash_item_to_share_link(item: &Value) -> Option<String> {
    let node_type = item.get("type")?.as_str()?.to_lowercase();
    let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("Proxy");
    let server = item.get("server")?.as_str()?.trim();
    let port = item.get("port")?.as_u64()? as u16;

    if server.is_empty() || port == 0 {
        return None;
    }

    let encoded_name = utf8_percent_encode(name, NON_ALPHANUMERIC).to_string();

    match node_type.as_str() {
        "vless" => {
            let uuid = item.get("uuid")?.as_str()?.trim();
            if uuid.is_empty() {
                return None;
            }

            let mut query_params = Vec::new();

            // Network / transport
            let network = item
                .get("network")
                .and_then(|n| n.as_str())
                .unwrap_or("tcp");
            query_params.push(format!("type={}", network));

            // TLS / Reality
            let is_tls = item.get("tls").and_then(|t| t.as_bool()).unwrap_or(false);
            let reality_opts = item.get("reality-opts");

            if let Some(ro) = reality_opts {
                query_params.push("security=reality".to_string());
                if let Some(pbk) = ro.get("public-key").and_then(|p| p.as_str()) {
                    query_params.push(format!("pbk={}", pbk));
                }
                if let Some(sid) = ro.get("short-id").and_then(|s| s.as_str()) {
                    query_params.push(format!("sid={}", sid));
                }
                if let Some(spx) = ro.get("spider-x").and_then(|s| s.as_str()) {
                    query_params.push(format!("spx={}", spx));
                }
            } else if is_tls {
                query_params.push("security=tls".to_string());
            } else {
                query_params.push("security=none".to_string());
            }

            // SNI / servername
            if let Some(sni) = item
                .get("servername")
                .or_else(|| item.get("sni"))
                .and_then(|s| s.as_str())
            {
                query_params.push(format!("sni={}", sni));
            }

            // Flow
            if let Some(flow) = item.get("flow").and_then(|f| f.as_str()) {
                query_params.push(format!("flow={}", flow));
            }

            // Fingerprint
            if let Some(fp) = item
                .get("client-fingerprint")
                .or_else(|| item.get("fingerprint"))
                .and_then(|f| f.as_str())
            {
                query_params.push(format!("fp={}", fp));
            }

            // WebSocket / HTTPUpgrade options
            if let Some(ws_opts) = item.get("ws-opts") {
                if let Some(path) = ws_opts.get("path").and_then(|p| p.as_str()) {
                    query_params.push(format!("path={}", utf8_percent_encode(path, NON_ALPHANUMERIC)));
                }
                if let Some(headers) = ws_opts.get("headers").and_then(|h| h.as_mapping()) {
                    if let Some(host) = headers.get(&Value::String("Host".to_string())).and_then(|h| h.as_str()) {
                        query_params.push(format!("host={}", host));
                    }
                }
            }

            // gRPC options
            if let Some(grpc_opts) = item.get("grpc-opts") {
                if let Some(service) = grpc_opts.get("grpc-service-name").and_then(|s| s.as_str()) {
                    query_params.push(format!("serviceName={}", service));
                }
            }

            let query_str = query_params.join("&");
            Some(format!(
                "vless://{}@{}:{}?{}#{}",
                uuid, server, port, query_str, encoded_name
            ))
        }

        "hysteria2" | "hy2" => {
            let password = item
                .get("password")
                .or_else(|| item.get("auth"))
                .and_then(|p| p.as_str())
                .unwrap_or("");

            let mut query_params = Vec::new();

            if let Some(sni) = item.get("sni").and_then(|s| s.as_str()) {
                query_params.push(format!("sni={}", sni));
            }

            if let Some(insecure) = item
                .get("skip-cert-verify")
                .or_else(|| item.get("insecure"))
                .and_then(|s| s.as_bool())
            {
                if insecure {
                    query_params.push("insecure=1".to_string());
                }
            }

            if let Some(obfs) = item.get("obfs").and_then(|o| o.as_str()) {
                query_params.push(format!("obfs={}", obfs));
                if let Some(obfs_pass) = item.get("obfs-password").and_then(|p| p.as_str()) {
                    query_params.push(format!("obfs-password={}", obfs_pass));
                }
            }

            let query_str = if query_params.is_empty() {
                String::new()
            } else {
                format!("?{}", query_params.join("&"))
            };

            Some(format!(
                "hy2://{}@{}:{}{}#{}",
                password, server, port, query_str, encoded_name
            ))
        }

        "trojan" => {
            let password = item.get("password")?.as_str()?;
            let mut query_params = Vec::new();

            if let Some(sni) = item.get("sni").and_then(|s| s.as_str()) {
                query_params.push(format!("sni={}", sni));
            }

            let query_str = if query_params.is_empty() {
                String::new()
            } else {
                format!("?{}", query_params.join("&"))
            };

            Some(format!(
                "trojan://{}@{}:{}{}#{}",
                password, server, port, query_str, encoded_name
            ))
        }

        "ss" => {
            let cipher = item.get("cipher")?.as_str()?;
            let password = item.get("password")?.as_str()?;
            let creds = format!("{}:{}", cipher, password);
            let encoded_creds = STANDARD.encode(creds);

            Some(format!(
                "ss://{}@{}:{}#{}",
                encoded_creds, server, port, encoded_name
            ))
        }

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BackendKind;

    #[test]
    fn test_parse_clash_yaml_vless_reality_and_hysteria2() {
        let yaml = r#"
proxies:
  - name: "Clash VLESS Reality"
    type: vless
    server: 1.2.3.4
    port: 443
    uuid: 11111111-1111-4111-8111-111111111111
    network: tcp
    tls: true
    servername: vpn.example.com
    flow: xtls-rprx-vision
    client-fingerprint: chrome
    reality-opts:
      public-key: ABCD1234EFGH5678
      short-id: 1234abcd
  - name: "Clash Hysteria 2"
    type: hysteria2
    server: 5.6.7.8
    port: 8443
    password: mysecretpassword
    sni: hy2.example.com
    skip-cert-verify: true
    obfs: salamander
    obfs-password: salamanderpass
"#;

        assert!(is_clash_yaml(yaml));
        let servers = parse_clash_yaml(yaml);
        assert_eq!(servers.len(), 2);

        // VLESS
        assert_eq!(servers[0].name, "Clash VLESS Reality");
        assert_eq!(servers[0].backend, BackendKind::Xray);
        let xray = servers[0].xray.as_ref().unwrap();
        assert_eq!(xray.host, "1.2.3.4");
        assert_eq!(xray.port, 443);
        assert_eq!(xray.reality_public_key.as_deref(), Some("ABCD1234EFGH5678"));

        // Hysteria2
        assert_eq!(servers[1].name, "Clash Hysteria 2");
        assert_eq!(servers[1].backend, BackendKind::Hysteria2);
        let hy2 = servers[1].hysteria2.as_ref().unwrap();
        assert_eq!(hy2.host, "5.6.7.8");
        assert_eq!(hy2.port, 8443);
        assert_eq!(hy2.auth, "mysecretpassword");
        assert_eq!(hy2.obfs_type.as_deref(), Some("salamander"));
    }
}
