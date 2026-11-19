//! Input errors.

use thiserror::Error;

/// Errors from starting or shutting down an input session.
#[derive(Debug, Error)]
pub enum InputError {
    #[error("backend failed to start: {0}")]
    Start(String),
    #[error("accessibility/input permission denied")]
    Permission,
    #[error("backend already running")]
    AlreadyRunning,
    #[error("input thread panicked")]
    WorkerPanicked,
    #[error("shutdown failed: {0}")]
    Shutdown(String),
}
