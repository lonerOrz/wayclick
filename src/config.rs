//! Configuration: load JSON, then *compile* it into `SoundId`-only rules.
//!
//! The hot path (the pipeline) only ever sees `CompiledConfig`, which holds
//! `SoundId`s. Filename strings are resolved exactly once, at load time, in
//! `compile()`. There is no `HashMap<String, _>` consulted per keypress.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::domain::{Action, CompiledRule, InputEvent, MouseButton, SoundId};

/// Raw JSON shape (what lives in `config.json`).
#[derive(Debug, Deserialize)]
pub struct RawConfig {
    /// Sounds played when a key has no explicit mapping.
    #[serde(default)]
    pub defaults: Vec<String>,
    /// `"<keycode>" -> "<soundfile.wav>"`. Keys are strings (JSON object keys).
    #[serde(default)]
    pub mappings: HashMap<String, String>,
}

/// A resolved sound table: filename -> `SoundId`.
///
/// `sounds[0]` is unused (SoundId is NonZero). `sounds[i]` is `Some` when the
/// file decoded successfully, `None` when missing/invalid.
#[derive(Debug, Clone)]
pub struct CompiledConfig {
    /// Indexed by `SoundId.get() - 1`.
    pub sounds: Vec<Option<String>>,
    pub rules: Vec<CompiledRule>,
    /// `SoundId`s that are defaults (played when a key has no explicit mapping).
    pub default_ids: Vec<SoundId>,
}

impl CompiledConfig {
    /// Number of sound slots (SoundId values are 1..=len).
    pub fn sound_count(&self) -> u16 {
        self.sounds.len() as u16
    }

    /// Look up the filename for a `SoundId` (for diagnostics/logging).
    pub fn filename(&self, id: SoundId) -> Option<&str> {
        self.sounds
            .get((id.get() - 1) as usize)
            .and_then(|o| o.as_deref())
    }
}

/// Source of configuration. `FileConfigSource` is the v1 impl; the trait is the
/// seam for hot-reload (a future `WatchedConfigSource`) without touching callers.
pub trait ConfigSource {
    /// Load and compile the configuration.
    fn load(&self) -> Result<CompiledConfig, ConfigError>;
}

/// Error type for config loading/compilation.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("io error reading config: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid JSON in config: {0}")]
    Json(#[from] serde_json::Error),
    #[error("config has no usable sounds")]
    Empty,
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
}

impl ConfigSource for FileConfigSource {
    fn load(&self) -> Result<CompiledConfig, ConfigError> {
        let path = self.dir.join("config.json");
        let raw: RawConfig = {
            let text = std::fs::read_to_string(&path)?;
            serde_json::from_str(&text)?
        };
        compile(raw)
    }
}

/// Resolve a `RawConfig` into a `CompiledConfig`.
///
/// All distinct filenames (defaults + mappings values) are assigned a `SoundId`
/// in first-seen order. Each mapping key is parsed: numeric keys become
/// `InputEvent::Key(code)`; the three well-known mouse keys (`272`/`273`/`274`)
/// become `InputEvent::Mouse(..)`. Everything else is ignored.
pub fn compile(raw: RawConfig) -> Result<CompiledConfig, ConfigError> {
    // Assign a SoundId to every distinct filename.
    let mut name_to_id: HashMap<String, SoundId> = HashMap::new();
    let mut sounds: Vec<Option<String>> = Vec::new(); // sounds[0] stays None (SoundId is NonZero)

    let mut intern = |name: &str, sounds: &mut Vec<Option<String>>| -> SoundId {
        if let Some(id) = name_to_id.get(name) {
            return *id;
        }
        let id = SoundId::new(sounds.len() as u16 + 1).expect("sound count overflow");
        name_to_id.insert(name.to_string(), id);
        sounds.push(Some(name.to_string()));
        id
    };

    // Defaults first so their ordering is stable, then mappings values.
    for d in &raw.defaults {
        intern(d, &mut sounds);
    }
    for v in raw.mappings.values() {
        intern(v, &mut sounds);
    }

    if sounds.is_empty() {
        return Err(ConfigError::Empty);
    }

    let default_ids: Vec<SoundId> = raw
        .defaults
        .iter()
        .filter_map(|d| name_to_id.get(d).copied())
        .collect();

    // Build rules from mappings.
    let mut rules: Vec<CompiledRule> = Vec::new();
    for (key, value) in &raw.mappings {
        let Some(trigger) = parse_trigger(key) else {
            continue;
        };
        let Some(&sid) = name_to_id.get(value) else {
            continue;
        };
        let actions = vec![Action::PlaySound(sid)];
        rules.push(CompiledRule::new(trigger, actions));
    }

    // A synthetic default rule: any unmapped key plays a random default sound.
    // The executor falls back to `default_ids` when no rule matches.
    Ok(CompiledConfig {
        sounds,
        rules,
        default_ids,
    })
}

/// Parse a JSON mapping key into an `InputEvent` trigger.
///
/// - `"272" | "273" | "274"` -> mouse buttons (mirrors the Python config).
/// - any other parseable `u16` -> `InputEvent::Key(code)`.
/// - anything else -> `None` (skipped).
fn parse_trigger(key: &str) -> Option<InputEvent> {
    match key {
        "272" => Some(InputEvent::Mouse(MouseButton::Left)),
        "273" => Some(InputEvent::Mouse(MouseButton::Right)),
        "274" => Some(InputEvent::Mouse(MouseButton::Middle)),
        _ => key.parse::<u16>().ok().map(InputEvent::Key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RawConfig {
        RawConfig {
            defaults: vec!["key1.wav".into(), "key3.wav".into()],
            mappings: HashMap::from([
                ("1".into(), "ctrl.wav".into()),
                ("2".into(), "key1.wav".into()),
                ("272".into(), "mouse.wav".into()),
                ("notanum".into(), "key1.wav".into()),
            ]),
        }
    }

    #[test]
    fn compiles_filenames_to_sound_ids() {
        let cfg = compile(sample()).unwrap();
        // 5 distinct filenames: key1, key3, ctrl, mouse -> 4 sounds.
        assert_eq!(cfg.sound_count(), 4);
    }

    #[test]
    fn resolves_mouse_trigger() {
        let cfg = compile(sample()).unwrap();
        let mouse_rule = cfg
            .rules
            .iter()
            .find(|r| matches!(r.trigger, InputEvent::Mouse(MouseButton::Left)))
            .expect("mouse rule present");
        assert!(matches!(mouse_rule.actions[0], Action::PlaySound(_)));
    }

    #[test]
    fn drops_non_numeric_key() {
        let cfg = compile(sample()).unwrap();
        assert!(!cfg.rules.iter().any(|r| r.trigger == InputEvent::Ignored));
    }
}
