//! BusterVPN Core library: data models, link parsers, config builders, and subscriptions.

pub mod config;
pub mod models;
pub mod parsing;

pub use config::*;
pub use models::{
    enums::*, hysteria2::Hysteria2ClientConfig, profile::*, routing::*, xray::*, ConfigurationError,
};
pub use parsing::{
    encoding::*, hysteria2::parse_hysteria2, shadowsocks::*, trojan::*, validation::*, vless::*,
    vmess::*, looks_like_share_link, parse_batch, parse_share_link, LinkParseError,
};
