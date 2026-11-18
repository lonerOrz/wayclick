//! Physical keyboard keys.
//!
//! [`PhysicalKey`] models the *physical position* of a key (USB HID usage
//! page 0x07 semantics), not the character a layout produces: the key
//! labelled `Q` on AZERTY is [`PhysicalKey::A`]. Platform-native codes are
//! converted by [`crate::input::translator`]; unrecognised codes are kept as
//! [`PhysicalKey::Unknown`] with their origin, so no input is silently lost.

use crate::domain::Platform;

/// A physical key position, independent of keyboard layout and platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum PhysicalKey {
    // Letters (physical QWERTY positions).
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    // Digit row.
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Digit0,

    // Function row.
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,

    // Control / editing.
    Enter,
    Escape,
    Space,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,

    // Arrow cluster.
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    // Modifiers (left/right are distinct physical keys).
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    LeftMeta,
    RightMeta,

    // Locks / system.
    CapsLock,
    NumLock,
    ScrollLock,
    PrintScreen,
    Pause,

    // Punctuation / symbols.
    Minus,
    Equal,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Apostrophe,
    Grave,
    Comma,
    Period,
    Slash,

    // Numpad.
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadEnter,
    NumpadDecimal,

    // International / extended.
    IntlBackslash,
    IntlYen,
    Application,

    /// An unrecognised native code, kept with its platform and raw value so
    /// it is never silently dropped and cannot collide across platforms.
    Unknown {
        platform: Platform,
        code: u32,
    },
}

impl PhysicalKey {
    /// Whether this is a key the domain does not recognise.
    #[must_use]
    pub fn is_unknown(self) -> bool {
        matches!(self, Self::Unknown { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_are_distinct() {
        assert_ne!(PhysicalKey::LeftShift, PhysicalKey::RightShift);
        assert_ne!(PhysicalKey::LeftCtrl, PhysicalKey::RightCtrl);
        assert_ne!(PhysicalKey::LeftAlt, PhysicalKey::RightAlt);
        assert_ne!(PhysicalKey::LeftMeta, PhysicalKey::RightMeta);
    }

    #[test]
    fn unknown_carries_platform_and_code() {
        let a = PhysicalKey::Unknown {
            platform: Platform::Linux,
            code: 9999,
        };
        let b = PhysicalKey::Unknown {
            platform: Platform::Windows,
            code: 9999,
        };
        assert_ne!(a, b);
        assert!(a.is_unknown());
        assert!(!PhysicalKey::A.is_unknown());
    }

    #[test]
    fn hash_is_consistent_with_equality() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(PhysicalKey::A);
        assert!(set.contains(&PhysicalKey::A));
        assert!(!set.contains(&PhysicalKey::B));
    }
}
