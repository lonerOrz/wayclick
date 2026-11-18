//! Linux (evdev) translator; owns every evdev constant.

use crate::domain::{InputEvent, MouseButton, PhysicalKey, Platform};

/// Translate an evdev key code into a physical key. Codes follow
/// `linux/input-event-codes.h`; unknown codes become [`PhysicalKey::Unknown`]
/// tagged with [`Platform::Linux`].
pub fn translate_key(code: u16) -> PhysicalKey {
    use PhysicalKey::*;
    match code {
        1 => Escape,
        2 => Digit1,
        3 => Digit2,
        4 => Digit3,
        5 => Digit4,
        6 => Digit5,
        7 => Digit6,
        8 => Digit7,
        9 => Digit8,
        10 => Digit9,
        11 => Digit0,
        12 => Minus,
        13 => Equal,
        14 => Backspace,
        15 => Tab,
        16 => Q,
        17 => W,
        18 => E,
        19 => R,
        20 => T,
        21 => Y,
        22 => U,
        23 => I,
        24 => O,
        25 => P,
        26 => LeftBracket,
        27 => RightBracket,
        28 => Enter,
        29 => LeftCtrl,
        30 => A,
        31 => S,
        32 => D,
        33 => F,
        34 => G,
        35 => H,
        36 => J,
        37 => K,
        38 => L,
        39 => Semicolon,
        40 => Apostrophe,
        41 => Grave,
        42 => LeftShift,
        43 => Backslash,
        44 => Z,
        45 => X,
        46 => C,
        47 => V,
        48 => B,
        49 => N,
        50 => M,
        51 => Comma,
        52 => Period,
        53 => Slash,
        54 => RightShift,
        55 => NumpadMultiply,
        56 => LeftAlt,
        57 => Space,
        58 => CapsLock,
        59..=68 => {
            let n = code - 58;
            match n {
                1 => F1,
                2 => F2,
                3 => F3,
                4 => F4,
                5 => F5,
                6 => F6,
                7 => F7,
                8 => F8,
                9 => F9,
                10 => F10,
                _ => unreachable!("range 59..=68 maps to F1..=F10"),
            }
        }
        69 => NumLock,
        70 => ScrollLock,
        71 => Numpad7,
        72 => Numpad8,
        73 => Numpad9,
        74 => NumpadSubtract,
        75 => Numpad4,
        76 => Numpad5,
        77 => Numpad6,
        78 => NumpadAdd,
        79 => Numpad1,
        80 => Numpad2,
        81 => Numpad3,
        82 => Numpad0,
        83 => NumpadDecimal,
        86 => IntlBackslash,
        87 => F11,
        88 => F12,
        96 => NumpadEnter,
        97 => RightCtrl,
        98 => NumpadDivide,
        99 => PrintScreen,
        100 => RightAlt,
        102 => Home,
        103 => ArrowUp,
        104 => PageUp,
        105 => ArrowLeft,
        106 => ArrowRight,
        107 => End,
        108 => ArrowDown,
        109 => PageDown,
        110 => Insert,
        111 => Delete,
        125 => LeftMeta,
        126 => RightMeta,
        127 => Application,
        183..=194 => {
            let n = code - 182;
            match n {
                1..=12 => {
                    // F13..=F24 — reconstruct via a fixed table (see const below).
                    FUNCTION_KEYS[(n - 1) as usize]
                }
                _ => unreachable!("range 183..=194 maps to F13..=F24"),
            }
        }
        _ => Unknown {
            platform: Platform::Linux,
            code: u32::from(code),
        },
    }
}

const FUNCTION_KEYS: [PhysicalKey; 12] = [
    PhysicalKey::F13,
    PhysicalKey::F14,
    PhysicalKey::F15,
    PhysicalKey::F16,
    PhysicalKey::F17,
    PhysicalKey::F18,
    PhysicalKey::F19,
    PhysicalKey::F20,
    PhysicalKey::F21,
    PhysicalKey::F22,
    PhysicalKey::F23,
    PhysicalKey::F24,
];

/// Translate an evdev `BTN_*` code into a mouse button, or `None` if the code
/// is not a recognised button.
#[must_use]
pub fn translate_mouse(code: u16) -> Option<MouseButton> {
    match code {
        0x110 => Some(MouseButton::Left),                   // BTN_LEFT
        0x111 => Some(MouseButton::Right),                  // BTN_RIGHT
        0x112 => Some(MouseButton::Middle),                 // BTN_MIDDLE
        0x113 | 0x116 => Some(MouseButton::Back),           // BTN_SIDE / BTN_BACK
        0x114 | 0x115 => Some(MouseButton::Forward),        // BTN_EXTRA / BTN_FORWARD
        0x117 => Some(MouseButton::Other(u32::from(code))), // BTN_TASK
        _ => None,
    }
}

/// Translate a numeric configuration key (an evdev code) into an input event;
/// mouse button codes first, every other code a keyboard key.
#[must_use]
pub fn from_config_code(code: u16) -> InputEvent {
    match translate_mouse(code) {
        Some(button) => InputEvent::MouseDown(button),
        None => InputEvent::KeyDown(translate_key(code)),
    }
}

/// Translate a live evdev key code into an input event.
#[must_use]
pub fn from_event_code(code: u16) -> InputEvent {
    from_config_code(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_a_and_space() {
        assert_eq!(translate_key(30), PhysicalKey::A);
        assert_eq!(translate_key(57), PhysicalKey::Space);
    }

    #[test]
    fn mouse_codes() {
        assert_eq!(
            from_config_code(0x110),
            InputEvent::MouseDown(MouseButton::Left)
        );
        assert_eq!(
            from_config_code(0x113),
            InputEvent::MouseDown(MouseButton::Back)
        );
    }

    #[test]
    fn unknown_code_keeps_origin() {
        assert_eq!(
            translate_key(65000),
            PhysicalKey::Unknown {
                platform: Platform::Linux,
                code: 65000,
            }
        );
    }

    #[test]
    fn function_keys() {
        assert_eq!(translate_key(59), PhysicalKey::F1);
        assert_eq!(translate_key(87), PhysicalKey::F11);
        assert_eq!(translate_key(183), PhysicalKey::F13);
        assert_eq!(translate_key(194), PhysicalKey::F24);
    }
}
