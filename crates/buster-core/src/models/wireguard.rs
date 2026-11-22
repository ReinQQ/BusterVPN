use serde::{Deserialize, Serialize};
use crate::models::ConfigurationError;
use crate::parsing::validation::{is_valid_host, is_valid_port};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireGuardClientConfig {
    pub host: String,
    pub port: u16,
    pub private_key: String,
    pub peer_public_key: String,
    pub preshared_key: Option<String>,
    pub local_address: Vec<String>,
    pub allowed_ips: Vec<String>,
    pub mtu: Option<u16>,
    pub reserved: Option<Vec<u8>>,
}

impl WireGuardClientConfig {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if !is_valid_host(&self.host) {
            return Err(ConfigurationError::InvalidServerHost(self.host.clone()));
        }
        if !is_valid_port(self.port as u32) {
            return Err(ConfigurationError::InvalidServerPort(self.port as u32));
        }
        if self.private_key.trim().is_empty() || self.peer_public_key.trim().is_empty() {
            return Err(ConfigurationError::EmptyCredential);
        }
        Ok(())
    }
}
