//! Bridge for backends whose events arrive on a foreign (non-tokio) thread.
//!
//! Win32 low-level hooks and CGEventTap callbacks are `extern` functions that
//! cannot capture state, so they push events into a process-global sender.
//! `start_bridged` owns the whole lifecycle: channel, global sender, hook thread,
//! start handshake, and teardown-on-drop.

use std::pin::Pin;
use std::sync::Mutex;
use std::sync::mpsc as std_mpsc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::stream::{BoxStream, Stream, StreamExt};
use tokio::sync::mpsc::{self, Sender};
use tokio_stream::wrappers::ReceiverStream;

use super::BackendError;
use crate::domain::InputEvent;

/// Bounded channel capacity for the callback -> pipeline bridge.
const CHANNEL_CAP: usize = 1024;

/// The single sink hook/tap callbacks push into. `Option` (not `OnceLock`) so a
/// fresh `events()` replaces it; [`GuardedSenderStream`] clears it on drop.
static SENDER: Mutex<Option<Sender<InputEvent>>> = Mutex::new(None);

/// Forward an event from a hook callback, dropping on backpressure rather than
/// blocking the hook thread.
pub(crate) fn emit(event: InputEvent) {
    if let Ok(guard) = SENDER.lock()
        && let Some(tx) = guard.as_ref()
    {
        let _ = tx.try_send(event);
    }
}

/// Install the global sender, rejecting if one is already live: a second
/// `events()` would overwrite it and the old hook thread would keep pushing into
/// the new channel (double-play).
fn try_start_sender(tx: Sender<InputEvent>) -> Result<(), BackendError> {
    let mut guard = SENDER
        .lock()
        .map_err(|_| BackendError::Start("sender lock poisoned".into()))?;
    if guard.is_some() {
        return Err(BackendError::Start("backend already running".into()));
    }
    *guard = Some(tx);
    Ok(())
}

fn clear_sender() {
    if let Ok(mut guard) = SENDER.lock() {
        *guard = None;
    }
}

/// Clears the global sender when dropped, so a dropped stream cannot leave a
/// stale sender behind for a lingering hook thread.
struct GuardedSenderStream {
    inner: ReceiverStream<InputEvent>,
}

impl GuardedSenderStream {
    fn new(inner: ReceiverStream<InputEvent>) -> Self {
        GuardedSenderStream { inner }
    }
}

impl Stream for GuardedSenderStream {
    type Item = InputEvent;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<InputEvent>> {
        // SAFETY: `inner` is not structurally pinned.
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.inner) };
        inner.poll_next(cx)
    }
}

impl Drop for GuardedSenderStream {
    fn drop(&mut self) {
        clear_sender();
    }
}

/// Handshake passed to the hook thread's `install` closure. Report `ok`/`fail`
/// once the hook is installed; dropping it counts as a failure.
pub(crate) struct Ready {
    tx: std_mpsc::Sender<Result<(), BackendError>>,
}

impl Ready {
    pub(crate) fn ok(self) {
        let _ = self.tx.send(Ok(()));
    }

    pub(crate) fn fail(self, err: BackendError) {
        let _ = self.tx.send(Err(err));
    }
}

/// Spawn `install` on a dedicated thread and bridge its callbacks into a stream.
///
/// `install` runs on the new thread and receives a [`Ready`] handle; it must
/// report success or failure once the hook is installed. `ready_timeout` bounds
/// how long we wait for that report.
pub(crate) fn start_bridged(
    thread_name: &str,
    ready_timeout: Option<Duration>,
    install: impl FnOnce(Ready) + Send + 'static,
) -> Result<BoxStream<'static, InputEvent>, BackendError> {
    let (tx, rx) = mpsc::channel::<InputEvent>(CHANNEL_CAP);
    try_start_sender(tx)?;

    let (ready_tx, ready_rx) = std_mpsc::channel::<Result<(), BackendError>>();

    let spawned = std::thread::Builder::new()
        .name(thread_name.to_owned())
        .spawn(move || install(Ready { tx: ready_tx }));

    let started = match spawned {
        Err(e) => Err(BackendError::Start(format!(
            "failed to spawn {thread_name} thread: {e}"
        ))),
        Ok(_) => match ready_timeout {
            Some(timeout) => ready_rx.recv_timeout(timeout).map_err(|_| {
                BackendError::Start(format!("timed out waiting for {thread_name} to start"))
            }),
            None => ready_rx
                .recv()
                .map_err(|_| BackendError::Start(format!("{thread_name} died before starting"))),
        }
        .and_then(|ready| ready),
    };

    match started {
        Ok(()) => Ok(GuardedSenderStream::new(ReceiverStream::new(rx)).boxed()),
        Err(err) => {
            clear_sender();
            Err(err)
        }
    }
}
