//! Bridge for backends whose events arrive on a foreign (non-tokio) thread:
//! Win32 hook and CGEventTap callbacks push into a process-global sender via
//! [`emit`], which never blocks or panics (FFI boundary). The bridge owns the
//! platform worker: `request_stop` fires the platform stop hook, the worker
//! calls [`worker_exited`] on exit (closing the stream), `join_worker` awaits
//! the thread and reports a panic.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Mutex, RwLock};

use std::time::Duration;

use futures::stream::{BoxStream, StreamExt};
use tokio::sync::mpsc::{self, Sender};
use tokio_stream::wrappers::ReceiverStream;

use super::BackendError;
use crate::domain::InputEvent;
use crate::input::error::InputError;

/// Bounded capacity of the callback → pipeline channel.
///
/// 1024 events buffers many seconds of even aggressive key repeat while
/// staying tiny in memory. The queue is deliberately bounded: an FFI hook
/// callback must never block or grow memory without limit, so when full the
/// newest event is dropped and counted in [`DROP_STATS`]. Dropping under
/// overload is preferable to stalling the OS input thread.
const CHANNEL_CAP: usize = 1024;

/// Why an FFI callback dropped an event.
#[derive(Debug, Default)]
pub struct DropStats {
    pub channel_full: AtomicU64,
    pub sender_closed: AtomicU64,
    pub lock_contention: AtomicU64,
}

impl DropStats {
    pub fn channel_full(&self) -> u64 {
        self.channel_full.load(Ordering::Relaxed)
    }
    pub fn sender_closed(&self) -> u64 {
        self.sender_closed.load(Ordering::Relaxed)
    }
    pub fn lock_contention(&self) -> u64 {
        self.lock_contention.load(Ordering::Relaxed)
    }
}

/// Global drop counters (monotonic for the process lifetime).
pub static DROP_STATS: DropStats = DropStats {
    channel_full: AtomicU64::new(0),
    sender_closed: AtomicU64::new(0),
    lock_contention: AtomicU64::new(0),
};

type StopFn = Box<dyn FnOnce() + Send>;

struct Globals {
    /// Platform stop hook (posts WM_QUIT / stops CFRunLoop).
    platform_stop: Option<StopFn>,
    worker: Option<std::thread::JoinHandle<()>>,
}

static GLOBALS: Mutex<Globals> = Mutex::new(Globals {
    platform_stop: None,
    worker: None,
});

/// Hot-path sender in its own lock: concurrent FFI callbacks take a shared
/// read lock instead of contending on the low-frequency stop/worker mutex.
static SENDER: RwLock<Option<Sender<InputEvent>>> = RwLock::new(None);

/// Forward an event from a hook callback: `read` + `try_send`, each drop
/// counted. Never blocks or panics (FFI boundary). The read lock is shared so
/// concurrent callbacks do not serialise on the sender.
pub(crate) fn emit(event: InputEvent) {
    let Ok(guard) = SENDER.read() else {
        DROP_STATS.lock_contention.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let Some(tx) = guard.as_ref() else {
        DROP_STATS.sender_closed.fetch_add(1, Ordering::Relaxed);
        return;
    };
    match tx.try_send(event) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            DROP_STATS.channel_full.fetch_add(1, Ordering::Relaxed);
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            DROP_STATS.sender_closed.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn try_start_sender(tx: Sender<InputEvent>) -> Result<(), BackendError> {
    // Lock SENDER first; all other paths that touch both locks use this order.
    let mut sender = SENDER
        .write()
        .map_err(|_| BackendError::Start("sender lock poisoned".into()))?;
    if sender.is_some() {
        return Err(BackendError::Start("backend already running".into()));
    }
    let mut guard = GLOBALS
        .lock()
        .map_err(|_| BackendError::Start("globals lock poisoned".into()))?;
    guard.platform_stop = None;
    guard.worker = None;
    *sender = Some(tx);
    Ok(())
}

fn set_worker(handle: std::thread::JoinHandle<()>) {
    if let Ok(mut guard) = GLOBALS.lock() {
        guard.worker = Some(handle);
    }
}

/// Register the platform stop hook (called by the install closure on success).
pub(crate) fn register_stop(stop: StopFn) {
    let running = SENDER.read().map(|s| s.is_some()).unwrap_or(false);
    if !running {
        return;
    }
    if let Ok(mut guard) = GLOBALS.lock() {
        guard.platform_stop = Some(stop);
    }
}

fn clear_sender() {
    if let Ok(mut sender) = SENDER.write() {
        *sender = None;
    }
    if let Ok(mut guard) = GLOBALS.lock() {
        guard.platform_stop = None;
    }
}

/// Called by the worker thread when its install/message-loop function returns,
/// for any reason; dropping the sender closes the receiver stream.
fn worker_exited() {
    if let Ok(mut sender) = SENDER.write() {
        *sender = None;
    }
    if let Ok(mut guard) = GLOBALS.lock() {
        guard.platform_stop = None;
        // Keep `worker` so join_worker can observe a panic.
    }
}

/// Fire the platform stop hook without waiting. One-shot and non-blocking.
pub(crate) fn request_stop() {
    let stop = {
        let Ok(mut guard) = GLOBALS.lock() else {
            return;
        };
        guard.platform_stop.take()
    };
    if let Some(stop) = stop {
        stop();
    }
}

/// Wait for the platform worker to exit and report its termination status.
pub(crate) fn join_worker() -> Result<(), InputError> {
    let handle = {
        let Ok(mut guard) = GLOBALS.lock() else {
            return Err(InputError::Shutdown("globals lock poisoned".into()));
        };
        guard.worker.take()
    };
    if let Some(handle) = handle
        && handle.join().is_err()
    {
        return Err(InputError::WorkerPanicked);
    }
    tracing::info!(
        channel_full = DROP_STATS.channel_full(),
        sender_closed = DROP_STATS.sender_closed(),
        lock_contention = DROP_STATS.lock_contention(),
        "input drop statistics"
    );
    clear_sender();
    Ok(())
}

/// Build the `(stop, join)` hooks for an [`InputSession`](crate::input::session::InputSession).
/// Split stop/join lets the App request shutdown before draining the stream.
pub(crate) fn session_hooks() -> (Box<dyn FnOnce() + Send>, crate::input::session::JoinFn) {
    (
        Box::new(request_stop),
        Box::new(|| Box::pin(async { join_worker() })),
    )
}

/// Handshake passed to the platform install closure.
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

/// Result of starting a bridged backend.
pub struct BridgedStart {
    pub events: BoxStream<'static, InputEvent>,
}

/// Spawn `install` on a dedicated thread and bridge its callbacks into a stream.
/// `ready_timeout` bounds the start handshake.
///
/// Every failure path rolls back the global sender. A ready timeout never
/// blocks on `join`: a stuck thread is left alone, and `worker_exited` still
/// closes the stream when it eventually exits.
pub(crate) fn start_bridged(
    thread_name: &str,
    ready_timeout: Option<Duration>,
    install: impl FnOnce(Ready) + Send + 'static,
) -> Result<BridgedStart, BackendError> {
    let (tx, rx) = mpsc::channel::<InputEvent>(CHANNEL_CAP);
    try_start_sender(tx)?;

    let (ready_tx, ready_rx) = std_mpsc::channel::<Result<(), BackendError>>();

    let handle = match std::thread::Builder::new()
        .name(thread_name.to_owned())
        .spawn(move || {
            install(Ready { tx: ready_tx });
            worker_exited();
        }) {
        Ok(handle) => handle,
        Err(e) => {
            clear_sender();
            return Err(BackendError::Start(format!(
                "failed to spawn {thread_name} thread: {e}"
            )));
        }
    };

    let started = match ready_timeout {
        Some(timeout) => ready_rx.recv_timeout(timeout).map_err(|_| {
            BackendError::Start(format!("timed out waiting for {thread_name} to start"))
        }),
        None => ready_rx
            .recv()
            .map_err(|_| BackendError::Start(format!("{thread_name} died before starting"))),
    }
    .and_then(|ready| ready);

    match started {
        Ok(()) => {
            set_worker(handle);
            Ok(BridgedStart {
                events: ReceiverStream::new(rx).boxed(),
            })
        }
        Err(err) => {
            // Join only if the thread already finished: a stuck install must
            // never stall startup failure.
            if handle.is_finished() && handle.join().is_err() {
                tracing::warn!("input worker panicked during startup");
            }
            clear_sender();
            Err(err)
        }
    }
}
