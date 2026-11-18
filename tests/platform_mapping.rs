//! Mapping-integrity checks across all three platform translators.
//!
//! Each platform encodes physical keys with different native codes. These
//! tests pin that the translators agree on the resulting [`PhysicalKey`] for a
//! representative set of standard keys, and that unknown codes keep their
//! platform origin (so they can be configured and never silently collide).

use wayclick::domain::{MouseButton, PhysicalKey, Platform};
use wayclick::input::translator::{linux, macos, windows};

/// A physical key plus the native code that selects it on each platform.
struct NativeCodes {
    linux: u16,
    windows: u16,
    windows_extended: bool,
    macos: u16,
}

fn assert_all_platforms(key: PhysicalKey, codes: NativeCodes) {
    assert_eq!(linux::translate_key(codes.linux), key, "linux mapping");
    assert_eq!(
        windows::translate_key(codes.windows, codes.windows_extended),
        key,
        "windows mapping"
    );
    assert_eq!(macos::translate_key(codes.macos), key, "macos mapping");
}

#[test]
fn letters_map_consistently_across_platforms() {
    // Physical A: evdev 30, Windows scan 0x1E, macOS virtual key 0.
    assert_all_platforms(
        PhysicalKey::A,
        NativeCodes {
            linux: 30,
            windows: 0x1E,
            windows_extended: false,
            macos: 0,
        },
    );
    // Physical Z: evdev 44, Windows scan 0x2C, macOS virtual key 6.
    assert_all_platforms(
        PhysicalKey::Z,
        NativeCodes {
            linux: 44,
            windows: 0x2C,
            windows_extended: false,
            macos: 6,
        },
    );
}

#[test]
fn digits_map_consistently_across_platforms() {
    // Digit 1: evdev 2, Windows scan 0x02, macOS virtual key 18.
    assert_all_platforms(
        PhysicalKey::Digit1,
        NativeCodes {
            linux: 2,
            windows: 0x02,
            windows_extended: false,
            macos: 18,
        },
    );
}

#[test]
fn modifiers_map_consistently_across_platforms() {
    // Left Shift: evdev 42, Windows scan 0x2A, macOS virtual key 56.
    assert_all_platforms(
        PhysicalKey::LeftShift,
        NativeCodes {
            linux: 42,
            windows: 0x2A,
            windows_extended: false,
            macos: 56,
        },
    );
    // Right Shift: evdev 54, Windows scan 0x36, macOS virtual key 60.
    assert_all_platforms(
        PhysicalKey::RightShift,
        NativeCodes {
            linux: 54,
            windows: 0x36,
            windows_extended: false,
            macos: 60,
        },
    );
}

#[test]
fn space_and_enter_map_consistently_across_platforms() {
    assert_all_platforms(
        PhysicalKey::Space,
        NativeCodes {
            linux: 57,
            windows: 0x39,
            windows_extended: false,
            macos: 49,
        },
    );
    assert_all_platforms(
        PhysicalKey::Enter,
        NativeCodes {
            linux: 28,
            windows: 0x1C,
            windows_extended: false,
            macos: 36,
        },
    );
}

#[test]
fn unknown_codes_keep_platform_origin() {
    assert_eq!(
        linux::translate_key(700),
        PhysicalKey::Unknown {
            platform: Platform::Linux,
            code: 700
        }
    );
    assert_eq!(
        windows::translate_key(0xFE, false),
        PhysicalKey::Unknown {
            platform: Platform::Windows,
            code: 0xFE
        }
    );
    assert_eq!(
        macos::translate_key(200),
        PhysicalKey::Unknown {
            platform: Platform::MacOS,
            code: 200
        }
    );
}

#[test]
fn distinct_platforms_unknowns_do_not_collide() {
    // Same numeric code on different platforms must not compare equal.
    let linux = PhysicalKey::Unknown {
        platform: Platform::Linux,
        code: 999,
    };
    let windows = PhysicalKey::Unknown {
        platform: Platform::Windows,
        code: 999,
    };
    let macos = PhysicalKey::Unknown {
        platform: Platform::MacOS,
        code: 999,
    };
    assert_ne!(linux, windows);
    assert_ne!(windows, macos);
    assert_ne!(linux, macos);
}

#[test]
fn mouse_buttons_translate_to_domain_variants() {
    // evdev: BTN_LEFT=0x110, BTN_RIGHT=0x111, BTN_MIDDLE=0x112.
    assert_eq!(linux::translate_mouse(0x110), Some(MouseButton::Left));
    assert_eq!(linux::translate_mouse(0x111), Some(MouseButton::Right));
    assert_eq!(linux::translate_mouse(0x112), Some(MouseButton::Middle));
    // Windows X1/X2.
    assert_eq!(windows::translate_xbutton(1), Some(MouseButton::Back));
    assert_eq!(windows::translate_xbutton(2), Some(MouseButton::Forward));
    // macOS button numbers.
    assert_eq!(macos::translate_button_number(0), MouseButton::Left);
    assert_eq!(macos::translate_button_number(3), MouseButton::Back);
}
