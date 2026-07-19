//! Executor: the concrete (non-trait) component that turns a resolved `SoundId`
//! into playback.
//!
//! Per the rewrite plan this is a *struct*, not a trait — there is exactly one
//! implementation path (rodio). A trait would be YAGNI; promote it only when a
//! second execution backend actually appears (à la cargo).

use std::sync::Arc;

use crate::audio::AudioEngine;
use crate::domain::SoundId;

pub struct Executor {
    engine: Arc<dyn AudioEngine>,
}

impl Executor {
    pub fn new(engine: Arc<dyn AudioEngine>) -> Executor {
        Executor { engine }
    }

    /// Play the sound bound to `id`. The caller (Pipeline) has already verified
    /// the id resolves to a decoded sample; the engine handles missing ids as a
    /// no-op. This seam keeps the executor engine-agnostic.
    pub fn play(&self, id: SoundId) {
        self.engine.play(id);
    }
}
