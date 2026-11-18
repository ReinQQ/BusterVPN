use serde::{Deserialize, Serialize};
use crate::models::ConfigurationError;
use crate::parsing::validation::{is_valid_host, is_valid_port};

/// Normalized Hysteria2 client configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hysteria2ClientConfig {
    pub host: String,
    pub port: u16,
    pub server_address: String,
    pub auth: String,

    pub sni: Option<String>,
    pub insecure: bool,
    pub pin_sha256: Option<String>,
    pub ech: Option<String>,

    /// "salamander", "gecko", or None when obfuscation is disabled.
    pub obfs_type: Option<String>,
    pub obfs_password: Option<String>,

    /// Optional BusterVPN extension: enables Hysteria Brutal when set.
    pub up_mbps: Option<u32>,
    pub down_mbps: Option<u32>,
}

impl Hysteria2ClientConfig {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if !is_valid_host(&self.host) {
            return Err(ConfigurationError::InvalidServerHost(self.host.clone()));
        }

        if !is_valid_port(self.port as u32) {
            return Err(ConfigurationError::InvalidServerPort(self.port as u32));
        }

        if self.server_address.trim().is_empty() || self.server_address.chars().any(char::is_whitespace) {
            return Err(ConfigurationError::InvalidServerAddress);
        }

        if self.auth.is_empty() {
            return Err(ConfigurationError::EmptyAuth);
        }

        if self.auth.len() > 4096 || self.auth.chars().any(char::is_control) {
            return Err(ConfigurationError::InvalidAuthCharacters);
        }

        if self.server_address.len() > 512 || self.server_address.chars().any(char::is_control) {
            return Err(ConfigurationError::InvalidServerAddress);
        }

        Ok(())
    }
}
