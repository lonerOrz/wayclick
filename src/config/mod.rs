//! Configuration: raw JSON → validated → compiled runtime data.
//!
//! Loading, validation, migration and compilation are separate stages. The
//! compiled output contains no strings that need parsing on the hot path.

pub mod compiled;
pub mod compiler;
pub mod diagnostic;
pub mod error;
pub mod loader;
pub mod migration;
pub mod parser;
pub mod raw;
pub mod validated;
pub mod validator;

pub use compiled::{CompiledConfig, SoundCatalog, SoundSpec};
pub use diagnostic::ConfigDiagnostic;
pub use error::ConfigError;
pub use loader::{ConfigSource, FileConfigSource, from_str};
pub use raw::RawConfig;
pub use validated::ValidatedConfig;

/// Current configuration schema version.
pub const CURRENT_VERSION: u32 = 1;

/// The default configuration, embedded at build time from
/// `assets/default/config.json`. Kept as the single source of truth: the same
/// file ships in release packages, so dev, embedded, and packaged defaults
/// cannot drift apart.
pub const DEFAULT_CONFIG_JSON: &str = include_str!("../../assets/default/config.json");

/// Write the embedded default config into `dir` if the user has none yet.
///
/// Returns `true` when a config file was created. The written file is plain
/// JSON the user can freely edit; sound files referenced by it are resolved
/// the same way everywhere (dev `assets/default`, packaged
/// `share/wayclick/config`, or the config directory itself).
pub fn ensure_user_config(dir: &std::path::Path) -> Result<bool, ConfigError> {
    let path = dir.join("config.json");
    if path.exists() {
        return Ok(false);
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, DEFAULT_CONFIG_JSON)?;
    tracing::info!(path = %path.display(), "created default config");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_default_config_is_valid() {
        let config = from_str(DEFAULT_CONFIG_JSON).expect("embedded config must compile");
        assert!(config.sound_count() > 0);
        assert!(!config.rules.is_empty());
    }

    #[test]
    fn ensure_user_config_creates_missing_file() {
        let dir = std::env::temp_dir().join(format!("wayclick-test-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let created = ensure_user_config(&dir).unwrap();
        assert!(created);
        // Second call is a no-op (file exists).
        assert!(!ensure_user_config(&dir).unwrap());
        // The written file parses.
        from_str(&std::fs::read_to_string(dir.join("config.json")).unwrap()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
