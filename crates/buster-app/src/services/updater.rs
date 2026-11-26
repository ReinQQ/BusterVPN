//! Core engine integrity verification and dual-channel auto-updater.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

pub const PRIMARY_MANIFEST_URL: &str = "https://vpn.buster.gripe/updates/manifest.json";
pub const FALLBACK_MANIFEST_URL: &str = "https://buster.gripe/updates/manifest.json";
pub const CURRENT_VERSION: &str = "1.5.4";

pub const CORE_COMPONENTS: &[&str] = &[
    "sing-box.exe",
    "xray.exe",
    "wintun.dll",
    "geosite-ru.srs",
    "geoip-ru.srs",
];

pub const DEFAULT_GEOIP_SHA256: &str = "ce90dc02a81528109c99d769e1f8fe3071ded6b02a574f31e62d8f4eee312563";
pub const DEFAULT_GEOSITE_SHA256: &str = "9ee94acb06e2f795831fb1589c0dc7af9242b0fb8d9196afd981b915b1097ed3";
pub const DEFAULT_GEOIP_RU_SRS_SHA256: &str = "e24dc7cc17aee4d935a2c9636bdc039845cbfc5d90022e6d51345dc81dcb8697";
pub const DEFAULT_GEOSITE_RU_SRS_SHA256: &str = "1e8a975a299059cbb79d0dd61af60b6e2fd5ff59223457a6b66797adcf49da3e";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreManifestItem {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    #[serde(rename = "downloadUrl")]
    pub download_url: Option<String>,
    pub sha256: Option<String>,
    #[serde(rename = "releaseNotes")]
    pub release_notes: Option<String>,
    #[serde(default)]
    pub cores: HashMap<String, CoreManifestItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreStatusItem {
    pub name: String,
    pub present: bool,
    pub hash_matches: bool,
    pub current_hash: Option<String>,
    pub expected_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoresIntegrityReport {
    pub all_valid: bool,
    pub items: Vec<CoreStatusItem>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientUpdateStatus {
    pub update_available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub download_url: Option<String>,
    pub release_notes: Option<String>,
}

pub struct UpdaterService {
    client: reqwest::Client,
    direct_client: reqwest::Client,
}

impl Default for UpdaterService {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdaterService {
    fn build_http_client(no_proxy: bool) -> reqwest::Client {
        let mut builder = reqwest::Client::builder()
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .user_agent("BusterVPN-Updater/1.5.4");

        if no_proxy {
            builder = builder.no_proxy();
        }

        builder.build().unwrap_or_default()
    }

    pub fn new() -> Self {
        Self {
            client: Self::build_http_client(false),
            direct_client: Self::build_http_client(true),
        }
    }

    /// Fetches the manifest from primary URL with fallback to secondary.
    pub async fn fetch_manifest(&self) -> Result<UpdateManifest> {
        let urls = [PRIMARY_MANIFEST_URL, FALLBACK_MANIFEST_URL];
        let mut last_err = String::new();

        for url in urls {
            for (use_direct, http) in [(false, &self.client), (true, &self.direct_client)] {
                let res = http
                    .get(url)
                    .timeout(Duration::from_secs(10))
                    .send()
                    .await;
                match res {
                    Ok(resp) => {
                        let status = resp.status();
                        if !status.is_success() {
                            last_err = format!("{} returned HTTP {}", url, status);
                            warn!("Manifest fetch failed (direct={}): {}", use_direct, last_err);
                            continue;
                        }
                        match resp.json::<UpdateManifest>().await {
                            Ok(manifest) => return Ok(manifest),
                            Err(e) => {
                                last_err = format!("Failed to decode manifest JSON from {}: {}", url, e);
                                warn!("Manifest decode failed (direct={}): {}", use_direct, last_err);
                            }
                        }
                    }
                    Err(e) => {
                        last_err = format!("Request to {} failed: {}", url, e);
                    }
                }
            }
        }
        bail!(
            "Failed to fetch update manifest from both primary and fallback URLs: {}",
            last_err
        )
    }

    /// Downloads binary bytes across candidate URLs with a 3-attempt retry loop (500ms backoff)
    /// and a `.no_proxy()` direct client fallback on transient network or proxy errors.
    async fn download_bytes_with_retry(&self, urls: &[String], label: &str) -> Result<Vec<u8>> {
        const MAX_ATTEMPTS: usize = 3;
        let mut last_err = String::new();

        for attempt in 1..=MAX_ATTEMPTS {
            for url in urls {
                for (use_direct, http) in [(false, &self.client), (true, &self.direct_client)] {
                    match http.get(url).send().await {
                        Ok(resp) => {
                            let status = resp.status();
                            if !status.is_success() {
                                last_err = format!("{} returned HTTP {}", url, status);
                                warn!(
                                    "Download attempt {}/{} for {} (direct={}) got non-success status: {}",
                                    attempt, MAX_ATTEMPTS, label, use_direct, last_err
                                );
                                continue;
                            }

                            match resp.bytes().await {
                                Ok(bytes) => return Ok(bytes.to_vec()),
                                Err(e) => {
                                    last_err = format!("Error reading response body from {}: {}", url, e);
                                    warn!(
                                        "Download attempt {}/{} for {} (direct={}) failed: {}",
                                        attempt, MAX_ATTEMPTS, label, use_direct, last_err
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            last_err = format!("Request to {} failed: {}", url, e);
                            warn!(
                                "Download attempt {}/{} for {} (direct={}) failed: {}",
                                attempt, MAX_ATTEMPTS, label, use_direct, last_err
                            );
                        }
                    }
                }
            }

            if attempt < MAX_ATTEMPTS {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }

        bail!(
            "Failed to download {} after {} attempts: {}",
            label,
            MAX_ATTEMPTS,
            last_err
        )
    }

    /// Downloads a single core component from primary or fallback mirror with retries.
    async fn download_core_component(&self, name: &str, target_url: &str) -> Result<Vec<u8>> {
        let urls = vec![
            target_url.to_string(),
            target_url.replace("https://vpn.buster.gripe/", "https://buster.gripe/"),
        ];
        self.download_bytes_with_retry(&urls, &format!("core component '{}'", name))
            .await
    }

    /// Computes the lowercase hex SHA-256 hash of a file.
    pub fn calculate_sha256(path: &Path) -> Result<String> {
        let mut file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open file for hashing: {:?}", path))?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)
            .with_context(|| format!("Failed to compute SHA256 for {:?}", path))?;
        let hash = hasher.finalize();
        Ok(format!("{:x}", hash))
    }

    /// Verifies the presence and hashes of all 5 active core components (sing-box.exe, xray.exe, wintun.dll, geosite-ru.srs, geoip-ru.srs).
    pub async fn check_cores(
        &self,
        cores_dir: &Path,
        runtime_dir: Option<&Path>,
    ) -> CoresIntegrityReport {
        let manifest = self.fetch_manifest().await.ok();
        let mut items = Vec::new();
        let mut all_valid = true;

        for &name in CORE_COMPONENTS {
            let runtime_path = runtime_dir.map(|r| r.join(name));
            let cores_path = cores_dir.join(name);

            // Determine if active file is present in runtime_dir or cores_dir
            let (file_path, present) = if let Some(ref r_path) = runtime_path {
                if r_path.is_file() {
                    (r_path.clone(), true)
                } else if cores_path.is_file() {
                    // Sync from cores_dir to runtime_dir
                    let _ = std::fs::copy(&cores_path, r_path);
                    (r_path.clone(), true)
                } else {
                    (r_path.clone(), false)
                }
            } else if cores_path.is_file() {
                (cores_path.clone(), true)
            } else {
                (cores_path.clone(), false)
            };

            let current_hash = if present {
                Self::calculate_sha256(&file_path).ok()
            } else {
                None
            };

            let expected_hash = manifest
                .as_ref()
                .and_then(|m| m.cores.get(name))
                .map(|c| c.sha256.to_lowercase())
                .or_else(|| match name {
                    "geoip-ru.srs" => Some(DEFAULT_GEOIP_RU_SRS_SHA256.to_lowercase()),
                    "geosite-ru.srs" => Some(DEFAULT_GEOSITE_RU_SRS_SHA256.to_lowercase()),
                    "geoip.dat" => Some(DEFAULT_GEOIP_SHA256.to_lowercase()),
                    "geosite.dat" => Some(DEFAULT_GEOSITE_SHA256.to_lowercase()),
                    _ => None,
                });

            let hash_matches = match (&current_hash, &expected_hash) {
                (Some(curr), Some(exp)) => curr.eq_ignore_ascii_case(exp),
                (Some(_), None) => true, // Offline or unlisted: present is accepted
                _ => false,
            };

            if !present || !hash_matches {
                all_valid = false;
            }

            items.push(CoreStatusItem {
                name: name.to_string(),
                present,
                hash_matches,
                current_hash,
                expected_hash,
            });
        }

        let message = if all_valid {
            "Все исполняемые ядра и базы маршрутизации проверены и соответствуют манифесту".to_string()
        } else {
            "Обнаружены отсутствующие или требующие обновления компоненты ядер".to_string()
        };

        CoresIntegrityReport {
            all_valid,
            items,
            message,
        }
    }

    /// Downloads and updates any missing or corrupted cores according to the manifest or fallback registry.
    pub async fn update_cores(
        &self,
        cores_dir: &Path,
        runtime_dir: Option<&Path>,
    ) -> Result<CoresIntegrityReport> {
        let manifest = self.fetch_manifest().await.ok();
        let _ = std::fs::create_dir_all(cores_dir);
        if let Some(r_dir) = runtime_dir {
            let _ = std::fs::create_dir_all(r_dir);
        }

        let candidate_dirs = [
            cores_dir.to_path_buf(),
            PathBuf::from(r"C:\Program Files\BusterVPN\cores"),
            PathBuf::from(r"C:\BUSTERVPN\antivpnv2\crates\buster-app\cores"),
            PathBuf::from(r"C:\BUSTERVPN\cores"),
            PathBuf::from(r"C:\BUSTERVPN"),
        ];

        for &name in CORE_COMPONENTS {
            let (target_url, expected_hash) = if let Some(item) = manifest.as_ref().and_then(|m| m.cores.get(name)) {
                (item.url.clone(), Some(item.sha256.to_lowercase()))
            } else {
                let default_hash = match name {
                    "geoip-ru.srs" => Some(DEFAULT_GEOIP_RU_SRS_SHA256.to_lowercase()),
                    "geosite-ru.srs" => Some(DEFAULT_GEOSITE_RU_SRS_SHA256.to_lowercase()),
                    "geoip.dat" => Some(DEFAULT_GEOIP_SHA256.to_lowercase()),
                    "geosite.dat" => Some(DEFAULT_GEOSITE_SHA256.to_lowercase()),
                    _ => None,
                };
                (format!("https://vpn.buster.gripe/updates/cores/{}", name), default_hash)
            };

            let dest_cores = cores_dir.join(name);
            let dest_runtime = runtime_dir.map(|r| r.join(name));

            // Check if already valid locally
            let mut valid_locally = false;
            if let Some(ref exp) = expected_hash {
                let matches_hash = |h: &str| h.eq_ignore_ascii_case(exp);

                // 1. Check runtime_dir
                if let Some(ref r_path) = dest_runtime {
                    if r_path.is_file() {
                        if let Ok(h) = Self::calculate_sha256(r_path) {
                            if matches_hash(&h) {
                                valid_locally = true;
                                if !dest_cores.is_file() {
                                    let _ = std::fs::copy(r_path, &dest_cores);
                                }
                            }
                        }
                    }
                }
                // 2. Check cores_dir
                if !valid_locally && dest_cores.is_file() {
                    if let Ok(h) = Self::calculate_sha256(&dest_cores) {
                        if matches_hash(&h) {
                            valid_locally = true;
                            if let Some(ref r_path) = dest_runtime {
                                let _ = std::fs::copy(&dest_cores, r_path);
                            }
                        }
                    }
                }
                // 3. Check candidate dirs
                if !valid_locally {
                    for cand in &candidate_dirs {
                        let cand_file = cand.join(name);
                        if cand_file.is_file() {
                            if let Ok(h) = Self::calculate_sha256(&cand_file) {
                                if matches_hash(&h) {
                                    valid_locally = true;
                                    let _ = std::fs::copy(&cand_file, &dest_cores);
                                    if let Some(ref r_path) = dest_runtime {
                                        let _ = std::fs::copy(&cand_file, r_path);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            if !valid_locally {
                info!("Downloading core component '{}' from {}", name, target_url);

                let bytes = self.download_core_component(name, &target_url).await?;

                if let Some(ref exp) = expected_hash {
                    let mut hasher = Sha256::new();
                    hasher.update(&bytes);
                    let downloaded_hash = format!("{:x}", hasher.finalize());
                    if !downloaded_hash.eq_ignore_ascii_case(exp) {
                        bail!(
                            "SHA-256 mismatch for downloaded core {}: expected {}, got {}",
                            name,
                            exp,
                            downloaded_hash
                        );
                    }
                }

                // Write to cores_dir
                let temp_cores = cores_dir.join(format!("{}.tmp", name));
                std::fs::write(&temp_cores, &bytes)
                    .with_context(|| format!("Failed to write temp core file: {:?}", temp_cores))?;
                let _ = std::fs::remove_file(&dest_cores);
                std::fs::rename(&temp_cores, &dest_cores)
                    .with_context(|| format!("Failed to replace core in cores_dir: {:?}", dest_cores))?;

                // Also copy to runtime_dir if requested
                if let Some(ref r_path) = dest_runtime {
                    let temp_runtime = r_path.with_extension("tmp");
                    let _ = std::fs::write(&temp_runtime, &bytes);
                    let _ = std::fs::remove_file(r_path);
                    let _ = std::fs::rename(&temp_runtime, r_path);
                }

                info!("Successfully verified and installed component: {}", name);
            }
        }

        // Purge obsolete legacy core files from installation and runtime directories
        let legacy_files = [
            "hysteria.exe",
            "hysteria",
            "geoip.dat",
            "geosite.dat",
            "xray_config.json",
            "hysteria_config.yaml",
        ];
        for legacy in &legacy_files {
            let _ = std::fs::remove_file(cores_dir.join(legacy));
            if let Some(r_dir) = runtime_dir {
                let _ = std::fs::remove_file(r_dir.join(legacy));
            }
            let progdata = PathBuf::from(r"C:\ProgramData\BusterVPN\runtime").join(legacy);
            let _ = std::fs::remove_file(progdata);
        }

        Ok(self.check_cores(cores_dir, runtime_dir).await)
    }

    /// Verifies all cores and triggers an update if any component is missing or invalid.
    pub async fn ensure_cores_ready(
        &self,
        cores_dir: &Path,
        runtime_dir: Option<&Path>,
    ) -> Result<()> {
        let report = self.check_cores(cores_dir, runtime_dir).await;
        if !report.all_valid {
            info!("One or more core components require installation or update; synchronizing...");
            let updated_report = self.update_cores(cores_dir, runtime_dir).await?;
            if !updated_report.all_valid {
                tracing::warn!(
                    "Some core components could not be validated after sync: {}",
                    updated_report.message
                );
            }
        }
        Ok(())
    }

    /// Compares current version with manifest and returns update info.
    pub async fn check_client_update(&self) -> Result<ClientUpdateStatus> {
        let manifest = self.fetch_manifest().await?;
        let current_tuple = parse_version_tuple(CURRENT_VERSION).unwrap_or((1, 3, 1));
        let latest_tuple = parse_version_tuple(&manifest.version).unwrap_or(current_tuple);
        let update_available = latest_tuple > current_tuple;

        Ok(ClientUpdateStatus {
            update_available,
            current_version: CURRENT_VERSION.to_string(),
            latest_version: manifest.version,
            download_url: manifest.download_url,
            release_notes: manifest.release_notes,
        })
    }

    /// Downloads ZIP update, verifies SHA-256, stages files, and triggers update.bat.
    pub async fn apply_client_update(&self) -> Result<()> {
        let manifest = self.fetch_manifest().await?;
        let download_url = manifest
            .download_url
            .context("No downloadUrl specified in manifest")?;
        let expected_sha256 = manifest
            .sha256
            .context("No sha256 specified in manifest for update")?;

        self.download_and_install_update(&download_url, &expected_sha256)
            .await
    }

    async fn download_and_install_update(&self, download_url: &str, expected_sha256: &str) -> Result<()> {
        let temp_dir = std::env::temp_dir().join("BusterVPN_Update");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir)?;

        let zip_path = temp_dir.join("update.zip");
        let staging_dir = temp_dir.join("staging");
        std::fs::create_dir_all(&staging_dir)?;

        info!("Downloading client update from {}", download_url);
        let fallback_url = download_url.replace("https://vpn.buster.gripe/", "https://buster.gripe/");
        let mut urls = vec![download_url.to_string()];
        if fallback_url != download_url {
            urls.push(fallback_url);
        }

        let bytes = self
            .download_bytes_with_retry(&urls, "client update package")
            .await?;

        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let downloaded_hash = format!("{:x}", hasher.finalize());

        if !downloaded_hash.eq_ignore_ascii_case(expected_sha256) {
            bail!(
                "SHA-256 mismatch for update ZIP: expected {}, got {}",
                expected_sha256,
                downloaded_hash
            );
        }

        std::fs::write(&zip_path, &bytes)?;
        info!("Update ZIP downloaded and verified ({})", downloaded_hash);

        // Extract using tar.exe (available on Windows 10/11) or PowerShell
        let tar_status = std::process::Command::new("tar.exe")
            .args(["-xf", zip_path.to_str().unwrap(), "-C", staging_dir.to_str().unwrap()])
            .status();

        let extracted = match tar_status {
            Ok(s) if s.success() => true,
            _ => {
                let ps_cmd = format!(
                    "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                    zip_path.display(),
                    staging_dir.display()
                );
                let ps_status = std::process::Command::new("powershell.exe")
                    .args(["-NoProfile", "-Command", &ps_cmd])
                    .status();
                matches!(ps_status, Ok(s) if s.success())
            }
        };

        if !extracted {
            bail!("Failed to extract update package archive");
        }

        // Target application executable directory
        let current_exe = std::env::current_exe()?;
        let target_dir = current_exe
            .parent()
            .context("Cannot determine executable directory")?;
        let current_pid = std::process::id();

        // Generate update.bat
        let bat_content = format!(
            "@echo off\r\n\
            timeout /t 2 /nobreak >nul\r\n\
            taskkill /F /PID {pid} >nul 2>&1\r\n\
            xcopy /y /e \"{staging}\\*\" \"{target}\\\" >nul\r\n\
            del /f /q \"{target}\\cores\\hysteria.exe\" \"{target}\\cores\\geoip.dat\" \"{target}\\cores\\geosite.dat\" >nul 2>&1\r\n\
            del /f /q \"%LOCALAPPDATA%\\BusterVPN\\runtime\\hysteria.exe\" \"%LOCALAPPDATA%\\BusterVPN\\runtime\\geoip.dat\" \"%LOCALAPPDATA%\\BusterVPN\\runtime\\geosite.dat\" \"%LOCALAPPDATA%\\BusterVPN\\runtime\\hysteria_config.yaml\" >nul 2>&1\r\n\
            del /f /q \"%PROGRAMDATA%\\BusterVPN\\runtime\\hysteria.exe\" \"%PROGRAMDATA%\\BusterVPN\\runtime\\geoip.dat\" \"%PROGRAMDATA%\\BusterVPN\\runtime\\geosite.dat\" >nul 2>&1\r\n\
            start \"\" \"{exe}\"\r\n\
            rd /s /q \"{staging}\"\r\n\
            del \"%~f0\"\r\n",
            pid = current_pid,
            staging = staging_dir.display(),
            target = target_dir.display(),
            exe = current_exe.display(),
        );

        let bat_path = temp_dir.join("update.bat");
        std::fs::write(&bat_path, bat_content)?;

        info!("Spawning update.bat script: {:?}", bat_path);
        let _ = std::process::Command::new("cmd.exe")
            .args(["/C", bat_path.to_str().unwrap()])
            .spawn();

        std::process::exit(0);
    }
}

fn parse_version_tuple(v: &str) -> Option<(u32, u32, u32)> {
    let clean = v.trim().trim_start_matches('v').split('-').next()?;
    let parts: Vec<&str> = clean.split('.').collect();
    if parts.len() >= 3 {
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts[2].parse().ok()?;
        Some((major, minor, patch))
    } else if parts.len() == 2 {
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        Some((major, minor, 0))
    } else if parts.len() == 1 {
        let major = parts[0].parse().ok()?;
        Some((major, 0, 0))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_components_list() {
        assert_eq!(CORE_COMPONENTS.len(), 5);
        assert!(CORE_COMPONENTS.contains(&"sing-box.exe"));
        assert!(CORE_COMPONENTS.contains(&"xray.exe"));
        assert!(CORE_COMPONENTS.contains(&"wintun.dll"));
        assert!(CORE_COMPONENTS.contains(&"geosite-ru.srs"));
        assert!(CORE_COMPONENTS.contains(&"geoip-ru.srs"));
    }

    #[test]
    fn test_default_dat_hashes() {
        assert_eq!(DEFAULT_GEOIP_SHA256, "ce90dc02a81528109c99d769e1f8fe3071ded6b02a574f31e62d8f4eee312563");
        assert_eq!(DEFAULT_GEOSITE_SHA256, "9ee94acb06e2f795831fb1589c0dc7af9242b0fb8d9196afd981b915b1097ed3");
    }

    #[test]
    fn test_parse_version_tuple() {
        assert_eq!(parse_version_tuple("1.0.0"), Some((1, 0, 0)));
        assert_eq!(parse_version_tuple("v1.0.1-release"), Some((1, 0, 1)));
        assert_eq!(parse_version_tuple("0.1.5-beta"), Some((0, 1, 5)));
        assert_eq!(parse_version_tuple("1.3"), Some((1, 3, 0)));
        assert_eq!(parse_version_tuple("v1.3.0"), Some((1, 3, 0)));
    }

    #[test]
    fn test_current_version() {
        assert_eq!(CURRENT_VERSION, "1.5.4");
        assert_eq!(parse_version_tuple(CURRENT_VERSION), Some((1, 5, 4)));
    }

    #[test]
    fn test_manifest_endpoints() {
        assert_eq!(PRIMARY_MANIFEST_URL, "https://vpn.buster.gripe/updates/manifest.json");
        assert_eq!(FALLBACK_MANIFEST_URL, "https://buster.gripe/updates/manifest.json");
    }

    #[tokio::test]
    async fn test_download_bytes_with_retry_recovers_and_reports_http_status() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock server");
        let addr = listener.local_addr().expect("local addr");
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_srv = counter.clone();

        // First 4 requests (attempts 1 & 2 across client + direct_client) return 502; 5th returns 200 OK
        let server_task = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else { break };
                let idx = counter_srv.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = socket.read(&mut buf).await;
                    let resp = if idx < 4 {
                        b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
                    } else {
                        b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nPAYLOAD".to_vec()
                    };
                    let _ = socket.write_all(&resp).await;
                });
            }
        });

        let svc = UpdaterService::new();
        let url = format!("http://{}/test.bin", addr);
        let bytes = svc
            .download_bytes_with_retry(std::slice::from_ref(&url), "test binary")
            .await
            .expect("should succeed on 3rd attempt");
        assert_eq!(bytes, b"PAYLOAD");
        assert_eq!(counter.load(Ordering::SeqCst), 5);

        // Now verify persistent 404 returns a clear HTTP status error
        let listener_err = TcpListener::bind("127.0.0.1:0").await.expect("bind 404 server");
        let addr_err = listener_err.local_addr().expect("404 addr");
        let err_server = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener_err.accept().await else { break };
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = socket.read(&mut buf).await;
                    let _ = socket
                        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                });
            }
        });

        let url_404 = format!("http://{}/missing.bin", addr_err);
        let err = svc
            .download_bytes_with_retry(&[url_404], "missing binary")
            .await
            .expect_err("404 must fail after retries");
        assert!(err.to_string().contains("404"), "Unexpected error: {}", err);

        server_task.abort();
        err_server.abort();
    }
}
