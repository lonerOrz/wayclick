//! Mouse buttons.
//!
//! [`MouseButton`] is pure domain semantics; evdev `BTN_*`, Windows
//! `WM_XBUTTON*` and macOS button numbers are converted in the platform
//! translators ([`crate::input::translator`]).

/// A standard mouse button.
///
/// `Back` and `Forward` are the X1/X2 side buttons. `Other` preserves a
/// translated-but-unnamed button reported by a platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    /// X1 side button.
    Back,
    /// X2 side button.
    Forward,
    /// A recognised button with no standard name.
    Other(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_and_forward_are_distinct() {
        assert_ne!(MouseButton::Back, MouseButton::Forward);
    }

    #[test]
    fn hash_is_consistent_with_equality() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(MouseButton::Left);
        assert!(set.contains(&MouseButton::Left));
        assert!(!set.contains(&MouseButton::Right));
    }
}
