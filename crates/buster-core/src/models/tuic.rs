use serde::{Deserialize, Serialize};
use crate::models::ConfigurationError;
use crate::parsing::validation::{is_valid_host, is_valid_port};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TuicClientConfig {
    pub uuid: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub congestion_control: Option<String>,
    pub alpn: Vec<String>,
    pub sni: Option<String>,
    pub allow_insecure: bool,
    pub disable_sni: bool,
}

impl TuicClientConfig {
    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if !is_valid_host(&self.host) {
            return Err(ConfigurationError::InvalidServerHost(self.host.clone()));
        }
        if !is_valid_port(self.port as u32) {
            return Err(ConfigurationError::InvalidServerPort(self.port as u32));
        }
        if self.uuid.trim().is_empty() && self.password.trim().is_empty() {
            return Err(ConfigurationError::EmptyCredential);
        }
        Ok(())
    }
}
