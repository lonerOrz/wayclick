//! Configuration versioning and migration. Legacy configs (no `version`
//! field) are treated as version 1; migration never touches the user's file.

use crate::config::diagnostic::{ConfigCode, ConfigDiagnostic, ConfigLocation};
use crate::config::{CURRENT_VERSION, error::ConfigError, raw::RawConfig};

/// Migrate a parsed raw config to the current schema version; unknown
/// versions are rejected.
pub fn migrate(raw: RawConfig) -> Result<RawConfig, ConfigError> {
    match raw.version {
        0 => Err(ConfigError::Diagnostics(vec![ConfigDiagnostic::error(
            ConfigCode::UnsupportedVersion,
            ConfigLocation::Version,
            "config version 0 is invalid",
        )])),
        1..=CURRENT_VERSION => Ok(raw),
        other => Err(ConfigError::UnsupportedVersion(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_version_one_accepted() {
        let raw = RawConfig {
            version: 1,
            ..RawConfig::default()
        };
        assert!(migrate(raw).is_ok());
    }

    #[test]
    fn unsupported_version_rejected() {
        let raw = RawConfig {
            version: CURRENT_VERSION + 1,
            ..RawConfig::default()
        };
        assert!(matches!(
            migrate(raw),
            Err(ConfigError::UnsupportedVersion(_))
        ));
    }

    #[test]
    fn version_zero_rejected() {
        let raw = RawConfig {
            version: 0,
            ..RawConfig::default()
        };
        assert!(migrate(raw).is_err());
    }
}
