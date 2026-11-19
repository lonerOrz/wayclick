//! Domain actions.

use crate::domain::SoundId;

/// An executable action; a closed, compiler-checked set, so no trait object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    PlaySound(SoundId),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_sound_carries_the_id() {
        let id = SoundId::from_index(0).unwrap();
        let action = Action::PlaySound(id);
        match action {
            Action::PlaySound(got) => assert_eq!(got, id),
        }
    }
}
