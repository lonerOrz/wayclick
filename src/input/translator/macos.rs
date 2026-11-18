//! macOS translator; owns Apple's legacy virtual keycode layout (different
//! from both USB HID and scan codes).

use crate::domain::{InputEvent, MouseButton, PhysicalKey, Platform};

/// Translate a CGKeyCode into a physical key; unknown keycodes become
/// [`PhysicalKey::Unknown`] tagged with [`Platform::MacOS`].
#[must_use]
pub fn translate_key(code: u16) -> PhysicalKey {
    use PhysicalKey::*;
    match code {
        0 => A,
        1 => S,
        2 => D,
        3 => F,
        4 => H,
        5 => G,
        6 => Z,
        7 => X,
        8 => C,
        9 => V,
        11 => B,
        12 => Q,
        13 => W,
        14 => E,
        15 => R,
        16 => Y,
        17 => T,
        18 => Digit1,
        19 => Digit2,
        20 => Digit3,
        21 => Digit4,
        22 => Digit6,
        23 => Digit5,
        24 => Equal,
        25 => Digit9,
        26 => Digit7,
        27 => Minus,
        28 => Digit8,
        29 => Digit0,
        30 => RightBracket,
        31 => O,
        32 => U,
        33 => LeftBracket,
        34 => I,
        35 => P,
        36 => Enter,
        37 => L,
        38 => J,
        39 => Apostrophe,
        40 => K,
        41 => Semicolon,
        42 => Backslash,
        43 => Comma,
        44 => Slash,
        45 => N,
        46 => M,
        47 => Period,
        48 => Tab,
        49 => Space,
        50 => Grave,
        51 => Backspace,
        53 => Escape,
        55 => LeftMeta,
        56 => LeftShift,
        57 => CapsLock,
        58 => LeftAlt,
        59 => LeftCtrl,
        60 => RightShift,
        61 => RightAlt,
        62 => RightCtrl,
        63 => RightMeta,
        65 => NumpadDecimal,
        67 => NumpadMultiply,
        69 => NumpadAdd,
        71 => NumLock,
        75 => NumpadDivide,
        76 => NumpadEnter,
        78 => NumpadSubtract,
        82 => Numpad0,
        83 => Numpad1,
        84 => Numpad2,
        85 => Numpad3,
        86 => Numpad4,
        87 => Numpad5,
        88 => Numpad6,
        89 => Numpad7,
        90 => F20,
        91 => Numpad8,
        92 => Numpad9,
        96 => F5,
        97 => F6,
        98 => F7,
        99 => F3,
        100 => F8,
        101 => F9,
        103 => F11,
        109 => F10,
        111 => F12,
        114 => Insert,
        115 => Home,
        116 => PageUp,
        117 => Delete,
        118 => F4,
        119 => End,
        120 => F2,
        121 => PageDown,
        122 => F1,
        123 => ArrowLeft,
        124 => ArrowRight,
        125 => ArrowDown,
        126 => ArrowUp,
        64 => F17,
        79 => F18,
        80 => F19,
        105 => F13,
        106 => F16,
        107 => F14,
        113 => F15,
        _ => Unknown {
            platform: Platform::MacOS,
            code: u32::from(code),
        },
    }
}

/// Translate a macOS `OtherMouseDown` button number into a mouse button.
#[must_use]
pub fn translate_button_number(n: i64) -> MouseButton {
    match n {
        0 => MouseButton::Left,
        1 => MouseButton::Right,
        2 => MouseButton::Middle,
        3 => MouseButton::Back,
        4 => MouseButton::Forward,
        other => MouseButton::Other(other as u32),
    }
}

/// Build a key-down event from a CGKeyCode.
#[must_use]
pub fn key_event(code: u16) -> InputEvent {
    InputEvent::KeyDown(translate_key(code))
}

/// Translate a configuration key code (a CGKeyCode) into a key-down event.
#[must_use]
pub fn from_config_key(code: u16) -> InputEvent {
    InputEvent::KeyDown(translate_key(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_arrows() {
        assert_eq!(translate_key(0), PhysicalKey::A);
        assert_eq!(translate_key(6), PhysicalKey::Z);
        assert_eq!(translate_key(126), PhysicalKey::ArrowUp);
    }

    #[test]
    fn modifiers() {
        assert_eq!(translate_key(56), PhysicalKey::LeftShift);
        assert_eq!(translate_key(60), PhysicalKey::RightShift);
        assert_eq!(translate_key(59), PhysicalKey::LeftCtrl);
    }

    #[test]
    fn function_keys_and_keypad_are_not_mis_mapped() {
        // 0x5A..0x5C: F20, keypad 8, keypad 9 (previously shifted by one).
        assert_eq!(translate_key(90), PhysicalKey::F20);
        assert_eq!(translate_key(91), PhysicalKey::Numpad8);
        assert_eq!(translate_key(92), PhysicalKey::Numpad9);
        // F13..F20 top-row and laptop function keys.
        assert_eq!(translate_key(64), PhysicalKey::F17);
        assert_eq!(translate_key(79), PhysicalKey::F18);
        assert_eq!(translate_key(80), PhysicalKey::F19);
        assert_eq!(translate_key(105), PhysicalKey::F13);
        assert_eq!(translate_key(106), PhysicalKey::F16);
        assert_eq!(translate_key(107), PhysicalKey::F14);
        assert_eq!(translate_key(113), PhysicalKey::F15);
    }

    #[test]
    fn buttons() {
        assert_eq!(translate_button_number(3), MouseButton::Back);
        assert_eq!(translate_button_number(4), MouseButton::Forward);
        assert_eq!(translate_button_number(9), MouseButton::Other(9));
    }

    #[test]
    fn unknown_key_code() {
        assert_eq!(
            translate_key(60000),
            PhysicalKey::Unknown {
                platform: Platform::MacOS,
                code: 60000,
            }
        );
    }
}
