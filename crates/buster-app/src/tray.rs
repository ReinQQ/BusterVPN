//! Dynamic system tray context menu builder and event router.

use buster_core::models::ServerProfile;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    AppHandle, Runtime,
};

pub const TRAY_ID: &str = "main_tray";

/// Builds the enhanced dynamic tray menu:
/// 1. Header (disabled): "BusterVPN — Подключено ({Active Server})" or "BusterVPN — Отключено"
/// 2. Separator
/// 3. Quick Action: "Отключить" or "Подключить"
/// 4. Submenu "Серверы": list of server profiles with checkmark on active
/// 5. Separator
/// 6. "Показать главное окно"
/// 7. "Выход"
pub fn build_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
    is_connected: bool,
    active_profile_name: Option<&str>,
    profiles: &[ServerProfile],
) -> Result<Menu<R>, tauri::Error> {
    let header_text = if is_connected {
        let name = active_profile_name.unwrap_or("Сервер");
        format!("● BusterVPN — Подключено ({})", name)
    } else {
        "○ BusterVPN — Отключено".to_string()
    };

    let header_item = MenuItem::with_id(app, "header_info", &header_text, false, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;

    let toggle_text = if is_connected { "Отключить" } else { "Подключить" };
    let toggle_action = MenuItem::with_id(app, "toggle_connect", toggle_text, true, None::<&str>)?;

    // Build "Серверы" Submenu
    let mut server_items: Vec<Box<dyn tauri::menu::IsMenuItem<R>>> = Vec::new();
    if profiles.is_empty() {
        let empty_item = MenuItem::with_id(app, "no_servers", "(Нет серверов)", false, None::<&str>)?;
        server_items.push(Box::new(empty_item));
    } else {
        for profile in profiles {
            let is_active = active_profile_name.map_or(false, |n| n == profile.name);
            let item_id = format!("select_profile_{}", profile.id);
            let label = if is_active {
                format!("✓ {}", profile.name)
            } else {
                profile.name.clone()
            };
            let check_item = CheckMenuItem::with_id(
                app,
                &item_id,
                &label,
                true,
                is_active,
                None::<&str>,
            )?;
            server_items.push(Box::new(check_item));
        }
    }

    let servers_refs: Vec<&dyn tauri::menu::IsMenuItem<R>> =
        server_items.iter().map(|b| b.as_ref()).collect();
    let servers_submenu = Submenu::with_items(app, "Серверы", true, &servers_refs)?;

    let sep2 = PredefinedMenuItem::separator(app)?;
    let show_item = MenuItem::with_id(app, "show", "Показать главное окно", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;

    Menu::with_items(
        app,
        &[
            &header_item,
            &sep1,
            &toggle_action,
            &servers_submenu,
            &sep2,
            &show_item,
            &quit_item,
        ],
    )
}
