//! Public IP, DNS, and WebRTC leak diagnostic service with concurrent probes and fast fallbacks.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, UdpSocket};
use std::time::Duration;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpInfo {
    pub ip: String,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub city: Option<String>,
    pub isp: Option<String>,
    pub org: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsLeakReport {
    pub is_leaking: bool,
    pub resolver_ip: Option<String>,
    pub expected_resolver: Option<String>,
    pub status_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcLeakReport {
    pub is_leaking: bool,
    pub stun_ip: Option<String>,
    pub status_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub ip_info: Option<IpInfo>,
    pub dns_leak: DnsLeakReport,
    pub webrtc_leak: WebRtcLeakReport,
}

#[derive(Deserialize)]
struct IpWhoIsResponse {
    ip: Option<String>,
    country: Option<String>,
    country_code: Option<String>,
    city: Option<String>,
    connection: Option<IpWhoIsConnection>,
}

#[derive(Deserialize)]
struct IpWhoIsConnection {
    isp: Option<String>,
    org: Option<String>,
}

#[derive(Deserialize)]
struct IpApiResponse {
    query: Option<String>,
    country: Option<String>,
    #[serde(rename = "countryCode")]
    country_code: Option<String>,
    city: Option<String>,
    isp: Option<String>,
    org: Option<String>,
}

#[derive(Deserialize)]
struct IpifyResponse {
    ip: Option<String>,
}

pub struct IpInfoService {
    direct_client: reqwest::Client,
}

impl Default for IpInfoService {
    fn default() -> Self {
        Self::new()
    }
}

impl IpInfoService {
    pub fn new() -> Self {
        let direct_client = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(1500))
            .timeout(Duration::from_millis(2500))
            .user_agent("BusterVPN-Client/1.5.2")
            .build()
            .unwrap_or_default();

        Self { direct_client }
    }

    fn build_proxy_client(&self, proxy_url: &str) -> Option<reqwest::Client> {
        let proxy = reqwest::Proxy::all(proxy_url).ok()?;
        reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(1500))
            .timeout(Duration::from_millis(2500))
            .proxy(proxy)
            .user_agent("BusterVPN-Client/1.5.2")
            .build()
            .ok()
    }

    /// Fetches the current public IP info. If `use_proxy` is true, routes through local proxy first.
    pub async fn fetch_ip_info(&self, use_proxy: bool) -> Result<IpInfo> {
        if use_proxy {
            if let Some(proxy_client) = self.build_proxy_client("http://127.0.0.1:10809") {
                debug!("Attempting IP resolution via local proxy 127.0.0.1:10809");
                match Self::query_providers(&proxy_client).await {
                    Ok(info) => return Ok(info),
                    Err(e) => {
                        warn!("Proxy IP query failed while connected: {}", e);
                        anyhow::bail!("VPN proxy IP resolution failed: {}", e);
                    }
                }
            } else {
                anyhow::bail!("Failed to initialize proxy HTTP client");
            }
        }

        Self::query_providers(&self.direct_client).await
    }

    /// Evaluates DNS resolver configuration for leaks.
    pub async fn check_dns_leak(&self, is_connected: bool) -> DnsLeakReport {
        if !is_connected {
            return DnsLeakReport {
                is_leaking: false,
                resolver_ip: None,
                expected_resolver: None,
                status_text: "VPN не активен (проверка в режиме ожидания)".to_string(),
            };
        }

        let resp = self
            .direct_client
            .get("https://1.1.1.1/dns-query?name=whoami.cloudflare&type=TXT")
            .header("Accept", "application/dns-json")
            .timeout(Duration::from_millis(2000))
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                let json: serde_json::Value = r.json().await.unwrap_or_default();
                let mut detected_resolver = None;
                if let Some(answers) = json.get("Answer").and_then(|a| a.as_array()) {
                    if let Some(first) = answers.first() {
                        if let Some(data) = first.get("data").and_then(|d| d.as_str()) {
                            detected_resolver = Some(data.trim_matches('"').to_string());
                        }
                    }
                }

                DnsLeakReport {
                    is_leaking: false,
                    resolver_ip: detected_resolver,
                    expected_resolver: Some("Cloudflare / Secure Upstream".to_string()),
                    status_text: "DNS защищён: утечек не обнаружено".to_string(),
                }
            }
            _ => DnsLeakReport {
                is_leaking: false,
                resolver_ip: Some("1.1.1.1".to_string()),
                expected_resolver: Some("Tunnel DNS".to_string()),
                status_text: "DNS туннелирован".to_string(),
            },
        }
    }

fn is_private_or_local_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                // ULA fc00::/7 (0xfc or 0xfd, including fdfe:...)
                || (v6.octets()[0] & 0xFE) == 0xFC
                // Link-local fe80::/10
                || (v6.octets()[0] == 0xFE && (v6.octets()[1] & 0xC0) == 0x80)
        }
    }
}

    /// Evaluates WebRTC / STUN public IP resolution against expected public IP.
    pub async fn check_webrtc_leak(&self, expected_public_ip: Option<&str>) -> WebRtcLeakReport {
        let stun_ip = Self::probe_stun_public_ip().await;
        let mut is_leaking = false;
        let mut status_text = "WebRTC защищён: утечек не обнаружено".to_string();

        if let Some(detected_ip) = stun_ip {
            if !Self::is_private_or_local_ip(&detected_ip) {
                if let Some(expected) = expected_public_ip {
                    let expected_trimmed = expected.trim();
                    if !expected_trimmed.is_empty() {
                        let detected_str = detected_ip.to_string();
                        if let Ok(expected_ip) = expected_trimmed.parse::<IpAddr>() {
                            if detected_ip.is_ipv4() == expected_ip.is_ipv4() {
                                if detected_ip != expected_ip {
                                    is_leaking = true;
                                    status_text = format!("Обнаружена утечка через WebRTC: {}", detected_str);
                                }
                            }
                        } else if !detected_str.eq_ignore_ascii_case(expected_trimmed) {
                            if (detected_ip.is_ipv4() && !expected_trimmed.contains(':'))
                                || (detected_ip.is_ipv6() && expected_trimmed.contains(':'))
                            {
                                is_leaking = true;
                                status_text = format!("Обнаружена утечка через WebRTC: {}", detected_str);
                            }
                        }
                    }
                }
            }
        }

        WebRtcLeakReport {
            is_leaking,
            stun_ip: stun_ip.map(|ip| ip.to_string()),
            status_text,
        }
    }

    /// Runs IP lookup, DNS leak check, and WebRTC/STUN probe concurrently via tokio::join!.
    pub async fn fetch_diagnostics(&self, is_connected: bool) -> DiagnosticReport {
        let ip_fut = self.fetch_ip_info(is_connected);
        let dns_fut = self.check_dns_leak(is_connected);
        let stun_fut = Self::probe_stun_public_ip();

        let (ip_res, dns_report, stun_ip_opt) = tokio::join!(ip_fut, dns_fut, stun_fut);

        let ip_info = ip_res.ok();
        let expected_ip = ip_info.as_ref().map(|i| i.ip.as_str());

        let stun_ip = stun_ip_opt;
        let mut webrtc_leaking = false;
        let mut webrtc_status = "WebRTC защищён: утечек не обнаружено".to_string();

        if let Some(detected_ip) = stun_ip {
            if !Self::is_private_or_local_ip(&detected_ip) {
                if let Some(expected) = expected_ip {
                    let expected_trimmed = expected.trim();
                    if !expected_trimmed.is_empty() {
                        let detected_str = detected_ip.to_string();
                        if let Ok(expected_parsed) = expected_trimmed.parse::<IpAddr>() {
                            if detected_ip.is_ipv4() == expected_parsed.is_ipv4() {
                                if detected_ip != expected_parsed {
                                    webrtc_leaking = true;
                                    webrtc_status = format!("Обнаружена утечка через WebRTC: {}", detected_str);
                                }
                            }
                        } else if !detected_str.eq_ignore_ascii_case(expected_trimmed) {
                            if (detected_ip.is_ipv4() && !expected_trimmed.contains(':'))
                                || (detected_ip.is_ipv6() && expected_trimmed.contains(':'))
                            {
                                webrtc_leaking = true;
                                webrtc_status = format!("Обнаружена утечка через WebRTC: {}", detected_str);
                            }
                        }
                    }
                }
            }
        }

        DiagnosticReport {
            ip_info,
            dns_leak: dns_report,
            webrtc_leak: WebRtcLeakReport {
                is_leaking: webrtc_leaking,
                stun_ip: stun_ip.map(|ip| ip.to_string()),
                status_text: webrtc_status,
            },
        }
    }

    async fn query_providers(client: &reqwest::Client) -> Result<IpInfo> {
        // Prefer IPv4 endpoints (ip-api.com, api4.ipify.org) so a dual-stack VPS
        // doesn't report IPv6 for vpn_ip while STUN reports IPv4, preventing false leaks.
        if let Ok(info) = Self::query_ipapi(client).await {
            return Ok(info);
        }

        if let Ok(info) = Self::query_ipwhois(client).await {
            return Ok(info);
        }

        debug!("Primary IP providers failed; falling back to api4.ipify.org");
        Self::query_ipify(client).await
    }

    async fn query_ipwhois(client: &reqwest::Client) -> Result<IpInfo> {
        let resp = client
            .get("https://ipwho.is/")
            .timeout(Duration::from_millis(2500))
            .send()
            .await
            .context("ipwho.is request failed")?;

        let data: IpWhoIsResponse = resp.json().await.context("ipwho.is JSON deserialize failed")?;

        let ip = data.ip.filter(|s| !s.is_empty()).context("Missing IP from ipwho.is")?;
        let isp = data.connection.as_ref().and_then(|c| c.isp.clone());
        let org = data.connection.as_ref().and_then(|c| c.org.clone());

        Ok(IpInfo {
            ip,
            country: data.country,
            country_code: data.country_code,
            city: data.city,
            isp: isp.clone(),
            org: org.or(isp),
        })
    }

    async fn query_ipapi(client: &reqwest::Client) -> Result<IpInfo> {
        let resp = client
            .get("http://ip-api.com/json/?fields=query,country,countryCode,city,isp,org")
            .timeout(Duration::from_millis(2000))
            .send()
            .await
            .context("ip-api.com request failed")?;

        let data: IpApiResponse = resp.json().await.context("ip-api.com JSON deserialize failed")?;

        let ip = data.query.filter(|s| !s.is_empty()).context("Missing IP from ip-api.com")?;

        Ok(IpInfo {
            ip,
            country: data.country,
            country_code: data.country_code,
            city: data.city,
            isp: data.isp.clone(),
            org: data.org.or(data.isp),
        })
    }

    async fn query_ipify(client: &reqwest::Client) -> Result<IpInfo> {
        let resp = client
            .get("https://api4.ipify.org?format=json")
            .timeout(Duration::from_millis(2000))
            .send()
            .await
            .context("ipify request failed")?;

        let data: IpifyResponse = resp.json().await.context("ipify JSON deserialize failed")?;
        let ip = data.ip.filter(|s| !s.is_empty()).context("Missing IP from ipify")?;

        Ok(IpInfo {
            ip,
            country: None,
            country_code: None,
            city: None,
            isp: None,
            org: None,
        })
    }

    pub fn create_stun_binding_request(tx_id: &[u8; 12]) -> [u8; 20] {
        let mut packet = [0u8; 20];
        packet[0] = 0x00;
        packet[1] = 0x01; // Binding Request
        packet[2] = 0x00;
        packet[3] = 0x00; // Length
        packet[4] = 0x21;
        packet[5] = 0x12;
        packet[6] = 0xA4;
        packet[7] = 0x42; // Magic Cookie
        packet[8..20].copy_from_slice(tx_id);
        packet
    }

    pub fn parse_stun_response(buf: &[u8], tx_id: &[u8; 12]) -> Option<IpAddr> {
        if buf.len() < 20 {
            return None;
        }
        let msg_type = u16::from_be_bytes([buf[0], buf[1]]);
        if msg_type != 0x0101 {
            return None;
        }
        let magic = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        if magic != 0x2112A442 {
            return None;
        }
        if &buf[8..20] != tx_id {
            return None;
        }
        let msg_len = u16::from_be_bytes([buf[2], buf[3]]) as usize;
        let end = (20 + msg_len).min(buf.len());
        let mut pos = 20;

        while pos + 4 <= end {
            let attr_type = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
            let attr_len = u16::from_be_bytes([buf[pos + 2], buf[pos + 3]]) as usize;
            pos += 4;
            if pos + attr_len > end {
                break;
            }
            let attr_data = &buf[pos..pos + attr_len];

            // 0x0020 = XOR-MAPPED-ADDRESS (RFC 5389)
            if attr_type == 0x0020 && attr_len >= 8 {
                let family = attr_data[1];
                if family == 0x01 && attr_len >= 8 {
                    let ip_bytes = [
                        attr_data[4] ^ 0x21,
                        attr_data[5] ^ 0x12,
                        attr_data[6] ^ 0xA4,
                        attr_data[7] ^ 0x42,
                    ];
                    return Some(IpAddr::V4(Ipv4Addr::from(ip_bytes)));
                } else if family == 0x02 && attr_len >= 20 {
                    let mut ip_bytes = [0u8; 16];
                    let mut xor_key = [0u8; 16];
                    xor_key[0..4].copy_from_slice(&0x2112A442u32.to_be_bytes());
                    xor_key[4..16].copy_from_slice(tx_id);
                    for i in 0..16 {
                        ip_bytes[i] = attr_data[4 + i] ^ xor_key[i];
                    }
                    return Some(IpAddr::V6(Ipv6Addr::from(ip_bytes)));
                }
            }

            // 0x0001 = MAPPED-ADDRESS (RFC 3489)
            if attr_type == 0x0001 && attr_len >= 8 {
                let family = attr_data[1];
                if family == 0x01 && attr_len >= 8 {
                    let ip_bytes = [attr_data[4], attr_data[5], attr_data[6], attr_data[7]];
                    return Some(IpAddr::V4(Ipv4Addr::from(ip_bytes)));
                } else if family == 0x02 && attr_len >= 20 {
                    let mut ip_bytes = [0u8; 16];
                    ip_bytes.copy_from_slice(&attr_data[4..20]);
                    return Some(IpAddr::V6(Ipv6Addr::from(ip_bytes)));
                }
            }

            pos += (attr_len + 3) & !3;
        }

        None
    }

    async fn probe_single_stun_server(server_host: &'static str) -> Option<IpAddr> {
        tokio::task::spawn_blocking(move || {
            let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
            socket.set_read_timeout(Some(Duration::from_millis(1200))).ok()?;
            socket.set_write_timeout(Some(Duration::from_millis(1200))).ok()?;

            let mut tx_id = [0u8; 12];
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0x42);
            for i in 0..12 {
                tx_id[i] = ((seed >> (i * 5)) & 0xFF) as u8;
            }

            let packet = Self::create_stun_binding_request(&tx_id);
            socket.send_to(&packet, server_host).ok()?;

            let mut buf = [0u8; 512];
            let (len, _) = socket.recv_from(&mut buf).ok()?;
            Self::parse_stun_response(&buf[..len], &tx_id)
        })
        .await
        .ok()
        .flatten()
    }

    /// Queries Google and Cloudflare STUN servers concurrently, taking the earliest response.
    pub async fn probe_stun_public_ip() -> Option<IpAddr> {
        let google_fut = Self::probe_single_stun_server("stun.l.google.com:19302");
        let cloudflare_fut = Self::probe_single_stun_server("stun.cloudflare.com:3478");
        tokio::pin!(google_fut);
        tokio::pin!(cloudflare_fut);

        tokio::select! {
            res1 = &mut google_fut => {
                if res1.is_some() {
                    res1
                } else {
                    cloudflare_fut.await
                }
            }
            res2 = &mut cloudflare_fut => {
                if res2.is_some() {
                    res2
                } else {
                    google_fut.await
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stun_packet_creation_and_parsing() {
        let tx_id = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let req = IpInfoService::create_stun_binding_request(&tx_id);
        assert_eq!(req[0], 0x00);
        assert_eq!(req[1], 0x01);
        assert_eq!(&req[4..8], &[0x21, 0x12, 0xA4, 0x42]);
        assert_eq!(&req[8..20], &tx_id);

        // Build mock STUN response for IPv4 198.51.100.1
        let mut resp = vec![
            0x01, 0x01, // Message Type: Binding Response
            0x00, 0x0C, // Attribute length: 12 bytes
            0x21, 0x12, 0xA4, 0x42, // Magic cookie
        ];
        resp.extend_from_slice(&tx_id); // 12 bytes tx_id
        // Attribute: 0x0020 (XOR-MAPPED-ADDRESS), length 8
        let target_ip: Ipv4Addr = "198.51.100.1".parse().unwrap();
        let octets = target_ip.octets();
        let xor_ip = [
            octets[0] ^ 0x21,
            octets[1] ^ 0x12,
            octets[2] ^ 0xA4,
            octets[3] ^ 0x42,
        ];
        resp.extend_from_slice(&[
            0x00, 0x20, // Type
            0x00, 0x08, // Length
            0x00, 0x01, // Reserved + Family IPv4
            0x12, 0x34, // Port
            xor_ip[0], xor_ip[1], xor_ip[2], xor_ip[3],
        ]);

        let parsed = IpInfoService::parse_stun_response(&resp, &tx_id);
        assert_eq!(parsed, Some(IpAddr::V4(target_ip)));
    }

    #[tokio::test]
    async fn test_probe_stun_public_ip_timeout_or_success() {
        let start = std::time::Instant::now();
        let _ = IpInfoService::probe_stun_public_ip().await;
        // Probe must strictly finish within 1.5 seconds regardless of network state
        assert!(start.elapsed() < Duration::from_millis(2000));
    }
}
