//! Audio errors.

use crate::domain::SoundId;

/// Errors from audio initialisation or playback.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("failed to open audio output: {0}")]
    Device(String),
    #[error("no sounds to play")]
    NoSounds,
    #[error("sound {0} is not loaded")]
    SoundNotLoaded(SoundId),
    #[error("sound {0} failed to decode: {1}")]
    Decode(SoundId, String),
    #[error("too many concurrent sounds (max {0} voices)")]
    TooManyVoices(usize),
}

impl From<rodio::stream::DeviceSinkError> for AudioError {
    fn from(e: rodio::stream::DeviceSinkError) -> Self {
        Self::Device(e.to_string())
    }
}
