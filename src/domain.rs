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
