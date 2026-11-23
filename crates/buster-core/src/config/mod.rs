pub mod singbox_builder;

pub use singbox_builder::*;

use thiserror::Error;
use crate::models::ConfigurationError;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigBuildError {
    #[error("Missing Xray client configuration")]
    MissingXrayConfig,
    #[error("Missing Hysteria2 client configuration")]
    MissingHysteriaConfig,
    #[error("Invalid proxy ports: socks={0}, http={1}")]
    InvalidProxyPorts(u16, u16),
    #[error("TUN CIDR prefix is invalid: {0}")]
    InvalidTunCidr(String),
    #[error("Configuration validation error: {0}")]
    Configuration(#[from] ConfigurationError),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Validation error: {0}")]
    Validation(String),
}
