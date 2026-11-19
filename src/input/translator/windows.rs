//! Windows translator.
//!
//! Prefers scan codes (`KBDLLHOOKSTRUCT.scanCode`) over virtual keys because
//! scan codes encode physical position. The `extended` flag disambiguates
//! left/right variants of Ctrl, Alt and the arrow/navigation cluster.

use crate::domain::{InputEvent, MouseButton, PhysicalKey, Platform};

/// Translate a Windows scan code plus its `LLKHF_EXTENDED` flag into a
/// physical key; unknown combinations become [`PhysicalKey::Unknown`] tagged
/// with [`Platform::Windows`].
#[must_use]
pub fn translate_key(scan_code: u16, extended: bool) -> PhysicalKey {
    use PhysicalKey::*;

    // Extended scan codes (E0 prefix, surfaced as 0x100.. by windows-sys when
    // the extended bit is set) — arrows, navigation, right modifiers.
    if extended {
        return match scan_code {
            0x1C => NumpadEnter,
            0x1D => RightCtrl,
            0x35 => NumpadDivide,
            0x37 => PrintScreen,
            0x38 => RightAlt,
            0x47 => Home,
            0x48 => ArrowUp,
            0x49 => PageUp,
            0x4B => ArrowLeft,
            0x4D => ArrowRight,
            0x4F => End,
            0x50 => ArrowDown,
            0x51 => PageDown,
            0x52 => Insert,

            0x53 => Delete,
            0x5B => LeftMeta,
            0x5C => RightMeta,
            0x5D => Application,
            _ => Unknown {
                platform: Platform::Windows,
                code: u32::from(scan_code) | 0x1_0000,
            },
        };
    }

    match scan_code {
        0x01 => Escape,
        0x02 => Digit1,
        0x03 => Digit2,
        0x04 => Digit3,
        0x05 => Digit4,
        0x06 => Digit5,
        0x07 => Digit6,
        0x08 => Digit7,
        0x09 => Digit8,
        0x0A => Digit9,
        0x0B => Digit0,
        0x0C => Minus,
        0x0D => Equal,
        0x0E => Backspace,
        0x0F => Tab,
        0x10 => Q,
        0x11 => W,
        0x12 => E,
        0x13 => R,
        0x14 => T,
        0x15 => Y,
        0x16 => U,
        0x17 => I,
        0x18 => O,
        0x19 => P,
        0x1A => LeftBracket,
        0x1B => RightBracket,
        0x1C => Enter,
        0x1D => LeftCtrl,
        0x1E => A,
        0x1F => S,
        0x20 => D,
        0x21 => F,
        0x22 => G,
        0x23 => H,
        0x24 => J,
        0x25 => K,
        0x26 => L,
        0x27 => Semicolon,
        0x28 => Apostrophe,
        0x29 => Grave,
        0x2A => LeftShift,
        0x2B => Backslash,
        0x2C => Z,
        0x2D => X,
        0x2E => C,
        0x2F => V,
        0x30 => B,
        0x31 => N,
        0x32 => M,
        0x33 => Comma,
        0x34 => Period,
        0x35 => Slash,
        0x36 => RightShift,
        0x37 => NumpadMultiply,
        0x38 => LeftAlt,
        0x39 => Space,
        0x3A => CapsLock,
        0x3B..=0x44 => {
            let n = scan_code - 0x3A;
            match n {
                1..=10 => F_KEYS[(n - 1) as usize],
                _ => unreachable!("3B..=44 maps to F1..=F10"),
            }
        }
        0x45 => NumLock,
        0x46 => ScrollLock,
        0x47 => Numpad7,
        0x48 => Numpad8,
        0x49 => Numpad9,
        0x4A => NumpadSubtract,
        0x4B => Numpad4,
        0x4C => Numpad5,
        0x4D => Numpad6,
        0x4E => NumpadAdd,
        0x4F => Numpad1,
        0x50 => Numpad2,
        0x51 => Numpad3,
        0x52 => Numpad0,
        0x53 => NumpadDecimal,
        0x56 => IntlBackslash,
        0x57 => F11,
        0x58 => F12,
        _ => Unknown {
            platform: Platform::Windows,
            code: u32::from(scan_code),
        },
    }
}

/// F1..=F10 for the 0x3B..=0x44 scan-code range.
const F_KEYS: [PhysicalKey; 10] = [
    PhysicalKey::F1,
    PhysicalKey::F2,
    PhysicalKey::F3,
    PhysicalKey::F4,
    PhysicalKey::F5,
    PhysicalKey::F6,
    PhysicalKey::F7,
    PhysicalKey::F8,
    PhysicalKey::F9,
    PhysicalKey::F10,
];

/// Translate a Windows `WM_XBUTTONDOWN` extra-button id (1 = X1/Back,
/// 2 = X2/Forward); other values are invalid.
#[must_use]
pub fn translate_xbutton(x_id: u16) -> Option<MouseButton> {
    match x_id {
        1 => Some(MouseButton::Back),
        2 => Some(MouseButton::Forward),
        _ => None,
    }
}

/// Translate a Windows mouse message into a mouse button.
#[must_use]
pub fn translate_mouse_message(message: u32, x_id: u16) -> Option<MouseButton> {
    // Win32 WM_* constants (winuser.h).
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_RBUTTONDOWN: u32 = 0x0204;
    const WM_MBUTTONDOWN: u32 = 0x0207;
    const WM_XBUTTONDOWN: u32 = 0x020B;
    match message {
        WM_LBUTTONDOWN => Some(MouseButton::Left),
        WM_RBUTTONDOWN => Some(MouseButton::Right),
        WM_MBUTTONDOWN => Some(MouseButton::Middle),
        WM_XBUTTONDOWN => translate_xbutton(x_id),
        _ => None,
    }
}

/// Build a key-down event from a scan code and extended flag.
#[must_use]
pub fn key_event(scan_code: u16, extended: bool) -> InputEvent {
    InputEvent::KeyDown(translate_key(scan_code, extended))
}

/// Translate a configuration key code into a key-down event. Config codes
/// are plain scan codes (non-extended); extended keys are configured via
/// their physical-key meaning, not a raw code.
#[must_use]
pub fn from_config_key(code: u16) -> InputEvent {
    InputEvent::KeyDown(translate_key(code, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_right_modifiers_via_extended() {
        assert_eq!(translate_key(0x1D, false), PhysicalKey::LeftCtrl);
        assert_eq!(translate_key(0x1D, true), PhysicalKey::RightCtrl);
        assert_eq!(translate_key(0x38, false), PhysicalKey::LeftAlt);
        assert_eq!(translate_key(0x38, true), PhysicalKey::RightAlt);
    }

    #[test]
    fn arrows_are_extended() {
        assert_eq!(translate_key(0x48, true), PhysicalKey::ArrowUp);
        assert_eq!(translate_key(0x4B, true), PhysicalKey::ArrowLeft);
    }

    #[test]
    fn meta_and_application_are_extended() {
        assert_eq!(translate_key(0x5B, true), PhysicalKey::LeftMeta);
        assert_eq!(translate_key(0x5C, true), PhysicalKey::RightMeta);
        assert_eq!(translate_key(0x5D, true), PhysicalKey::Application);
    }

    #[test]
    fn invalid_xbutton_is_rejected() {
        assert_eq!(translate_xbutton(1), Some(MouseButton::Back));
        assert_eq!(translate_xbutton(2), Some(MouseButton::Forward));
        assert_eq!(translate_xbutton(3), None);
        assert_eq!(translate_xbutton(0), None);
    }

    #[test]
    fn unknown_scan_code() {
        assert_eq!(
            translate_key(0xFE, false),
            PhysicalKey::Unknown {
                platform: Platform::Windows,
                code: 0xFE,
            }
        );
    }
}
