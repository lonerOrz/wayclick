//! A decoded PCM sound with validated metadata.

use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;

/// Interleaved f32 PCM plus its native format.
///
/// Invariants, enforced at construction:
/// - PCM data is non-empty.
/// - Channel count and sample rate are non-zero.
/// - Sample count is a multiple of the channel count.
/// - Samples are all finite (no NaN / infinity).
#[derive(Debug, Clone)]
pub struct DecodedSound {
    samples: Arc<[f32]>,
    channels: NonZeroU16,
    sample_rate: NonZeroU32,
}

impl DecodedSound {
    /// Build a decoded sound, validating all format invariants.
    pub fn new(
        samples: Arc<[f32]>,
        channels: NonZeroU16,
        sample_rate: NonZeroU32,
    ) -> Result<Self, SoundDataError> {
        if samples.is_empty() {
            return Err(SoundDataError::Empty);
        }
        if !samples.len().is_multiple_of(channels.get() as usize) {
            return Err(SoundDataError::FrameAlignment);
        }
        if samples.iter().any(|s| !s.is_finite()) {
            return Err(SoundDataError::NonFiniteSample);
        }
        Ok(Self {
            samples,
            channels,
            sample_rate,
        })
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Shared PCM handle for zero-copy playback.
    pub fn shared_samples(&self) -> Arc<[f32]> {
        Arc::clone(&self.samples)
    }

    pub fn channels(&self) -> NonZeroU16 {
        self.channels
    }

    pub fn sample_rate(&self) -> NonZeroU32 {
        self.sample_rate
    }
}

/// Violations of [`DecodedSound`] invariants.
#[derive(Debug, thiserror::Error)]
pub enum SoundDataError {
    #[error("decoded sound has no samples")]
    Empty,
    #[error("sample count is not a multiple of the channel count")]
    FrameAlignment,
    #[error("decoded sound contains a non-finite sample")]
    NonFiniteSample,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nz(v: u16) -> NonZeroU16 {
        NonZeroU16::new(v).unwrap()
    }
    fn nz32(v: u32) -> NonZeroU32 {
        NonZeroU32::new(v).unwrap()
    }

    #[test]
    fn valid_sound() {
        let s = DecodedSound::new(Arc::from([0.0f32, 1.0, -1.0, 0.5]), nz(2), nz32(48000)).unwrap();
        assert_eq!(s.channels(), nz(2));
        assert_eq!(s.samples().len(), 4);
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(
            DecodedSound::new(Arc::from([]), nz(2), nz32(48000)),
            Err(SoundDataError::Empty)
        ));
    }

    #[test]
    fn rejects_misaligned_frames() {
        assert!(matches!(
            DecodedSound::new(Arc::from([0.0f32, 1.0, 0.5]), nz(2), nz32(48000)),
            Err(SoundDataError::FrameAlignment)
        ));
    }

    #[test]
    fn rejects_non_finite() {
        assert!(matches!(
            DecodedSound::new(Arc::from([f32::NAN, 0.0]), nz(1), nz32(48000)),
            Err(SoundDataError::NonFiniteSample)
        ));
    }

    #[test]
    fn shared_clone_is_zero_copy() {
        let s = DecodedSound::new(Arc::from([0.0f32, 1.0]), nz(1), nz32(48000)).unwrap();
        let a = s.shared_samples();
        let b = s.shared_samples();
        assert!(Arc::ptr_eq(&a, &b));
    }
}
