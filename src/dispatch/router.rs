//! Event routing: `InputEvent` → action, with O(1) average lookup.

use rustc_hash::{FxBuildHasher, FxHashMap};

use crate::domain::{Action, CompiledRule, InputEvent, SoundId};

/// Immutable rule index; defaults are the fallback for unmatched events.
#[derive(Debug, Clone)]
pub struct Router {
    routes: FxHashMap<InputEvent, Action>,
    defaults: Box<[SoundId]>,
}

impl Router {
    /// Build a router; returns `Err` with the duplicate event if two rules
    /// share a trigger.
    pub fn new(rules: Vec<CompiledRule>, defaults: Vec<SoundId>) -> Result<Self, InputEvent> {
        let mut routes = FxHashMap::with_capacity_and_hasher(rules.len(), FxBuildHasher);
        for rule in rules {
            if routes.insert(rule.trigger(), rule.action()).is_some() {
                return Err(rule.trigger());
            }
        }
        Ok(Self {
            routes,
            defaults: defaults.into_boxed_slice(),
        })
    }

    /// Look up the action for an event.
    #[must_use]
    pub fn dispatch(&self, event: InputEvent) -> Option<&Action> {
        self.routes.get(&event)
    }

    /// Default sound ids, in declaration order (duplicates = weight).
    #[must_use]
    pub fn defaults(&self) -> &[SoundId] {
        &self.defaults
    }

    pub fn has_defaults(&self) -> bool {
        !self.defaults.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{PhysicalKey, SoundId};

    fn rule(event: InputEvent, id: usize) -> CompiledRule {
        CompiledRule::new(event, Action::PlaySound(SoundId::from_index(id).unwrap()))
    }

    #[test]
    fn routes_an_event() {
        let event = InputEvent::KeyDown(PhysicalKey::A);
        let router = Router::new(vec![rule(event, 0)], vec![]).unwrap();
        assert_eq!(
            router.dispatch(event),
            Some(&Action::PlaySound(SoundId::from_index(0).unwrap()))
        );
    }

    #[test]
    fn no_match_returns_none() {
        let router =
            Router::new(vec![rule(InputEvent::KeyDown(PhysicalKey::A), 0)], vec![]).unwrap();
        assert_eq!(router.dispatch(InputEvent::KeyDown(PhysicalKey::B)), None);
    }

    #[test]
    fn duplicate_trigger_is_rejected() {
        let event = InputEvent::KeyDown(PhysicalKey::A);
        let result = Router::new(vec![rule(event, 0), rule(event, 1)], vec![]);
        assert_eq!(result.err(), Some(event));
    }
}
