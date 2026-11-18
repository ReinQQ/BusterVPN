use thiserror::Error;

#[derive(Debug, Error)]
pub enum WindowsError {
    #[error("Win32 API error: {0}")]
    Win32(#[from] windows::core::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Process handle is unavailable")]
    MissingProcessHandle,
    #[error("Process start error: {0}")]
    ProcessStart(String),
}
