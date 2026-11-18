//! Input session: owns the event stream plus the platform stop and join
//! hooks. Created only after the platform listener started successfully.
//! [`InputSession::shutdown`] is the ordered close (stop, then join); `Drop`
//! only fires stop, since a worker cannot be awaited from `Drop`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use futures::stream::BoxStream;

use crate::domain::InputEvent;
use crate::input::error::InputError;

type StopFn = Box<dyn FnOnce() + Send>;
/// Awaits the platform worker and reports how it terminated.
pub type JoinFn =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<(), InputError>> + Send>> + Send>;

/// An active input listener.
pub struct InputSession {
    events: Option<BoxStream<'static, InputEvent>>,
    stop: Mutex<Option<StopFn>>,
    join: Mutex<Option<JoinFn>>,
}

impl InputSession {
    pub fn new(events: BoxStream<'static, InputEvent>, stop: StopFn, join: JoinFn) -> Self {
        Self {
            events: Some(events),
            stop: Mutex::new(Some(stop)),
            join: Mutex::new(Some(join)),
        }
    }

    /// Take the event stream. The stream can only be taken once.
    pub fn take_events(&mut self) -> Option<BoxStream<'static, InputEvent>> {
        self.events.take()
    }

    /// Request the platform listener to stop without waiting for its worker.
    /// App calls this before draining: the worker closes the stream once it
    /// stops, which unblocks stream consumption.
    pub fn request_stop(&self) {
        self.fire_stop();
    }

    /// Stop the listener and wait for its worker to exit.
    ///
    /// Repeated calls are harmless (both hooks are one-shot); a second call
    /// simply observes that there is nothing left to do.
    pub async fn shutdown(self) -> Result<(), InputError> {
        self.fire_stop();
        let join = {
            let Ok(mut guard) = self.join.lock() else {
                return Err(InputError::Shutdown("join lock poisoned".into()));
            };
            guard.take()
        };
        match join {
            Some(join) => join().await,
            None => Ok(()),
        }
    }

    fn fire_stop(&self) {
        if let Ok(mut guard) = self.stop.lock()
            && let Some(stop) = guard.take()
        {
            stop();
        }
    }
}

impl Drop for InputSession {
    fn drop(&mut self) {
        self.fire_stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use futures::stream;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn shutdown_fires_stop_and_join() {
        let stopped = Arc::new(AtomicUsize::new(0));
        let joined = Arc::new(AtomicUsize::new(0));
        let s = stopped.clone();
        let j = joined.clone();
        let session = InputSession::new(
            stream::empty().boxed(),
            Box::new(move || {
                s.fetch_add(1, Ordering::SeqCst);
            }),
            Box::new(move || {
                let j = j.clone();
                Box::pin(async move {
                    j.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }),
        );
        session.shutdown().await.unwrap();
        assert_eq!(stopped.load(Ordering::SeqCst), 1);
        assert_eq!(joined.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn shutdown_propagates_join_error() {
        let session = InputSession::new(
            stream::empty().boxed(),
            Box::new(|| {}),
            Box::new(|| Box::pin(async { Err(InputError::WorkerPanicked) })),
        );
        assert!(matches!(
            session.shutdown().await,
            Err(InputError::WorkerPanicked)
        ));
    }

    #[test]
    fn drop_fires_stop_as_fallback() {
        let stopped = Arc::new(AtomicUsize::new(0));
        let s = stopped.clone();
        {
            let _session = InputSession::new(
                stream::empty().boxed(),
                Box::new(move || {
                    s.fetch_add(1, Ordering::SeqCst);
                }),
                Box::new(|| Box::pin(async { Ok(()) })),
            );
        }
        assert_eq!(stopped.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn events_can_be_taken_once() {
        let mut session = InputSession::new(
            stream::empty().boxed(),
            Box::new(|| {}),
            Box::new(|| Box::pin(async { Ok(()) })),
        );
        assert!(session.take_events().is_some());
        assert!(session.take_events().is_none());
    }
}
