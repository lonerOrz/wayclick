//! Input backends: each platform translates its native events into `InputEvent`.
//!
//! `InputBackend::events()` returns a `Stream` of `InputEvent`s. The pipeline
//! consumes that stream; backends own the platform specifics (evdev / Win32
//! low-level hooks / CGEventTap). `Ignored` is emitted for events we deliberately
//! drop (trackpads, repeats) so the pipeline can count them but not play.

use futures::stream::BoxStream;

use crate::domain::InputEvent;

pub mod evdev_backend;
pub mod macos_backend;
pub mod windows_backend;

/// A platform input source.
pub trait InputBackend: Send {
    /// Spawn the listener and return a stream of normalized events.
    ///
    /// The stream is owned by the caller; the backend keeps running until the
    /// stream (and any internal task) is dropped.
    fn events(&mut self) -> Result<BoxStream<'static, InputEvent>, BackendError>;

    /// Human-readable backend name (for `check` / diagnostics).
    fn name(&self) -> &'static str;
}

/// Error starting a backend (permissions, device access, etc.).
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("backend failed to start: {0}")]
    Start(String),
    #[error("accessibility/input permission denied")]
    Permission,
}

/// Construct the backend for the current platform.
pub fn for_current_platform(enable_trackpads: bool) -> Box<dyn InputBackend> {
    #[cfg(target_os = "linux")]
    {
        Box::new(evdev_backend::EvdevBackend::new(enable_trackpads))
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows_backend::WindowsBackend::new(enable_trackpads))
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos_backend::MacosBackend::new(enable_trackpads))
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = enable_trackpads;
        unimplemented!("unsupported platform")
    }
}
