//! Configuration errors.

use std::fmt;

use crate::config::diagnostic::ConfigDiagnostic;

/// Errors from loading, validating, migrating, or compiling configuration.
#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Empty,
    TooManySounds(usize),
    UnsupportedVersion(u32),
    InvalidSoundPath(String),
    DuplicateTrigger(String),
    /// One or more structured diagnostics rejected the config.
    Diagnostics(Vec<ConfigDiagnostic>),
}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error reading config: {e}"),
            Self::Json(e) => write!(f, "invalid JSON in config: {e}"),
            Self::Empty => write!(f, "config has no usable sounds"),
            Self::TooManySounds(n) => write!(f, "too many sounds: {n} (max {})", u32::MAX - 1),
            Self::UnsupportedVersion(v) => write!(
                f,
                "unsupported config version: {v} (max {})",
                crate::config::CURRENT_VERSION
            ),
            Self::InvalidSoundPath(p) => write!(f, "invalid sound path: {p}"),
            Self::DuplicateTrigger(k) => write!(f, "duplicate trigger: {k}"),
            Self::Diagnostics(diagnostics) => {
                for d in diagnostics {
                    writeln!(f, "{d}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}
