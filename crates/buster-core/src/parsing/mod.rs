pub mod clash;
pub mod encoding;
pub mod hysteria2;
pub mod shadowsocks;
pub mod shadowtls;
pub mod trojan;
pub mod tuic;
pub mod validation;
pub mod vless;
pub mod vmess;
pub mod wireguard;

pub use clash::*;
pub use encoding::*;
pub use hysteria2::*;
pub use shadowsocks::*;
pub use shadowtls::*;
pub use trojan::*;
pub use tuic::*;
pub use validation::*;
pub use vless::*;
pub use vmess::*;
pub use wireguard::*;

use thiserror::Error;
use crate::models::ParsedServer;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LinkParseError {
    #[error("Empty share link")]
    EmptyLink,
    #[error("Missing scheme delimiter '://'")]
    MissingSchemeDelimiter,
    #[error("Unsupported share link scheme: '{0}'")]
    UnsupportedScheme(String),
    #[error("VLESS parse error: {0}")]
    Vless(#[from] VlessParseError),
    #[error("VMess parse error: {0}")]
    Vmess(#[from] VmessParseError),
    #[error("Trojan parse error: {0}")]
    Trojan(#[from] TrojanParseError),
    #[error("Shadowsocks parse error: {0}")]
    Shadowsocks(#[from] ShadowsocksParseError),
    #[error("Hysteria2 parse error: {0}")]
    Hysteria2(#[from] Hysteria2ParseError),
    #[error("TUIC parse error: {0}")]
    Tuic(#[from] TuicParseError),
    #[error("WireGuard parse error: {0}")]
    WireGuard(#[from] WireGuardParseError),
    #[error("ShadowTLS parse error: {0}")]
    ShadowTls(#[from] ShadowTlsParseError),
}

const SUPPORTED_SCHEMES: &[&str] = &[
    "vless",
    "vmess",
    "trojan",
    "ss",
    "hysteria2",
    "hy2",
    "hysteria2+realm",
    "hy2+realm",
    "tuic",
    "wg",
    "wireguard",
    "shadowtls",
    "anytls",
];

/// Checks if a string starts with any recognized share link scheme or is a WireGuard INI block.
pub fn looks_like_share_link(input: &str) -> bool {
    let trimmed = input.trim().trim_start_matches('\u{FEFF}');
    if is_wireguard_ini(trimmed) {
        return true;
    }
    if let Some(idx) = trimmed.find("://") {
        let scheme = trimmed[..idx].to_ascii_lowercase();
        SUPPORTED_SCHEMES.contains(&scheme.as_str())
    } else {
        false
    }
}

/// Dispatches any supported share link or config to its dedicated protocol parser.
pub fn parse_share_link(input: &str) -> Result<ParsedServer, LinkParseError> {
    let trimmed = input.trim().trim_start_matches('\u{FEFF}');
    if trimmed.is_empty() {
        return Err(LinkParseError::EmptyLink);
    }

    if is_wireguard_ini(trimmed) {
        return Ok(parse_wireguard(trimmed)?);
    }

    let scheme_end = trimmed
        .find("://")
        .ok_or(LinkParseError::MissingSchemeDelimiter)?;

    let scheme = trimmed[..scheme_end].to_ascii_lowercase();
    match scheme.as_str() {
        "vless" => Ok(parse_vless(trimmed)?),
        "vmess" => Ok(parse_vmess(trimmed)?),
        "trojan" => Ok(parse_trojan(trimmed)?),
        "ss" => Ok(parse_shadowsocks(trimmed)?),
        "hy2" | "hysteria2" | "hy2+realm" | "hysteria2+realm" => Ok(parse_hysteria2(trimmed)?),
        "tuic" => Ok(parse_tuic(trimmed)?),
        "wg" | "wireguard" => Ok(parse_wireguard(trimmed)?),
        "shadowtls" | "anytls" => Ok(parse_shadowtls_or_anytls(trimmed)?),
        _ => Err(LinkParseError::UnsupportedScheme(scheme)),
    }
}

/// Splits multi-line or whitespace-separated input and returns all successfully parsed servers.
pub fn parse_batch(input: &str) -> Vec<ParsedServer> {
    input
        .lines()
        .flat_map(|line| line.split_whitespace())
        .filter(|part| looks_like_share_link(part))
        .filter_map(|link| parse_share_link(link).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{BackendKind, XrayProtocol};
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    #[test]
    fn test_dispatcher_all_protocols() {
        // VLESS
        let vless = "vless://11111111-1111-4111-8111-111111111111@vless.example.com:443?security=none#VLESS%20Node";
        let res_vless = parse_share_link(vless).expect("VLESS should dispatch");
        assert_eq!(res_vless.backend, BackendKind::Xray);
        assert_eq!(res_vless.name, "VLESS Node");

        // VMess
        let vmess_json = r#"{"add": "vmess.example.com", "port": 443, "id": "22222222-2222-4222-8222-222222222222", "ps": "VMess Node"}"#;
        let vmess = format!("vmess://{}", STANDARD.encode(vmess_json));
        let res_vmess = parse_share_link(&vmess).expect("VMess should dispatch");
        assert_eq!(res_vmess.backend, BackendKind::Xray);
        assert_eq!(res_vmess.name, "VMess Node");

        // Trojan
        let trojan = "trojan://password123@trojan.example.com:443#Trojan%20Node";
        let res_trojan = parse_share_link(trojan).expect("Trojan should dispatch");
        assert_eq!(res_trojan.backend, BackendKind::Xray);
        assert_eq!(res_trojan.xray.unwrap().protocol, XrayProtocol::Trojan);

        // Shadowsocks
        let ss = "ss://chacha20-ietf-poly1305:secret@ss.example.com:8388#SS%20Node";
        let res_ss = parse_share_link(ss).expect("SS should dispatch");
        assert_eq!(res_ss.backend, BackendKind::Xray);
        assert_eq!(res_ss.xray.unwrap().protocol, XrayProtocol::Shadowsocks);

        // Hysteria 2
        let hy2 = "hy2://secret@hy.example.com:443,5000-6000#HY2%20Node";
        let res_hy2 = parse_share_link(hy2).expect("HY2 should dispatch");
        assert_eq!(res_hy2.backend, BackendKind::Hysteria2);
        assert_eq!(res_hy2.name, "HY2 Node");

        // TUIC
        let tuic = "tuic://b53494e0-9e66-41ea-9c02-e20ecdd39ec2:mysecret@tuic.example.com:8443#TUIC%20Node";
        let res_tuic = parse_share_link(tuic).expect("TUIC should dispatch");
        assert_eq!(res_tuic.backend, BackendKind::SingBox);
        assert_eq!(res_tuic.name, "TUIC Node");

        // WireGuard
        let wg = "wg://aGVsbG93b3JsZDExMTExMTExMTExMTExMTExMTExMTEx@198.51.100.1:51820?public_key=YnllZ29vZGJ5ZTIyMjIyMjIyMjIyMjIyMjIyMjIyMjIy#WG%20Node";
        let res_wg = parse_share_link(wg).expect("WireGuard should dispatch");
        assert_eq!(res_wg.backend, BackendKind::SingBox);
        assert_eq!(res_wg.name, "WG Node");

        // ShadowTLS
        let stls = "shadowtls://secret@stls.example.com:8443?sni=example.com#ShadowTLS%20Node";
        let res_stls = parse_share_link(stls).expect("ShadowTLS should dispatch");
        assert_eq!(res_stls.backend, BackendKind::SingBox);
        assert_eq!(res_stls.name, "ShadowTLS Node");

        // AnyTLS
        let anytls = "anytls://anysecret@any.example.com:443?sni=example.com#AnyTLS%20Node";
        let res_anytls = parse_share_link(anytls).expect("AnyTLS should dispatch");
        assert_eq!(res_anytls.backend, BackendKind::SingBox);
        assert_eq!(res_anytls.name, "AnyTLS Node");
    }

    #[test]
    fn test_batch_parser() {
        let batch = format!(
            "{}\n\n{}\nrandom garbage text\n{}",
            "trojan://pass@example.com:443#T1",
            "hy2://pass@example.com:443#H1",
            "ss://aes-128-gcm:pass@example.com:8388#S1"
        );

        let servers = parse_batch(&batch);
        assert_eq!(servers.len(), 3);
        assert_eq!(servers[0].name, "T1");
        assert_eq!(servers[1].name, "H1");
        assert_eq!(servers[2].name, "S1");
    }
}
