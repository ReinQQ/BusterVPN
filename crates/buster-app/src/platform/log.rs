//! Cross-platform log definitions for VPN engine output streaming.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogLine {
    pub timestamp: DateTime<Utc>,
    pub stream: LogStream,
    pub text: String,
    pub level: LogLevel,
}

impl LogLine {
    pub fn new(text: impl Into<String>, stream: LogStream, level: LogLevel) -> Self {
        Self {
            timestamp: Utc::now(),
            stream,
            text: text.into(),
            level,
        }
    }

    /// Automatically classifies the log level based on common keywords used by Xray and Hysteria.
    pub fn classify(text: impl Into<String>, stream: LogStream) -> Self {
        let text_str = text.into();
        let lower = text_str.to_ascii_lowercase();

        let is_wintun_probe = lower.contains("failed to find matching adapter name")
            || lower.contains("0x00000490")
            || (lower.contains("wintun") && lower.contains("element not found"));

        let level = if is_wintun_probe {
            LogLevel::Debug
        } else if lower.contains("[error]")
            || lower.contains("error:")
            || lower.contains("fatal")
            || lower.contains("panic")
            || lower.contains("failed to")
        {
            LogLevel::Error
        } else if lower.contains("[warning]")
            || lower.contains("[warn]")
            || lower.contains("warning:")
            || lower.contains("warn:")
        {
            LogLevel::Warn
        } else {
            LogLevel::Info
        };

        Self {
            timestamp: Utc::now(),
            stream,
            text: text_str,
            level,
        }
    }
}
