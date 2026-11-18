//! Domain value types: the vocabulary shared by every layer.
//!
//! This module has no dependency on any platform API, audio library, or async
//! runtime. Platform-native codes are translated into these types at the
//! adapter boundary (see [`crate::input::translator`]).

mod action;
mod default_policy;
mod input_event;
mod keyboard;
mod mouse;
mod platform;
mod rule;
mod sound_id;

pub use action::Action;
pub use default_policy::DefaultPolicy;
pub use input_event::InputEvent;
pub use keyboard::PhysicalKey;
pub use mouse::MouseButton;
pub use platform::Platform;
pub use rule::CompiledRule;
pub use sound_id::SoundId;
