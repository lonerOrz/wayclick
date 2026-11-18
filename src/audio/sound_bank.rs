//! Immutable bank of decoded sounds, indexed by [`SoundId`].

use std::sync::Arc;

use crate::audio::DecodedSound;
use crate::domain::SoundId;

/// Owns decoded sounds in `SoundId` order. A slot may be empty when its file
/// failed to decode; playing it reports [`AudioError::SoundNotLoaded`].
#[derive(Debug, Clone, Default)]
pub struct SoundBank {
    sounds: Vec<Option<Arc<DecodedSound>>>,
}

impl SoundBank {
    pub fn from_slots(slots: Vec<Option<Arc<DecodedSound>>>) -> Self {
        Self { sounds: slots }
    }

    pub fn from_sounds(sounds: Vec<DecodedSound>) -> Self {
        Self {
            sounds: sounds.into_iter().map(Arc::new).map(Some).collect(),
        }
    }

    pub fn get(&self, id: SoundId) -> Option<&Arc<DecodedSound>> {
        self.sounds.get(id.index()).and_then(Option::as_ref)
    }

    pub fn len(&self) -> usize {
        self.sounds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty()
    }

    pub fn has_any(&self) -> bool {
        self.sounds.iter().any(Option::is_some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::{NonZeroU16, NonZeroU32};

    fn sound() -> DecodedSound {
        DecodedSound::new(
            Arc::from([0.0f32, 1.0]),
            NonZeroU16::new(1).unwrap(),
            NonZeroU32::new(48000).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn get_by_sound_id() {
        let bank = SoundBank::from_sounds(vec![sound()]);
        let id = SoundId::from_index(0).unwrap();
        assert!(bank.get(id).is_some());
        assert!(bank.get(SoundId::from_index(5).unwrap()).is_none());
    }

    #[test]
    fn empty_slots_are_allowed() {
        let bank = SoundBank::from_slots(vec![None, Some(Arc::new(sound()))]);
        assert!(bank.has_any());
        assert!(bank.get(SoundId::from_index(0).unwrap()).is_none());
        assert!(bank.get(SoundId::from_index(1).unwrap()).is_some());
    }
}
