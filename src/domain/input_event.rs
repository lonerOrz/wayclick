//! Normalised input events.

use crate::domain::{MouseButton, PhysicalKey};

/// A press-edge input event; one physical press, acted on every time it is
/// received. Backends translate native events into this and drop what they
/// do not want (trackpads, auto-repeats, releases).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InputEvent {
    KeyDown(PhysicalKey),
    MouseDown(MouseButton),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_and_mouse_events_are_distinct() {
        assert_ne!(
            InputEvent::KeyDown(PhysicalKey::A),
            InputEvent::MouseDown(MouseButton::Left)
        );
    }
}
