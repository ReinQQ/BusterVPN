use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::models::{
    AnyTlsClientConfig, BackendKind, ConfigurationError, Hysteria2ClientConfig,
    ShadowTlsClientConfig, TuicClientConfig, WireGuardClientConfig, XrayClientConfig,
};

/// Persisted server entry. Raw link contains credentials and is protected with DPAPI
/// before writing to disk on Windows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerProfile {
    pub id: Uuid,
    pub name: String,
    pub backend: BackendKind,
    pub group: String,

    /// Subscription record that owns this node. None for manually pasted links.
    pub subscription_id: Option<Uuid>,

    /// Original share link. Sensitive: contains UUID/passwords.
    pub raw_link: Option<String>,

    pub host: Option<String>,
    pub port: u16,

    pub is_favorite: bool,
    pub last_latency_ms: Option<i64>,
    pub last_latency_at_utc: Option<DateTime<Utc>>,
    pub created_at_utc: DateTime<Utc>,
}

impl ServerProfile {
    pub fn new(name: impl Into<String>, backend: BackendKind, host: impl Into<String>, port: u16) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            backend,
            group: backend.display_name().to_string(),
            subscription_id: None,
            raw_link: None,
            host: Some(host.into()),
            port,
            is_favorite: false,
            last_latency_ms: None,
            last_latency_at_utc: None,
            created_at_utc: Utc::now(),
        }
    }

    pub fn endpoint(&self) -> String {
        match &self.host {
            None => "No endpoint".to_string(),
            Some(h) if h.trim().is_empty() => "No endpoint".to_string(),
            Some(h) if self.port > 0 => format!("{}:{}", h, self.port),
            Some(h) => h.clone(),
        }
    }

    pub fn initial(&self) -> String {
        self.name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string())
    }

    pub fn validate(&self) -> Result<(), ConfigurationError> {
        if self.raw_link.as_deref().unwrap_or("").trim().is_empty() {
            return Err(ConfigurationError::MissingProfilePayload);
        }

        if self.port == 0 {
            return Err(ConfigurationError::InvalidServerPort(self.port as u32));
        }

        Ok(())
    }
}

/// A parsed share link/manual profile, ready to be turned into a core config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ParsedServer {
    pub backend: BackendKind,
    pub name: String,
    pub host: Option<String>,
    pub port: u16,
    pub xray: Option<XrayClientConfig>,
    pub hysteria2: Option<Hysteria2ClientConfig>,
    pub tuic: Option<TuicClientConfig>,
    pub wireguard: Option<WireGuardClientConfig>,
    pub shadowtls: Option<ShadowTlsClientConfig>,
    pub anytls: Option<AnyTlsClientConfig>,
}

impl ParsedServer {
    pub fn to_profile(&self, raw_link: Option<String>) -> ServerProfile {
        ServerProfile {
            id: Uuid::new_v4(),
            name: self.name.clone(),
            backend: self.backend,
            group: self.backend.display_name().to_string(),
            subscription_id: None,
            raw_link,
            host: self.host.clone(),
            port: self.port,
            is_favorite: false,
            last_latency_ms: None,
            last_latency_at_utc: None,
            created_at_utc: Utc::now(),
        }
    }
}
