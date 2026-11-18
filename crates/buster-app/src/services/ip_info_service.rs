//! Public IP and geolocation lookup service with proxy support and fallback.

use std::time::Duration;
use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};
use tracing::{debug, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpInfo {
    pub ip: String,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub city: Option<String>,
    pub isp: Option<String>,
    pub org: Option<String>,
}

#[derive(Deserialize)]
struct IpWhoIsResponse {
    ip: Option<String>,
    country: Option<String>,
    country_code: Option<String>,
    city: Option<String>,
    connection: Option<IpWhoIsConnection>,
}

#[derive(Deserialize)]
struct IpWhoIsConnection {
    isp: Option<String>,
    org: Option<String>,
}

#[derive(Deserialize)]
struct IpApiResponse {
    query: Option<String>,
    country: Option<String>,
    #[serde(rename = "countryCode")]
    country_code: Option<String>,
    city: Option<String>,
    isp: Option<String>,
    org: Option<String>,
}

pub struct IpInfoService {
    direct_client: reqwest::Client,
}

impl Default for IpInfoService {
    fn default() -> Self {
        Self::new()
    }
}

impl IpInfoService {
    pub fn new() -> Self {
        let direct_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .user_agent("BusterVPN-Client/1.4.0")
            .build()
            .unwrap_or_default();

        Self { direct_client }
    }

    fn build_proxy_client(&self, proxy_url: &str) -> Option<reqwest::Client> {
        let proxy = reqwest::Proxy::all(proxy_url).ok()?;
        reqwest::Client::builder()
            .timeout(Duration::from_secs(6))
            .proxy(proxy)
            .user_agent("BusterVPN-Client/1.4.0")
            .build()
            .ok()
    }

    /// Fetches the current public IP info. If `use_proxy` is true, routes through local proxy first.
    pub async fn fetch_ip_info(&self, use_proxy: bool) -> Result<IpInfo> {
        if use_proxy {
            // Try via local HTTP proxy (default port 10809)
            if let Some(proxy_client) = self.build_proxy_client("http://127.0.0.1:10809") {
                debug!("Attempting IP resolution via local proxy 127.0.0.1:10809");
                if let Ok(info) = Self::query_providers(&proxy_client).await {
                    return Ok(info);
                }
                warn!("Proxy IP query failed, falling back to direct query");
            }
        }

        // Direct query fallback
        Self::query_providers(&self.direct_client).await
    }

    async fn query_providers(client: &reqwest::Client) -> Result<IpInfo> {
        // Provider 1: ipwho.is (HTTPS, fast, rich geolocation, unblocked)
        if let Ok(info) = Self::query_ipwhois(client).await {
            return Ok(info);
        }

        // Provider 2: ip-api.com
        if let Ok(info) = Self::query_ipapi(client).await {
            return Ok(info);
        }

        anyhow::bail!("All IP info lookup providers failed")
    }

    async fn query_ipwhois(client: &reqwest::Client) -> Result<IpInfo> {
        let resp = client
            .get("https://ipwho.is/")
            .send()
            .await
            .context("ipwho.is request failed")?;

        let data: IpWhoIsResponse = resp.json().await.context("ipwho.is JSON deserialize failed")?;

        let ip = data.ip.filter(|s| !s.is_empty()).context("Missing IP from ipwho.is")?;
        let isp = data.connection.as_ref().and_then(|c| c.isp.clone());
        let org = data.connection.as_ref().and_then(|c| c.org.clone());

        Ok(IpInfo {
            ip,
            country: data.country,
            country_code: data.country_code,
            city: data.city,
            isp: isp.clone(),
            org: org.or(isp),
        })
    }

    async fn query_ipapi(client: &reqwest::Client) -> Result<IpInfo> {
        let resp = client
            .get("http://ip-api.com/json/?fields=query,country,countryCode,city,isp,org")
            .send()
            .await
            .context("ip-api.com request failed")?;

        let data: IpApiResponse = resp.json().await.context("ip-api.com JSON deserialize failed")?;

        let ip = data.query.filter(|s| !s.is_empty()).context("Missing IP from ip-api.com")?;

        Ok(IpInfo {
            ip,
            country: data.country,
            country_code: data.country_code,
            city: data.city,
            isp: data.isp.clone(),
            org: data.org.or(data.isp),
        })
    }
}
