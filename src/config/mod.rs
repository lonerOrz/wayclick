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
