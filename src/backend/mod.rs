//! Input backends: each platform translates its native events into `InputEvent`.
//!
//! `InputBackend::events` returns a stream the pipeline consumes. Backends own
//! their platform specifics (evdev / Win32 hooks / CGEventTap) *and* their own
//! filtering: an event a backend doesn't want is simply never emitted.

use crate::domain::InputEvent;

#[cfg(target_os = "linux")]
pub mod evdev_backend;
#[cfg(target_os = "macos")]
pub mod macos_backend;
#[cfg(target_os = "windows")]
pub mod windows_backend;

/// Bridge for backends whose events arrive on a foreign (non-tokio) thread.
///
/// Windows and macOS hook callbacks are `extern` functions that cannot be async,
/// so they push into a process-global sender that this module owns. evdev does
/// not use it: it can spawn tokio tasks directly.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) mod bridge;

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

/// Stream type returned by every backend.
pub type BoxStream<'a, T> = futures::stream::BoxStream<'a, T>;

/// Error starting a backend.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// The hook/tap could not be installed.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[error("backend failed to start: {0}")]
    Start(String),
    /// The process lacks permission to observe input devices / the event tap.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[error("accessibility/input permission denied")]
    Permission,
}

/// Construct the backend for the current platform.
///
/// `enable_trackpads` and `runtime` are both Linux-only: evdev can tell a
/// trackpad from a keyboard by name, and it spawns its driver on the runtime,
/// while the Windows/macOS hooks do neither (they run on foreign threads).
/// Handing the runtime in here keeps the "must be inside a runtime" precondition
/// out of the `InputBackend` interface — a caller cannot forget it.
pub fn for_current_platform(
    enable_trackpads: bool,
    runtime: tokio::runtime::Handle,
) -> Box<dyn InputBackend> {
    #[cfg(not(target_os = "linux"))]
    let _ = (enable_trackpads, runtime);

    #[cfg(target_os = "linux")]
    {
        Box::new(evdev_backend::EvdevBackend::new(enable_trackpads, runtime))
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows_backend::WindowsBackend)
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos_backend::MacosBackend)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        unimplemented!("unsupported platform")
    }
}
