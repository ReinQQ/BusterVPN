use std::path::Path;
use serde_json::{json, Map, Value};
use crate::config::ConfigBuildError;
use crate::models::{
    RoutingAction, RoutingMatchType, RoutingMode, RoutingRule,
    ServerProfile, SplitTunnelingConfig, XrayProtocol, XraySecurity, XrayTransport,
};
use crate::models::profile::ParsedServer;

pub struct SingboxBuildParams<'a> {
    pub server: &'a ParsedServer,
    pub routing_mode: RoutingMode,
    pub socks_port: u16,
    pub http_port: u16,
    pub bypass_local_networks: bool,
    pub block_quic: bool,
    pub routing_rules: &'a [RoutingRule],
    pub split_tunneling: Option<SplitTunnelingConfig>,
    pub dns_servers: Option<Vec<String>>,
    pub working_dir: Option<&'a Path>,
    pub interface_name: Option<&'a str>,
    pub resolved_server_ips: Option<Vec<String>>,
    pub vless_socks_detour_port: Option<u16>,
}

impl<'a> SingboxBuildParams<'a> {
    pub fn new(server: &'a ParsedServer, routing_mode: RoutingMode) -> Self {
        Self {
            server,
            routing_mode,
            socks_port: 10808,
            http_port: 10809,
            bypass_local_networks: true,
            block_quic: true,
            routing_rules: &[],
            split_tunneling: None,
            dns_servers: None,
            working_dir: None,
            interface_name: None,
            resolved_server_ips: None,
            vless_socks_detour_port: None,
        }
    }
}

pub fn build_singbox_config(params: &SingboxBuildParams) -> Result<String, ConfigBuildError> {
    let mut root = Map::new();

    // 1. Log block
    root.insert(
        "log".to_string(),
        json!({
            "level": "info",
            "timestamp": true
        }),
    );

    // 2. DNS block
    let remote_dns_server = params
        .dns_servers
        .as_ref()
        .and_then(|list| list.first())
        .map(|s| {
            s.trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_start_matches("udp://")
                .trim_start_matches("tcp://")
                .split('/')
                .next()
                .unwrap_or("1.1.1.1")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or("1.1.1.1");

    let clean_path = |p: &std::path::Path| -> String {
        let s = p.to_string_lossy().to_string();
        s.strip_prefix(r"\\?\").unwrap_or(&s).replace('\\', "/")
    };

    let (geosite_srs, geoip_srs, has_srs) = if let Some(dir) = params.working_dir {
        let geosite_file = dir.join("geosite-ru.srs");
        let geoip_file = dir.join("geoip-ru.srs");
        let exists = !dir.is_dir() || geosite_file.is_file() || geoip_file.is_file();
        (clean_path(&geosite_file), clean_path(&geoip_file), exists)
    } else {
        ("geosite-ru.srs".to_string(), "geoip-ru.srs".to_string(), true)
    };

    let mut server_domains = Vec::new();
    let mut collect_host = |h: &str| {
        let t = h.trim();
        if !t.is_empty() && t.parse::<std::net::IpAddr>().is_err() && !server_domains.contains(&t.to_string()) {
            server_domains.push(t.to_string());
        }
    };

    if let Some(ref h) = params.server.host {
        collect_host(h);
    }
    if let Some(ref x) = params.server.xray {
        collect_host(&x.host);
    }
    if let Some(ref h) = params.server.hysteria2 {
        collect_host(&h.host);
    }
    if let Some(ref t) = params.server.tuic {
        collect_host(&t.host);
    }
    if let Some(ref s) = params.server.shadowtls {
        collect_host(&s.host);
    }
    if let Some(ref a) = params.server.anytls {
        collect_host(&a.host);
    }
    if let Some(ref w) = params.server.wireguard {
        collect_host(&w.host);
    }

    let mut dns_rules = Vec::new();

    // 1. Explicit rule: VPN server domain resolves via local DNS (direct) to eliminate domain lookup deadlock
    if !server_domains.is_empty() {
        dns_rules.push(json!({
            "domain": server_domains,
            "server": "local"
        }));
    }

    // 2. Rule-sets or domain suffixes for local bypass
    if has_srs {
        dns_rules.push(json!({
            "rule_set": ["geosite-ru"],
            "server": "local"
        }));
    } else {
        dns_rules.push(json!({
            "domain_suffix": [".ru", ".su", ".xn--p1ai", ".by", ".kz"],
            "server": "local"
        }));
    }

    root.insert(
        "dns".to_string(),
        json!({
            "servers": [
                {
                    "tag": "remote",
                    "type": "udp",
                    "server": remote_dns_server,
                    "detour": "proxy"
                },
                {
                    "tag": "local",
                    "type": "udp",
                    "server": "77.88.8.8"
                }
            ],
            "rules": dns_rules,
            "final": "remote",
            "strategy": "prefer_ipv4"
        }),
    );

    // 3. Inbounds
    let mut inbounds = Vec::new();
    let tun_name = params.interface_name.unwrap_or("BusterVPN_TUN");

    if params.routing_mode.has_tun() {
        inbounds.push(json!({
            "type": "tun",
            "tag": "tun-in",
            "interface_name": tun_name,
            "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
            "mtu": 1400,
            "auto_route": true,
            "strict_route": true,
            "stack": "system"
        }));
    }

    if params.routing_mode.has_local_proxy() {
        inbounds.push(json!({
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": params.socks_port
        }));

        if params.http_port > 0 && params.http_port != params.socks_port {
            inbounds.push(json!({
                "type": "mixed",
                "tag": "mixed-http-in",
                "listen": "127.0.0.1",
                "listen_port": params.http_port
            }));
        }
    }

    root.insert("inbounds".to_string(), Value::Array(inbounds));

    // 4. Outbounds & Endpoints
    if let Some(ref wg) = params.server.wireguard {
        let mut peer = Map::new();
        peer.insert("address".to_string(), json!(wg.host));
        peer.insert("port".to_string(), json!(wg.port));
        peer.insert("public_key".to_string(), json!(wg.peer_public_key));
        peer.insert("allowed_ips".to_string(), json!(["0.0.0.0/0", "::/0"]));

        if let Some(ref psk) = wg.preshared_key {
            peer.insert("pre_shared_key".to_string(), json!(psk));
        }
        if let Some(ref res) = wg.reserved {
            peer.insert("reserved".to_string(), json!(res));
        }

        let mut wg_map = Map::new();
        wg_map.insert("type".to_string(), json!("wireguard"));
        wg_map.insert("tag".to_string(), json!("proxy"));
        wg_map.insert("address".to_string(), json!(wg.local_address));
        wg_map.insert("private_key".to_string(), json!(wg.private_key));
        if let Some(mtu) = wg.mtu {
            wg_map.insert("mtu".to_string(), json!(mtu));
        }
        wg_map.insert("peers".to_string(), json!([Value::Object(peer)]));

        root.insert("endpoints".to_string(), json!([Value::Object(wg_map)]));

        let outbounds = vec![
            json!({ "type": "direct", "tag": "direct" }),
        ];
        root.insert("outbounds".to_string(), Value::Array(outbounds));
    } else {
        let proxy_outbound = build_proxy_outbound(params)?;
        let outbounds = vec![
            proxy_outbound,
            json!({ "type": "direct", "tag": "direct" }),
        ];
        root.insert("outbounds".to_string(), Value::Array(outbounds));
    }

    // 5. Route block & rule_sets
    let mut route_map = Map::new();

    if has_srs {
        route_map.insert(
            "rule_set".to_string(),
            json!([
                {
                    "tag": "geosite-ru",
                    "type": "local",
                    "format": "binary",
                    "path": geosite_srs
                },
                {
                    "tag": "geoip-ru",
                    "type": "local",
                    "format": "binary",
                    "path": geoip_srs
                }
            ]),
        );
    }

    let mut rules = Vec::new();
    if params.vless_socks_detour_port.is_some() {
        rules.push(json!({
            "process_name": ["xray.exe"],
            "outbound": "direct"
        }));
    }
    rules.push(json!({ "action": "sniff" }));
    rules.push(json!({ "protocol": "dns", "action": "hijack-dns" }));

    // Direct route for VPN server host domain and its resolved IPs
    if !server_domains.is_empty() {
        rules.push(json!({
            "domain": server_domains,
            "outbound": "direct"
        }));
    }
    if let Some(ref ips) = params.resolved_server_ips {
        for ip in ips {
            if let Ok(ip_addr) = ip.parse::<std::net::IpAddr>() {
                let cidr = match ip_addr {
                    std::net::IpAddr::V4(_) => format!("{}/32", ip),
                    std::net::IpAddr::V6(_) => format!("{}/128", ip),
                };
                rules.push(json!({
                    "ip_cidr": [cidr],
                    "outbound": "direct"
                }));
            }
        }
    }

    if params.block_quic {
        rules.push(json!({
            "port": 443,
            "network": "udp",
            "action": "reject"
        }));
    }

    // Route UDP STUN traffic through proxy so external STUN probes see VPN exit IP
    rules.push(json!({
        "port": [3478, 19302],
        "network": "udp",
        "outbound": "proxy"
    }));

    if params.bypass_local_networks {
        rules.push(json!({ "ip_is_private": true, "outbound": "direct" }));
    }

    if has_srs {
        rules.push(json!({
            "rule_set": ["geosite-ru", "geoip-ru"],
            "outbound": "direct"
        }));
    } else {
        rules.push(json!({
            "domain_suffix": [".ru", ".su", ".xn--p1ai", ".by", ".kz"],
            "outbound": "direct"
        }));
    }

    if let Some(ref st) = params.split_tunneling {
        if st.enabled {
            if !st.bypass_apps.is_empty() {
                rules.push(json!({
                    "process_name": st.bypass_apps,
                    "outbound": "direct"
                }));
            }
            if !st.proxy_apps.is_empty() {
                rules.push(json!({
                    "process_name": st.proxy_apps,
                    "outbound": "proxy"
                }));
            }
        }
    }

    for rule in params.routing_rules {
        let (is_block, target_outbound) = match rule.action {
            RoutingAction::Direct => (false, "direct"),
            RoutingAction::Proxy => (false, "proxy"),
            RoutingAction::Block => (true, ""),
        };

        match rule.match_type {
            RoutingMatchType::DomainSuffix => {
                if is_block {
                    rules.push(json!({
                        "domain_suffix": [rule.value],
                        "action": "reject"
                    }));
                } else {
                    rules.push(json!({
                        "domain_suffix": [rule.value],
                        "outbound": target_outbound
                    }));
                }
            }
            RoutingMatchType::DomainKeyword => {
                if is_block {
                    rules.push(json!({
                        "domain_keyword": [rule.value],
                        "action": "reject"
                    }));
                } else {
                    rules.push(json!({
                        "domain_keyword": [rule.value],
                        "outbound": target_outbound
                    }));
                }
            }
            RoutingMatchType::IpCidr => {
                if is_block {
                    rules.push(json!({
                        "ip_cidr": [rule.value],
                        "action": "reject"
                    }));
                } else {
                    rules.push(json!({
                        "ip_cidr": [rule.value],
                        "outbound": target_outbound
                    }));
                }
            }
        }
    }

    route_map.insert("rules".to_string(), Value::Array(rules));
    route_map.insert("final".to_string(), json!("proxy"));
    route_map.insert("default_domain_resolver".to_string(), json!("local"));
    route_map.insert("auto_detect_interface".to_string(), Value::Bool(true));

    root.insert("route".to_string(), Value::Object(route_map));

    serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| ConfigBuildError::Validation(e.to_string()))
}

/// Builds a minimal, standalone Xray JSON configuration for VLESS profiles
/// acting as a localhost SOCKS5 sidecar detour (zero .dat files and zero TUN needed).
pub fn build_xray_vless_detour_config(profile: &ServerProfile, socks_port: u16) -> Result<String, String> {
    let raw_link = profile.raw_link.as_deref().ok_or_else(|| {
        format!("Profile '{}' has no raw share link payload", profile.name)
    })?;
    let parsed = crate::parsing::parse_share_link(raw_link)
        .map_err(|e| format!("Failed to parse profile raw link: {e}"))?;
    build_xray_vless_detour_config_from_parsed(&parsed, socks_port)
}

/// Builds a minimal, standalone Xray JSON configuration from a `ParsedServer`.
pub fn build_xray_vless_detour_config_from_parsed(server: &ParsedServer, socks_port: u16) -> Result<String, String> {
    let xray = server
        .xray
        .as_ref()
        .ok_or_else(|| "Server configuration is missing Xray settings".to_string())?;

    if xray.protocol != XrayProtocol::Vless {
        return Err(format!("Expected VLESS protocol, found {:?}", xray.protocol));
    }

    let mut user = json!({
        "id": xray.credential,
        "encryption": "none",
        "level": 0
    });
    if let Some(ref flow) = xray.flow {
        if !flow.trim().is_empty() {
            user["flow"] = json!(flow.trim());
        }
    }

    let network = match xray.transport {
        XrayTransport::Tcp => "tcp",
        XrayTransport::WebSocket => "ws",
        XrayTransport::Grpc => "grpc",
        XrayTransport::HttpUpgrade => "httpupgrade",
    };

    let security = match xray.security {
        XraySecurity::None => "none",
        XraySecurity::Tls => "tls",
        XraySecurity::Reality => "reality",
    };

    let mut stream = json!({
        "network": network,
        "security": security
    });

    let host_header = xray.host_header.as_deref().unwrap_or(xray.host.as_str());

    match xray.transport {
        XrayTransport::WebSocket => {
            stream["wsSettings"] = json!({
                "path": xray.path.as_deref().unwrap_or("/"),
                "headers": {
                    "Host": host_header
                }
            });
        }
        XrayTransport::Grpc => {
            stream["grpcSettings"] = json!({
                "serviceName": xray.service_name.as_deref().unwrap_or(""),
                "authority": host_header,
                "multiMode": false
            });
        }
        XrayTransport::HttpUpgrade => {
            stream["httpupgradeSettings"] = json!({
                "path": xray.path.as_deref().unwrap_or("/"),
                "host": host_header
            });
        }
        XrayTransport::Tcp => {}
    }

    match xray.security {
        XraySecurity::Reality => {
            let server_name = xray.sni.as_deref().unwrap_or(xray.host.as_str());
            let pbk = xray.reality_public_key.as_deref().unwrap_or("");
            let fingerprint = xray.fingerprint.as_deref().unwrap_or("chrome");
            let alpn = if !xray.alpn.is_empty() {
                json!(xray.alpn)
            } else {
                json!(["h2", "http/1.1"])
            };
            stream["realitySettings"] = json!({
                "show": false,
                "serverName": server_name,
                "fingerprint": fingerprint,
                "password": pbk,
                "shortId": xray.reality_short_id.as_deref().unwrap_or(""),
                "spiderX": xray.spider_x.as_deref().unwrap_or("/"),
                "alpn": alpn
            });
        }
        XraySecurity::Tls => {
            let server_name = xray.sni.as_deref().unwrap_or(xray.host.as_str());
            let fingerprint = xray.fingerprint.as_deref().unwrap_or("chrome");
            let alpn = if !xray.alpn.is_empty() {
                json!(xray.alpn)
            } else {
                json!(["h2", "http/1.1"])
            };
            let tls = json!({
                "serverName": server_name,
                "allowInsecure": xray.allow_insecure,
                "fingerprint": fingerprint,
                "alpn": alpn
            });
            stream["tlsSettings"] = tls;
        }
        XraySecurity::None => {}
    }

    let outbounds = json!([
        {
            "tag": "proxy",
            "protocol": "vless",
            "settings": {
                "vnext": [{
                    "address": xray.host,
                    "port": xray.port,
                    "users": [user]
                }]
            },
            "streamSettings": stream
        },
        {
            "tag": "direct",
            "protocol": "freedom"
        }
    ]);

    let inbounds = json!([
        {
            "tag": "socks-in",
            "port": socks_port,
            "listen": "127.0.0.1",
            "protocol": "socks",
            "settings": {
                "auth": "noauth",
                "udp": true
            }
        }
    ]);

    let config = json!({
        "log": {
            "loglevel": "warning"
        },
        "inbounds": inbounds,
        "outbounds": outbounds
    });

    serde_json::to_string_pretty(&config).map_err(|e| e.to_string())
}

fn build_proxy_outbound(params: &SingboxBuildParams) -> Result<Value, ConfigBuildError> {
    let server = params.server;

    // Zero-fallback SOCKS5 sidecar detour for VLESS (requires xray.exe sidecar).
    if let Some(ref xray) = server.xray {
        if xray.protocol == XrayProtocol::Vless {
            let socks_port = params.vless_socks_detour_port.ok_or_else(|| {
                ConfigBuildError::Validation(
                    "VLESS requires xray.exe SOCKS5 sidecar detour port".to_string(),
                )
            })?;
            return Ok(json!({
                "type": "socks",
                "tag": "proxy",
                "server": "127.0.0.1",
                "server_port": socks_port,
                "version": "5"
            }));
        }
    }
    // 1. TUIC
    if let Some(ref tuic) = server.tuic {
        let mut map = Map::new();
        map.insert("type".to_string(), json!("tuic"));
        map.insert("tag".to_string(), json!("proxy"));
        map.insert("server".to_string(), json!(tuic.host));
        map.insert("server_port".to_string(), json!(tuic.port));
        map.insert("uuid".to_string(), json!(tuic.uuid));
        map.insert("password".to_string(), json!(tuic.password));

        if let Some(ref cc) = tuic.congestion_control {
            map.insert("congestion_control".to_string(), json!(cc));
        }

        let mut tls = Map::new();
        tls.insert("enabled".to_string(), json!(true));
        if let Some(ref sni) = tuic.sni {
            tls.insert("server_name".to_string(), json!(sni));
        }
        if !tuic.alpn.is_empty() {
            tls.insert("alpn".to_string(), json!(tuic.alpn));
        }
        if tuic.allow_insecure {
            tls.insert("insecure".to_string(), json!(true));
        }
        if tuic.disable_sni {
            tls.insert("disable_sni".to_string(), json!(true));
        }
        map.insert("tls".to_string(), Value::Object(tls));

        return Ok(Value::Object(map));
    }

    // 2. ShadowTLS
    if let Some(ref stls) = server.shadowtls {
        let mut map = Map::new();
        map.insert("type".to_string(), json!("shadowtls"));
        map.insert("tag".to_string(), json!("proxy"));
        map.insert("server".to_string(), json!(stls.host));
        map.insert("server_port".to_string(), json!(stls.port));
        map.insert("version".to_string(), json!(stls.version));
        map.insert("password".to_string(), json!(stls.password));

        let mut tls = Map::new();
        tls.insert("enabled".to_string(), json!(true));
        if let Some(ref sni) = stls.sni {
            tls.insert("server_name".to_string(), json!(sni));
        }
        map.insert("tls".to_string(), Value::Object(tls));

        return Ok(Value::Object(map));
    }

    // 4. AnyTLS
    if let Some(ref anytls) = server.anytls {
        let mut map = Map::new();
        map.insert("type".to_string(), json!("anytls"));
        map.insert("tag".to_string(), json!("proxy"));
        map.insert("server".to_string(), json!(anytls.host));
        map.insert("server_port".to_string(), json!(anytls.port));
        map.insert("password".to_string(), json!(anytls.password));

        let mut tls = Map::new();
        tls.insert("enabled".to_string(), json!(true));
        if let Some(ref sni) = anytls.sni {
            tls.insert("server_name".to_string(), json!(sni));
        }
        if !anytls.alpn.is_empty() {
            tls.insert("alpn".to_string(), json!(anytls.alpn));
        }
        map.insert("tls".to_string(), Value::Object(tls));

        return Ok(Value::Object(map));
    }

    // 5. Hysteria 2
    if let Some(ref hy2) = server.hysteria2 {
        let mut map = Map::new();
        map.insert("type".to_string(), json!("hysteria2"));
        map.insert("tag".to_string(), json!("proxy"));
        map.insert("server".to_string(), json!(hy2.host));
        map.insert("server_port".to_string(), json!(hy2.port));
        map.insert("password".to_string(), json!(hy2.auth));

        let mut tls = Map::new();
        tls.insert("enabled".to_string(), json!(true));
        if let Some(ref sni) = hy2.sni {
            tls.insert("server_name".to_string(), json!(sni));
        }
        if hy2.insecure {
            tls.insert("insecure".to_string(), json!(true));
        }
        map.insert("tls".to_string(), Value::Object(tls));

        if let Some(ref obfs_type) = hy2.obfs_type {
            if let Some(ref obfs_pw) = hy2.obfs_password {
                map.insert(
                    "obfs".to_string(),
                    json!({
                        "type": obfs_type,
                        "password": obfs_pw
                    }),
                );
            }
        }

        if let Some(up) = hy2.up_mbps {
            map.insert("up_mbps".to_string(), json!(up));
        }
        if let Some(down) = hy2.down_mbps {
            map.insert("down_mbps".to_string(), json!(down));
        }

        return Ok(Value::Object(map));
    }

    // 6. Xray client config (VLESS, Trojan, Shadowsocks, VMess)
    if let Some(ref xray) = server.xray {
        let mut map = Map::new();
        map.insert("tag".to_string(), json!("proxy"));
        map.insert("server".to_string(), json!(xray.host));
        map.insert("server_port".to_string(), json!(xray.port));

        match xray.protocol {
            XrayProtocol::Vless => {
                return Err(ConfigBuildError::Validation(
                    "VLESS requires xray.exe SOCKS5 sidecar detour port".to_string(),
                ));
            }
            XrayProtocol::Trojan => {
                map.insert("type".to_string(), json!("trojan"));
                map.insert("password".to_string(), json!(xray.credential));

                let mut tls = Map::new();
                tls.insert("enabled".to_string(), json!(true));
                if let Some(ref sni) = xray.sni {
                    tls.insert("server_name".to_string(), json!(sni));
                }
                if xray.allow_insecure {
                    tls.insert("insecure".to_string(), json!(true));
                }
                if !xray.alpn.is_empty() {
                    tls.insert("alpn".to_string(), json!(xray.alpn));
                }
                map.insert("tls".to_string(), Value::Object(tls));

                insert_singbox_transport(&mut map, xray);
            }
            XrayProtocol::Shadowsocks => {
                map.insert("type".to_string(), json!("shadowsocks"));
                map.insert(
                    "method".to_string(),
                    json!(xray.method.as_deref().unwrap_or("aes-128-gcm")),
                );
                map.insert("password".to_string(), json!(xray.credential));
            }
            XrayProtocol::Vmess => {
                map.insert("type".to_string(), json!("vmess"));
                map.insert("uuid".to_string(), json!(xray.credential));
                map.insert("alter_id".to_string(), json!(xray.alter_id));
                map.insert("security".to_string(), json!("auto"));

                if xray.security == XraySecurity::Tls {
                    let mut tls = Map::new();
                    tls.insert("enabled".to_string(), json!(true));
                    if let Some(ref sni) = xray.sni {
                        tls.insert("server_name".to_string(), json!(sni));
                    }
                    if xray.allow_insecure {
                        tls.insert("insecure".to_string(), json!(true));
                    }
                    map.insert("tls".to_string(), Value::Object(tls));
                }

                insert_singbox_transport(&mut map, xray);
            }
        }

        return Ok(Value::Object(map));
    }

    Err(ConfigBuildError::Validation(
        "No supported outbound configuration found for Sing-box".to_string(),
    ))
}

fn insert_singbox_transport(map: &mut Map<String, Value>, xray: &crate::models::XrayClientConfig) {
    match xray.transport {
        XrayTransport::WebSocket => {
            let mut tr = Map::new();
            tr.insert("type".to_string(), json!("ws"));
            if let Some(ref path) = xray.path {
                tr.insert("path".to_string(), json!(path));
            }
            if let Some(ref host) = xray.host_header {
                tr.insert("headers".to_string(), json!({ "Host": host }));
            }
            map.insert("transport".to_string(), Value::Object(tr));
        }
        XrayTransport::Grpc => {
            let mut tr = Map::new();
            tr.insert("type".to_string(), json!("grpc"));
            if let Some(ref sname) = xray.service_name {
                tr.insert("service_name".to_string(), json!(sname));
            }
            map.insert("transport".to_string(), Value::Object(tr));
        }
        XrayTransport::HttpUpgrade => {
            let mut tr = Map::new();
            tr.insert("type".to_string(), json!("httpupgrade"));
            if let Some(ref path) = xray.path {
                tr.insert("path".to_string(), json!(path));
            }
            if let Some(ref host) = xray.host_header {
                tr.insert("host".to_string(), json!(host));
            }
            map.insert("transport".to_string(), Value::Object(tr));
        }
        XrayTransport::Tcp => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsing::parse_share_link;

    #[test]
    fn test_build_singbox_vless_reality() {
        let link = "vless://11111111-1111-4111-8111-111111111111@reality.example.com:443?encryption=none&flow=xtls-rprx-vision&security=reality&sni=www.apple.com&fp=chrome&pbk=abcdef1234567890abcdef1234567890abcdef12345&sid=1234#RealityTest";
        let parsed = parse_share_link(link).expect("Parse VLESS Reality");

        let mut params = SingboxBuildParams::new(&parsed, RoutingMode::Tun);
        params.bypass_local_networks = true;
        params.block_quic = true;
        params.vless_socks_detour_port = Some(10810);

        let json_str = build_singbox_config(&params).expect("Build Sing-box JSON");
        let v: Value = serde_json::from_str(&json_str).expect("Valid JSON");

        // Inbound checks (1.14+ schema)
        assert_eq!(v["inbounds"][0]["type"], "tun");
        assert_eq!(v["inbounds"][0]["interface_name"], "BusterVPN_TUN");
        assert_eq!(v["inbounds"][0]["address"][0], "172.19.0.1/30");
        assert!(v["inbounds"][0].get("inet4_address").is_none());
        assert!(v["inbounds"][0].get("sniff").is_none());
        assert_eq!(v["inbounds"][1]["type"], "mixed");
        assert!(v["inbounds"][1].get("sniff").is_none());

        // DNS checks (typed objects, no "address" string)
        assert_eq!(v["dns"]["servers"][0]["tag"], "remote");
        assert_eq!(v["dns"]["servers"][0]["type"], "udp");
        assert_eq!(v["dns"]["servers"][0]["server"], "1.1.1.1");
        assert_eq!(v["dns"]["servers"][0]["detour"], "proxy");
        assert_eq!(v["dns"]["servers"][1]["tag"], "local");
        assert_eq!(v["dns"]["servers"][1]["type"], "udp");
        assert_eq!(v["dns"]["servers"][1]["server"], "77.88.8.8");
        assert_eq!(v["dns"]["final"], "remote");
        assert_eq!(v["dns"]["strategy"], "prefer_ipv4");

        // Outbound checks: only proxy (SOCKS5 sidecar) and direct (block and dns removed)
        let outbounds = v["outbounds"].as_array().expect("Outbounds array");
        assert_eq!(outbounds.len(), 2);
        assert_eq!(outbounds[1]["type"], "direct");
        assert!(outbounds.iter().all(|o| o["type"] != "block" && o["type"] != "dns"));

        let proxy = &outbounds[0];
        assert_eq!(proxy["type"], "socks");
        assert_eq!(proxy["tag"], "proxy");
        assert_eq!(proxy["server"], "127.0.0.1");
        assert_eq!(proxy["server_port"], 10810);
        assert_eq!(proxy["version"], "5");

        // Route checks (xray.exe direct rule at top, default_domain_resolver, modern actions)
        assert_eq!(v["route"]["default_domain_resolver"], "local");
        assert_eq!(v["route"]["final"], "proxy");
        assert_eq!(v["route"]["rules"][0]["process_name"], json!(["xray.exe"]));
        assert_eq!(v["route"]["rules"][0]["outbound"], "direct");
        assert_eq!(v["route"]["rules"][1]["action"], "sniff");
        assert_eq!(v["route"]["rules"][2]["protocol"], "dns");
        assert_eq!(v["route"]["rules"][2]["action"], "hijack-dns");
        assert_eq!(v["route"]["rules"][3]["domain"][0], "reality.example.com");
        assert_eq!(v["route"]["rules"][3]["outbound"], "direct");
        assert_eq!(v["route"]["rules"][4]["action"], "reject");
        assert_eq!(v["route"]["rules"][5]["network"], "udp");
        assert_eq!(v["route"]["rules"][5]["outbound"], "proxy");
        assert_eq!(v["route"]["rules"][6]["ip_is_private"], true);
        assert_eq!(v["route"]["rules"][6]["outbound"], "direct");
    }

    #[test]
    fn test_vless_reality_outbound_structure_and_ipv6_tun() {
        let link = "vless://11111111-1111-4111-8111-111111111111@reality.example.com:443?encryption=none&flow=xtls-rprx-vision&security=reality&sni=www.apple.com&fp=chrome&pbk=abcdef1234567890abcdef1234567890abcdef12345&sid=1234#RealityTest";
        let parsed = parse_share_link(link).expect("Parse VLESS Reality");

        let mut params = SingboxBuildParams::new(&parsed, RoutingMode::Tun);
        params.bypass_local_networks = true;
        params.block_quic = true;
        params.resolved_server_ips = Some(vec!["93.184.216.34".to_string()]);
        params.vless_socks_detour_port = Some(10810);

        let json_str = build_singbox_config(&params).expect("Build Sing-box JSON");
        let v: Value = serde_json::from_str(&json_str).expect("Valid JSON");

        // TUN address checks
        let addresses = v["inbounds"][0]["address"].as_array().expect("addresses array");
        assert!(addresses.iter().any(|a| a == "172.19.0.1/30"));
        assert!(addresses.iter().any(|a| a == "fdfe:dcba:9876::1/126"));

        // DNS checks
        assert!(v["dns"]["servers"][1].get("detour").is_none());
        assert_eq!(v["dns"]["servers"][1]["server"], "77.88.8.8");
        assert_eq!(v["dns"]["rules"][0]["domain"][0], "reality.example.com");

        // Xray sidecar Reality checks
        let xray_str = build_xray_vless_detour_config_from_parsed(&parsed, 10810).expect("Build Xray detour");
        let xv: Value = serde_json::from_str(&xray_str).expect("Valid Xray JSON");
        assert_eq!(xv["outbounds"][0]["streamSettings"]["realitySettings"]["serverName"], "www.apple.com");
        assert_eq!(
            xv["outbounds"][0]["streamSettings"]["realitySettings"]["password"],
            "abcdef1234567890abcdef1234567890abcdef12345"
        );
        assert_eq!(xv["outbounds"][0]["streamSettings"]["realitySettings"]["fingerprint"], "chrome");

        // STUN UDP proxy route check
        let stun_rule = v["route"]["rules"]
            .as_array()
            .expect("rules array")
            .iter()
            .find(|r| {
                r.get("network") == Some(&json!("udp"))
                    && r.get("outbound") == Some(&json!("proxy"))
                    && r.get("port") == Some(&json!([3478, 19302]))
            });
        assert!(stun_rule.is_some(), "STUN UDP route to proxy must be present");
        assert_eq!(v["outbounds"][0]["type"], "socks");
        assert_eq!(v["outbounds"][0]["version"], "5");
    }

    #[test]
    fn test_3x_ui_vless_reality_url_parsing_and_singbox_outbound() {
        let pbk = "n7_J_8q0h4rZz2a1b3c4d5e6f7g8h9i0j1k2l3m4n5o";
        let sid = "6ba7b810";
        let link = format!(
            "vless://27f8a1e2-45b6-4c7a-9a0d-83b1c6e4e5f6@vpn.buster.gripe:443?type=tcp&security=reality&pbk={}&fp=chrome&sni=github.com&sid={}&spx=%2F&flow=xtls-rprx-vision#3x-ui-Reality",
            pbk, sid
        );

        let parsed = parse_share_link(&link).expect("Parse 3x-ui VLESS Reality");
        let mut params = SingboxBuildParams::new(&parsed, RoutingMode::Tun);
        params.vless_socks_detour_port = Some(10810);
        let json_str = build_singbox_config(&params).expect("Build sing-box config");
        let v: Value = serde_json::from_str(&json_str).expect("Valid JSON");

        let proxy = &v["outbounds"][0];
        assert_eq!(proxy["type"], "socks");
        assert_eq!(proxy["server"], "127.0.0.1");
        assert_eq!(proxy["server_port"], 10810);
        assert_eq!(proxy["version"], "5");

        let xray_str = build_xray_vless_detour_config_from_parsed(&parsed, 10810).expect("Build Xray detour");
        let xv: Value = serde_json::from_str(&xray_str).expect("Valid Xray JSON");
        let reality = &xv["outbounds"][0]["streamSettings"]["realitySettings"];
        assert_eq!(reality["serverName"], "github.com");
        assert_ne!(reality["serverName"], "vpn.buster.gripe");
        assert_eq!(reality["password"], pbk);
        assert_eq!(reality["shortId"], sid);
        assert_eq!(reality["fingerprint"], "chrome");

        // Verify DNS rule for VPN server host
        assert_eq!(v["dns"]["rules"][0]["domain"][0], "vpn.buster.gripe");
        assert_eq!(v["dns"]["rules"][0]["server"], "local");
    }

    #[test]
    fn test_build_singbox_absolute_srs_paths() {
        let link = "vless://11111111-1111-4111-8111-111111111111@reality.example.com:443?encryption=none&flow=xtls-rprx-vision&security=reality&sni=www.apple.com&fp=chrome&pbk=abcdef1234567890abcdef1234567890abcdef12345&sid=1234#RealityTest";
        let parsed = parse_share_link(link).expect("Parse VLESS Reality");

        let mut params = SingboxBuildParams::new(&parsed, RoutingMode::Tun);
        let custom_dir = Path::new(r"C:\ProgramData\BusterVPN\runtime");
        params.working_dir = Some(custom_dir);
        params.vless_socks_detour_port = Some(10810);

        let json_str = build_singbox_config(&params).expect("Build Sing-box JSON");
        let v: Value = serde_json::from_str(&json_str).expect("Valid JSON");

        let rule_sets = v["route"]["rule_set"].as_array().expect("Rule sets");
        assert_eq!(rule_sets[0]["tag"], "geosite-ru");
        assert_eq!(rule_sets[0]["path"], "C:/ProgramData/BusterVPN/runtime/geosite-ru.srs");
        assert_eq!(rule_sets[1]["tag"], "geoip-ru");
        assert_eq!(rule_sets[1]["path"], "C:/ProgramData/BusterVPN/runtime/geoip-ru.srs");
    }

    #[test]
    fn test_build_singbox_tuic() {
        let link = "tuic://b53494e0-9e66-41ea-9c02-e20ecdd39ec2:mysecret@tuic.example.com:8443/?congestion_control=bbr&alpn=h3#TUICTest";
        let parsed = parse_share_link(link).expect("Parse TUIC");

        let params = SingboxBuildParams::new(&parsed, RoutingMode::SystemProxy);
        let json_str = build_singbox_config(&params).expect("Build Sing-box JSON");
        let v: Value = serde_json::from_str(&json_str).expect("Valid JSON");

        let proxy = &v["outbounds"][0];
        assert_eq!(proxy["type"], "tuic");
        assert_eq!(proxy["server"], "tuic.example.com");
        assert_eq!(proxy["uuid"], "b53494e0-9e66-41ea-9c02-e20ecdd39ec2");
        assert_eq!(proxy["congestion_control"], "bbr");
        assert_eq!(v["route"]["default_domain_resolver"], "local");
        assert_eq!(v["route"]["final"], "proxy");
    }

    #[test]
    fn test_build_singbox_wireguard() {
        let link = "wg://aGVsbG93b3JsZDExMTExMTExMTExMTExMTExMTExMTEx@198.51.100.1:51820?public_key=YnllZ29vZGJ5ZTIyMjIyMjIyMjIyMjIyMjIyMjIyMjIy&ip=10.0.0.2/32#WGTest";
        let parsed = parse_share_link(link).expect("Parse WG");

        let params = SingboxBuildParams::new(&parsed, RoutingMode::TunAndSystemProxy);
        let json_str = build_singbox_config(&params).expect("Build Sing-box JSON");
        let v: Value = serde_json::from_str(&json_str).expect("Valid JSON");

        let proxy = &v["endpoints"][0];
        assert_eq!(proxy["type"], "wireguard");
        assert_eq!(proxy["tag"], "proxy");
        assert_eq!(proxy["address"][0], "10.0.0.2/32");
        assert_eq!(proxy["peers"][0]["address"], "198.51.100.1");
        assert_eq!(proxy["peers"][0]["port"], 51820);
        assert_eq!(proxy["peers"][0]["allowed_ips"][0], "0.0.0.0/0");
        assert_eq!(v["route"]["default_domain_resolver"], "local");
        assert_eq!(v["route"]["final"], "proxy");
    }

    #[test]
    fn test_singbox_check_all_protocols_exit_code_zero() {
        let test_links = [
            ("VLESS Reality", "vless://11111111-1111-4111-8111-111111111111@reality.example.com:443?encryption=none&flow=xtls-rprx-vision&security=reality&sni=www.apple.com&fp=chrome&pbk=abcdef1234567890abcdef1234567890abcdef12345&sid=1234#RealityTest"),
            ("TUIC", "tuic://b53494e0-9e66-41ea-9c02-e20ecdd39ec2:mysecret@tuic.example.com:8443/?congestion_control=bbr&alpn=h3#TUICTest"),
            ("WireGuard", "wg://aGVsbG93b3JsZDExMTExMTExMTExMTExMTExMTExMTEx@198.51.100.1:51820?public_key=YnllZ29vZGJ5ZTIyMjIyMjIyMjIyMjIyMjIyMjIyMjIy&ip=10.0.0.2/32#WGTest"),
            ("Hysteria2", "hy2://mypassword@hy2.example.com:443?sni=hy2.example.com#Hy2Test"),
            ("Trojan", "trojan://trojanpassword@trojan.example.com:443?sni=trojan.example.com#TrojanTest"),
            ("Shadowsocks", "ss://YWVzLTEyOC1nY206c3NfcGFzc3dvcmQ=@ss.example.com:8388#SSTest"),
            ("ShadowTLS", "shadowtls://stls_password@stls.example.com:443?sni=stls.example.com&version=3#ShadowTLSTest"),
            ("AnyTLS", "anytls://anytls_password@anytls.example.com:443?sni=anytls.example.com#AnyTLSTest"),
        ];

        let cores_dir = std::path::Path::new(r"C:\BUSTERVPN\antivpnv2\crates\buster-app\cores");
        let singbox_exe = cores_dir.join("sing-box.exe");

        if !singbox_exe.is_file() {
            return;
        }

        let temp_dir = std::env::temp_dir();

        for (name, link) in test_links {
            let parsed = parse_share_link(link).expect(name);
            let mut params = SingboxBuildParams::new(&parsed, RoutingMode::Tun);
            params.bypass_local_networks = true;
            params.block_quic = true;
            params.working_dir = Some(cores_dir);
            if parsed.xray.as_ref().is_some_and(|x| x.protocol == XrayProtocol::Vless) {
                params.vless_socks_detour_port = Some(10810);
            }

            let json_str = build_singbox_config(&params).expect(name);
            let config_file = temp_dir.join(format!("test_check_{}.json", name.replace(' ', "_")));
            std::fs::write(&config_file, &json_str).expect("write temp config");

            let output = std::process::Command::new(&singbox_exe)
                .current_dir(&temp_dir)
                .args(["check", "-c", config_file.to_str().unwrap()])
                .output()
                .expect("run sing-box check");

            println!("{}: check exit code {:?}", name, output.status.code());
            let _ = std::fs::remove_file(&config_file);

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                panic!("sing-box check failed for {}: status={:?}\nstdout: {}\nstderr: {}\nconfig:\n{}", name, output.status.code(), stdout, stderr, json_str);
            }
        }
    }

    #[test]
    fn test_vless_socks_detour_outbound() {
        let link = "vless://11111111-1111-4111-8111-111111111111@reality.example.com:443?encryption=none&flow=xtls-rprx-vision&security=reality&sni=www.apple.com&fp=chrome&pbk=abcdef1234567890abcdef1234567890abcdef12345&sid=1234#RealityTest";
        let parsed = parse_share_link(link).expect("Parse VLESS");

        // 1. With detour port set: emits SOCKS5 outbound and xray.exe direct rule at top of route.rules
        let mut params = SingboxBuildParams::new(&parsed, RoutingMode::Tun);
        params.vless_socks_detour_port = Some(10810);
        let json_str = build_singbox_config(&params).expect("Build Sing-box JSON with detour");
        let v: Value = serde_json::from_str(&json_str).expect("Valid JSON");
        let proxy = &v["outbounds"][0];
        assert_eq!(proxy["type"], "socks");
        assert_eq!(proxy["tag"], "proxy");
        assert_eq!(proxy["server"], "127.0.0.1");
        assert_eq!(proxy["server_port"], 10810);
        assert_eq!(proxy["version"], "5");
        assert_eq!(v["route"]["rules"][0]["process_name"], json!(["xray.exe"]));
        assert_eq!(v["route"]["rules"][0]["outbound"], "direct");

        // 2. Without detour port: returns explicit Err (zero fallback to native sing-box VLESS)
        let mut params_no_detour = SingboxBuildParams::new(&parsed, RoutingMode::Tun);
        params_no_detour.vless_socks_detour_port = None;
        let err = build_singbox_config(&params_no_detour).expect_err("VLESS without detour port must fail");
        assert!(
            err.to_string().contains("VLESS requires xray.exe SOCKS5 sidecar detour port"),
            "Unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_build_xray_vless_detour_config_validity() {
        let link = "vless://11111111-1111-4111-8111-111111111111@reality.example.com:443?encryption=none&flow=xtls-rprx-vision&security=reality&sni=www.apple.com&fp=chrome&pbk=abcdef1234567890abcdef1234567890abcdef12345&sid=1234#RealityTest";
        let parsed = parse_share_link(link).expect("Parse VLESS");

        let mut profile = ServerProfile::new("RealityTest", crate::models::BackendKind::SingBox, "reality.example.com", 443);
        profile.raw_link = Some(link.to_string());

        let xray_cfg_str = build_xray_vless_detour_config(&profile, 10810).expect("Build Xray Detour Config");
        let parsed_cfg_str = build_xray_vless_detour_config_from_parsed(&parsed, 10810).expect("Build from parsed");

        assert_eq!(xray_cfg_str, parsed_cfg_str);

        let v: Value = serde_json::from_str(&xray_cfg_str).expect("Valid JSON");
        assert_eq!(v["log"]["loglevel"], "warning");
        assert_eq!(v["inbounds"][0]["tag"], "socks-in");
        assert_eq!(v["inbounds"][0]["port"], 10810);
        assert_eq!(v["inbounds"][0]["protocol"], "socks");
        assert_eq!(v["inbounds"][0]["settings"]["udp"], true);

        assert_eq!(v["outbounds"][0]["tag"], "proxy");
        assert_eq!(v["outbounds"][0]["protocol"], "vless");
        assert_eq!(v["outbounds"][0]["streamSettings"]["security"], "reality");
        assert_eq!(v["outbounds"][0]["streamSettings"]["realitySettings"]["serverName"], "www.apple.com");
        assert_eq!(v["outbounds"][1]["tag"], "direct");
        assert_eq!(v["outbounds"][1]["protocol"], "freedom");

        // Verify with bundled xray.exe if present
        let cores_dir = std::path::Path::new(r"C:\BUSTERVPN\antivpnv2\crates\buster-app\cores");
        let xray_exe = cores_dir.join("xray.exe");
        if xray_exe.is_file() {
            let temp_cfg = std::env::temp_dir().join("test_detour_xray.json");
            std::fs::write(&temp_cfg, &xray_cfg_str).expect("write test_detour_xray.json");
            let output = std::process::Command::new(&xray_exe)
                .args(["-test", "-config", temp_cfg.to_str().unwrap()])
                .output()
                .expect("run xray test");
            let _ = std::fs::remove_file(&temp_cfg);
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                panic!("xray -test -config failed: status={:?}\nstdout: {}\nstderr: {}", output.status.code(), stdout, stderr);
            }
        }
    }
}
