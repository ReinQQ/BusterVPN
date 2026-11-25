//! Subscription models, user info, and document structures.

use buster_core::models::ServerProfile;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionBodyFormat {
    Unknown,
    Base64,
    PlainText,
    Json,
    ClashYaml,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionUserInfo {
    #[serde(alias = "upload_bytes")]
    pub uploaded_bytes: i64,
    #[serde(alias = "download_bytes")]
    pub downloaded_bytes: i64,
    pub total_bytes: i64,
    pub expires_at_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub expire_timestamp: Option<i64>,
}

impl SubscriptionUserInfo {
    pub fn used_bytes(&self) -> i64 {
        self.uploaded_bytes.max(0) + self.downloaded_bytes.max(0)
    }

    pub fn is_unlimited(&self) -> bool {
        self.total_bytes <= 0
    }

    /// Parses values from the `Subscription-Userinfo` HTTP header:
    /// e.g. "upload=1000; download=2000; total=10000000; expire=1700000000"
    pub fn parse(header_value: &str) -> Option<Self> {
        let mut upload = None;
        let mut download = None;
        let mut total = None;
        let mut expire = None;

        for pair in header_value.split(';') {
            let mut parts = pair.split('=');
            if let (Some(key), Some(val)) = (parts.next(), parts.next()) {
                let key = key.trim().to_lowercase();
                let val_str = val.trim();
                if let Ok(v) = val_str.parse::<i64>() {
                    match key.as_str() {
                        "upload" => upload = Some(v),
                        "download" => download = Some(v),
                        "total" => total = Some(v),
                        "expire" => expire = Some(v),
                        _ => {}
                    }
                }
            }
        }

        if upload.is_none() && download.is_none() && total.is_none() && expire.is_none() {
            return None;
        }

        let expire_timestamp = expire.filter(|&e| e > 0);
        let expires_at_utc = expire_timestamp.and_then(|e| {
            let secs = if e > 100_000_000_000 { e / 1000 } else { e };
            DateTime::from_timestamp(secs, 0)
        });

        Some(Self {
            uploaded_bytes: upload.unwrap_or(0).max(0),
            downloaded_bytes: download.unwrap_or(0).max(0),
            total_bytes: total.unwrap_or(0).max(0),
            expires_at_utc,
            expire_timestamp,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRecord {
    pub id: Uuid,
    pub url: String,
    pub title: String,
    pub group: String,
    pub node_count: usize,
    pub user_info: Option<SubscriptionUserInfo>,
    pub auto_update: bool,
    pub added_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub last_error: Option<String>,
}

impl SubscriptionRecord {
    pub fn new(url: String, title: String) -> Self {
        let clean_title = crate::services::subscription_service::decode_subscription_title(&title);
        let group = clean_title.clone();
        Self {
            id: Uuid::new_v4(),
            url,
            title: clean_title,
            group,
            node_count: 0,
            user_info: None,
            auto_update: true,
            added_at_utc: Utc::now(),
            updated_at_utc: Utc::now(),
            last_error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SubscriptionDocument {
    pub source_url: String,
    pub format: SubscriptionBodyFormat,
    pub title: Option<String>,
    pub profiles: Vec<ServerProfile>,
    pub user_info: Option<SubscriptionUserInfo>,
    pub warnings: Vec<String>,
}
