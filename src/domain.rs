//! Domain types: the value vocabulary shared across the whole pipeline.

use std::num::NonZeroU16;

/// A resolved sound slot. `0` is reserved, hence `NonZeroU16`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SoundId(NonZeroU16);

impl SoundId {
    /// 1-based index; `None` for `0` (the "no sound" slot).
    #[inline]
    pub fn new(index: u16) -> Option<SoundId> {
        NonZeroU16::new(index).map(SoundId)
    }

    /// The 1-based index; cache lookups use `get() - 1`.
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
    /// Map a Linux evdev `BTN_*` code to a button. The JSON config reuses the
    /// same numbers, so config and the evdev backend share this one ladder.
    /// `None` for a code that isn't a button.
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
    pub fn from_windows_xbutton(x_id: u16) -> MouseButton {
        if x_id == 1 {
            MouseButton::Back
        } else {
            MouseButton::Forward
        }
    }
}

/// A normalized input event. Backends translate native events into this and drop
/// what they don't want (trackpads, auto-repeats), so every event the pipeline
/// sees should be acted on. Only the press edge is emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputEvent {
    /// A keyboard key, by platform keycode (evdev / Win VK / CG keycode).
    Key(u16),
    /// A mouse button press.
    Mouse(MouseButton),
}

impl InputEvent {
    /// Map a numeric keycode: evdev `BTN_*` numbers become `Mouse`, else `Key`.
    pub fn from_evdev_code(code: u16) -> InputEvent {
        match MouseButton::from_evdev_code(code) {
            Some(button) => InputEvent::Mouse(button),
            None => InputEvent::Key(code),
        }
    }
}

/// An action the executor can perform. Add variants here, not an `Action` trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    PlaySound(SoundId),
}

/// When `trigger` fires, run `actions` in order. Filenames are already resolved
/// to `SoundId`s, so the hot path does no string lookups.
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
        assert_eq!(MouseButton::from_evdev_code(30), None); // KEY_A
    }

    #[test]
    fn cg_and_windows_ladders() {
        assert_eq!(MouseButton::from_cg_button_number(3), MouseButton::Back);
        assert_eq!(MouseButton::from_cg_button_number(9), MouseButton::Other(9));
        assert_eq!(MouseButton::from_windows_xbutton(1), MouseButton::Back);
        assert_eq!(MouseButton::from_windows_xbutton(2), MouseButton::Forward);
    }

    #[test]
    fn input_event_from_evdev_code() {
        assert_eq!(
            InputEvent::from_evdev_code(0x110),
            InputEvent::Mouse(MouseButton::Left)
        );
        assert_eq!(InputEvent::from_evdev_code(30), InputEvent::Key(30)); // KEY_A
    }
}
