//! Raw configuration: the external JSON shape, not yet validated.

use std::collections::BTreeMap;

use serde::Deserialize;

/// Raw JSON shape (`config.json`), not yet validated. `mappings` is a
/// `BTreeMap` so `SoundId` allocation is deterministic across runs.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawConfig {
    /// Policy for unmatched events: "random" (default) or "ignore".
    #[serde(default)]
    pub default_policy: Option<String>,
    /// Output gain applied to every sound, `0.0..=1.0` (default `1.0`).
    #[serde(default)]
    pub volume: Option<f32>,
    /// Maximum number of sounds playing at once (default 32).
    #[serde(default)]
    pub max_voices: Option<u32>,
    /// Played when a key has no explicit mapping; duplicates = weight.
    #[serde(default)]
    pub defaults: Vec<String>,
    /// `"<keycode>" -> "<soundfile.wav>"`.
    #[serde(default)]
    pub mappings: BTreeMap<String, String>,
}
