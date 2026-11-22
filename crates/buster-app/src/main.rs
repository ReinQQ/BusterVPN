#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use buster_app::commands::*;

fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting BusterVPN v{}", env!("CARGO_PKG_VERSION"));

    let app_data = dirs_app_data_path();
    let mut cores_dir = app_data.join("cores");
    let _ = std::fs::create_dir_all(&cores_dir);

    // If cores are not in AppData, check next to the executable
    let singbox_name = if cfg!(target_os = "windows") { "sing-box.exe" } else { "sing-box" };
    if !cores_dir.join(singbox_name).exists() {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let local_cores_linux = exe_dir.join("cores-linux");
                let local_cores = exe_dir.join("cores");
                if local_cores.join(singbox_name).exists() {
                    cores_dir = local_cores;
                } else if local_cores_linux.join(singbox_name).exists() {
                    cores_dir = local_cores_linux;
                }
            }
        }
    }

    let state = AppState::new(app_data, cores_dir);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .manage(state)
        .setup(|app| {
            let args: Vec<String> = std::env::args().collect();
            let is_minimized = args.iter().any(|arg| arg == "--minimized");

            if let Some(window) = app.get_webview_window("main") {
                if is_minimized {
                    let _ = window.hide();
                    tracing::info!("BusterVPN started minimized in system tray (--minimized)");
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }

            // Background bootstrap: verify and synchronize cores and routing dat files asynchronously
            let app_state = app.state::<AppState>();
            let updater = app_state.updater_service.clone();
            let cores_dir = app_state.cores_dir.clone();
            let runtime_dir = app_state.app_data_dir.join("runtime");
            tauri::async_runtime::spawn(async move {
                if let Err(e) = updater.ensure_cores_ready(&cores_dir, Some(&runtime_dir)).await {
                    tracing::warn!("Startup core verification and sync failed: {}", e);
                }
            });

            // Asynchronous non-blocking background check for updater & ip_info
            let updater_check = app_state.updater_service.clone();
            let ip_info_service = app_state.ip_info_service.clone();
            tauri::async_runtime::spawn(async move {
                let _ = buster_app::services::session_coordinator::SessionCoordinator::ensure_service_running().await;
                let _ = updater_check.check_client_update().await;
                let _ = ip_info_service.fetch_ip_info(false).await;
            });

            // Sync elevated autostart task if user previously enabled start_with_windows
            let settings_store = app_state.settings_store.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(settings) = settings_store.load().await {
                    #[cfg(target_os = "windows")]
                    {
                        if settings.start_with_windows && !buster_windows::autostart::is_autostart_enabled() {
                            let _ = buster_windows::autostart::enable_autostart(None);
                        }
                    }
                    #[cfg(target_os = "linux")]
                    {
                        if settings.start_with_windows && !buster_app::platform::is_autostart_enabled() {
                            let _ = buster_app::platform::configure_autostart(true);
                        }
                    }
                }
            });

            // Build Initial Minimal Tray Menu immediately without blocking on DPAPI/disk I/O
            let menu = buster_app::tray::build_tray_menu(
                app.handle(),
                false,
                None,
                &[],
            )?;

            // Populate tray menu asynchronously right after window is displayed
            let app_handle = app.handle().clone();
            let p_store = app_state.profile_store.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(profiles) = p_store.load().await {
                    if let Some(tray) = app_handle.tray_by_id(buster_app::tray::TRAY_ID) {
                        if let Ok(menu) = buster_app::tray::build_tray_menu(
                            &app_handle,
                            false,
                            None,
                            &profiles,
                        ) {
                            let _ = tray.set_menu(Some(menu));
                        }
                    }
                }
            });

            #[cfg(target_os = "windows")]
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.ico"))
                .unwrap_or_else(|_| app.default_window_icon().unwrap().clone());
            #[cfg(not(target_os = "windows"))]
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/128x128.png"))
                .unwrap_or_else(|_| app.default_window_icon().unwrap().clone());

            let _tray = TrayIconBuilder::with_id(buster_app::tray::TRAY_ID)
                .icon(tray_icon)
                .tooltip("BusterVPN Client")
                .menu(&menu)
                .on_menu_event(|app, event| {
                    let event_id = event.id.as_ref().to_string();
                    match event_id.as_str() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        "toggle_connect" => {
                            let app_handle = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let state = app_handle.state::<AppState>();
                                let is_connected = state.coordinator.get_status().await == buster_core::models::ConnectionStatus::Connected;
                                if is_connected {
                                    let _ = disconnect_tunnel_core(&app_handle, &state).await;
                                } else {
                                    let _ = connect_tunnel_core(&app_handle, &state, None).await;
                                }
                            });
                        }
                        id if id.starts_with("select_profile_") => {
                            let profile_uuid = id.trim_start_matches("select_profile_").to_string();
                            let app_handle = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let state = app_handle.state::<AppState>();
                                let _ = connect_tunnel_core(&app_handle, &state, Some(profile_uuid)).await;
                            });
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Minimize to tray instead of closing
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            connect_tunnel,
            disconnect_tunnel,
            get_profiles,
            save_profile,
            delete_profile,
            import_share_link,
            get_subscriptions,
            add_subscription,
            update_subscription,
            delete_subscription,
            get_settings,
            save_settings,
            ping_profile,
            ping_all_profiles,
            get_ip_info,
            fetch_ip_info,
            get_recent_logs,
            check_cores,
            update_cores,
            check_update,
            apply_update,
            pick_exe_file,
            import_content,
            update_tray_menu,
            check_dns_leak,
            check_webrtc_leak,
            fetch_diagnostics,
            get_service_status,
            manage_service,
            set_service_auto_connect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running BusterVPN application");
}

fn dirs_app_data_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(local).join("BusterVPN")
        } else {
            PathBuf::from(".bustervpn")
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".config").join("bustervpn")
        } else {
            PathBuf::from(".bustervpn")
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        PathBuf::from(".bustervpn")
    }
}
