//! Validated configuration: fully parsed, no unresolved strings remain.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::diagnostic::ConfigDiagnostic;
use crate::domain::{DefaultPolicy, InputEvent};

/// A config that has passed validation: every mapping key is a parsed
/// [`InputEvent`] and every sound is a [`PathBuf`]; nothing needs re-parsing
/// at runtime.
#[derive(Debug, Clone)]
pub struct ValidatedConfig {
    /// Policy for unmatched events.
    pub default_policy: DefaultPolicy,
    /// Output gain applied to every sound, in `0.0..=1.0`.
    pub volume: f32,
    /// Maximum number of concurrent sounds.
    pub max_voices: u32,
    /// Default sound paths, in declaration order (duplicates = weight).
    pub defaults: Vec<PathBuf>,
    /// Mapping from parsed trigger to sound path, in deterministic key order.
    pub mappings: BTreeMap<InputEvent, PathBuf>,
    /// Diagnostics accumulated during validation.
    pub diagnostics: Vec<ConfigDiagnostic>,
}
