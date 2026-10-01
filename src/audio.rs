//! Audio: decode once into `Arc<[f32]>`, play by cloning.

use crate::domain::SoundId;

pub mod rodio_engine;
pub use rodio_engine::RodioEngine;

/// A swappable audio backend.
pub trait AudioEngine: Send + Sync {
    fn play(&self, id: SoundId);
}
