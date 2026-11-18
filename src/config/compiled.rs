//! Runtime sound catalog and compiled configuration.

use std::path::{Path, PathBuf};

use crate::domain::{CompiledRule, DefaultPolicy, SoundId};

/// A resolved sound resource descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundSpec {
    path: PathBuf,
}

impl SoundSpec {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The sound file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Owns the ordered set of sound descriptors: `sounds[i]` is the sound for
/// `SoundId(i + 1)`, reachable only through `SoundId`-based accessors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoundCatalog {
    sounds: Vec<SoundSpec>,
}

impl SoundCatalog {
    pub fn push(&mut self, spec: SoundSpec) -> SoundId {
        let id = SoundId::from_index(self.sounds.len())
            .expect("catalog length exceeds SoundId capacity; validated at compile time");
        self.sounds.push(spec);
        id
    }

    /// Look up a sound descriptor by id.
    pub fn get(&self, id: SoundId) -> Option<&SoundSpec> {
        self.sounds.get(id.index())
    }

    pub fn len(&self) -> usize {
        self.sounds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty()
    }

    /// Iterate over `(SoundId, spec)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (SoundId, &SoundSpec)> {
        self.sounds
            .iter()
            .enumerate()
            .filter_map(|(i, spec)| SoundId::from_index(i).map(|id| (id, spec)))
    }

    /// The raw path slice, for the audio decoder that loads all sounds.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.sounds.iter().map(|spec| spec.path())
    }
}

/// The compiled runtime configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledConfig {
    pub catalog: SoundCatalog,
    pub rules: Vec<CompiledRule>,
    pub default_ids: Vec<SoundId>,
    pub default_policy: DefaultPolicy,
    /// Output gain in thousandths (`1000` = unity). Kept integral so the
    /// struct stays `Eq`.
    pub volume_milli: u16,
    /// Maximum concurrent voices.
    pub max_voices: u32,
}

impl CompiledConfig {
    pub fn sound_count(&self) -> usize {
        self.catalog.len()
    }

    /// Output gain as a linear factor (`1.0` = unity).
    pub fn volume(&self) -> f32 {
        f32::from(self.volume_milli) / 1000.0
    }
}
