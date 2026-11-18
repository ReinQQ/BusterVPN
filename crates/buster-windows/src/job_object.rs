use std::ffi::c_void;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject, TerminateJobObject,
    JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use crate::error::WindowsError;

/// Safe RAII wrapper around a Windows Job Object configured with `KILL_ON_JOB_CLOSE`.
///
/// When BusterVPN exits, crashes, or is force-killed by the OS/Task Manager, Windows
/// automatically closes this handle. Closing the handle terminates every child core
/// process assigned to this Job Object, preventing orphaned Xray/Hysteria cores
/// from locking the Wintun adapter.
#[derive(Debug)]
pub struct SafeJobObject(HANDLE);

unsafe impl Send for SafeJobObject {}
unsafe impl Sync for SafeJobObject {}

impl SafeJobObject {
    /// Creates a new anonymous Job Object configured to kill all member processes on close.
    pub fn new() -> Result<Self, WindowsError> {
        unsafe {
            let handle = CreateJobObjectW(None, None)?;

            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags =
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;

            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;

            tracing::debug!("Created Windows Job Object with KILL_ON_JOB_CLOSE limit");
            Ok(Self(handle))
        }
    }

    /// Assigns a running process to this Job Object.
    pub fn assign_process(&self, process_handle: HANDLE) -> Result<(), WindowsError> {
        if self.0.is_invalid() {
            return Err(WindowsError::MissingProcessHandle);
        }

        unsafe {
            AssignProcessToJobObject(self.0, process_handle)?;
        }

        tracing::debug!("Process {:?} successfully assigned to Job Object", process_handle);
        Ok(())
    }

    /// Explicitly terminates all processes currently associated with this Job Object.
    pub fn terminate(&self, exit_code: u32) -> Result<(), WindowsError> {
        if !self.0.is_invalid() {
            unsafe {
                TerminateJobObject(self.0, exit_code)?;
            }
        }
        Ok(())
    }

    /// Returns the raw Win32 HANDLE.
    pub fn raw_handle(&self) -> HANDLE {
        self.0
    }
}

impl Drop for SafeJobObject {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
            tracing::debug!("SafeJobObject handle closed; all assigned processes terminated by OS");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_job_object_succeeds() {
        let job = SafeJobObject::new();
        assert!(job.is_ok(), "SafeJobObject::new() should succeed on Windows");
        let job = job.unwrap();
        assert!(!job.raw_handle().is_invalid());
        // Handle automatically closed on drop
    }
}
