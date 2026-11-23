use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::Sender;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::CREATE_NO_WINDOW;
use crate::error::WindowsError;
use crate::job_object::SafeJobObject;
use crate::log::{LogLine, LogStream};

const RUNTIME_ASSETS: [&str; 5] = [
    "sing-box.exe",
    "xray.exe",
    "wintun.dll",
    "geosite-ru.srs",
    "geoip-ru.srs",
];

const LEGACY_RUNTIME_FILES: [&str; 4] = [
    "hysteria.exe",
    "geoip.dat",
    "geosite.dat",
    "hysteria_config.yaml",
];

const MAX_STDERR_LINES: usize = 50;

/// Synchronizes core executables and routing databases into `runtime_dir`,
/// checking `exe_dir/cores` first and then `extra_search_dirs`, and purges legacy files.
pub fn sync_runtime_assets(
    runtime_dir: &Path,
    extra_search_dirs: &[PathBuf],
) -> Result<(), WindowsError> {
    std::fs::create_dir_all(runtime_dir)?;

    let mut search_dirs = Vec::new();
    if let Some(exe_cores) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("cores")))
    {
        search_dirs.push(exe_cores);
    }
    for dir in extra_search_dirs {
        if !search_dirs.iter().any(|existing| existing == dir) {
            search_dirs.push(dir.clone());
        }
    }

    for asset in RUNTIME_ASSETS {
        let dst = runtime_dir.join(asset);
        let dst_len = dst.metadata().map(|m| m.len()).unwrap_or(0);
        for cand in &search_dirs {
            let src = cand.join(asset);
            if src == dst || !src.is_file() {
                continue;
            }
            let src_len = src.metadata().map(|m| m.len()).unwrap_or(0);
            if dst_len == 0 || dst_len != src_len {
                if std::fs::copy(&src, &dst).is_ok() {
                    tracing::info!(
                        "Synchronized runtime asset '{}' ({} bytes) from {}",
                        asset,
                        src_len,
                        src.display()
                    );
                    break;
                }
            } else {
                break;
            }
        }
    }

    for legacy in LEGACY_RUNTIME_FILES {
        let legacy_file = runtime_dir.join(legacy);
        if legacy_file.exists() {
            let _ = std::fs::remove_file(&legacy_file);
        }
    }

    Ok(())
}

/// Polls `127.0.0.1:<port>` (up to 30 retries x 50ms) until `xray.exe` is listening,
/// aborting immediately with captured stderr if the sidecar exits prematurely.
pub async fn wait_for_sidecar_ready(
    sidecar: &mut CoreProcessManager,
    port: u16,
) -> Result<(), WindowsError> {
    let addr: std::net::SocketAddr = ([127, 0, 0, 1], port).into();

    for _ in 0..30 {
        if let Ok(Some(status)) = sidecar.try_wait() {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            let stderr = sidecar.recent_stderr();
            let msg = if stderr.is_empty() {
                format!("xray.exe exited prematurely with status {}", status)
            } else {
                format!("xray.exe exited prematurely ({}): {}", status, stderr)
            };
            return Err(WindowsError::ProcessStart(msg));
        }

        let listening = tokio::task::spawn_blocking(move || {
            std::net::TcpStream::connect_timeout(
                &addr,
                std::time::Duration::from_millis(50),
            )
            .is_ok()
        })
        .await
        .unwrap_or(false);

        if listening {
            return Ok(());
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let stderr = sidecar.recent_stderr();
    let _ = sidecar.terminate();
    let msg = if stderr.is_empty() {
        format!("xray.exe did not start listening on 127.0.0.1:{} within timeout", port)
    } else {
        format!(
            "xray.exe did not start listening on 127.0.0.1:{} within timeout: {}",
            port, stderr
        )
    };
    Err(WindowsError::ProcessStart(msg))
}

/// Manages the lifecycle of a child core process (`sing-box.exe` or `xray.exe`).
///
/// Guaranteed safety invariants:
/// 1. The process is spawned with `CREATE_NO_WINDOW` (never flashes a console window).
/// 2. Bound immediately to a `SafeJobObject` with `KILL_ON_JOB_CLOSE`.
/// 3. Bound to Tokio's `kill_on_drop(true)`.
/// 4. Stdout and Stderr are asynchronously pumped line-by-line without blocking OS pipes.
#[derive(Debug)]
pub struct CoreProcessManager {
    child: Child,
    job: Arc<SafeJobObject>,
    pid: u32,
    recent_stderr: Arc<StdMutex<Vec<String>>>,
}

impl CoreProcessManager {
    /// Spawns a core process without a console window and assigns it to the lifecycle Job Object.
    pub fn spawn_core(
        exe_path: &Path,
        args: Vec<String>,
        job: Arc<SafeJobObject>,
    ) -> Result<Self, WindowsError> {
        let working_dir = exe_path
            .parent()
            .unwrap_or_else(|| Path::new("."));
        Self::spawn_core_with_workdir(exe_path, args, job, working_dir)
    }

    /// Spawns a core process with an explicit working directory and assigns it to the lifecycle Job Object.
    pub fn spawn_core_with_workdir(
        exe_path: &Path,
        args: Vec<String>,
        job: Arc<SafeJobObject>,
        working_dir: &Path,
    ) -> Result<Self, WindowsError> {
        if !exe_path.exists() {
            return Err(WindowsError::ProcessStart(format!(
                "Core executable not found at: {}",
                exe_path.display()
            )));
        }

        let mut cmd = Command::new(exe_path);
        cmd.args(args)
            .current_dir(working_dir)
            .creation_flags(CREATE_NO_WINDOW.0)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .env("XRAY_LOCATION_ASSET", working_dir);

        let child = cmd.spawn().map_err(|e| {
            WindowsError::ProcessStart(format!(
                "Failed to spawn core '{}': {}",
                exe_path.display(),
                e
            ))
        })?;

        let pid = child.id().unwrap_or(0);

        // Assign the newly spawned process to the Job Object
        let raw_handle = child
            .raw_handle()
            .ok_or(WindowsError::MissingProcessHandle)?;
        let win_handle = HANDLE(raw_handle as _);

        job.assign_process(win_handle)?;

        tracing::info!(
            "Successfully started core '{}' (PID {}) in working dir '{}' and assigned to Job Object",
            exe_path.display(),
            pid,
            working_dir.display()
        );

        Ok(Self {
            child,
            job,
            pid,
            recent_stderr: Arc::new(StdMutex::new(Vec::new())),
        })
    }

    /// Starts non-blocking asynchronous streaming of stdout and stderr into the provided channel.
    pub fn start_log_streaming(&mut self, sender: Sender<LogLine>) {
        if let Some(stdout) = self.child.stdout.take() {
            let tx = sender.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let log = LogLine::classify(line, LogStream::Stdout);
                    if tx.send(log).await.is_err() {
                        break;
                    }
                }
            });
        }

        if let Some(stderr) = self.child.stderr.take() {
            let tx = sender;
            let stderr_buf = self.recent_stderr.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Ok(mut buf) = stderr_buf.lock() {
                        if buf.len() >= MAX_STDERR_LINES {
                            buf.remove(0);
                        }
                        buf.push(line.clone());
                    }
                    let log = LogLine::classify(line, LogStream::Stderr);
                    if tx.send(log).await.is_err() {
                        break;
                    }
                }
            });
        }
    }

    /// Polls whether the child process has exited without blocking.
    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Returns captured recent stderr lines joined by newline.
    pub fn recent_stderr(&self) -> String {
        self.recent_stderr
            .lock()
            .map(|buf| buf.join("\n"))
            .unwrap_or_default()
    }

    /// Forcibly terminates the core process tree.
    pub async fn kill(&mut self) -> Result<(), WindowsError> {
        let _ = self.child.kill().await;
        let _ = self.job.terminate(1);
        tracing::warn!("Forcibly terminated core process PID {}", self.pid);
        Ok(())
    }

    /// Synchronous kill wrapper for cleanup during Drop/disconnect.
    pub fn terminate(&mut self) -> Result<(), WindowsError> {
        let _ = self.child.start_kill();
        let _ = self.job.terminate(1);
        tracing::warn!("Terminated core process PID {}", self.pid);
        Ok(())
    }

    /// Returns the PID of the spawned core process.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Returns a reference to the associated Job Object.
    pub fn job(&self) -> &Arc<SafeJobObject> {
        &self.job
    }
}

/// Checks if a process with the given PID is currently active.
pub fn is_pid_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::Foundation::CloseHandle;

    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(_) => return false,
        };

        let mut exit_code = 0u32;
        let res = GetExitCodeProcess(handle, &mut exit_code);
        let _ = CloseHandle(handle);

        res.is_ok() && exit_code == 259 // STILL_ACTIVE = 259
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_spawn_nonexistent_exe_fails_gracefully() {
        let job = Arc::new(SafeJobObject::new().expect("Create Job Object"));
        let bad_path = PathBuf::from("C:\\nonexistent_buster_core.exe");

        let res = CoreProcessManager::spawn_core(&bad_path, vec![], job);
        assert!(res.is_err());
        match res.unwrap_err() {
            WindowsError::ProcessStart(msg) => {
                assert!(msg.contains("Core executable not found"));
            }
            other => panic!("Unexpected error: {:?}", other),
        }
    }
}
