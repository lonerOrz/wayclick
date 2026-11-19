//! Sound decoding: bytes on disk into a validated [`DecodedSound`].
//!
//! Rodio is used as the decoder; the concrete rodio error type is stringified
//! into [`DecodeError`] so this module's API does not leak it.

use std::io::Cursor;

use std::path::Path;

use rodio::Decoder;
use rodio::source::Source;

use crate::audio::decoded_sound::{DecodedSound, SoundDataError};

/// Decode a sound file into PCM.
pub fn decode(path: &Path) -> Result<DecodedSound, DecodeError> {
    let bytes =
        std::fs::read(path).map_err(|e| DecodeError::Read(path.to_path_buf(), e.to_string()))?;
    let decoder = Decoder::try_from(Cursor::new(bytes))
        .map_err(|e| DecodeError::Format(path.to_path_buf(), e.to_string()))?;

    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();

    let samples: std::sync::Arc<[f32]> = decoder.collect::<Vec<f32>>().into();
    DecodedSound::new(samples, channels, sample_rate).map_err(DecodeError::Data)
}

/// Failures while reading or decoding a sound file.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("failed to read {0}: {1}")]
    Read(std::path::PathBuf, String),
    #[error("unrecognised audio format in {0}: {1}")]
    Format(std::path::PathBuf, String),
    #[error("decoder reported zero channels")]
    ZeroChannels,
    #[error("decoder reported zero sample rate")]
    ZeroSampleRate,
    #[error("invalid sound data: {0}")]
    Data(#[from] SoundDataError),
}
