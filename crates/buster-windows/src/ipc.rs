use std::path::Path;
use std::time::Duration;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::ClientOptions;

use buster_core::models::ConnectionStatus;
use crate::log::LogLine;

pub const SERVICE_NAME: &str = "BusterVPNService";
pub const SERVICE_DISPLAY_NAME: &str = "BusterVPN Background Service";
pub const PIPE_NAME: &str = r"\\.\pipe\BusterVPNService";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcRequest {
    pub id: u64,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponse {
    pub id: u64,
    pub result: Option<Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectTunnelParams {
    pub profile_name: String,
    pub core_kind: String, // "xray", "hysteria2", "sing-box"
    pub config_content: String,
    pub config_filename: String,
    pub core_args: Vec<String>,
    pub server_host: String,
    pub server_ips: Vec<String>,
    pub dns_servers: Vec<String>,
    pub kill_switch: bool,
    pub tun_enabled: bool,
    pub cores_dir: Option<String>,
    #[serde(default)]
    pub sidecar_config: Option<String>,
    #[serde(default)]
    pub sidecar_filename: Option<String>,
    #[serde(default)]
    pub sidecar_args: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatusReport {
    pub status: ConnectionStatus,
    pub active_profile: Option<String>,
    pub connected_at: Option<DateTime<Utc>>,
    pub core_pid: Option<u32>,
    pub tun_enabled: bool,
    pub kill_switch: bool,
    pub auto_connect_on_boot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceConfig {
    pub auto_connect_on_boot: bool,
    pub last_connect_params: Option<ConnectTunnelParams>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowsServiceState {
    Running,
    Stopped,
    NotInstalled,
    StartPending,
    StopPending,
    Other(u32),
}

/// Helper client communicating with BusterVPNService over Named Pipe.
pub struct BusterServiceClient;

impl BusterServiceClient {
    /// Tests if the named pipe can be opened.
    pub async fn is_service_available() -> bool {
        match ClientOptions::new().open(PIPE_NAME) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Sends an IPC request over the named pipe and awaits the response.
    pub async fn send_request(method: &str, params: Option<Value>) -> Result<Value> {
        let mut client_opt = None;
        let mut last_err = None;
        for _ in 0..15 {
            match ClientOptions::new().open(PIPE_NAME) {
                Ok(c) => {
                    client_opt = Some(c);
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
        let client = client_opt.ok_or_else(|| {
            anyhow::anyhow!("Failed to connect to BusterVPNService named pipe: {:?}", last_err)
        })?;

        let (reader, mut writer) = tokio::io::split(client);
        let mut buf_reader = BufReader::new(reader);

        let req = IpcRequest {
            id: 1,
            method: method.to_string(),
            params,
        };

        let mut req_str = serde_json::to_string(&req)?;
        req_str.push('\n');

        tokio::time::timeout(Duration::from_secs(4), writer.write_all(req_str.as_bytes()))
            .await
            .context("Timed out writing to named pipe")??;

        writer.flush().await?;

        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(12), buf_reader.read_line(&mut line))
            .await
            .context("Timed out waiting for response from BusterVPNService")??;

        if line.trim().is_empty() {
            bail!("Empty response from BusterVPNService");
        }

        let resp: IpcResponse = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse response: {}", line))?;

        if let Some(err) = resp.error {
            bail!("{}", err);
        }

        Ok(resp.result.unwrap_or(Value::Null))
    }

    pub async fn ping() -> Result<String> {
        let res = Self::send_request("Ping", None).await?;
        Ok(res.as_str().unwrap_or("Pong").to_string())
    }

    pub async fn get_status() -> Result<ServiceStatusReport> {
        let res = Self::send_request("GetStatus", None).await?;
        let report: ServiceStatusReport = serde_json::from_value(res)?;
        Ok(report)
    }

    pub async fn connect_tunnel(params: ConnectTunnelParams) -> Result<()> {
        let val = serde_json::to_value(params)?;
        Self::send_request("ConnectTunnel", Some(val)).await?;
        Ok(())
    }

    pub async fn disconnect_tunnel() -> Result<()> {
        Self::send_request("DisconnectTunnel", None).await?;
        Ok(())
    }

    pub async fn get_logs() -> Result<Vec<LogLine>> {
        let res = Self::send_request("GetLogs", None).await?;
        let logs: Vec<LogLine> = serde_json::from_value(res)?;
        Ok(logs)
    }

    pub async fn set_auto_connect(enabled: bool) -> Result<()> {
        Self::send_request("SetAutoConnectOnBoot", Some(Value::Bool(enabled))).await?;
        Ok(())
    }

    pub async fn get_service_config() -> Result<ServiceConfig> {
        let res = Self::send_request("GetServiceConfig", None).await?;
        let cfg: ServiceConfig = serde_json::from_value(res)?;
        Ok(cfg)
    }
}

/// Service Control Manager (SCM) lifecycle operations.
pub struct WindowsServiceManager;

impl WindowsServiceManager {
    /// Queries the service status in SCM.
    pub fn query_status() -> WindowsServiceState {
        use windows::Win32::System::Services::*;

        unsafe {
            let scm = match OpenSCManagerW(None, None, SC_MANAGER_CONNECT) {
                Ok(h) if !h.is_invalid() => h,
                _ => return WindowsServiceState::NotInstalled,
            };

            let name_w: Vec<u16> = SERVICE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let svc = match OpenServiceW(scm, windows::core::PCWSTR(name_w.as_ptr()), SERVICE_QUERY_STATUS) {
                Ok(h) if !h.is_invalid() => h,
                _ => {
                    let _ = CloseServiceHandle(scm);
                    return WindowsServiceState::NotInstalled;
                }
            };

            let mut status = SERVICE_STATUS::default();
            let res = QueryServiceStatus(svc, &mut status);

            let _ = CloseServiceHandle(svc);
            let _ = CloseServiceHandle(scm);

            if res.is_ok() {
                match status.dwCurrentState {
                    SERVICE_RUNNING => WindowsServiceState::Running,
                    SERVICE_STOPPED => WindowsServiceState::Stopped,
                    SERVICE_START_PENDING => WindowsServiceState::StartPending,
                    SERVICE_STOP_PENDING => WindowsServiceState::StopPending,
                    other => WindowsServiceState::Other(other.0),
                }
            } else {
                WindowsServiceState::NotInstalled
            }
        }
    }

    /// Installs the BusterVPNService Windows Service.
    pub fn install_service(exe_path: &Path) -> Result<()> {
        use windows::Win32::System::Services::*;

        let exe_str = exe_path.to_string_lossy().to_string();
        let bin_path = format!("\"{}\"", exe_str);

        unsafe {
            let scm = OpenSCManagerW(None, None, SC_MANAGER_ALL_ACCESS)
                .context("Failed to open Service Control Manager with administrative rights")?;

            let name_w: Vec<u16> = SERVICE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let display_w: Vec<u16> = SERVICE_DISPLAY_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let path_w: Vec<u16> = bin_path.encode_utf16().chain(std::iter::once(0)).collect();

            let svc = CreateServiceW(
                scm,
                windows::core::PCWSTR(name_w.as_ptr()),
                windows::core::PCWSTR(display_w.as_ptr()),
                SERVICE_ALL_ACCESS,
                SERVICE_WIN32_OWN_PROCESS,
                SERVICE_AUTO_START,
                SERVICE_ERROR_NORMAL,
                windows::core::PCWSTR(path_w.as_ptr()),
                None,
                None,
                None,
                None,
                None,
            );

            let res = match svc {
                Ok(h) => {
                    let _ = CloseServiceHandle(h);
                    Ok(())
                }
                Err(e) => Err(anyhow::anyhow!("CreateServiceW failed: {}", e)),
            };

            let _ = CloseServiceHandle(scm);
            res
        }
    }

    /// Uninstalls and deletes the BusterVPNService.
    pub fn uninstall_service() -> Result<()> {
        use windows::Win32::System::Services::*;

        let _ = Self::stop_service();

        unsafe {
            let scm = OpenSCManagerW(None, None, SC_MANAGER_ALL_ACCESS)
                .context("Failed to open Service Control Manager")?;

            let name_w: Vec<u16> = SERVICE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let delete_access = 0x00010000u32; // DELETE standard access right
            let svc = OpenServiceW(scm, windows::core::PCWSTR(name_w.as_ptr()), delete_access)
                .context("Failed to open BusterVPNService for deletion")?;

            let res = match DeleteService(svc) {
                Ok(_) => Ok(()),
                Err(e) => Err(anyhow::anyhow!("DeleteService failed: {}", e)),
            };

            let _ = CloseServiceHandle(svc);
            let _ = CloseServiceHandle(scm);
            res
        }
    }

    /// Starts BusterVPNService.
    pub fn start_service() -> Result<()> {
        use windows::Win32::System::Services::*;

        unsafe {
            let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT)
                .context("Failed to open Service Control Manager")?;

            let name_w: Vec<u16> = SERVICE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let svc = OpenServiceW(scm, windows::core::PCWSTR(name_w.as_ptr()), SERVICE_START)
                .context("Failed to open BusterVPNService to start")?;

            let res = match StartServiceW(svc, None) {
                Ok(_) => Ok(()),
                Err(e) => Err(anyhow::anyhow!("StartServiceW failed: {}", e)),
            };

            let _ = CloseServiceHandle(svc);
            let _ = CloseServiceHandle(scm);
            res
        }
    }

    /// Stops BusterVPNService.
    pub fn stop_service() -> Result<()> {
        use windows::Win32::System::Services::*;

        unsafe {
            let scm = OpenSCManagerW(None, None, SC_MANAGER_CONNECT)
                .context("Failed to open Service Control Manager")?;

            let name_w: Vec<u16> = SERVICE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let svc = match OpenServiceW(scm, windows::core::PCWSTR(name_w.as_ptr()), SERVICE_STOP) {
                Ok(h) => h,
                Err(_) => {
                    let _ = CloseServiceHandle(scm);
                    return Ok(());
                }
            };

            let mut status = SERVICE_STATUS::default();
            let _ = ControlService(svc, SERVICE_CONTROL_STOP, &mut status);

            let _ = CloseServiceHandle(svc);
            let _ = CloseServiceHandle(scm);
            Ok(())
        }
    }
}
