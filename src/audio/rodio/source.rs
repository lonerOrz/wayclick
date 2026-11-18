//! Rodio `Source` over shared PCM samples.

use std::sync::Arc;

use rodio::source::Source;
use rodio::{ChannelCount, SampleRate};

/// A `rodio::Source` over an `Arc<[f32]>`; cloning is O(1) (refcount only).
#[derive(Clone)]
pub struct SharedSamples {
    samples: Arc<[f32]>,
    channels: ChannelCount,
    sample_rate: SampleRate,
    cursor: usize,
}

impl SharedSamples {
    pub fn new(samples: Arc<[f32]>, channels: ChannelCount, sample_rate: SampleRate) -> Self {
        Self {
            samples,
            channels,
            sample_rate,
            cursor: 0,
        }
    }
}

impl Iterator for SharedSamples {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        let sample = self.samples.get(self.cursor).copied()?;
        self.cursor += 1;
        Some(sample)
    }
}

impl Source for SharedSamples {
    fn current_span_len(&self) -> Option<usize> {
        // Rodio reads this as the length of the current contiguous span (how
        // many samples remain before the format could change), not "remaining
        // samples". The format is constant here, so `Some(remaining)` is valid
        // only on a frame boundary with data left; otherwise `None`.
        let remaining = self.samples.len().checked_sub(self.cursor)?;
        (remaining > 0 && remaining.is_multiple_of(self.channels.get() as usize))
            .then_some(remaining)
    }

    fn channels(&self) -> ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        Some(duration_from_samples(
            self.samples.len(),
            self.channels.get() as u64,
            self.sample_rate.get() as u64,
        ))
    }
}

/// Convert a sample count to a duration; the `u128` intermediate keeps
/// `frames * 1e9` safe for every `usize` sample count, saturating on
/// conversion back to `u64`.
fn duration_from_samples(
    sample_count: usize,
    channels: u64,
    sample_rate: u64,
) -> std::time::Duration {
    debug_assert!(
        channels > 0 && sample_rate > 0,
        "format invariants guarantee non-zero values"
    );
    let frames = sample_count as u128 / channels as u128;
    let nanos = frames * 1_000_000_000u128 / sample_rate as u128;
    std::time::Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::{NonZeroU16, NonZeroU32};
    use std::time::Duration;

    fn samples(n: usize) -> Arc<[f32]> {
        Arc::from(vec![0.0f32; n])
    }

    fn mono(n: usize) -> SharedSamples {
        SharedSamples::new(
            samples(n),
            NonZeroU16::new(1).unwrap(),
            NonZeroU32::new(48_000).unwrap(),
        )
    }

    fn stereo(n: usize) -> SharedSamples {
        SharedSamples::new(
            samples(n),
            NonZeroU16::new(2).unwrap(),
            NonZeroU32::new(48_000).unwrap(),
        )
    }

    #[test]
    fn first_call_is_whole_clip() {
        assert_eq!(mono(4).current_span_len(), Some(4));
        assert_eq!(stereo(6).current_span_len(), Some(6));
    }

    #[test]
    fn remaining_stays_frame_aligned() {
        let mut src = stereo(6); // 3 stereo frames
        assert_eq!(src.next(), Some(0.0));
        assert_eq!(src.next(), Some(0.0)); // one full frame consumed
        assert_eq!(src.current_span_len(), Some(4));
        assert_eq!(src.next(), Some(0.0)); // half a frame: now misaligned
        assert_eq!(src.current_span_len(), None);
    }

    #[test]
    fn non_aligned_cursor_reports_none() {
        // Test-only field access: a misaligned cursor cannot arise through
        // public playback, but the Source contract must still be upheld.
        let mut src = stereo(6);
        src.cursor = 1;
        assert_eq!(src.current_span_len(), None);
    }

    #[test]
    fn end_of_stream_reports_none() {
        let mut src = mono(2);
        assert_eq!(src.next(), Some(0.0));
        assert_eq!(src.next(), Some(0.0));
        assert_eq!(src.current_span_len(), None);
        assert_eq!(src.next(), None);

        // Defensive: a cursor past the end must not underflow.
        let mut past = mono(2);
        past.cursor = 3;
        assert_eq!(past.current_span_len(), None);
    }

    #[test]
    fn mono_and_stereo_differ_in_alignment() {
        let m = mono(3);
        let s = stereo(3);
        assert_eq!(m.channels().get(), 1);
        assert_eq!(s.channels().get(), 2);
        assert_eq!(m.sample_rate(), s.sample_rate());
        assert_eq!(m.current_span_len(), Some(3)); // 3 mono frames
        assert_eq!(s.current_span_len(), None); // 1.5 stereo frames
    }

    #[test]
    fn duration_matches_frames_over_rate() {
        assert_eq!(mono(48_000).total_duration(), Some(Duration::from_secs(1)));
        assert_eq!(
            stereo(96_000).total_duration(),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            mono(24_000).total_duration(),
            Some(Duration::from_millis(500))
        );
    }

    #[test]
    fn duration_does_not_overflow_for_long_clips() {
        let d = duration_from_samples(usize::MAX, 1, 48_000);
        assert_eq!(d, Duration::from_nanos(u64::MAX));
    }
}
