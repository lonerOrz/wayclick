//! Configuration: load JSON, then compile it into `SoundId`-only rules.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::domain::{Action, CompiledRule, InputEvent, SoundId};

/// Raw JSON shape (`config.json`).
#[derive(Debug, Deserialize)]
pub struct RawConfig {
    /// Played when a key has no explicit mapping.
    #[serde(default)]
    pub defaults: Vec<String>,
    /// `"<keycode>" -> "<soundfile.wav>"`.
    #[serde(default)]
    pub mappings: HashMap<String, String>,
}

/// The compiled config. `sounds[0]` is unused (SoundId is NonZero); `sounds[i]`
/// is `None` when that file is missing or invalid.
#[derive(Debug, Clone)]
pub struct CompiledConfig {
    /// Indexed by `SoundId.get() - 1`.
    pub sounds: Vec<Option<String>>,
    pub rules: Vec<CompiledRule>,
    /// Played when a key has no explicit mapping.
    pub default_ids: Vec<SoundId>,
}

impl CompiledConfig {
    /// Number of sound slots (SoundId values are `1..=len`).
    pub fn sound_count(&self) -> u16 {
        self.sounds.len() as u16
    }

    /// Filename for a `SoundId` (diagnostics only).
    pub fn filename(&self, id: SoundId) -> Option<&str> {
        self.sounds
            .get((id.get() - 1) as usize)
            .and_then(|o| o.as_deref())
    }
}

/// Source of configuration; the trait is the seam for hot-reload.
pub trait ConfigSource {
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

/// Resolve a `RawConfig` into a `CompiledConfig`: every distinct filename gets a
/// `SoundId` (first-seen order), and every numeric mapping key becomes a trigger.
pub fn compile(raw: RawConfig) -> Result<CompiledConfig, ConfigError> {
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

    Ok(CompiledConfig {
        sounds,
        rules,
        default_ids,
    })
}

/// Parse a JSON mapping key into an `InputEvent` trigger; non-numeric keys are
/// skipped.
fn parse_trigger(key: &str) -> Option<InputEvent> {
    let code = key.parse::<u16>().ok()?;
    Some(InputEvent::from_evdev_code(code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MouseButton;

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
        // Only "1", "2" and "272" produce rules.
        assert_eq!(cfg.rules.len(), 3);
    }
}
