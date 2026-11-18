//! Windows Autostart management using Task Scheduler (elevated) and Registry cleanup.
#![cfg(windows)]

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
use winreg::RegKey;
use crate::error::WindowsError;

const CREATE_NO_WINDOW: u32 = 0x08000000;
pub const TASK_NAME: &str = "BusterVPN";
const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Enables autostart with highest elevation on user logon via Windows Task Scheduler (`schtasks.exe`).
///
/// Command executed:
/// `schtasks /create /tn "BusterVPN" /tr "\"<EXE_PATH>\" --minimized" /sc onlogon /rl highest /f`
pub fn enable_autostart(custom_exe_path: Option<&Path>) -> Result<(), WindowsError> {
    let exe: PathBuf = match custom_exe_path {
        Some(p) => p.to_path_buf(),
        None => std::env::current_exe()?,
    };

    let tr_arg = format!(r#""{}" --minimized"#, exe.display());

    let output = Command::new("schtasks")
        .args([
            "/create",
            "/tn",
            TASK_NAME,
            "/tr",
            &tr_arg,
            "/sc",
            "onlogon",
            "/rl",
            "highest",
            "/f",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;

    // Clean any lingering non-elevated registry entries to prevent duplicate launches
    clean_registry_autostart();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(WindowsError::ProcessStart(format!(
            "Failed to register scheduled task '{}': {}",
            TASK_NAME,
            stderr.trim()
        )));
    }

    tracing::info!("Registered elevated Task Scheduler autostart for {:?}", exe);
    Ok(())
}

/// Disables autostart by removing the scheduled task and clearing any registry keys.
pub fn disable_autostart() -> Result<(), WindowsError> {
    let output = Command::new("schtasks")
        .args(["/delete", "/tn", TASK_NAME, "/f"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;

    clean_registry_autostart();

    if !output.status.success() {
        tracing::debug!(
            "schtasks /delete for '{}' returned code {:?} (task may already be deleted)",
            TASK_NAME,
            output.status.code()
        );
    } else {
        tracing::info!("Successfully removed Task Scheduler autostart for '{}'", TASK_NAME);
    }

    Ok(())
}

/// Checks whether the elevated scheduled task is currently registered in Windows Task Scheduler.
pub fn is_autostart_enabled() -> bool {
    match Command::new("schtasks")
        .args(["/query", "/tn", TASK_NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// Configures autostart according to the provided boolean preference.
pub fn configure_autostart(enabled: bool) -> Result<(), WindowsError> {
    if enabled {
        enable_autostart(None)
    } else {
        disable_autostart()
    }
}

/// Deletes any lingering HKCU\Software\Microsoft\Windows\CurrentVersion\Run\BusterVPN entry.
pub fn clean_registry_autostart() {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run_key) = hkcu.open_subkey_with_flags(RUN_KEY_PATH, KEY_SET_VALUE) {
        let _ = run_key.delete_value(TASK_NAME);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_registry_autostart_does_not_panic() {
        clean_registry_autostart();
    }

    #[test]
    fn test_autostart_command_formatting() {
        let dummy_path = Path::new(r"C:\Program Files\BusterVPN\BusterVPN.exe");
        let tr_arg = format!(r#""{}" --minimized"#, dummy_path.display());
        assert_eq!(tr_arg, r#""C:\Program Files\BusterVPN\BusterVPN.exe" --minimized"#);
    }
}
