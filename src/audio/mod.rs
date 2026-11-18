//! Audio: decode once into shared PCM, play by cloning an `Arc`.

pub mod buffer;
pub mod decoded_sound;
pub mod decoder;
pub mod error;
pub mod rodio;
pub mod sound_bank;

pub use buffer::{AudioBufferSize, AudioDevice, BufferFrames};
pub use decoded_sound::{DecodedSound, SoundDataError};
pub use decoder::DecodeError;
pub use error::AudioError;
pub use rodio::RodioEngine;
pub use sound_bank::SoundBank;

/// A swappable audio backend.
pub trait AudioEngine: Send + Sync {
    /// Submit a playback request; success means the mixer accepted it, not
    /// that the sound reached the output device.
    fn play(&self, id: crate::domain::SoundId) -> Result<(), AudioError>;
}
