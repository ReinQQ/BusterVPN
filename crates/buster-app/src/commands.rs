//! Tauri IPC command handlers and AppState for frontend UI interaction.

use std::collections::HashMap;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;
use buster_core::models::{ConnectionStatus, ServerProfile};
use buster_core::parsing::parse_share_link;
#[cfg(target_os = "windows")]
use buster_windows::LogLine;
#[cfg(not(target_os = "windows"))]
use crate::platform::LogLine;

use crate::models::{AppSettings, SubscriptionRecord};
use crate::services::{
    ClientUpdateStatus, CoresIntegrityReport, IpInfo, IpInfoService, LatencyService,
    SessionCoordinator, SubscriptionService, UpdaterService,
};
use crate::storage::{ProfileStore, SettingsStore, SubscriptionStore};

#[derive(Clone)]
pub struct AppState {
    pub coordinator: Arc<SessionCoordinator>,
    pub profile_store: Arc<ProfileStore>,
    pub subscription_store: Arc<SubscriptionStore>,
    pub settings_store: Arc<SettingsStore>,
    pub subscription_service: Arc<SubscriptionService>,
    pub latency_service: Arc<LatencyService>,
    pub ip_info_service: Arc<IpInfoService>,
    pub updater_service: Arc<UpdaterService>,
    pub cores_dir: PathBuf,
    pub app_data_dir: PathBuf,
}

impl AppState {
    pub fn new(app_data_dir: PathBuf, cores_dir: PathBuf) -> Self {
        let coordinator = Arc::new(SessionCoordinator::new(app_data_dir.clone()));
        coordinator.startup_recovery();

        Self {
            coordinator,
            profile_store: Arc::new(ProfileStore::new(app_data_dir.join("profiles.dat"))),
            subscription_store: Arc::new(SubscriptionStore::new(app_data_dir.join("subscriptions.dat"))),
            settings_store: Arc::new(SettingsStore::new(app_data_dir.join("settings.json"))),
            subscription_service: Arc::new(SubscriptionService::new()),
            latency_service: Arc::new(LatencyService::new(Duration::from_millis(1500))),
            ip_info_service: Arc::new(IpInfoService::new()),
            updater_service: Arc::new(UpdaterService::new()),
            cores_dir,
            app_data_dir,
        }
    }
}

// ==========================================
// IPC Command Functions (Tauri v2)
// ==========================================

#[tauri::command]
pub async fn get_status(state: tauri::State<'_, AppState>) -> Result<ConnectionStatus, String> {
    Ok(state.coordinator.get_status().await)
}

pub async fn sync_tray_state(app: &tauri::AppHandle, state: &AppState) {
    let is_connected = state.coordinator.get_status().await == ConnectionStatus::Connected;
    let active_name = state.coordinator.get_active_profile_name().await;
    let all_profiles = state.profile_store.load().await.unwrap_or_default();
    if let Some(tray) = app.tray_by_id(crate::tray::TRAY_ID) {
        if let Ok(new_menu) = crate::tray::build_tray_menu(
            app,
            is_connected,
            active_name.as_deref(),
            &all_profiles,
        ) {
            let _ = tray.set_menu(Some(new_menu));
        }
    }
}

pub async fn connect_tunnel_core(
    app: &tauri::AppHandle,
    state: &AppState,
    profile_id: Option<String>,
) -> Result<(), String> {
    let profiles = state.profile_store.load().await.map_err(|e| e.to_string())?;
    if profiles.is_empty() {
        return Err("No server profiles available. Please add or import a profile first.".to_string());
    }

    let settings = state.settings_store.load().await.map_err(|e| e.to_string())?;

    let profile = if let Some(id_str) = profile_id.as_deref().filter(|s| !s.trim().is_empty()) {
        let id = Uuid::parse_str(id_str).map_err(|e| format!("Invalid profile UUID: {}", e))?;
        profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| format!("Server profile '{}' not found", id_str))?
    } else {
        // Auto-select: first favorite, or settings.last_profile_id, or first available
        if let Some(fav) = profiles.iter().find(|p| p.is_favorite) {
            fav.clone()
        } else if let Some(last_id) = settings.last_profile_id {
            if let Some(last_prof) = profiles.iter().find(|p| p.id == last_id) {
                last_prof.clone()
            } else {
                profiles[0].clone()
            }
        } else {
            profiles[0].clone()
        }
    };

    // If connected, cleanly disconnect first before establishing new tunnel
    if state.coordinator.get_status().await != ConnectionStatus::Disconnected {
        let _ = state.coordinator.disconnect().await;
    }

    state
        .coordinator
        .connect(&profile, &settings, &state.cores_dir)
        .await
        .map_err(|e| e.to_string())?;

    // Remember last used profile
    let mut updated_settings = settings;
    updated_settings.last_profile_id = Some(profile.id);
    let _ = state.settings_store.save(&updated_settings).await;

    sync_tray_state(app, state).await;
    use tauri::Emitter;
    let _ = app.emit("vpn-status-changed", ());

    Ok(())
}

pub async fn disconnect_tunnel_core(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<(), String> {
    state.coordinator.disconnect().await.map_err(|e| e.to_string())?;
    sync_tray_state(app, state).await;
    use tauri::Emitter;
    let _ = app.emit("vpn-status-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn connect_tunnel(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    profile_id: Option<String>,
) -> Result<(), String> {
    connect_tunnel_core(&app, &state, profile_id).await
}

#[tauri::command]
pub async fn disconnect_tunnel(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    disconnect_tunnel_core(&app, &state).await
}

#[tauri::command]
pub async fn get_profiles(state: tauri::State<'_, AppState>) -> Result<Vec<ServerProfile>, String> {
    state.profile_store.load().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_profile(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    profile: ServerProfile,
) -> Result<(), String> {
    let mut profiles = state.profile_store.load().await.map_err(|e| e.to_string())?;
    if let Some(pos) = profiles.iter().position(|p| p.id == profile.id) {
        profiles[pos] = profile;
    } else {
        profiles.push(profile);
    }
    state.profile_store.save(&profiles).await.map_err(|e| e.to_string())?;
    sync_tray_state(&app, &state).await;
    Ok(())
}

#[tauri::command]
pub async fn delete_profile(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    profile_id: Option<String>,
    #[allow(non_snake_case)]
    profileId: Option<String>,
) -> Result<(), String> {
    let raw_id = profile_id
        .or(profileId)
        .ok_or_else(|| "Missing required parameter 'profile_id' / 'profileId'".to_string())?;
    let id = Uuid::parse_str(&raw_id).map_err(|e| format!("Invalid profile UUID: {}", e))?;
    let mut profiles = state.profile_store.load().await.map_err(|e| e.to_string())?;
    profiles.retain(|p| p.id != id);
    state.profile_store.save(&profiles).await.map_err(|e| e.to_string())?;
    sync_tray_state(&app, &state).await;
    Ok(())
}

#[tauri::command]
pub async fn import_share_link(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    link: String,
) -> Result<ServerProfile, String> {
    let parsed = parse_share_link(&link).map_err(|e| e.to_string())?;
    let profile = parsed.to_profile(Some(link));

    save_profile(app, state, profile.clone()).await?;
    Ok(profile)
}

#[tauri::command]
pub async fn get_subscriptions(state: tauri::State<'_, AppState>) -> Result<Vec<SubscriptionRecord>, String> {
    state.subscription_store.load().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_subscription(
    state: tauri::State<'_, AppState>,
    url: String,
    title: String,
) -> Result<SubscriptionRecord, String> {
    let doc = state.subscription_service.fetch(&url).await.map_err(|e| e.to_string())?;

    let mut record = SubscriptionRecord::new(url, title);
    record.node_count = doc.profiles.len();
    record.user_info = doc.user_info;

    // Save record
    let mut records = state.subscription_store.load().await.map_err(|e| e.to_string())?;
    records.push(record.clone());
    state.subscription_store.save(&records).await.map_err(|e| e.to_string())?;

    // Append imported server profiles
    let mut profiles = state.profile_store.load().await.map_err(|e| e.to_string())?;
    for mut profile in doc.profiles {
        profile.subscription_id = Some(record.id);
        profiles.push(profile);
    }
    state.profile_store.save(&profiles).await.map_err(|e| e.to_string())?;

    Ok(record)
}

#[tauri::command]
pub async fn update_subscription(
    state: tauri::State<'_, AppState>,
    sub_id: Option<String>,
    url: Option<String>,
    #[allow(non_snake_case)]
    subId: Option<String>,
) -> Result<SubscriptionRecord, String> {
    let mut records = state.subscription_store.load().await.map_err(|e| e.to_string())?;

    let target_idx = if let Some(ref u) = url.as_deref().filter(|s| !s.trim().is_empty()) {
        records.iter().position(|r| r.url == *u)
    } else if let Some(ref id_str) = sub_id.or(subId).as_deref().filter(|s| !s.trim().is_empty()) {
        if let Ok(id) = Uuid::parse_str(id_str) {
            records.iter().position(|r| r.id == id)
        } else {
            records.iter().position(|r| r.url == *id_str)
        }
    } else if !records.is_empty() {
        Some(0)
    } else {
        None
    };

    let (id, target_url) = if let Some(idx) = target_idx {
        (records[idx].id, records[idx].url.clone())
    } else if let Some(u) = url {
        let rec = SubscriptionRecord::new(u.clone(), "Subscription".to_string());
        let id = rec.id;
        records.push(rec);
        (id, u)
    } else {
        return Err("No subscription found to update".to_string());
    };

    let doc = state.subscription_service.fetch(&target_url).await.map_err(|e| e.to_string())?;

    let record = records
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| "Subscription record not found".to_string())?;

    record.node_count = doc.profiles.len();
    record.user_info = doc.user_info;
    record.updated_at_utc = chrono::Utc::now();
    if let Some(ref t) = doc.title {
        record.title = t.clone();
    }
    let updated_record = record.clone();

    // Persist subscriptions
    state.subscription_store.save(&records).await.map_err(|e| e.to_string())?;

    // Replace profiles belonging to this subscription
    let mut profiles = state.profile_store.load().await.map_err(|e| e.to_string())?;
    profiles.retain(|p| p.subscription_id != Some(id));
    for mut p in doc.profiles {
        p.subscription_id = Some(id);
        profiles.push(p);
    }
    state.profile_store.save(&profiles).await.map_err(|e| e.to_string())?;

    Ok(updated_record)
}

#[tauri::command]
pub async fn delete_subscription(
    state: tauri::State<'_, AppState>,
    sub_id: Option<String>,
    #[allow(non_snake_case)]
    subId: Option<String>,
) -> Result<(), String> {
    let raw_id = sub_id.or(subId).ok_or("Missing sub_id")?;
    let mut records = state.subscription_store.load().await.map_err(|e| e.to_string())?;

    let removed_id = if let Ok(id) = Uuid::parse_str(&raw_id) {
        records.retain(|r| r.id != id);
        Some(id)
    } else {
        let matching_rec = records.iter().find(|r| r.url == raw_id).cloned();
        records.retain(|r| r.url != raw_id);
        matching_rec.map(|r| r.id)
    };

    state.subscription_store.save(&records).await.map_err(|e| e.to_string())?;

    // Remove profiles belonging to this subscription
    if let Some(id) = removed_id {
        let mut profiles = state.profile_store.load().await.map_err(|e| e.to_string())?;
        profiles.retain(|p| p.subscription_id != Some(id));
        state.profile_store.save(&profiles).await.map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, AppState>) -> Result<AppSettings, String> {
    let mut settings = state.settings_store.load().await.map_err(|e| e.to_string())?;
    if crate::platform::is_autostart_enabled() {
        settings.start_with_windows = true;
    }
    Ok(settings)
}

#[tauri::command]
pub async fn save_settings(state: tauri::State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    if let Err(e) = crate::platform::configure_autostart(settings.start_with_windows) {
        tracing::warn!("Failed to configure autostart: {}", e);
    }
    state.settings_store.save(&settings).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ping_profile(state: tauri::State<'_, AppState>, profile_id: String) -> Result<u64, String> {
    let id = Uuid::parse_str(&profile_id).map_err(|e| format!("Invalid profile UUID: {}", e))?;
    let mut profiles = state.profile_store.load().await.map_err(|e| e.to_string())?;

    let profile = profiles
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("Server profile '{}' not found", profile_id))?;

    let host = profile.host.clone().unwrap_or_default();
    let port = profile.port;

    let latency = state.latency_service.ping_tcp(&host, port).await.map_err(|e| e.to_string())?;

    profile.last_latency_ms = Some(latency as i64);
    profile.last_latency_at_utc = Some(chrono::Utc::now());

    let _ = state.profile_store.save(&profiles).await;
    Ok(latency)
}

#[tauri::command]
pub async fn ping_all_profiles(state: tauri::State<'_, AppState>) -> Result<HashMap<String, u64>, String> {
    let mut profiles = state.profile_store.load().await.map_err(|e| e.to_string())?;
    let mut results = HashMap::new();

    for profile in profiles.iter_mut() {
        let host = profile.host.clone().unwrap_or_default();
        let port = profile.port;
        let id_str = profile.id.to_string();

        if let Ok(latency) = state.latency_service.ping_tcp(&host, port).await {
            profile.last_latency_ms = Some(latency as i64);
            profile.last_latency_at_utc = Some(chrono::Utc::now());
            results.insert(id_str, latency);
        } else {
            profile.last_latency_ms = Some(-1); // Timeout indicator
        }
    }

    let _ = state.profile_store.save(&profiles).await;
    Ok(results)
}

#[tauri::command]
pub async fn check_cores(state: tauri::State<'_, AppState>) -> Result<CoresIntegrityReport, String> {
    let runtime_dir = state.app_data_dir.join("runtime");
    Ok(state.updater_service.check_cores(&state.cores_dir, Some(&runtime_dir)).await)
}

#[tauri::command]
pub async fn update_cores(state: tauri::State<'_, AppState>) -> Result<CoresIntegrityReport, String> {
    let runtime_dir = state.app_data_dir.join("runtime");
    state.updater_service.update_cores(&state.cores_dir, Some(&runtime_dir)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_update(state: tauri::State<'_, AppState>) -> Result<ClientUpdateStatus, String> {
    state.updater_service.check_client_update().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_update(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.updater_service.apply_client_update().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn fetch_ip_info(state: tauri::State<'_, AppState>) -> Result<IpInfo, String> {
    let is_connected = state.coordinator.get_status().await == ConnectionStatus::Connected;
    state.ip_info_service.fetch_ip_info(is_connected).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_ip_info(state: tauri::State<'_, AppState>) -> Result<IpInfo, String> {
    fetch_ip_info(state).await
}

#[tauri::command]
pub async fn get_recent_logs(state: tauri::State<'_, AppState>) -> Result<Vec<LogLine>, String> {
    Ok(state.coordinator.get_logs().await)
}

#[tauri::command]
pub async fn pick_exe_file() -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(|| {
            let ps_script = r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Filter = 'Executables (*.exe)|*.exe|All Files (*.*)|*.*'
$dialog.Title = 'Select Application (.exe)'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    [System.IO.Path]::GetFileName($dialog.FileName)
}
"#;
            let output = std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .output()
                .map_err(|e| format!("Failed to open file dialog: {}", e))?;

            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if result.is_empty() {
                Ok(None)
            } else {
                Ok(Some(result))
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(|| {
            if let Ok(output) = std::process::Command::new("zenity")
                .args(["--file-selection", "--title=Select Application"])
                .output()
            {
                if output.status.success() {
                    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if let Some(file_name) = std::path::Path::new(&path_str).file_name() {
                        return Ok(Some(file_name.to_string_lossy().to_string()));
                    }
                }
            }
            if let Ok(output) = std::process::Command::new("kdialog")
                .args(["--getopenfilename", ".", "*|All Files"])
                .output()
            {
                if output.status.success() {
                    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if let Some(file_name) = std::path::Path::new(&path_str).file_name() {
                        return Ok(Some(file_name.to_string_lossy().to_string()));
                    }
                }
            }
            Ok(None)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Ok(None)
    }
}

#[tauri::command]
pub async fn import_content(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    content: String,
) -> Result<usize, String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let _ = add_subscription(state, trimmed.to_string(), "Subscription".to_string()).await?;
        return Ok(1);
    }

    let parsed_profiles = state.subscription_service.parse_text_content(trimmed, None);
    if parsed_profiles.is_empty() {
        // Fallback to parse_share_link
        let parsed = buster_core::parsing::parse_share_link(trimmed).map_err(|e| e.to_string())?;
        let profile = parsed.to_profile(Some(trimmed.to_string()));
        save_profile(app.clone(), state, profile).await?;
        return Ok(1);
    }

    let count = parsed_profiles.len();
    let mut profiles = state.profile_store.load().await.map_err(|e| e.to_string())?;
    for p in parsed_profiles {
        profiles.push(p);
    }
    state.profile_store.save(&profiles).await.map_err(|e| e.to_string())?;
    sync_tray_state(&app, &state).await;

    Ok(count)
}

#[tauri::command]
pub async fn update_tray_menu(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    status: Option<String>,
    #[allow(non_snake_case)]
    active_profile: Option<String>,
    #[allow(unused_variables)]
    profiles: Option<Vec<String>>,
) -> Result<(), String> {
    let conn_status = if let Some(s) = status {
        s.to_lowercase() == "connected"
    } else {
        state.coordinator.get_status().await == ConnectionStatus::Connected
    };

    let all_profiles = state.profile_store.load().await.unwrap_or_default();
    let act_name = active_profile.or_else(|| {
        all_profiles.iter().find(|p| p.is_favorite).map(|p| p.name.clone())
            .or_else(|| all_profiles.first().map(|p| p.name.clone()))
    });

    if let Some(tray) = app.tray_by_id(crate::tray::TRAY_ID) {
        if let Ok(new_menu) = crate::tray::build_tray_menu(
            &app,
            conn_status,
            act_name.as_deref(),
            &all_profiles,
        ) {
            let _ = tray.set_menu(Some(new_menu));
        }
    }

    Ok(())
}
