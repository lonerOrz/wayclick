//! Input translation: platform-native codes into domain types. Config keys
//! flow through the Linux evdev translator so numeric configuration keeps its
//! historical meaning.

pub mod linux;
pub mod macos;
pub mod windows;
