//! Validation: turn a raw config into a validated one, collecting diagnostics.
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::diagnostic::{ConfigCode, ConfigDiagnostic, ConfigLocation, DiagnosticSeverity};
use crate::config::parser;
use crate::config::raw::RawConfig;
use crate::config::validated::ValidatedConfig;
use crate::domain::DefaultPolicy;

/// Linux evdev `KEY_MAX`: valid `EV_KEY` codes are 0..=KEY_MAX. Codes beyond
/// this cannot be produced by Linux evdev and will never match on Linux.
const KEY_MAX: u16 = 0x2ff;

/// Default output gain in thousandths (unity).
const DEFAULT_VOLUME_MILLI: u16 = 1000;
/// Default maximum concurrent voices.
const DEFAULT_MAX_VOICES: u32 = 32;

/// Validate a raw config.
///
/// Invalid mapping keys produce warnings and are skipped. Two distinct raw
/// keys that parse to the same `InputEvent` (e.g. `"1"` and `"01"`) are an
/// error; the first wins insertion, but the error rejects the config
/// downstream. Empty sound paths are also errors. A config with no usable
/// defaults or mappings still validates — the "no sounds" check happens at
/// compile time.
pub fn validate(raw: &RawConfig) -> ValidatedConfig {
    let mut diagnostics = Vec::new();

    let default_policy = parse_default_policy(raw.default_policy.as_deref(), &mut diagnostics);
    let volume_milli = parse_volume(raw.volume, &mut diagnostics);
    let max_voices = parse_max_voices(raw.max_voices, &mut diagnostics);
    let defaults: Vec<PathBuf> = raw
        .defaults
        .iter()
        .filter_map(|name| validate_path(name, ConfigLocation::Defaults, &mut diagnostics))
        .collect();

    let mut mappings = BTreeMap::new();
    // Tracks the raw key that first produced each trigger so a duplicate can
    // name both keys in its diagnostic.
    let mut first_keys: BTreeMap<crate::domain::InputEvent, String> = BTreeMap::new();
    for (key, value) in &raw.mappings {
        let location = ConfigLocation::Mapping(key.clone());

        // Numeric key codes beyond Linux KEY_MAX cannot be produced by evdev
        // and will never match a Linux input event. Warn so the config author
        // can fix or remove them. Applies to bare codes and `linux:`/`evdev:`
        // prefixed codes; other platforms have their own code spaces.
        let linux_code = key.parse::<u16>().ok().or_else(|| {
            key.split_once(':').and_then(|(platform, code)| {
                if platform == "linux" || platform == "evdev" {
                    parser::parse_code(code)
                } else {
                    None
                }
            })
        });
        if let Some(code) = linux_code
            && code > KEY_MAX
        {
            diagnostics.push(
                ConfigDiagnostic::warning(
                    ConfigCode::KeyCodeOutOfRange,
                    location.clone(),
                    format!("keycode {code} exceeds Linux KEY_MAX ({KEY_MAX})"),
                )
                .with_hint("use a valid evdev keycode (0-767)"),
            );
        }

        let Some(trigger) = parser::parse_trigger(key) else {
            diagnostics.push(
                ConfigDiagnostic::warning(ConfigCode::InvalidKey, location.clone(), "invalid key")
                    .with_hint("use a numeric evdev keycode, e.g. \"30\""),
            );
            continue;
        };
        let Some(path) = validate_path(value, location, &mut diagnostics) else {
            continue;
        };
        // Two distinct raw keys may parse to the same event (e.g. "1"/"01").
        // Keep first-wins insertion so validated data stays well-formed, but
        // surface the collision as an error so the config is rejected.
        if let Some(first_key) = first_keys.get(&trigger) {
            diagnostics.push(
                ConfigDiagnostic::error(
                    ConfigCode::DuplicateTrigger,
                    ConfigLocation::Mapping(key.clone()),
                    format!("mapping \"{key}\" duplicates trigger \"{first_key}\""),
                )
                .with_hint("use only one key per input trigger"),
            );
            continue;
        }
        first_keys.insert(trigger, key.clone());
        mappings.insert(trigger, path);
    }

    ValidatedConfig {
        default_policy,
        volume: volume_milli as f32 / 1000.0,
        max_voices,
        defaults,
        mappings,
        diagnostics,
    }
}

/// Parse the optional `default_policy` string. An unknown value is a warning
/// and falls back to `Random` so existing configs keep working.
fn parse_default_policy(
    value: Option<&str>,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) -> DefaultPolicy {
    match value {
        None | Some("random") => DefaultPolicy::Random,
        Some("ignore") => DefaultPolicy::Ignore,
        Some(other) => {
            diagnostics.push(
                ConfigDiagnostic::warning(
                    ConfigCode::InvalidDefaultPolicy,
                    ConfigLocation::Root,
                    format!("unknown default_policy \"{other}\""),
                )
                .with_hint("use \"random\" or \"ignore\""),
            );
            DefaultPolicy::Random
        }
    }
}

/// Parse the optional `volume` (0.0..=1.0). Out-of-range values are clamped
/// with a warning; the default is unity gain.
fn parse_volume(value: Option<f32>, diagnostics: &mut Vec<ConfigDiagnostic>) -> u16 {
    match value {
        None => DEFAULT_VOLUME_MILLI,
        Some(v) if v.is_finite() && (0.0..=1.0).contains(&v) => (v * 1000.0).round() as u16,
        Some(v) => {
            diagnostics.push(
                ConfigDiagnostic::warning(
                    ConfigCode::InvalidVolume,
                    ConfigLocation::Root,
                    format!("volume {v} is out of range"),
                )
                .with_hint("use a value between 0.0 and 1.0"),
            );
            (v.clamp(0.0, 1.0) * 1000.0).round() as u16
        }
    }
}

/// Parse the optional `max_voices`. Zero is a warning and falls back to the
/// default so playback is never silently disabled.
fn parse_max_voices(value: Option<u32>, diagnostics: &mut Vec<ConfigDiagnostic>) -> u32 {
    match value {
        None => DEFAULT_MAX_VOICES,
        Some(0) => {
            diagnostics.push(
                ConfigDiagnostic::warning(
                    ConfigCode::InvalidMaxVoices,
                    ConfigLocation::Root,
                    "max_voices 0 disables playback",
                )
                .with_hint("use a positive value, e.g. 32"),
            );
            DEFAULT_MAX_VOICES
        }
        Some(n) => n,
    }
}

/// Validate a sound path. Relative paths are resolved under the config
/// directory at load time; absolute paths are used verbatim.
fn validate_path(
    name: &str,
    location: ConfigLocation,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) -> Option<PathBuf> {
    if name.is_empty() {
        diagnostics.push(
            ConfigDiagnostic::error(
                ConfigCode::InvalidSoundPath,
                location,
                "empty sound path".to_string(),
            )
            .with_hint("provide a non-empty sound file path"),
        );
        return None;
    }
    Some(PathBuf::from(name))
}

/// Whether any diagnostic is an error severe enough to reject the config.
pub fn has_errors(config: &ValidatedConfig) -> bool {
    config
        .diagnostics
        .iter()
        .any(|d| d.severity == DiagnosticSeverity::Error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(mappings: BTreeMap<String, String>) -> RawConfig {
        RawConfig {
            version: 1,
            default_policy: None,
            volume: None,
            max_voices: None,
            defaults: vec![],
            mappings,
        }
    }

    #[test]
    fn key_code_out_of_range_warns() {
        let raw = raw(BTreeMap::from([
            ("700".to_string(), "key1.wav".to_string()),
            ("3597".to_string(), "key2.wav".to_string()),
        ]));
        let validated = validate(&raw);
        assert!(
            validated
                .diagnostics
                .iter()
                .any(|d| matches!(d.code, ConfigCode::KeyCodeOutOfRange))
        );
        // Out-of-range keys are still parsed (they become Unknown), so mapping
        // is kept but flagged.
        assert_eq!(validated.mappings.len(), 2);
    }

    #[test]
    fn key_code_at_max_is_ok() {
        let raw = raw(BTreeMap::from([(
            "767".to_string(),
            "key1.wav".to_string(),
        )]));
        let validated = validate(&raw);
        assert!(
            !validated
                .diagnostics
                .iter()
                .any(|d| matches!(d.code, ConfigCode::KeyCodeOutOfRange))
        );
    }

    #[test]
    fn invalid_key_and_out_of_range_coexist() {
        let raw = raw(BTreeMap::from([
            ("notakey".to_string(), "key1.wav".to_string()),
            ("60000".to_string(), "key2.wav".to_string()),
        ]));
        let validated = validate(&raw);
        let has_invalid = validated
            .diagnostics
            .iter()
            .any(|d| matches!(d.code, ConfigCode::InvalidKey));
        let has_range = validated
            .diagnostics
            .iter()
            .any(|d| matches!(d.code, ConfigCode::KeyCodeOutOfRange));
        assert!(has_invalid);
        assert!(has_range);
    }

    #[test]
    fn defaults_validation() {
        let mut r = raw(BTreeMap::new());
        r.defaults = vec!["key1.wav".to_string(), "".to_string()];
        let validated = validate(&r);
        assert_eq!(validated.defaults.len(), 1);
        assert!(has_errors(&validated));
    }

    #[test]
    fn default_policy_parsing() {
        let make = |policy: Option<&str>| RawConfig {
            version: 1,
            default_policy: policy.map(str::to_string),
            volume: None,
            max_voices: None,
            defaults: vec![],
            mappings: BTreeMap::new(),
        };

        assert_eq!(validate(&make(None)).default_policy, DefaultPolicy::Random);
        assert_eq!(
            validate(&make(Some("random"))).default_policy,
            DefaultPolicy::Random
        );
        assert_eq!(
            validate(&make(Some("ignore"))).default_policy,
            DefaultPolicy::Ignore
        );

        let unknown = validate(&make(Some("bogus")));
        assert_eq!(unknown.default_policy, DefaultPolicy::Random);
        assert!(
            unknown
                .diagnostics
                .iter()
                .any(|d| matches!(d.code, ConfigCode::InvalidDefaultPolicy))
        );
    }

    #[test]
    fn volume_defaults_and_clamps() {
        assert_eq!(validate(&raw(BTreeMap::new())).volume, 1.0);

        let mut r = raw(BTreeMap::new());
        r.volume = Some(0.5);
        assert_eq!(validate(&r).volume, 0.5);

        let mut r = raw(BTreeMap::new());
        r.volume = Some(2.0);
        let v = validate(&r);
        assert_eq!(v.volume, 1.0);
        assert!(
            v.diagnostics
                .iter()
                .any(|d| matches!(d.code, ConfigCode::InvalidVolume))
        );
    }

    #[test]
    fn max_voices_defaults_and_rejects_zero() {
        assert_eq!(validate(&raw(BTreeMap::new())).max_voices, 32);

        let mut r = raw(BTreeMap::new());
        r.max_voices = Some(8);
        assert_eq!(validate(&r).max_voices, 8);

        let mut r = raw(BTreeMap::new());
        r.max_voices = Some(0);
        let v = validate(&r);
        assert_eq!(v.max_voices, 32);
        assert!(
            v.diagnostics
                .iter()
                .any(|d| matches!(d.code, ConfigCode::InvalidMaxVoices))
        );
    }
}
