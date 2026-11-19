//! The operating system a native input code originated from.

/// The operating system a raw input code originated from; carried inside
/// [`PhysicalKey::Unknown`](crate::domain::PhysicalKey::Unknown) so codes from
/// different platforms cannot collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Platform {
    Linux,
    Windows,
    MacOS,
}
