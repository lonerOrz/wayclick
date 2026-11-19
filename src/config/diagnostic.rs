//! Configuration diagnostics: structured, locatable warnings and errors.

use std::fmt;

/// How severe a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

impl fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Machine-readable diagnostic category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigCode {
    InvalidKey,
    KeyCodeOutOfRange,
    MissingSound,
    DuplicateTrigger,
    InvalidSoundPath,
    InvalidMouseButton,
    InvalidDefaultPolicy,
    InvalidVolume,
    InvalidMaxVoices,
}

impl fmt::Display for ConfigCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::InvalidKey => "invalid_key",
            Self::KeyCodeOutOfRange => "key_code_out_of_range",
            Self::MissingSound => "missing_sound",
            Self::DuplicateTrigger => "duplicate_trigger",
            Self::InvalidSoundPath => "invalid_sound_path",
            Self::InvalidMouseButton => "invalid_mouse_button",
            Self::InvalidDefaultPolicy => "invalid_default_policy",
            Self::InvalidVolume => "invalid_volume",
            Self::InvalidMaxVoices => "invalid_max_voices",
        };
        write!(f, "{name}")
    }
}

/// Where in the config a diagnostic originated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigLocation {
    Defaults,
    Mapping(String),
    Root,
}

impl fmt::Display for ConfigLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Defaults => write!(f, "defaults"),
            Self::Mapping(key) => write!(f, "mapping \"{key}\""),
            Self::Root => write!(f, "root"),
        }
    }
}

/// One structured diagnostic produced during validation or compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: ConfigCode,
    pub location: ConfigLocation,
    pub message: String,
    /// Suggested fix, when one can be offered.
    pub hint: Option<String>,
}

impl ConfigDiagnostic {
    pub fn warning(code: ConfigCode, location: ConfigLocation, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            code,
            location,
            message: message.into(),
            hint: None,
        }
    }

    pub fn error(code: ConfigCode, location: ConfigLocation, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            code,
            location,
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl fmt::Display for ConfigDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Lead with severity + message so the common case reads
        // `error: mapping "01" duplicates trigger "1"`, then add the
        // machine-readable code, location and optional hint.
        write!(f, "{}: {}", self.severity, self.message)?;
        write!(f, " [code={}, at {}]", self.code, self.location)?;
        if let Some(hint) = &self.hint {
            write!(f, " (hint: {hint})")?;
        }
        Ok(())
    }
}
