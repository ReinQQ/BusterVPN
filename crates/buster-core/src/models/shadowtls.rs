use serde::{Deserialize, Serialize};
use crate::models::ConfigurationError;
use crate::parsing::validation::{is_valid_host, is_valid_port};

fn validate_tls_client(host: &str, port: u16, password: &str) -> Result<(), ConfigurationError> {
    if !is_valid_host(host) {
        return Err(ConfigurationError::InvalidServerHost(host.to_string()));
    }
    if !is_valid_port(port as u32) {
        return Err(ConfigurationError::InvalidServerPort(port as u32));
    }
    if password.trim().is_empty() {
        return Err(ConfigurationError::EmptyCredential);
    }
    Ok(())
}

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
        validate_tls_client(&self.host, self.port, &self.password)
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
        validate_tls_client(&self.host, self.port, &self.password)
    }
}
