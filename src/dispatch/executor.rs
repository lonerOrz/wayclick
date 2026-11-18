//! Action execution: turn a compiled `Action` into an audio play request.

use crate::audio::{AudioEngine, AudioError};
use crate::domain::{Action, SoundId};

/// Executes actions against an audio engine; never reads config or touches
/// platform input APIs.
pub struct Executor {
    engine: std::sync::Arc<dyn AudioEngine>,
}

impl Executor {
    pub fn new(engine: std::sync::Arc<dyn AudioEngine>) -> Self {
        Self { engine }
    }

    /// Execute one action, returning the play outcome.
    pub fn execute(&self, action: Action) -> Result<(), AudioError> {
        match action {
            Action::PlaySound(id) => self.play(id),
        }
    }

    fn play(&self, id: SoundId) -> Result<(), AudioError> {
        self.engine.play(id)
    }
}
