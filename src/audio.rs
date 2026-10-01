//! Audio: decode once into `Arc<[f32]>`, play by cloning.
//!
//! `AudioEngine` is a trait so the backend (rodio today; cpal/kira/SDL later) is
//! swappable without touching the pipeline. `play(id)` hands the mixer a cheap
//! clone of the `Arc<[f32]>` slice — zero file I/O, zero re-decode on the hot path.

use crate::domain::SoundId;

pub mod rodio_engine;
pub use rodio_engine::RodioEngine;

/// A swappable audio backend.
pub trait AudioEngine: Send + Sync {
    /// Play the sound bound to `id`. No-op if the id is unset/invalid.
    fn play(&self, id: SoundId);
}
