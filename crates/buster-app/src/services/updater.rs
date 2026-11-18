//! Core engine integrity verification and dual-channel auto-updater.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

pub const PRIMARY_MANIFEST_URL: &str = "https://vpn.buster.gripe/updates/manifest.json";
pub const FALLBACK_MANIFEST_URL: &str = "https://buster.gripe/updates/manifest.json";
pub const CURRENT_VERSION: &str = "1.4.0";

#[cfg(target_os = "windows")]
pub const CORE_COMPONENTS: &[&str] = &[
    "xray.exe",
    "hysteria.exe",
    "wintun.dll",
    "geoip.dat",
    "geosite.dat",
];

#[cfg(not(target_os = "windows"))]
pub const CORE_COMPONENTS: &[&str] = &[
    "xray",
    "hysteria",
    "geoip.dat",
    "geosite.dat",
];

pub const DEFAULT_GEOIP_SHA256: &str = "ce90dc02a81528109c99d769e1f8fe3071ded6b02a574f31e62d8f4eee312563";
pub const DEFAULT_GEOSITE_SHA256: &str = "9ee94acb06e2f795831fb1589c0dc7af9242b0fb8d9196afd981b915b1097ed3";

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
}

impl Default for UpdaterService {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdaterService {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(6))
            .user_agent("BusterVPN-Updater/1.4.0")
            .build()
            .unwrap_or_default();

        Self { client }
    }

    /// Fetches the manifest from primary URL with fallback to secondary.
    pub async fn fetch_manifest(&self) -> Result<UpdateManifest> {
        let urls = [PRIMARY_MANIFEST_URL, FALLBACK_MANIFEST_URL];
        for url in urls {
            if let Ok(resp) = self.client.get(url).send().await {
                if resp.status().is_success() {
                    if let Ok(manifest) = resp.json::<UpdateManifest>().await {
                        return Ok(manifest);
                    }
                }
            }
        }
        bail!("Failed to fetch update manifest from both primary and fallback URLs")
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

    /// Verifies the presence and hashes of all 5 core components (xray.exe, hysteria.exe, wintun.dll, geoip.dat, geosite.dat).
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
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if name == "xray" || name == "hysteria" {
                            if let Ok(meta) = std::fs::metadata(r_path) {
                                let mut perms = meta.permissions();
                                perms.set_mode(0o755);
                                let _ = std::fs::set_permissions(r_path, perms);
                            }
                        }
                    }
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
                    "geoip.dat" => Some(DEFAULT_GEOIP_SHA256.to_lowercase()),
                    "geosite.dat" => Some(DEFAULT_GEOSITE_SHA256.to_lowercase()),
                    _ => None,
                });

            let hash_matches = match (&current_hash, &expected_hash) {
                (Some(curr), Some(exp)) => {
                    curr.eq_ignore_ascii_case(exp)
                        || (name == "geosite.dat" && curr.eq_ignore_ascii_case("7ea815b277e3a653f01ce32c6e0e33f1555c73b2abdce802ac27c27dd5fd6488"))
                        || (name == "geoip.dat" && curr.eq_ignore_ascii_case("d46c85ee9e5798806b0e706aeadbc50a95124cf18be1ce0126bf717b805db4e6"))
                }
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
            PathBuf::from(r"C:\BUSTERVPN\cores"),
            PathBuf::from(r"C:\BUSTERVPN"),
            PathBuf::from("/usr/lib/bustervpn/cores"),
            PathBuf::from("/opt/bustervpn/cores"),
        ];

        for &name in CORE_COMPONENTS {
            let (target_url, expected_hash) = if let Some(item) = manifest.as_ref().and_then(|m| m.cores.get(name)) {
                (item.url.clone(), Some(item.sha256.to_lowercase()))
            } else {
                let default_hash = match name {
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
                let matches_hash = |h: &str| {
                    h.eq_ignore_ascii_case(exp)
                        || (name == "geosite.dat" && h.eq_ignore_ascii_case("7ea815b277e3a653f01ce32c6e0e33f1555c73b2abdce802ac27c27dd5fd6488"))
                        || (name == "geoip.dat" && h.eq_ignore_ascii_case("d46c85ee9e5798806b0e706aeadbc50a95124cf18be1ce0126bf717b805db4e6"))
                };

                // 1. Check runtime_dir
                if let Some(ref r_path) = dest_runtime {
                    if r_path.is_file() {
                        let is_small_geosite = name == "geosite.dat" && r_path.metadata().map(|m| m.len() < 10 * 1024 * 1024).unwrap_or(false);
                        if !is_small_geosite {
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
                }
                // 2. Check cores_dir
                if !valid_locally && dest_cores.is_file() {
                    let is_small_geosite = name == "geosite.dat" && dest_cores.metadata().map(|m| m.len() < 10 * 1024 * 1024).unwrap_or(false);
                    if !is_small_geosite {
                        if let Ok(h) = Self::calculate_sha256(&dest_cores) {
                            if matches_hash(&h) {
                                valid_locally = true;
                                if let Some(ref r_path) = dest_runtime {
                                    let _ = std::fs::copy(&dest_cores, r_path);
                                }
                            }
                        }
                    }
                }
                // 3. Check candidate dirs
                if !valid_locally {
                    for cand in &candidate_dirs {
                        let cand_file = cand.join(name);
                        if cand_file.is_file() {
                            let is_small_geosite = name == "geosite.dat" && cand_file.metadata().map(|m| m.len() < 10 * 1024 * 1024).unwrap_or(false);
                            if is_small_geosite {
                                continue;
                            }
                            if let Ok(h) = Self::calculate_sha256(&cand_file) {
                                if matches_hash(&h) {
                                    valid_locally = true;
                                    let _ = std::fs::copy(&cand_file, &dest_cores);
                                    if let Some(ref r_path) = dest_runtime {
                                        let _ = std::fs::copy(&cand_file, r_path);
                                    }
                                    #[cfg(unix)]
                                    {
                                        use std::os::unix::fs::PermissionsExt;
                                        if name == "xray" || name == "hysteria" {
                                            if let Ok(meta) = std::fs::metadata(&dest_cores) {
                                                let mut perms = meta.permissions();
                                                perms.set_mode(0o755);
                                                let _ = std::fs::set_permissions(&dest_cores, perms);
                                            }
                                            if let Some(ref r_path) = dest_runtime {
                                                if let Ok(meta) = std::fs::metadata(r_path) {
                                                    let mut perms = meta.permissions();
                                                    perms.set_mode(0o755);
                                                    let _ = std::fs::set_permissions(r_path, perms);
                                                }
                                            }
                                        }
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

                let urls = [
                    target_url.clone(),
                    target_url.replace("https://vpn.buster.gripe/", "https://buster.gripe/"),
                ];

                let mut downloaded_bytes = None;
                for u in &urls {
                    if let Ok(resp) = self.client.get(u).send().await {
                        if resp.status().is_success() {
                            if let Ok(b) = resp.bytes().await {
                                downloaded_bytes = Some(b);
                                break;
                            }
                        }
                    }
                }

                let bytes = downloaded_bytes
                    .ok_or_else(|| anyhow::anyhow!("Failed to download component '{}' from all candidate URLs", name))?;

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

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if name == "xray" || name == "hysteria" {
                        if let Ok(meta) = std::fs::metadata(&dest_cores) {
                            let mut perms = meta.permissions();
                            perms.set_mode(0o755);
                            let _ = std::fs::set_permissions(&dest_cores, perms);
                        }
                        if let Some(ref r_path) = dest_runtime {
                            if let Ok(meta) = std::fs::metadata(r_path) {
                                let mut perms = meta.permissions();
                                perms.set_mode(0o755);
                                let _ = std::fs::set_permissions(r_path, perms);
                            }
                        }
                    }
                }

                info!("Successfully verified and installed component: {}", name);
            }
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

        let temp_dir = std::env::temp_dir().join("BusterVPN_Update");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir)?;

        let zip_path = temp_dir.join("update.zip");
        let staging_dir = temp_dir.join("staging");
        std::fs::create_dir_all(&staging_dir)?;

        info!("Downloading client update from {}", download_url);
        let resp = self.client.get(&download_url).send().await?;
        if !resp.status().is_success() {
            bail!("Failed to download update: HTTP {}", resp.status());
        }

        let bytes = resp.bytes().await?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let downloaded_hash = format!("{:x}", hasher.finalize());

        if !downloaded_hash.eq_ignore_ascii_case(&expected_sha256) {
            bail!(
                "SHA-256 mismatch for update ZIP: expected {}, got {}",
                expected_sha256,
                downloaded_hash
            );
        }

        std::fs::write(&zip_path, &bytes)?;
        info!("Update ZIP downloaded and verified ({})", downloaded_hash);

        #[cfg(target_os = "windows")]
        {
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

        #[cfg(not(target_os = "windows"))]
        {
            let tar_status = std::process::Command::new("tar")
                .args(["-xf", zip_path.to_str().unwrap(), "-C", staging_dir.to_str().unwrap()])
                .status();

            if !matches!(tar_status, Ok(s) if s.success()) {
                bail!("Failed to extract update package archive");
            }

            let current_exe = std::env::current_exe()?;
            let target_dir = current_exe
                .parent()
                .context("Cannot determine executable directory")?;
            let current_pid = std::process::id();

            let sh_content = format!(
                "#!/bin/sh\n\
                sleep 2\n\
                kill -9 {pid} 2>/dev/null || true\n\
                cp -rf \"{staging}/.\" \"{target}/\"\n\
                chmod +x \"{exe}\"\n\
                nohup \"{exe}\" >/dev/null 2>&1 &\n\
                rm -rf \"{staging}\"\n\
                rm -f \"$0\"\n",
                pid = current_pid,
                staging = staging_dir.display(),
                target = target_dir.display(),
                exe = current_exe.display(),
            );

            let sh_path = temp_dir.join("update.sh");
            std::fs::write(&sh_path, &sh_content)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o755);
                let _ = std::fs::set_permissions(&sh_path, perms);
            }

            info!("Spawning update.sh script: {:?}", sh_path);
            let _ = std::process::Command::new("sh")
                .arg(sh_path.to_str().unwrap())
                .spawn();

            std::process::exit(0);
        }
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
        #[cfg(target_os = "windows")]
        {
            assert_eq!(CORE_COMPONENTS.len(), 5);
            assert!(CORE_COMPONENTS.contains(&"xray.exe"));
            assert!(CORE_COMPONENTS.contains(&"hysteria.exe"));
            assert!(CORE_COMPONENTS.contains(&"wintun.dll"));
            assert!(CORE_COMPONENTS.contains(&"geoip.dat"));
            assert!(CORE_COMPONENTS.contains(&"geosite.dat"));
        }
        #[cfg(not(target_os = "windows"))]
        {
            assert_eq!(CORE_COMPONENTS.len(), 4);
            assert!(CORE_COMPONENTS.contains(&"xray"));
            assert!(CORE_COMPONENTS.contains(&"hysteria"));
            assert!(CORE_COMPONENTS.contains(&"geoip.dat"));
            assert!(CORE_COMPONENTS.contains(&"geosite.dat"));
        }
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
        assert_eq!(CURRENT_VERSION, "1.4.0");
        assert_eq!(parse_version_tuple(CURRENT_VERSION), Some((1, 4, 0)));
    }

    #[test]
    fn test_manifest_endpoints() {
        assert_eq!(PRIMARY_MANIFEST_URL, "https://vpn.buster.gripe/updates/manifest.json");
        assert_eq!(FALLBACK_MANIFEST_URL, "https://buster.gripe/updates/manifest.json");
    }
}
