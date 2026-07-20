//! Domain types: the value-level vocabulary shared across the whole pipeline.
//!
//! Design rules (from the rewrite plan):
//! - `SoundId` is a `NonZeroU16`, never a `usize` and never a raw filename `String`.
//! - Inputs are modeled as a closed `InputEvent` enum, not strings.
//! - `Action` has exactly one variant for v1 (`PlaySound`); more are a seam, not a trait.

use std::num::NonZeroU16;

/// A resolved sound slot. `0` is reserved/unused so `NonZeroU16` is safe.
///
/// `SoundId(1)` is the first sound, `SoundId(2)` the second, etc. The audio
/// cache is a `Vec<Option<Arc<[f32]>>>` indexed by `SoundId.get() - 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SoundId(NonZeroU16);

impl SoundId {
    /// Construct from a 1-based index. Returns `None` for `0` (the "no sound" slot).
    #[inline]
    pub fn new(index: u16) -> Option<SoundId> {
        NonZeroU16::new(index).map(SoundId)
    }

    /// The 1-based index. Use for cache lookups as `get() - 1`.
    #[inline]
    pub fn get(self) -> u16 {
        self.0.get()
    }
}

/// Mouse buttons we care about. Keyboards carry a raw `u16` keycode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    /// X1 / X2 side buttons.
    Back,
    Forward,
    /// Any other button the backend reported but we don't name.
    Other(u16),
}

impl MouseButton {
    /// Map a Linux evdev button code (the `BTN_*` numeric values) to a button.
    ///
    /// This is the canonical mouse-button vocabulary: evdev's `BTN_LEFT` (0x110)
    /// etc. The JSON config reuses the same numbers (272/273/274 = 0x110/1/2),
    /// so config and the evdev backend share this one ladder. Returns `None`
    /// for a code that isn't a button (a keyboard key).
    pub fn from_evdev_code(code: u16) -> Option<MouseButton> {
        match code {
            0x110 => Some(MouseButton::Left),
            0x111 => Some(MouseButton::Right),
            0x112 => Some(MouseButton::Middle),
            0x113 | 0x116 => Some(MouseButton::Back), // BTN_SIDE / BTN_BACK
            0x114 | 0x115 => Some(MouseButton::Forward), // BTN_EXTRA / BTN_FORWARD
            0x117 => Some(MouseButton::Other(code)),  // BTN_TASK: in-range, un-named
            _ => None,
        }
    }

    /// Map a macOS `OtherMouseDown` button number (0..=4) to a button.
    #[allow(dead_code)] // used only by the macOS backend (cfg-gated).
    pub fn from_cg_button_number(n: i64) -> MouseButton {
        match n {
            0 => MouseButton::Left,
            1 => MouseButton::Right,
            2 => MouseButton::Middle,
            3 => MouseButton::Back,
            4 => MouseButton::Forward,
            other => MouseButton::Other(other as u16),
        }
    }

    /// Map a Windows `WM_XBUTTONDOWN` extra-button id (1 or 2) to a button.
    /// 1 = XBUTTON1 (Back), any other value = XBUTTON2 (Forward).
    #[allow(dead_code)] // used only by the Windows backend (cfg-gated).
    pub fn from_windows_xbutton(x_id: u16) -> MouseButton {
        if x_id == 1 {
            MouseButton::Back
        } else {
            MouseButton::Forward
        }
    }
}

/// A normalized input event flowing through the pipeline.
///
/// Backends translate their platform-specific events into this enum. `value`
/// semantics (press/release/repeat) are already collapsed: we only emit `Key`
/// on the *press* edge, so there is no double-play (the macOS Python bug).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputEvent {
    /// A keyboard key, identified by its platform keycode (evdev/Win VK/CG keycode).
    Key(u16),
    /// A mouse button press.
    Mouse(MouseButton),
    /// Something we detected but don't handle (e.g. a trackpad, a repeat).
    Ignored,
}

/// An action the executor can perform. v1 has exactly one variant.
///
/// New variants (Notify, Http, Mqtt) are a *seam*: add them here when needed,
/// do not introduce an `Action` trait — there is only one executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    PlaySound(SoundId),
}

/// A single rule: when `trigger` fires, run `actions` (in order).
///
/// `trigger` is a `InputEvent`; the `CompiledRule` already resolved any
/// filename in the JSON to a `SoundId`, so the hot path does no string lookups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledRule {
    pub trigger: InputEvent,
    pub actions: Vec<Action>,
}

impl CompiledRule {
    pub fn new(trigger: InputEvent, actions: Vec<Action>) -> CompiledRule {
        CompiledRule { trigger, actions }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evdev_code_ladder() {
        assert_eq!(MouseButton::from_evdev_code(0x110), Some(MouseButton::Left));
        assert_eq!(
            MouseButton::from_evdev_code(0x111),
            Some(MouseButton::Right)
        );
        assert_eq!(
            MouseButton::from_evdev_code(0x112),
            Some(MouseButton::Middle)
        );
        assert_eq!(MouseButton::from_evdev_code(0x113), Some(MouseButton::Back)); // SIDE
        assert_eq!(MouseButton::from_evdev_code(0x116), Some(MouseButton::Back)); // BACK
        assert_eq!(
            MouseButton::from_evdev_code(0x114),
            Some(MouseButton::Forward)
        ); // EXTRA
        assert_eq!(
            MouseButton::from_evdev_code(0x115),
            Some(MouseButton::Forward)
        ); // FORWARD
        // A keyboard key is not a button.
        assert_eq!(MouseButton::from_evdev_code(30), None); // KEY_A
    }

    #[test]
    fn cg_and_windows_ladders() {
        assert_eq!(MouseButton::from_cg_button_number(3), MouseButton::Back);
        assert_eq!(MouseButton::from_cg_button_number(9), MouseButton::Other(9));
        assert_eq!(MouseButton::from_windows_xbutton(1), MouseButton::Back);
        assert_eq!(MouseButton::from_windows_xbutton(2), MouseButton::Forward);
    }
}
