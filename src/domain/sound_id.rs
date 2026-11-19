//! Typed sound identifier.

use std::num::NonZeroU32;

/// Identifier for a sound slot, allocated during config compilation; 1-based
/// newtype over [`NonZeroU32`]. The id ↔ index mapping lives only here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SoundId(NonZeroU32);

impl SoundId {
    /// Build the id for 0-based container index `index`; `None` when the slot
    /// count would overflow `u32`.
    #[must_use]
    pub fn from_index(index: usize) -> Option<Self> {
        let value = u32::try_from(index).ok()?.checked_add(1)?;
        NonZeroU32::new(value).map(Self)
    }

    /// The 0-based container index for this id.
    #[must_use]
    pub fn index(self) -> usize {
        (self.0.get() - 1) as usize
    }
}

impl std::fmt::Display for SoundId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_round_trips() {
        for index in [0usize, 1, 2, 100, 65_534] {
            let id = SoundId::from_index(index).unwrap();
            assert_eq!(id.index(), index, "index {index}");
        }
    }

    #[test]
    fn first_id_is_one() {
        assert_eq!(SoundId::from_index(0).unwrap().index(), 0);
    }

    #[test]
    fn overflow_returns_none() {
        assert!(SoundId::from_index(usize::MAX).is_none());
        assert!(SoundId::from_index(u32::MAX as usize).is_none());
    }

    #[test]
    fn max_representable_index() {
        // index == u32::MAX - 1 -> id == u32::MAX, still representable.
        let id = SoundId::from_index(u32::MAX as usize - 1).unwrap();
        assert_eq!(id.index(), u32::MAX as usize - 1);
    }
}
