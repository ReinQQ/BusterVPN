pub mod enums;
pub mod hysteria2;
pub mod profile;
pub mod routing;
pub mod xray;

pub use enums::*;
pub use hysteria2::*;
pub use profile::*;
pub use routing::*;
pub use xray::*;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigurationError {
    #[error("Unknown backend")]
    UnknownBackend,
    #[error("Server has no profile payload")]
    MissingProfilePayload,
    #[error("Server port {0} is outside valid range (1..=65535)")]
    InvalidServerPort(u32),
    #[error("Server host '{0}' is invalid")]
    InvalidServerHost(String),
    #[error("Xray credential is empty")]
    EmptyCredential,
    #[error("Shadowsocks encryption method is missing")]
    MissingShadowsocksMethod,
    #[error("VLESS Reality requires a public key (pbk)")]
    RealityMissingPublicKey,
    #[error("Reality is supported only with TCP/gRPC transports")]
    RealityUnsupportedTransport,
    #[error("Hysteria2 server address is invalid")]
    InvalidServerAddress,
    #[error("Hysteria2 authentication value is missing")]
    EmptyAuth,
    #[error("Hysteria2 authentication value contains invalid characters")]
    InvalidAuthCharacters,
}
