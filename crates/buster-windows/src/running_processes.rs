//! Windows running user process enumeration via Win32 ToolHelp32 API.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::error::WindowsError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunningProcessInfo {
    pub name: String,
    pub path: Option<String>,
}

const EXCLUDED_PROCESSES: &[&str] = &[
    "system",
    "system idle process",
    "registry",
    "memory compression",
    "secure system",
    "svchost.exe",
    "csrss.exe",
    "smss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "lsaiso.exe",
    "lsm.exe",
    "dwm.exe",
    "fontdrvhost.exe",
    "spoolsv.exe",
    "conhost.exe",
    "dllhost.exe",
    "sihost.exe",
    "taskhostw.exe",
    "ctfmon.exe",
    "runtimebroker.exe",
    "searchindexer.exe",
    "searchprotocolhost.exe",
    "searchfilterhost.exe",
    "wmiprvse.exe",
    "audiodg.exe",
    "dashost.exe",
    "securityhealthservice.exe",
    "msmpeng.exe",
    "nissrv.exe",
    "tiworker.exe",
    "trustedinstaller.exe",
    "wuauclt.exe",
    "usoclient.exe",
    "mousocoreworker.exe",
    "wudfhost.exe",
    "smartscreen.exe",
    "applicationframehost.exe",
    "shellexperiencehost.exe",
    "startmenuexperiencehost.exe",
    "textinputhost.exe",
    "lockapp.exe",
    "backgroundtaskhost.exe",
    "compkgsrv.exe",
    "aggregatorhost.exe",
    "widgetservice.exe",
    "widgets.exe",
    "phoneexperiencehost.exe",
    "crashpad_handler.exe",
    "msedgewebview2.exe",
    "werfault.exe",
    "buster-app.exe",
    "buster-service.exe",
    "busterctl.exe",
    "sing-box.exe",
    "xray.exe",
];

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() && self.0 != HANDLE(std::ptr::null_mut()) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

fn query_process_image_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let _guard = OwnedHandle(handle);

    let mut buffer = vec![0u16; 1024];
    let mut size = buffer.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };
    if ok.is_ok() && size > 0 {
        let len = (size as usize).min(buffer.len());
        let path = String::from_utf16_lossy(&buffer[..len]);
        if !path.trim().is_empty() {
            return Some(path);
        }
    }
    None
}

fn is_excluded_process(name_lower: &str) -> bool {
    !name_lower.ends_with(".exe") || EXCLUDED_PROCESSES.contains(&name_lower)
}

/// Enumerates unique active user `.exe` processes on Windows, excluding core OS & BusterVPN binaries.
pub fn list_running_processes() -> Result<Vec<RunningProcessInfo>, WindowsError> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }?;

    if snapshot == INVALID_HANDLE_VALUE {
        return Err(WindowsError::Win32(windows::core::Error::from_win32()));
    }
    let _snapshot_guard = OwnedHandle(snapshot);

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };

    let mut by_name: HashMap<String, RunningProcessInfo> = HashMap::new();

    if unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok() {
        loop {
            let nul_pos = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let exe_name = String::from_utf16_lossy(&entry.szExeFile[..nul_pos])
                .trim()
                .to_string();

            if !exe_name.is_empty() {
                let key = exe_name.to_ascii_lowercase();
                if !is_excluded_process(&key) {
                    let needs_path = by_name
                        .get(&key)
                        .map(|existing| existing.path.is_none())
                        .unwrap_or(true);
                    if needs_path {
                        let path = query_process_image_path(entry.th32ProcessID);
                        by_name
                            .entry(key)
                            .and_modify(|existing| {
                                if existing.path.is_none() && path.is_some() {
                                    existing.path = path.clone();
                                }
                            })
                            .or_insert(RunningProcessInfo {
                                name: exe_name,
                                path,
                            });
                    }
                }
            }

            if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                break;
            }
        }
    }

    let mut list: Vec<RunningProcessInfo> = by_name.into_values().collect();
    list.sort_by_key(|a| a.name.to_ascii_lowercase());
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_running_processes_filters_and_sorts() {
        let procs = list_running_processes().expect("list_running_processes should succeed");
        for proc in &procs {
            let lower = proc.name.to_ascii_lowercase();
            assert!(lower.ends_with(".exe"), "Expected .exe name, got {}", proc.name);
            assert_ne!(lower, "svchost.exe");
            assert_ne!(lower, "csrss.exe");
            assert_ne!(lower, "buster-app.exe");
            assert_ne!(lower, "sing-box.exe");
        }
        for pair in procs.windows(2) {
            assert!(
                pair[0].name.to_ascii_lowercase() < pair[1].name.to_ascii_lowercase(),
                "Expected strictly sorted unique list: {} vs {}",
                pair[0].name,
                pair[1].name
            );
        }
    }
}
