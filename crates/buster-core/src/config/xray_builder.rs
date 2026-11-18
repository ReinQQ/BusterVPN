use std::net::Ipv4Addr;
use std::path::Path;
use serde_json::{json, Value};
use crate::config::ConfigBuildError;
use crate::models::{
    to_idn_ascii, ParsedServer, RoutingAction, RoutingMatchType, RoutingMode, RoutingRule,
    SplitTunnelingConfig, SplitTunnelMode,
    XrayClientConfig, XrayProtocol, XraySecurity, XrayTransport,
    PRIVATE_IPV4_CIDRS, PRIVATE_IPV6_CIDRS,
};
use crate::parsing::validation::is_valid_port;

pub const DEFAULT_TUN_NAME: &str = "BusterVPN_TUN";
pub const DEFAULT_TUN_IPV4_ADDRESS: &str = "172.19.0.1/30";
pub const DEFAULT_TUN_IPV4_GATEWAY: &str = "172.19.0.1/30";
pub const TUN_DNS_SERVERS: &[&str] = &["1.1.1.1", "8.8.8.8"];

pub const PROXY_OUTBOUND_TAG: &str = "proxy";
pub const DIRECT_OUTBOUND_TAG: &str = "direct";
pub const BLOCK_OUTBOUND_TAG: &str = "block";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalBindInfo {
    pub lan_ip: Ipv4Addr,
    pub interface_alias: Option<String>,
}

#[derive(Debug, Clone)]
pub struct XrayBuildParams<'a> {
    pub profile: &'a ParsedServer,
    pub routing_mode: RoutingMode,
    pub routing_rules: &'a [RoutingRule],
    pub bypass_local_networks: bool,
    pub socks_port: u16,
    pub http_port: u16,
    pub tun_name: &'a str,
    pub resolved_server_ip: Ipv4Addr,
    pub physical_bind: Option<PhysicalBindInfo>,
    pub working_dir: Option<&'a Path>,
    pub split_tunneling: Option<SplitTunnelingConfig>,
    pub dns_servers: Option<Vec<String>>,
}

impl<'a> XrayBuildParams<'a> {
    pub fn new(
        profile: &'a ParsedServer,
        routing_mode: RoutingMode,
        resolved_server_ip: Ipv4Addr,
    ) -> Self {
        Self {
            profile,
            routing_mode,
            routing_rules: &[],
            bypass_local_networks: true,
            socks_port: 10808,
            http_port: 10809,
            tun_name: DEFAULT_TUN_NAME,
            resolved_server_ip,
            physical_bind: None,
            working_dir: None,
            split_tunneling: None,
            dns_servers: None,
        }
    }
}

pub fn build_xray_config(params: &XrayBuildParams) -> Result<Value, ConfigBuildError> {
    let client = params
        .profile
        .xray
        .as_ref()
        .ok_or(ConfigBuildError::MissingXrayConfig)?;

    client.validate()?;

    if params.routing_mode.has_local_proxy()
        && (!is_valid_port(params.socks_port as u32) || !is_valid_port(params.http_port as u32))
    {
        return Err(ConfigBuildError::InvalidProxyPorts(
            params.socks_port,
            params.http_port,
        ));
    }

    let bind_physical = params.routing_mode.has_tun();
    let lan_ip_str = if bind_physical {
        params.physical_bind.as_ref().map(|b| b.lan_ip.to_string())
    } else {
        None
    };
    let iface_alias = if bind_physical {
        params
            .physical_bind
            .as_ref()
            .and_then(|b| b.interface_alias.as_deref())
    } else {
        None
    };

    let inbounds = build_inbounds(params)?;
    let outbounds = build_outbounds(client, params.resolved_server_ip, lan_ip_str.as_deref(), iface_alias)?;
    let routing = build_routing(params);

    let default_dns = vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()];
    let dns_list = params.dns_servers.as_ref().unwrap_or(&default_dns);

    let mut dns_obj = json!({
        "servers": dns_list
    });

    // Secondary safeguard: if host is a domain name, insert static DNS mapping to avoid recursion
    if client.host.parse::<std::net::IpAddr>().is_err() {
        dns_obj["hosts"] = json!({
            &client.host: params.resolved_server_ip.to_string()
        });
    }

    let config = json!({
        "log": {
            "loglevel": "warning"
        },
        "dns": dns_obj,
        "inbounds": inbounds,
        "outbounds": outbounds,
        "routing": routing
    });

    Ok(config)
}

pub fn build_xray_config_json(params: &XrayBuildParams) -> Result<String, ConfigBuildError> {
    let val = build_xray_config(params)?;
    serde_json::to_string_pretty(&val).map_err(|e| ConfigBuildError::Serialization(e.to_string()))
}

fn build_inbounds(params: &XrayBuildParams) -> Result<Vec<Value>, ConfigBuildError> {
    let mut inbounds = Vec::new();

    let sniffing = json!({
        "enabled": true,
        "destOverride": ["http", "tls", "quic"],
        "routeOnly": true
    });

    if params.routing_mode.has_tun() {
        require_cidr_prefix(DEFAULT_TUN_IPV4_ADDRESS, "tun ip")?;
        require_cidr_prefix(DEFAULT_TUN_IPV4_GATEWAY, "tun gateway")?;

        let tun_name = if params.tun_name.trim().is_empty() {
            DEFAULT_TUN_NAME.to_string()
        } else {
            params.tun_name.to_string()
        };

        let mut tun_settings = json!({
            "name": tun_name,
            "desc": "BusterVPN",
            "mtu": 1500,
            // CRITICAL: Must be CIDR prefix, e.g. "172.19.0.1/30" to prevent Xray netip.ParsePrefix panic
            "ip": [DEFAULT_TUN_IPV4_ADDRESS],
            "gateway": [DEFAULT_TUN_IPV4_GATEWAY],
            "dns": TUN_DNS_SERVERS,
            "autoSystemRoutingTable": ["0.0.0.0/0", "::/0"]
        });

        if let Some(ref bind) = params.physical_bind {
            if let Some(ref iface) = bind.interface_alias {
                if iface != "auto" && !iface.trim().is_empty() {
                    tun_settings["autoOutboundsInterface"] = json!(iface);
                }
            }
        }

        let tun = json!({
            "tag": "tun-in",
            "protocol": "tun",
            "settings": tun_settings,
            "sniffing": sniffing
        });
        inbounds.push(tun);
    }

    if params.routing_mode.has_local_proxy() {
        let socks = json!({
            "tag": "socks-in",
            "listen": "127.0.0.1",
            "port": params.socks_port,
            "protocol": "socks",
            "settings": {
                "auth": "noauth",
                "udp": true,
                "userLevel": 0
            },
            "sniffing": sniffing
        });

        let http = json!({
            "tag": "http-in",
            "listen": "127.0.0.1",
            "port": params.http_port,
            "protocol": "http",
            "settings": {
                "userLevel": 0
            },
            "sniffing": sniffing
        });

        inbounds.push(socks);
        inbounds.push(http);
    }

    Ok(inbounds)
}

fn build_outbounds(
    client: &XrayClientConfig,
    resolved_server_ip: Ipv4Addr,
    physical_lan_ip: Option<&str>,
    physical_iface: Option<&str>,
) -> Result<Vec<Value>, ConfigBuildError> {
    let resolved_ip_str = resolved_server_ip.to_string();

    let mut proxy = json!({
        "tag": PROXY_OUTBOUND_TAG,
        "protocol": match client.protocol {
            XrayProtocol::Vless => "vless",
            XrayProtocol::Vmess => "vmess",
            XrayProtocol::Trojan => "trojan",
            XrayProtocol::Shadowsocks => "shadowsocks",
        },
        "settings": build_outbound_settings(client, &resolved_ip_str)?,
        "streamSettings": build_stream_settings(client, physical_iface)?
    });

    if let Some(lan_ip) = physical_lan_ip {
        proxy["sendThrough"] = json!(lan_ip);
    }

    let mut direct = json!({
        "tag": DIRECT_OUTBOUND_TAG,
        "protocol": "freedom"
    });

    if let Some(lan_ip) = physical_lan_ip {
        direct["sendThrough"] = json!(lan_ip);
    }

    if let Some(iface) = physical_iface {
        direct["streamSettings"] = json!({
            "sockopt": {
                "interface": iface
            }
        });
    }

    let block = json!({
        "tag": BLOCK_OUTBOUND_TAG,
        "protocol": "blackhole"
    });

    let blocked = json!({
        "tag": "blocked",
        "protocol": "blackhole"
    });

    let dns_out = json!({
        "tag": "dns-out",
        "protocol": "dns"
    });

    Ok(vec![proxy, direct, block, blocked, dns_out])
}

fn build_outbound_settings(client: &XrayClientConfig, address: &str) -> Result<Value, ConfigBuildError> {
    let settings = match client.protocol {
        XrayProtocol::Vless => {
            let mut user = json!({
                "id": client.credential,
                "encryption": "none",
                "level": 0
            });
            if let Some(flow) = &client.flow {
                user["flow"] = json!(flow);
            }

            json!({
                "vnext": [{
                    "address": address,
                    "port": client.port,
                    "users": [user]
                }]
            })
        }
        XrayProtocol::Vmess => json!({
            "vnext": [{
                "address": address,
                "port": client.port,
                "users": [{
                    "id": client.credential,
                    "alterId": client.alter_id,
                    "security": client.method.as_deref().unwrap_or("auto"),
                    "level": 0
                }]
            }]
        }),
        XrayProtocol::Trojan => json!({
            "servers": [{
                "address": address,
                "port": client.port,
                "password": client.credential,
                "level": 0
            }]
        }),
        XrayProtocol::Shadowsocks => json!({
            "servers": [{
                "address": address,
                "port": client.port,
                "method": client.method,
                "password": client.credential,
                "uot": true
            }]
        }),
    };

    Ok(settings)
}

fn build_stream_settings(
    client: &XrayClientConfig,
    physical_iface: Option<&str>,
) -> Result<Value, ConfigBuildError> {
    let network = match client.transport {
        XrayTransport::Tcp => "tcp",
        XrayTransport::WebSocket => "ws",
        XrayTransport::Grpc => "grpc",
        XrayTransport::HttpUpgrade => "httpupgrade",
    };

    let security = match client.security {
        XraySecurity::None => "none",
        XraySecurity::Tls => "tls",
        XraySecurity::Reality => "reality",
    };

    let mut stream = json!({
        "network": network,
        "security": security
    });

    // Transport Host / authority must retain original domain name, not resolved IP
    let host_header = client
        .host_header
        .as_deref()
        .unwrap_or(client.host.as_str());

    match client.transport {
        XrayTransport::WebSocket => {
            stream["wsSettings"] = json!({
                "path": client.path.as_deref().unwrap_or("/"),
                "headers": {
                    "Host": host_header
                }
            });
        }
        XrayTransport::Grpc => {
            stream["grpcSettings"] = json!({
                "serviceName": client.service_name.as_deref().unwrap_or(""),
                "authority": host_header,
                "multiMode": false
            });
        }
        XrayTransport::HttpUpgrade => {
            stream["httpupgradeSettings"] = json!({
                "path": client.path.as_deref().unwrap_or("/"),
                "host": host_header
            });
        }
        XrayTransport::Tcp => {}
    }

    match client.security {
        XraySecurity::Reality => {
            let server_name = client.sni.as_deref().unwrap_or(client.host.as_str());
            let pbk = client.reality_public_key.as_deref().unwrap_or("");
            let fingerprint = client.fingerprint.as_deref().unwrap_or("chrome");
            let alpn = if !client.alpn.is_empty() {
                json!(client.alpn)
            } else {
                json!(["h2", "http/1.1"])
            };
            stream["realitySettings"] = json!({
                "show": false,
                "serverName": server_name,
                "fingerprint": fingerprint,
                "password": pbk,
                "shortId": client.reality_short_id.as_deref().unwrap_or(""),
                "spiderX": client.spider_x.as_deref().unwrap_or("/"),
                "alpn": alpn
            });
        }
        XraySecurity::Tls => {
            let server_name = client.sni.as_deref().unwrap_or(client.host.as_str());
            let fingerprint = client.fingerprint.as_deref().unwrap_or("chrome");
            let alpn = if !client.alpn.is_empty() {
                json!(client.alpn)
            } else {
                json!(["h2", "http/1.1"])
            };
            let tls = json!({
                "serverName": server_name,
                "allowInsecure": client.allow_insecure,
                "fingerprint": fingerprint,
                "alpn": alpn
            });
            stream["tlsSettings"] = tls;
        }
        XraySecurity::None => {}
    }

    if let Some(iface) = physical_iface {
        stream["sockopt"] = json!({
            "interface": iface
        });
    }

    Ok(stream)
}

fn build_routing(params: &XrayBuildParams) -> Value {
    let mut rules = Vec::new();

    // Block outbound UDP 443 (QUIC / HTTP/3) to force browsers to fallback to standard TCP TLS 1.3
    // Prevents SSL_ERROR_UNSUPPORTED_VERSION on Google and Cloudflare under VLESS Reality
    rules.push(json!({
        "type": "field",
        "outboundTag": "blocked",
        "port": "443",
        "network": "udp"
    }));

    // Direct port 53 DNS traffic from tun-in to internal dns-out outbound
    if params.routing_mode.has_tun() {
        rules.push(json!({
            "type": "field",
            "inboundTag": ["tun-in"],
            "port": "53",
            "outboundTag": "dns-out"
        }));
    }

    // App-based Split Tunneling rules
    if let Some(ref st) = params.split_tunneling {
        if st.enabled {
            let bypass_apps = st.get_bypass_apps();
            let proxy_apps = st.get_proxy_apps();

            // 1. Bypass process rules: "outboundTag": "direct", "process": [ ...bypass_apps ]
            if !bypass_apps.is_empty() {
                rules.push(json!({
                    "type": "field",
                    "outboundTag": DIRECT_OUTBOUND_TAG,
                    "process": bypass_apps
                }));
            }

            // 2. Proxy process rules: "outboundTag": "proxy", "process": [ ...proxy_apps ]
            if !proxy_apps.is_empty() {
                rules.push(json!({
                    "type": "field",
                    "outboundTag": PROXY_OUTBOUND_TAG,
                    "process": proxy_apps
                }));
            }
        }
    }

    // Direct domain routing rules for Russian domains and popular local services
    let direct_domains = vec![
        "domain:ru".to_string(),
        "domain:su".to_string(),
        "domain:рф".to_string(),
        "domain:xn--p1ai".to_string(),
        "yandex".to_string(),
        "vk.com".to_string(),
        "gosuslugi.ru".to_string(),
        "sberbank".to_string(),
        "tbank".to_string(),
        "ozon.ru".to_string(),
        "wildberries.ru".to_string(),
        "avito.ru".to_string(),
        "kinopoisk.ru".to_string(),
        "rutube.ru".to_string(),
    ];

    rules.push(json!({
        "type": "field",
        "outboundTag": DIRECT_OUTBOUND_TAG,
        "domain": direct_domains
    }));

    let geoip_exists = params
        .working_dir
        .map(|d| d.join("geoip.dat").is_file())
        .unwrap_or(true);

    if geoip_exists {
        rules.push(json!({
            "type": "field",
            "outboundTag": DIRECT_OUTBOUND_TAG,
            "ip": [
                "geoip:ru",
                "geoip:private"
            ]
        }));
    } else {
        rules.push(json!({
            "type": "field",
            "outboundTag": DIRECT_OUTBOUND_TAG,
            "ip": [
                "geoip:private"
            ]
        }));
    }

    // Bypass rule for the VPN server's own resolved IP (Direct, prevents routing loop)
    rules.push(json!({
        "type": "field",
        "ip": [format!("{}/32", params.resolved_server_ip)],
        "outboundTag": DIRECT_OUTBOUND_TAG
    }));

    if params.bypass_local_networks {
        rules.push(json!({
            "type": "field",
            "domain": [
                "full:localhost",
                "domain:local",
                "domain:lan",
                "domain:home.arpa"
            ],
            "outboundTag": DIRECT_OUTBOUND_TAG
        }));

        let mut private_cidrs = Vec::new();
        private_cidrs.extend_from_slice(PRIVATE_IPV4_CIDRS);
        private_cidrs.extend_from_slice(PRIVATE_IPV6_CIDRS);

        rules.push(json!({
            "type": "field",
            "ip": private_cidrs,
            "outboundTag": DIRECT_OUTBOUND_TAG
        }));
    }

    for rule in params.routing_rules.iter().filter(|r| r.enabled) {
        let mut r = json!({ "type": "field" });
        match rule.match_type {
            RoutingMatchType::DomainSuffix => {
                let ascii_domain = to_idn_ascii(&rule.value);
                r["domain"] = json!([format!("domain:{}", ascii_domain)]);
            }
            RoutingMatchType::DomainKeyword => {
                r["domain"] = json!([format!("keyword:{}", rule.value)]);
            }
            RoutingMatchType::IpCidr => {
                r["ip"] = json!([&rule.value]);
            }
        }

        r["outboundTag"] = json!(match rule.action {
            RoutingAction::Direct => DIRECT_OUTBOUND_TAG,
            RoutingAction::Block => BLOCK_OUTBOUND_TAG,
            RoutingAction::Proxy => PROXY_OUTBOUND_TAG,
        });

        rules.push(r);
    }

    // If Split Tunneling is configured to ProxySelected, direct all remaining non-selected traffic
    if let Some(ref st) = params.split_tunneling {
        if st.enabled && st.mode == SplitTunnelMode::ProxySelected && (!st.apps.is_empty() || !st.proxy_apps.is_empty()) {
            rules.push(json!({
                "type": "field",
                "outboundTag": DIRECT_OUTBOUND_TAG,
                "network": "tcp,udp"
            }));
        }
    }

    json!({
        "domainStrategy": "AsIs",
        "rules": rules
    })
}

fn require_cidr_prefix(value: &str, field: &str) -> Result<(), ConfigBuildError> {
    if !value.contains('/') {
        return Err(ConfigBuildError::InvalidTunCidr(format!(
            "TUN {} must be a CIDR prefix, got '{}'",
            field, value
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use crate::parsing::vless::parse_vless;

    #[test]
    fn test_xray_tun_uses_resolved_server_ip_and_binds_nic() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@vpn.buster.gripe:443?security=reality&sni=vpn.buster.gripe&fp=chrome&pbk=PUBLIC_KEY&sid=abcd&type=tcp#TunEdge";
        let parsed = parse_vless(uri).expect("Parse VLESS");

        let resolved_ip = Ipv4Addr::new(144, 31, 104, 143);
        let mut params = XrayBuildParams::new(&parsed, RoutingMode::TunAndSystemProxy, resolved_ip);
        params.bypass_local_networks = false;
        params.physical_bind = Some(PhysicalBindInfo {
            lan_ip: Ipv4Addr::new(192, 168, 0, 25),
            interface_alias: Some("Ethernet".to_string()),
        });

        let config = build_xray_config(&params).expect("Build config");

        let outbounds = config["outbounds"].as_array().expect("Outbounds array");
        let proxy = outbounds
            .iter()
            .find(|o| o["tag"] == PROXY_OUTBOUND_TAG)
            .expect("Proxy outbound");

        // Resolved IP is dialed
        assert_eq!(
            proxy["settings"]["vnext"][0]["address"],
            "144.31.104.143"
        );

        // Original SNI preserved
        assert_eq!(
            proxy["streamSettings"]["realitySettings"]["serverName"],
            "vpn.buster.gripe"
        );

        // Bound to physical NIC
        assert_eq!(proxy["sendThrough"], "192.168.0.25");
        assert_eq!(
            proxy["streamSettings"]["sockopt"]["interface"],
            "Ethernet"
        );

        // Direct outbound also bound to physical NIC
        let direct = outbounds
            .iter()
            .find(|o| o["tag"] == DIRECT_OUTBOUND_TAG)
            .expect("Direct outbound");
        assert_eq!(direct["sendThrough"], "192.168.0.25");
        assert_eq!(
            direct["streamSettings"]["sockopt"]["interface"],
            "Ethernet"
        );

        // DNS hosts entry added
        assert_eq!(
            config["dns"]["hosts"]["vpn.buster.gripe"],
            "144.31.104.143"
        );

        // TUN Inbound checks
        let inbounds = config["inbounds"].as_array().expect("Inbounds array");
        let tun = inbounds
            .iter()
            .find(|i| i["tag"] == "tun-in")
            .expect("TUN inbound");
        assert_eq!(tun["protocol"], "tun");
        assert_eq!(tun["settings"]["ip"][0], "172.19.0.1/30");
        assert_eq!(tun["settings"]["gateway"][0], "172.19.0.1/30");
        assert_eq!(tun["settings"]["mtu"], 1500);
    }

    #[test]
    fn test_xray_punycode_cyrillic_routing_rule() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@example.com:443?security=tls&sni=example.com#Test";
        let parsed = parse_vless(uri).expect("Parse VLESS");

        let rule = RoutingRule::new("рф", RoutingMatchType::DomainSuffix, RoutingAction::Direct);
        let rules = vec![rule];

        let mut params = XrayBuildParams::new(
            &parsed,
            RoutingMode::SystemProxy,
            Ipv4Addr::new(93, 184, 216, 34),
        );
        params.routing_rules = &rules;
        params.bypass_local_networks = false;

        let config = build_xray_config(&params).expect("Build config");
        let rules = config["routing"]["rules"].as_array().expect("Rules array");

        let found_punycode = rules.iter().any(|r| {
            r.get("domain")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().any(|v| v == "domain:xn--p1ai"))
                .unwrap_or(false)
        });

        assert!(found_punycode, "Must convert Cyrillic 'рф' to 'domain:xn--p1ai'");
    }

    #[test]
    fn test_xray_blocks_quic_udp_443() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@example.com:443?security=reality&sni=example.com&fp=chrome&pbk=PUBLIC_KEY&sid=abcd&type=tcp#QuicTest";
        let parsed = parse_vless(uri).expect("Parse VLESS");

        let params = XrayBuildParams::new(
            &parsed,
            RoutingMode::TunAndSystemProxy,
            Ipv4Addr::new(93, 184, 216, 34),
        );

        let config = build_xray_config(&params).expect("Build config");
        let rules = config["routing"]["rules"].as_array().expect("Rules array");

        let first_rule = &rules[0];
        assert_eq!(first_rule["outboundTag"], "blocked");
        assert_eq!(first_rule["port"], "443");
        assert_eq!(first_rule["network"], "udp");

        let outbounds = config["outbounds"].as_array().expect("Outbounds array");
        assert!(outbounds.iter().any(|o| o["tag"] == "blocked" && o["protocol"] == "blackhole"));
    }

    #[test]
    fn test_xray_reality_utls_and_alpn() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@example.com:443?security=reality&sni=example.com&pbk=PUBLIC_KEY&sid=abcd&type=tcp#RealityTest";
        let parsed = parse_vless(uri).expect("Parse VLESS");

        let params = XrayBuildParams::new(
            &parsed,
            RoutingMode::SystemProxy,
            Ipv4Addr::new(93, 184, 216, 34),
        );

        let config = build_xray_config(&params).expect("Build config");
        let proxy_outbound = config["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["tag"] == "proxy")
            .unwrap();

        let reality = &proxy_outbound["streamSettings"]["realitySettings"];
        assert_eq!(reality["fingerprint"], "chrome");
        assert_eq!(reality["alpn"], serde_json::json!(["h2", "http/1.1"]));
    }

    #[test]
    fn test_xray_dns_outbound_and_routing() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@vpn.buster.gripe:443?security=reality&sni=vpn.buster.gripe&fp=chrome&pbk=PUBLIC_KEY&sid=abcd&type=tcp#DnsTest";
        let parsed = parse_vless(uri).expect("Parse VLESS");

        let params = XrayBuildParams::new(
            &parsed,
            RoutingMode::TunAndSystemProxy,
            Ipv4Addr::new(93, 184, 216, 34),
        );

        let config = build_xray_config(&params).expect("Build config");
        assert_eq!(config["dns"]["servers"], json!(["1.1.1.1", "8.8.8.8"]));

        let outbounds = config["outbounds"].as_array().expect("Outbounds array");
        assert!(outbounds.iter().any(|o| o["tag"] == "dns-out" && o["protocol"] == "dns"));

        let rules = config["routing"]["rules"].as_array().expect("Rules array");
        let dns_rule = rules.iter().find(|r| r["outboundTag"] == "dns-out").expect("DNS rule");
        assert_eq!(dns_rule["inboundTag"], json!(["tun-in"]));
        assert_eq!(dns_rule["port"], "53");
    }

    #[test]
    fn test_xray_direct_domains_and_geoip_ru_bypass() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@vpn.buster.gripe:443?security=reality&sni=vpn.buster.gripe&fp=chrome&pbk=PUBLIC_KEY&sid=abcd&type=tcp#GeoTest";
        let parsed = parse_vless(uri).expect("Parse VLESS");

        let params = XrayBuildParams::new(
            &parsed,
            RoutingMode::TunAndSystemProxy,
            Ipv4Addr::new(93, 184, 216, 34),
        );

        let config = build_xray_config(&params).expect("Build config");
        let rules = config["routing"]["rules"].as_array().expect("Rules array");

        // Verify direct domain rule exists with all Russian domain patterns and NO geosite:ru
        let domain_rule = rules.iter().find(|r| {
            r.get("domain")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().any(|v| v == "domain:ru"))
                .unwrap_or(false)
        }).expect("domain:ru direct rule");
        assert_eq!(domain_rule["outboundTag"], "direct");
        let domains = domain_rule["domain"].as_array().unwrap();
        assert!(!domains.contains(&json!("geosite:ru")), "geosite:ru must NEVER be emitted to avoid Xray EOF crash");
        assert!(domains.contains(&json!("domain:ru")));
        assert!(domains.contains(&json!("domain:su")));
        assert!(domains.contains(&json!("domain:рф")));
        assert!(domains.contains(&json!("domain:xn--p1ai")));
        assert!(domains.contains(&json!("yandex")));
        assert!(domains.contains(&json!("vk.com")));
        assert!(domains.contains(&json!("gosuslugi.ru")));
        assert!(domains.contains(&json!("sberbank")));
        assert!(domains.contains(&json!("tbank")));
        assert!(domains.contains(&json!("ozon.ru")));
        assert!(domains.contains(&json!("wildberries.ru")));
        assert!(domains.contains(&json!("avito.ru")));
        assert!(domains.contains(&json!("kinopoisk.ru")));
        assert!(domains.contains(&json!("rutube.ru")));

        // Verify geoip:ru direct rule exists
        let geoip_rule = rules.iter().find(|r| {
            r.get("ip")
                .and_then(|i| i.as_array())
                .map(|arr| arr.iter().any(|v| v == "geoip:ru"))
                .unwrap_or(false)
        }).expect("geoip:ru rule");
        assert_eq!(geoip_rule["outboundTag"], "direct");
        assert!(geoip_rule["ip"].as_array().unwrap().contains(&json!("geoip:private")));
    }

    #[test]
    fn test_xray_geoip_fallback_when_missing() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@vpn.buster.gripe:443?security=reality&sni=vpn.buster.gripe&fp=chrome&pbk=PUBLIC_KEY&sid=abcd&type=tcp#GeoFallbackTest";
        let parsed = parse_vless(uri).expect("Parse VLESS");

        let empty_temp = std::env::temp_dir().join("bustervpn_nonexistent_dir");
        let mut params = XrayBuildParams::new(
            &parsed,
            RoutingMode::TunAndSystemProxy,
            Ipv4Addr::new(93, 184, 216, 34),
        );
        params.working_dir = Some(&empty_temp);

        let config = build_xray_config(&params).expect("Build config");
        let rules = config["routing"]["rules"].as_array().expect("Rules array");

        // Verify direct domain rule is present
        let domain_rule = rules.iter().find(|r| {
            r.get("domain")
                .and_then(|d| d.as_array())
                .map(|arr| arr.iter().any(|v| v == "domain:ru"))
                .unwrap_or(false)
        }).expect("domain direct rule");
        assert_eq!(domain_rule["outboundTag"], "direct");

        // Verify geoip:ru is NOT present when geoip.dat is missing
        let has_geoip_ru = rules.iter().any(|r| {
            r.get("ip")
                .and_then(|i| i.as_array())
                .map(|arr| arr.iter().any(|v| v == "geoip:ru"))
                .unwrap_or(false)
        });
        assert!(!has_geoip_ru, "geoip:ru should NOT be present when geoip.dat is missing");

        let has_geoip_private = rules.iter().any(|r| {
            r.get("ip")
                .and_then(|i| i.as_array())
                .map(|arr| arr.iter().any(|v| v == "geoip:private"))
                .unwrap_or(false)
        });
        assert!(has_geoip_private, "geoip:private should be present");
    }

    #[test]
    fn test_xray_split_tunneling_bypass() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@vpn.buster.gripe:443?security=reality&sni=vpn.buster.gripe&fp=chrome&pbk=PUBLIC_KEY&sid=abcd&type=tcp#SplitBypass";
        let parsed = parse_vless(uri).expect("Parse VLESS");

        let mut params = XrayBuildParams::new(
            &parsed,
            RoutingMode::TunAndSystemProxy,
            Ipv4Addr::new(93, 184, 216, 34),
        );
        params.split_tunneling = Some(SplitTunnelingConfig {
            enabled: true,
            mode: SplitTunnelMode::BypassSelected,
            apps: vec!["telegram.exe".to_string(), "discord.exe".to_string()],
            ..Default::default()
        });

        let config = build_xray_config(&params).expect("Build config");
        let rules = config["routing"]["rules"].as_array().expect("Rules array");

        let split_rule = rules.iter().find(|r| {
            r.get("process")
                .and_then(|p| p.as_array())
                .map(|arr| arr.contains(&json!("telegram.exe")))
                .unwrap_or(false)
        }).expect("split tunneling bypass rule found");

        assert_eq!(split_rule["outboundTag"], DIRECT_OUTBOUND_TAG);
    }

    #[test]
    fn test_xray_split_tunneling_proxy_selected() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@vpn.buster.gripe:443?security=reality&sni=vpn.buster.gripe&fp=chrome&pbk=PUBLIC_KEY&sid=abcd&type=tcp#SplitProxy";
        let parsed = parse_vless(uri).expect("Parse VLESS");

        let mut params = XrayBuildParams::new(
            &parsed,
            RoutingMode::TunAndSystemProxy,
            Ipv4Addr::new(93, 184, 216, 34),
        );
        params.split_tunneling = Some(SplitTunnelingConfig {
            enabled: true,
            mode: SplitTunnelMode::ProxySelected,
            apps: vec!["chrome.exe".to_string()],
            ..Default::default()
        });

        let config = build_xray_config(&params).expect("Build config");
        let rules = config["routing"]["rules"].as_array().expect("Rules array");

        // Process rule -> proxy
        let proxy_rule = rules.iter().find(|r| {
            r.get("process")
                .and_then(|p| p.as_array())
                .map(|arr| arr.contains(&json!("chrome.exe")))
                .unwrap_or(false)
        }).expect("split tunneling proxy rule found");
        assert_eq!(proxy_rule["outboundTag"], PROXY_OUTBOUND_TAG);

        // Fallback rule -> direct for all other traffic
        let last_rule = rules.last().expect("last rule");
        assert_eq!(last_rule["outboundTag"], DIRECT_OUTBOUND_TAG);
        assert_eq!(last_rule["network"], "tcp,udp");
    }

    #[test]
    fn test_xray_dual_categorized_split_tunneling() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@vpn.buster.gripe:443?security=reality&sni=vpn.buster.gripe&fp=chrome&pbk=PUBLIC_KEY&sid=abcd&type=tcp#DualSplit";
        let parsed = parse_vless(uri).expect("Parse VLESS");

        let mut params = XrayBuildParams::new(
            &parsed,
            RoutingMode::TunAndSystemProxy,
            Ipv4Addr::new(93, 184, 216, 34),
        );
        params.split_tunneling = Some(SplitTunnelingConfig {
            enabled: true,
            mode: SplitTunnelMode::BypassSelected,
            apps: vec![],
            bypass_apps: vec!["chrome.exe".to_string(), "steam.exe".to_string()],
            proxy_apps: vec!["discord.exe".to_string(), "telegram.exe".to_string()],
        });

        let config = build_xray_config(&params).expect("Build config");
        let rules = config["routing"]["rules"].as_array().expect("Rules array");

        // Rule order check: QUIC (blocked) -> Bypass (direct) -> Proxy (proxy) -> RU domains (direct)
        let bypass_idx = rules.iter().position(|r| {
            r.get("process")
                .and_then(|p| p.as_array())
                .map(|arr| arr.contains(&json!("chrome.exe")))
                .unwrap_or(false)
        }).expect("bypass process rule");
        assert_eq!(rules[bypass_idx]["outboundTag"], DIRECT_OUTBOUND_TAG);

        let proxy_idx = rules.iter().position(|r| {
            r.get("process")
                .and_then(|p| p.as_array())
                .map(|arr| arr.contains(&json!("discord.exe")))
                .unwrap_or(false)
        }).expect("proxy process rule");
        assert_eq!(rules[proxy_idx]["outboundTag"], PROXY_OUTBOUND_TAG);

        assert!(bypass_idx < proxy_idx, "Bypass rule MUST precede Proxy rule in routing priority");
    }

    #[test]
    fn test_xray_tun_name_never_empty() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@vpn.buster.gripe:443?security=reality&sni=vpn.buster.gripe&fp=chrome&pbk=PUBLIC_KEY&sid=abcd&type=tcp#EmptyTun";
        let parsed = parse_vless(uri).expect("Parse VLESS");

        let mut params = XrayBuildParams::new(
            &parsed,
            RoutingMode::Tun,
            Ipv4Addr::new(93, 184, 216, 34),
        );
        params.tun_name = "   "; // whitespace / empty string

        let config = build_xray_config(&params).expect("Build config");
        let inbounds = config["inbounds"].as_array().expect("inbounds");
        let tun = inbounds.iter().find(|i| i["protocol"] == "tun").expect("tun inbound");
        assert_eq!(tun["settings"]["name"], DEFAULT_TUN_NAME);
    }

    #[test]
    fn test_xray_custom_dns_servers() {
        let uri = "vless://11111111-1111-4111-8111-111111111111@vpn.buster.gripe:443?security=reality&sni=vpn.buster.gripe&fp=chrome&pbk=PUBLIC_KEY&sid=abcd&type=tcp#DnsTest";
        let parsed = parse_vless(uri).expect("Parse VLESS");

        let mut params = XrayBuildParams::new(
            &parsed,
            RoutingMode::TunAndSystemProxy,
            Ipv4Addr::new(93, 184, 216, 34),
        );
        params.dns_servers = Some(vec!["94.140.14.14".to_string(), "94.140.15.15".to_string()]);

        let config = build_xray_config(&params).expect("Build config");
        let dns_servers = config["dns"]["servers"].as_array().expect("DNS servers array");
        assert_eq!(dns_servers, &vec![json!("94.140.14.14"), json!("94.140.15.15")]);
    }
}
