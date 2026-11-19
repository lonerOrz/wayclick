//! Default-sound random selection with implicit duplicate weighting.

use crate::domain::SoundId;

/// Selects a default sound at random; duplicate ids act as weights.
#[derive(Debug, Default)]
pub struct RandomSelector {
    // RNG state is a thread-local (see `choose`); this struct is a zero-sized
    // handle so it can be injected and tested independently.
    _private: (),
}

impl RandomSelector {
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Pick one id from `defaults`, or `None` if the slice is empty.
    #[must_use]
    pub fn choose(&self, defaults: &[SoundId]) -> Option<SoundId> {
        if defaults.is_empty() {
            return None;
        }
        let index = fastrand(defaults.len());
        defaults.get(index).copied()
    }
}

fn fastrand(n: usize) -> usize {
    use rand::Rng;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    thread_local! {
        static RNG: std::cell::RefCell<SmallRng> =
            std::cell::RefCell::new(SmallRng::from_entropy());
    }
    if n == 0 {
        return 0;
    }
    RNG.with(|r| (*r.borrow_mut()).gen_range(0..n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(i: usize) -> SoundId {
        SoundId::from_index(i).unwrap()
    }

    #[test]
    fn empty_defaults_yield_none() {
        assert_eq!(RandomSelector::new().choose(&[]), None);
    }

    #[test]
    fn choice_is_within_defaults() {
        let defaults = [id(0), id(1), id(2)];
        let selector = RandomSelector::new();
        for _ in 0..100 {
            let picked = selector.choose(&defaults).unwrap();
            assert!(defaults.contains(&picked));
        }
    }

    #[test]
    fn duplicates_are_preserved_as_weights() {
        let defaults = [id(0), id(0), id(1)];
        let selector = RandomSelector::new();
        for _ in 0..100 {
            assert!(
                selector
                    .choose(&defaults)
                    .is_some_and(|p| p == id(0) || p == id(1))
            );
        }
    }
}
