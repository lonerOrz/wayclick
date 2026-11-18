//! Input backends: each platform translates native events into `InputEvent`.

use crate::input::session::InputSession;

#[cfg(target_os = "linux")]
pub mod evdev_backend;
#[cfg(target_os = "macos")]
pub mod macos_backend;
#[cfg(target_os = "windows")]
pub mod windows_backend;

/// Bridge for backends whose events arrive on a foreign (non-tokio) thread.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) mod bridge;

/// A platform input source. Backends filter their own events: anything they
/// don't want is simply never emitted.
pub trait InputBackend: Send {
    /// Start the listener and return a session owning its lifecycle.
    fn events(&mut self) -> Result<InputSession, BackendError>;

    /// Human-readable backend name.
    fn name(&self) -> &'static str;
}

/// Stream type emitted by input sessions.
pub type BoxStream<'a, T> = futures::stream::BoxStream<'a, T>;

/// Error starting a backend.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// The hook/tap could not be installed.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[error("backend failed to start: {0}")]
    Start(String),
    /// Missing permission to observe input devices / the event tap.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[error("accessibility/input permission denied")]
    Permission,
}

/// Construct the backend for the current platform. `enable_trackpads` and
/// `runtime` are Linux-only; taking the runtime here keeps "must be inside a
/// runtime" out of the trait.
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
