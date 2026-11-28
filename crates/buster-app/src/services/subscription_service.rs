//! Subscription downloader, body decoder, and server profile extractor.

use std::time::Duration;
use base64::Engine;
use reqwest::header::HeaderMap;
use anyhow::{bail, Context, Result};
use buster_core::models::ServerProfile;
use buster_core::parsing::{is_clash_yaml, parse_clash_profiles, parse_share_link};
use crate::models::{SubscriptionBodyFormat, SubscriptionDocument, SubscriptionUserInfo};

pub const APP_VERSION: &str = "1.6.0";
pub const USER_AGENT: &str = "BusterVPN/1.6.0 (Windows NT 10.0; Win64; x64)";

pub struct SubscriptionService {
    client: reqwest::Client,
}

impl Default for SubscriptionService {
    fn default() -> Self {
        Self::new()
    }
}

impl SubscriptionService {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_default();

        Self { client }
    }

    /// Fetches a subscription URL and parses all contained server nodes.
    pub async fn fetch(&self, url: &str) -> Result<SubscriptionDocument> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("Failed to connect to subscription URL: {}", url))?;

        let status = response.status();
        if !status.is_success() {
            bail!("Subscription server responded with HTTP status {}", status);
        }

        let headers = response.headers().clone();
        let user_info = extract_user_info(&headers);
        let title = extract_title(&headers);

        let body_bytes = response
            .bytes()
            .await
            .context("Failed to read subscription response body")?;

        let raw_text = String::from_utf8_lossy(&body_bytes);

        let mut warnings = Vec::new();
        let (profiles, format) = if is_clash_yaml(&raw_text) {
            let list = parse_clash_profiles(&raw_text, title.as_deref());
            (list, SubscriptionBodyFormat::ClashYaml)
        } else if let Some(decoded) = try_decode_base64(&raw_text) {
            if is_clash_yaml(&decoded) {
                let list = parse_clash_profiles(&decoded, title.as_deref());
                (list, SubscriptionBodyFormat::ClashYaml)
            } else {
                let mut list = Vec::new();
                for line in decoded.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        continue;
                    }
                    match parse_share_link(trimmed) {
                        Ok(parsed) => {
                            let mut profile = parsed.to_profile(Some(trimmed.to_string()));
                            if let Some(ref t) = title {
                                profile.group = t.clone();
                            }
                            list.push(profile);
                        }
                        Err(e) => {
                            warnings.push(format!("Could not parse link '{}': {}", trimmed, e));
                        }
                    }
                }
                (list, SubscriptionBodyFormat::Base64)
            }
        } else {
            let mut list = Vec::new();
            for line in raw_text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                match parse_share_link(trimmed) {
                    Ok(parsed) => {
                        let mut profile = parsed.to_profile(Some(trimmed.to_string()));
                        if let Some(ref t) = title {
                            profile.group = t.clone();
                        }
                        list.push(profile);
                    }
                    Err(e) => {
                        warnings.push(format!("Could not parse link '{}': {}", trimmed, e));
                    }
                }
            }
            (list, SubscriptionBodyFormat::PlainText)
        };

        tracing::info!(
            "Subscription fetched from {}: parsed {} profiles, {} warnings (format: {:?})",
            url,
            profiles.len(),
            warnings.len(),
            format
        );

        Ok(SubscriptionDocument {
            source_url: url.to_string(),
            format,
            title,
            profiles,
            user_info,
            warnings,
        })
    }

    /// Parses arbitrary text content (Clash YAML, Base64 list, or raw multi-line share links)
    /// into server profiles.
    pub fn parse_text_content(&self, text: &str, title: Option<String>) -> Vec<ServerProfile> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        if is_clash_yaml(trimmed) {
            return parse_clash_profiles(trimmed, title.as_deref());
        }

        if let Some(decoded) = try_decode_base64(trimmed) {
            if is_clash_yaml(&decoded) {
                return parse_clash_profiles(&decoded, title.as_deref());
            }

            let mut list = Vec::new();
            for line in decoded.lines() {
                let l = line.trim();
                if l.is_empty() || l.starts_with('#') {
                    continue;
                }
                if let Ok(parsed) = parse_share_link(l) {
                    let mut profile = parsed.to_profile(Some(l.to_string()));
                    if let Some(ref t) = title {
                        profile.group = t.clone();
                    }
                    list.push(profile);
                }
            }
            if !list.is_empty() {
                return list;
            }
        }

        let mut list = Vec::new();
        for line in trimmed.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            if let Ok(parsed) = parse_share_link(l) {
                let mut profile = parsed.to_profile(Some(l.to_string()));
                if let Some(ref t) = title {
                    profile.group = t.clone();
                }
                list.push(profile);
            }
        }
        list
    }
}

fn extract_user_info(headers: &HeaderMap) -> Option<SubscriptionUserInfo> {
    headers
        .get("Subscription-Userinfo")
        .or_else(|| headers.get("subscription-userinfo"))
        .and_then(|val| val.to_str().ok())
        .and_then(SubscriptionUserInfo::parse)
}

fn extract_title(headers: &HeaderMap) -> Option<String> {
    headers
        .get("Profile-Title")
        .or_else(|| headers.get("profile-title"))
        .and_then(|val| val.to_str().ok())
        .map(|s| decode_subscription_title(s.trim()))
}

/// Decodes subscription title if prefixed with `base64:` (case-insensitive).
pub fn decode_subscription_title(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(b64_part) = trimmed.strip_prefix("base64:").or_else(|| trimmed.strip_prefix("BASE64:")) {
        let b64_clean: String = b64_part.chars().filter(|c| !c.is_whitespace()).collect();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64_clean)
            .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(&b64_clean))
            .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(&b64_clean))
            .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&b64_clean));

        if let Ok(bytes) = decoded {
            if let Ok(decoded_str) = String::from_utf8(bytes) {
                let s = decoded_str.trim();
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
    }
    trimmed.to_string()
}

fn try_decode_base64(input: &str) -> Option<String> {
    let clean: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.len() < 16 {
        return None;
    }

    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&clean) {
        if let Ok(s) = String::from_utf8(bytes) {
            if s.contains("://") {
                return Some(s);
            }
        }
    }

    if let Ok(bytes) = base64::engine::general_purpose::URL_SAFE.decode(&clean) {
        if let Ok(s) = String::from_utf8(bytes) {
            if s.contains("://") {
                return Some(s);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_version_and_user_agent() {
        assert_eq!(APP_VERSION, "1.6.0");
        assert!(USER_AGENT.contains("1.6.0"));
    }

    #[test]
    fn test_decode_base64_subscription_title() {
        assert_eq!(
            decode_subscription_title("base64:QnVzdGVyTmV0d29yaw=="),
            "BusterNetwork"
        );
        assert_eq!(
            decode_subscription_title("base64: 0JHRg9GB0YLQtdGA "),
            "Бустер"
        );
        assert_eq!(
            decode_subscription_title("Standard Title"),
            "Standard Title"
        );
    }

    #[test]
    fn test_parse_text_content_clash_and_links() {
        let service = SubscriptionService::new();
        let clash_yaml = r#"
proxies:
  - name: "Clash Node"
    type: vless
    server: 1.1.1.1
    port: 443
    uuid: 11111111-1111-4111-8111-111111111111
    network: tcp
"#;
        let profiles = service.parse_text_content(clash_yaml, Some("My Clash".to_string()));
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "Clash Node");
        assert_eq!(profiles[0].group, "My Clash");

        let raw_links = "vless://11111111-1111-4111-8111-111111111111@1.2.3.4:443#RawNode\n# comment\nhy2://pass@5.6.7.8:8443#Hy2Node";
        let profiles2 = service.parse_text_content(raw_links, None);
        assert_eq!(profiles2.len(), 2);
        assert_eq!(profiles2[0].name, "RawNode");
        assert_eq!(profiles2[1].name, "Hy2Node");
    }
}
