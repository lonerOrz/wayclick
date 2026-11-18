//! Configuration loading: read and parse JSON from a directory.

use std::path::Path;
use std::time::SystemTime;

use crate::config::compiled::CompiledConfig;
use crate::config::compiler;
use crate::config::error::ConfigError;
use crate::config::migration;
use crate::config::raw::RawConfig;
use crate::config::validated::ValidatedConfig;
use crate::config::validator;

/// Source of configuration; the trait is the seam for hot-reload and testing.
pub trait ConfigSource {
    fn load(&self) -> Result<CompiledConfig, ConfigError>;

    /// Load with the validation diagnostics attached.
    fn load_with_diagnostics(&self) -> Result<(CompiledConfig, ValidatedConfig), ConfigError>;
}

/// Reads `config.json` from a directory.
pub struct FileConfigSource {
    dir: std::path::PathBuf,
}

impl FileConfigSource {
    pub fn new(dir: impl AsRef<Path>) -> FileConfigSource {
        FileConfigSource {
            dir: dir.as_ref().to_path_buf(),
        }
    }

    /// Parse the raw file without compiling it (for diagnostics).
    fn read_raw(&self) -> Result<RawConfig, ConfigError> {
        let path = self.dir.join("config.json");
        let text = std::fs::read_to_string(&path)?;
        serde_json::from_str(&text).map_err(ConfigError::from)
    }

    /// Last modification time of `config.json`, for hot-reload change detection.
    pub fn last_modified(&self) -> Option<SystemTime> {
        std::fs::metadata(self.dir.join("config.json"))
            .and_then(|meta| meta.modified())
            .ok()
    }
}

impl ConfigSource for FileConfigSource {
    fn load(&self) -> Result<CompiledConfig, ConfigError> {
        self.load_with_diagnostics().map(|(config, _)| config)
    }

    fn load_with_diagnostics(&self) -> Result<(CompiledConfig, ValidatedConfig), ConfigError> {
        compile_raw(self.read_raw()?)
    }
}

/// The single load pipeline shared by file and string sources:
/// migrate → validate → reject error diagnostics → compile.
fn compile_raw(raw: RawConfig) -> Result<(CompiledConfig, ValidatedConfig), ConfigError> {
    let raw = migration::migrate(raw)?;
    let validated = validator::validate(&raw);
    if validator::has_errors(&validated) {
        return Err(ConfigError::Diagnostics(validated.diagnostics.clone()));
    }
    let compiled = compiler::compile(&validated)?;
    Ok((compiled, validated))
}

/// Compile a raw config string (useful for tests and embedding).
pub fn from_str(text: &str) -> Result<CompiledConfig, ConfigError> {
    let raw: RawConfig = serde_json::from_str(text)?;
    compile_raw(raw).map(|(config, _)| config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CURRENT_VERSION;

    #[test]
    fn legacy_config_without_version_loads() {
        let json = r#"{"defaults":["a.wav"],"mappings":{"1":"b.wav"}}"#;
        let cfg = from_str(json).unwrap();
        assert_eq!(cfg.sound_count(), 2);
        assert_eq!(cfg.rules.len(), 1);
    }

    #[test]
    fn current_version_config_loads() {
        let json = format!(
            r#"{{"version":{},"defaults":["a.wav"],"mappings":{{}}}}"#,
            CURRENT_VERSION
        );
        assert!(from_str(&json).is_ok());
    }

    #[test]
    fn future_version_is_rejected() {
        let json = format!(r#"{{"version":{}}}"#, CURRENT_VERSION + 1);
        assert!(matches!(
            from_str(&json),
            Err(ConfigError::UnsupportedVersion(_))
        ));
    }

    #[test]
    fn default_policy_ignore_is_compiled() {
        let json = r#"{"default_policy":"ignore","defaults":["a.wav"]}"#;
        let cfg = from_str(json).unwrap();
        assert_eq!(cfg.default_policy, crate::domain::DefaultPolicy::Ignore);
    }

    #[test]
    fn missing_file_reports_last_modified_none() {
        let source = FileConfigSource::new("/nonexistent-wayclick-config");
        assert!(source.last_modified().is_none());
    }
}
