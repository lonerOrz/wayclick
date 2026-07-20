//! Input backends: each platform translates its native events into `InputEvent`.
//!
//! `InputBackend::events()` returns a `Stream` of `InputEvent`s. The pipeline
//! consumes that stream; backends own the platform specifics (evdev / Win32
//! low-level hooks / CGEventTap). `Ignored` is emitted for events we deliberately
//! drop (trackpads, repeats) so the pipeline can count them but not play.

use crate::domain::InputEvent;

// Imports for the Windows/macOS hook/tap bridge (gated: evdev uses its own path).
#[cfg(any(target_os = "windows", target_os = "macos"))]
use futures::stream::Stream;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::pin::Pin;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::sync::Mutex;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::task::{Context, Poll};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use tokio::sync::mpsc::Sender;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use tokio_stream::wrappers::ReceiverStream;

#[cfg(target_os = "linux")]
pub mod evdev_backend;
#[cfg(target_os = "macos")]
pub mod macos_backend;
#[cfg(target_os = "windows")]
pub mod windows_backend;

/// Bounded channel capacity for the backend→pipeline bridge.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) const CHANNEL_CAP: usize = 1024;

/// The single sink hook/tap callbacks push into. Callbacks are free `extern`
/// functions and cannot capture, so the sender lives in this process-global.
/// `Option` (not `OnceLock`) so a fresh `events()` replaces it, and shutdown
/// clears it via `GuardedSenderStream`'s `Drop`.
///
/// Only Windows/macOS backends use this — evdev bridges via its own task.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) static SENDER: Mutex<Option<Sender<InputEvent>>> = Mutex::new(None);

/// Forward an event into the live sender, dropping on backpressure rather than
/// blocking the hook/tap thread.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) fn emit(event: InputEvent) {
    if let Ok(guard) = SENDER.lock() {
        if let Some(tx) = guard.as_ref() {
            let _ = tx.try_send(event);
        }
    }
}

/// Install a fresh sender, rejecting if one is already live. A second `events()`
/// call while the previous hook/tap thread is alive would overwrite `SENDER` and
/// the old thread would keep pushing into the new channel (double-play).
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) fn try_start_sender(tx: Sender<InputEvent>) -> Result<(), BackendError> {
    let mut guard = SENDER
        .lock()
        .map_err(|_| BackendError::Start("sender lock poisoned".into()))?;
    if guard.is_some() {
        return Err(BackendError::Start("backend already running".into()));
    }
    *guard = Some(tx);
    Ok(())
}

/// Stream wrapper that clears the global `SENDER` when dropped, so a dropped
/// stream (or a failed start) can't leave a stale sender for a lingering hook.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) struct GuardedSenderStream {
    inner: ReceiverStream<InputEvent>,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl GuardedSenderStream {
    pub(crate) fn new(rx: ReceiverStream<InputEvent>) -> Self {
        GuardedSenderStream { inner: rx }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl Stream for GuardedSenderStream {
    type Item = InputEvent;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<InputEvent>> {
        // SAFETY: no structural pinning of `inner` required.
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.inner) };
        inner.poll_next(cx)
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl Drop for GuardedSenderStream {
    fn drop(&mut self) {
        if let Ok(mut guard) = SENDER.lock() {
            *guard = None;
        }
    }
}

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

/// Error starting a backend (permissions, device access, etc.).
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    // Windows low-level hooks fail at "install hook / spawn pump thread", so they
    // only use `Start`. Linux evdev and macOS CGEventTap additionally have an
    // accessibility/device-access gate, so they also use `Permission`. Gate each
    // variant to the platforms that actually construct it — no `allow(dead_code)`.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[error("backend failed to start: {0}")]
    Start(String),
    #[cfg(any(target_os = "linux", target_os = "macos"))]
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
