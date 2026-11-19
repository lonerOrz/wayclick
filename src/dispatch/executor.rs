//! Action execution: turn a compiled `Action` into an audio play request.

use std::sync::{Arc, RwLock};

use crate::audio::{AudioEngine, AudioError};
use crate::domain::{Action, SoundId};

/// Executes actions against an audio engine; never reads config or touches
/// platform input APIs.
///
/// The engine sits behind a lock so a config hot-reload can swap in a freshly
/// built engine (new sound bank) atomically. In-flight `execute` calls finish
/// on the engine they started with; the old engine is reclaimed once its last
/// user drops it — sounds still playing keep their `Arc`'d PCM until done.
pub struct Executor {
    engine: RwLock<Arc<dyn AudioEngine>>,
}

impl Executor {
    pub fn new(engine: Arc<dyn AudioEngine>) -> Self {
        Self {
            engine: RwLock::new(engine),
        }
    }

    /// Swap in a rebuilt engine. Previously queued sounds finish on the old
    /// engine; new play requests go to the new one.
    pub fn replace_engine(&self, engine: Arc<dyn AudioEngine>) {
        if let Ok(mut guard) = self.engine.write() {
            *guard = engine;
        }
    }

    /// Execute one action, returning the play outcome.
    pub fn execute(&self, action: Action) -> Result<(), AudioError> {
        match action {
            Action::PlaySound(id) => self.play(id),
        }
    }

    fn play(&self, id: SoundId) -> Result<(), AudioError> {
        // Clone the Arc out of the lock so the read guard is dropped before
        // `play()` runs — the mixer submit must not hold the swap lock.
        let engine = match self.engine.read() {
            Ok(guard) => Arc::clone(&guard),
            Err(_) => return Err(AudioError::Device("audio engine lock poisoned".into())),
        };
        engine.play(id)
    }
}
