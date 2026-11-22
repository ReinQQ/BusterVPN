use serde::{Deserialize, Serialize};
use crate::models::ConfigurationError;
use crate::parsing::validation::{is_valid_host, is_valid_port};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowTlsClientConfig {
    pub host: String,
    pub port: u16,
    pub password: String,
    pub sni: Option<String>,
    pub version: u8,
}

impl ShadowTlsClientConfig {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if !is_valid_host(&self.host) {
            return Err(ConfigurationError::InvalidServerHost(self.host.clone()));
        }
        if !is_valid_port(self.port as u32) {
            return Err(ConfigurationError::InvalidServerPort(self.port as u32));
        }
        if self.password.trim().is_empty() {
            return Err(ConfigurationError::EmptyCredential);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnyTlsClientConfig {
    pub host: String,
    pub port: u16,
    pub password: String,
    pub sni: Option<String>,
    pub alpn: Vec<String>,
}

impl AnyTlsClientConfig {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if !is_valid_host(&self.host) {
            return Err(ConfigurationError::InvalidServerHost(self.host.clone()));
        }
        if !is_valid_port(self.port as u32) {
            return Err(ConfigurationError::InvalidServerPort(self.port as u32));
        }
        if self.password.trim().is_empty() {
            return Err(ConfigurationError::EmptyCredential);
        }
        Ok(())
    }
}
