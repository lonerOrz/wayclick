//! Trigger parsing: convert config keys into domain input events.
//!
//! Two forms are accepted:
//!   * A bare number is a historical evdev key code, routed through the Linux
//!     translator so old configurations keep their meaning.
//!   * `<platform>:<code>` selects a platform-native code, routed through that
//!     platform's translator. `platform` is one of `linux`, `win`/`windows`,
//!     `mac`/`macos`. This lets a config target keys the domain does not know
//!     by name on a specific OS.

use crate::domain::InputEvent;
use crate::input::translator;

/// Parse a mapping key into an input event; `None` for keys that cannot be
/// interpreted (the caller emits a diagnostic instead of dropping silently).
pub fn parse_trigger(key: &str) -> Option<InputEvent> {
    if let Some((platform, code)) = key.split_once(':') {
        let code = parse_code(code)?;
        return match platform {
            "linux" | "evdev" => Some(translator::linux::from_config_code(code)),
            "win" | "windows" => Some(translator::windows::from_config_key(code)),
            "mac" | "macos" => Some(translator::macos::from_config_key(code)),
            _ => None,
        };
    }
    let code = parse_code(key)?;
    Some(translator::linux::from_config_code(code))
}
/// Parse a code as decimal, or hexadecimal when prefixed with `0x`.
pub(crate) fn parse_code(text: &str) -> Option<u16> {
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u16::from_str_radix(hex, 16).ok()
    } else {
        text.parse::<u16>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MouseButton, PhysicalKey, Platform};

    #[test]
    fn numeric_key_is_evdev_code() {
        assert_eq!(
            parse_trigger("1"),
            Some(InputEvent::KeyDown(PhysicalKey::Escape))
        );
        assert_eq!(
            parse_trigger("272"),
            Some(InputEvent::MouseDown(MouseButton::Left))
        );
    }

    #[test]
    fn non_numeric_key_is_rejected() {
        assert_eq!(parse_trigger("KeyA"), None);
        assert_eq!(parse_trigger(""), None);
        assert_eq!(parse_trigger("70000"), None);
    }

    #[test]
    fn platform_prefixed_key_uses_native_translator() {
        // Windows scan code 0x1E is the A key, independent of evdev code 30.
        assert_eq!(
            parse_trigger("win:0x1E"),
            Some(InputEvent::KeyDown(PhysicalKey::A))
        );
        // macOS virtual keycode 0 is A.
        assert_eq!(
            parse_trigger("mac:0"),
            Some(InputEvent::KeyDown(PhysicalKey::A))
        );
        assert_eq!(
            parse_trigger("linux:30"),
            Some(InputEvent::KeyDown(PhysicalKey::A))
        );
    }

    #[test]
    fn platform_prefixed_unknown_keeps_origin() {
        assert_eq!(
            parse_trigger("win:0x123"),
            Some(InputEvent::KeyDown(PhysicalKey::Unknown {
                platform: Platform::Windows,
                code: 0x123,
            }))
        );
        assert_eq!(
            parse_trigger("mac:200"),
            Some(InputEvent::KeyDown(PhysicalKey::Unknown {
                platform: Platform::MacOS,
                code: 200,
            }))
        );
    }

    #[test]
    fn unknown_platform_prefix_is_rejected() {
        assert_eq!(parse_trigger("bsd:1"), None);
        assert_eq!(parse_trigger("win:notanumber"), None);
    }

    #[test]
    fn hex_bare_code_is_supported() {
        assert_eq!(
            parse_trigger("0x1"),
            Some(InputEvent::KeyDown(PhysicalKey::Escape))
        );
    }
}
