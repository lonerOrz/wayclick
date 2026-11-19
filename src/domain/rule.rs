//! A compiled trigger-to-action rule.

use crate::domain::{Action, InputEvent};

/// An immutable rule produced by config compilation; private fields keep it
/// out of invalid states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledRule {
    trigger: InputEvent,
    action: Action,
}

impl CompiledRule {
    /// Build a rule that plays one sound when `trigger` fires.
    #[must_use]
    pub fn new(trigger: InputEvent, action: Action) -> Self {
        Self { trigger, action }
    }

    #[must_use]
    pub fn trigger(&self) -> InputEvent {
        self.trigger
    }

    #[must_use]
    pub fn action(&self) -> Action {
        self.action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{PhysicalKey, SoundId};

    #[test]
    fn constructor_and_accessors() {
        let id = SoundId::from_index(0).unwrap();
        let action = Action::PlaySound(id);
        let rule = CompiledRule::new(InputEvent::KeyDown(PhysicalKey::A), action);
        assert_eq!(rule.trigger(), InputEvent::KeyDown(PhysicalKey::A));
        assert_eq!(rule.action(), action);
    }
}
