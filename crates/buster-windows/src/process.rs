use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::Sender;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::CREATE_NO_WINDOW;
use crate::error::WindowsError;
use crate::job_object::SafeJobObject;
use crate::log::{LogLine, LogStream};

/// Manages the lifecycle of a child core process (Xray or Hysteria 2).
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

        Ok(Self { child, job, pid })
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
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let log = LogLine::classify(line, LogStream::Stderr);
                    if tx.send(log).await.is_err() {
                        break;
                    }
                }
            });
        }
    }

    /// Asynchronously streams stdout and stderr until the core exits, returning the exit status.
    pub async fn stream_logs(&mut self, sender: Sender<LogLine>) -> Result<ExitStatus, WindowsError> {
        let stdout = self.child.stdout.take();
        let stderr = self.child.stderr.take();

        let tx_out = sender.clone();
        let stdout_task = tokio::spawn(async move {
            if let Some(stdout) = stdout {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let log = LogLine::classify(line, LogStream::Stdout);
                    if tx_out.send(log).await.is_err() {
                        break;
                    }
                }
            }
        });

        let tx_err = sender.clone();
        let stderr_task = tokio::spawn(async move {
            if let Some(stderr) = stderr {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let log = LogLine::classify(line, LogStream::Stderr);
                    if tx_err.send(log).await.is_err() {
                        break;
                    }
                }
            }
        });

        // Await process termination
        let status = self.child.wait().await?;

        // Wait for stdout/stderr readers to flush
        let _ = tokio::join!(stdout_task, stderr_task);

        tracing::info!("Core process (PID {}) exited with status: {}", self.pid, status);
        Ok(status)
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
