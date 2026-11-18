use serde::{Deserialize, Serialize};
use crate::models::ConfigurationError;
use crate::parsing::validation::{is_valid_host, is_valid_port};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum XrayProtocol {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum XrayTransport {
    #[default]
    Tcp,
    WebSocket,
    Grpc,
    HttpUpgrade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum XraySecurity {
    #[default]
    None,
    Tls,
    Reality,
}

/// Normalized client settings for the Xray core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XrayClientConfig {
    pub protocol: XrayProtocol,
    pub host: String,
    pub port: u16,

    /// VLESS/VMess UUID, Trojan password, or Shadowsocks password.
    pub credential: String,

    /// Shadowsocks method. Unused for other protocols.
    pub method: Option<String>,

    /// Legacy VMess alterId. Modern deployments use 0.
    pub alter_id: i32,

    /// VLESS flow control, for example "xtls-rprx-vision".
    pub flow: Option<String>,

    pub transport: XrayTransport,
    pub security: XraySecurity,

    pub sni: Option<String>,
    pub host_header: Option<String>,
    pub path: Option<String>,
    pub service_name: Option<String>,
    pub fingerprint: Option<String>,
    pub reality_public_key: Option<String>,
    pub reality_short_id: Option<String>,
    pub spider_x: Option<String>,
    pub alpn: Vec<String>,
    pub allow_insecure: bool,
}

impl XrayClientConfig {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if !is_valid_host(&self.host) {
            return Err(ConfigurationError::InvalidServerHost(self.host.clone()));
        }

        if !is_valid_port(self.port as u32) {
            return Err(ConfigurationError::InvalidServerPort(self.port as u32));
        }

        if self.credential.trim().is_empty() {
            return Err(ConfigurationError::EmptyCredential);
        }

        if self.protocol == XrayProtocol::Shadowsocks && self.method.as_deref().unwrap_or("").trim().is_empty() {
            return Err(ConfigurationError::MissingShadowsocksMethod);
        }

        if self.security == XraySecurity::Reality {
            if self.reality_public_key.as_deref().unwrap_or("").trim().is_empty() {
                return Err(ConfigurationError::RealityMissingPublicKey);
            }

            if !matches!(self.transport, XrayTransport::Tcp | XrayTransport::Grpc) {
                return Err(ConfigurationError::RealityUnsupportedTransport);
            }
        }

        Ok(())
    }
}
