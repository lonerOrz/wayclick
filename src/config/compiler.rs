//! Compilation: turn a validated config into runtime data.
//!
//! Sound ids are interned in deterministic order: defaults first (declaration
//! order), then mapping values in trigger-key order. The resulting
//! [`CompiledConfig`] is immutable and contains no strings that need parsing.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::compiled::{CompiledConfig, SoundCatalog, SoundSpec};
use crate::config::error::ConfigError;
use crate::config::validated::ValidatedConfig;
use crate::domain::{Action, CompiledRule, SoundId};

/// Compile a validated config into runtime data.
pub fn compile(validated: &ValidatedConfig) -> Result<CompiledConfig, ConfigError> {
    let mut catalog = SoundCatalog::default();
    let mut by_path: HashMap<PathBuf, SoundId> = HashMap::new();

    let mut intern = |path: &PathBuf| -> Result<SoundId, ConfigError> {
        if let Some(&id) = by_path.get(path) {
            return Ok(id);
        }
        let id = SoundId::from_index(catalog.len())
            .ok_or(ConfigError::TooManySounds(catalog.len().saturating_add(1)))?;
        catalog.push(SoundSpec::new(path.clone()));
        by_path.insert(path.clone(), id);
        Ok(id)
    };

    for path in &validated.defaults {
        intern(path)?;
    }
    for path in validated.mappings.values() {
        let _ = intern(path)?;
    }

    if catalog.is_empty() {
        return Err(ConfigError::Empty);
    }

    let default_ids: Vec<SoundId> = validated
        .defaults
        .iter()
        .filter_map(|path| by_path.get(path).copied())
        .collect();

    let mut rules = Vec::with_capacity(validated.mappings.len());
    for (trigger, path) in &validated.mappings {
        let Some(&id) = by_path.get(path) else {
            continue;
        };
        rules.push(CompiledRule::new(*trigger, Action::PlaySound(id)));
    }

    Ok(CompiledConfig {
        catalog,
        rules,
        default_ids,
        default_policy: validated.default_policy,
        volume_milli: (validated.volume.clamp(0.0, 1.0) * 1000.0).round() as u16,
        max_voices: validated.max_voices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::raw::RawConfig;
    use crate::config::validator::validate;
    use crate::domain::InputEvent;
    use std::collections::BTreeMap;

    fn compile_raw(raw: RawConfig) -> CompiledConfig {
        compile(&validate(&raw)).unwrap()
    }

    #[test]
    fn sound_ids_are_deterministic_and_interned() {
        let raw = RawConfig {
            defaults: vec!["a.wav".into()],
            mappings: BTreeMap::from([("1".into(), "b.wav".into()), ("2".into(), "a.wav".into())]),
            ..RawConfig::default()
        };
        let a = compile_raw(raw.clone());
        let b = compile_raw(raw);
        assert_eq!(a.sound_count(), 2);
        assert_eq!(a.default_ids[0].index(), 0); // a.wav = id 1
        assert_eq!(a, b, "identical config must compile identically");
    }

    #[test]
    fn empty_config_is_rejected() {
        let raw = RawConfig::default();
        let validated = validate(&raw);
        assert!(matches!(compile(&validated), Err(ConfigError::Empty)));
    }

    #[test]
    fn rule_uses_interned_id() {
        let raw = RawConfig {
            mappings: BTreeMap::from([("1".into(), "esc.wav".into())]),
            ..RawConfig::default()
        };
        let cfg = compile_raw(raw);
        assert_eq!(cfg.rules.len(), 1);
        let Action::PlaySound(id) = cfg.rules[0].action();
        assert_eq!(
            cfg.catalog.get(id).unwrap().path(),
            std::path::Path::new("esc.wav")
        );
        assert_eq!(
            cfg.rules[0].trigger(),
            InputEvent::KeyDown(crate::domain::PhysicalKey::Escape)
        );
    }

    #[test]
    fn volume_and_max_voices_are_compiled() {
        let raw = RawConfig {
            defaults: vec!["a.wav".into()],
            volume: Some(0.25),
            max_voices: Some(8),
            ..RawConfig::default()
        };
        let cfg = compile_raw(raw);
        assert_eq!(cfg.volume(), 0.25);
        assert_eq!(cfg.max_voices, 8);
    }
}
