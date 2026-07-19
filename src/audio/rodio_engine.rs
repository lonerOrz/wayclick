//! Rodio backend: decode wavs once at startup, mix cloned samples on play.
//!
//! Each sound is decoded to an `Arc<[f32]>` plus its own `(channels, sample_rate)`
//! so the hot path is a cheap `Arc` clone into a `SamplesBuffer` — no file I/O,
//! no re-decode. The owning `MixerDeviceSink` is held as a field; dropping it
//! stops all playback, so it must outlive the engine.

use std::io::Cursor;
use std::num::{NonZeroU16, NonZeroU32};
use std::path::Path;
use std::sync::Arc;

use rodio::Decoder;
use rodio::source::Source;
use rodio::stream::{DeviceSinkBuilder, MixerDeviceSink};
use rodio::{ChannelCount, SampleRate};

use crate::audio::AudioEngine;
use crate::domain::SoundId;

/// A `rodio::Source` over a shared `Arc<[f32]>` slice. Cloning is O(1) (bumps the
/// `Arc`); playback never copies the samples off the hot path.
#[derive(Clone)]
struct SharedSamples {
    samples: Arc<[f32]>,
    channels: ChannelCount,
    sample_rate: SampleRate,
    cursor: usize,
}

impl Iterator for SharedSamples {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        if self.cursor < self.samples.len() {
            let s = self.samples[self.cursor];
            self.cursor += 1;
            Some(s)
        } else {
            None
        }
    }
}

impl Source for SharedSamples {
    fn current_span_len(&self) -> Option<usize> {
        // One contiguous span of the remaining samples (multiple of channels).
        let remaining = self.samples.len() - self.cursor;
        remaining
            .is_multiple_of(self.channels.get() as usize)
            .then_some(remaining)
    }
    fn channels(&self) -> ChannelCount {
        self.channels
    }
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        let frames = self.samples.len() as u64 / self.channels.get() as u64;
        Some(std::time::Duration::from_nanos(
            frames * 1_000_000_000 / self.sample_rate.get() as u64,
        ))
    }
}

/// One pre-decoded sound: interleaved f32 PCM plus its native format.
#[derive(Debug, Clone)]
pub struct DecodedSound {
    pub samples: Arc<[f32]>,
    pub channels: NonZeroU16,
    pub sample_rate: NonZeroU32,
}

/// Errors from opening the output or (fatally) having nothing to play.
///
/// Per-file decode failures are *not* errors: they log a warning and leave that
/// slot empty, matching the Python "invalid wav -> skip" behaviour.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("failed to open audio output: {0}")]
    Rodio(String),
    #[error("no sounds to play")]
    NoSounds,
}

impl From<rodio::stream::DeviceSinkError> for AudioError {
    fn from(e: rodio::stream::DeviceSinkError) -> AudioError {
        AudioError::Rodio(e.to_string())
    }
}

/// A rodio-backed [`AudioEngine`]. `Send + Sync` (mixer handle + immutable data).
pub struct RodioEngine {
    sink: MixerDeviceSink,
    decoded: Vec<Option<DecodedSound>>,
}

impl RodioEngine {
    /// Open the default output and take ownership of the pre-decoded sounds.
    ///
    /// `buffer_frames`, if set, requests a fixed cpal buffer size for lower
    /// latency; `None` uses the device default.
    pub fn new(
        decoded: Vec<Option<DecodedSound>>,
        buffer_frames: Option<u32>,
    ) -> Result<RodioEngine, AudioError> {
        if decoded.iter().all(Option::is_none) {
            return Err(AudioError::NoSounds);
        }

        let sink = match buffer_frames {
            Some(frames) => rodio::stream::DeviceSinkBuilder::from_default_device()?
                .with_buffer_size(rodio::cpal::BufferSize::Fixed(frames))
                .open_stream()?,
            None => DeviceSinkBuilder::open_default_sink()?,
        };

        Ok(RodioEngine { sink, decoded })
    }

    /// Decode every named wav in `slots` into a [`RodioEngine`].
    ///
    /// `slots[i]` is the path for `SoundId(i + 1)` (or `None` for an empty slot).
    /// A file that is missing or fails to decode logs a warning and becomes an
    /// empty slot rather than aborting startup.
    pub fn from_dir(
        dir: &Path,
        slots: &[Option<String>],
        buffer_frames: Option<u32>,
    ) -> Result<RodioEngine, AudioError> {
        let decoded = slots
            .iter()
            .map(|slot| slot.as_deref().and_then(|name| decode_wav(&dir.join(name))))
            .collect();
        RodioEngine::new(decoded, buffer_frames)
    }

    /// Decode every named wav without opening an audio device.
    ///
    /// Used by `wayclick check` so the self-test can prove config + wav decoding
    /// work on a headless machine (no sound card). Returns the decoded slots.
    pub fn decode_dir(dir: &Path, slots: &[Option<String>]) -> Vec<Option<DecodedSound>> {
        slots
            .iter()
            .map(|slot| slot.as_deref().and_then(|name| decode_wav(&dir.join(name))))
            .collect()
    }
}

/// Read + decode one wav into a [`DecodedSound`]; `None` (with a warning) on any failure.
fn decode_wav(path: &Path) -> Option<DecodedSound> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "failed to read sound file");
            return None;
        }
    };
    let decoder = match Decoder::try_from(Cursor::new(bytes)) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "failed to decode sound file");
            return None;
        }
    };
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Arc<[f32]> = decoder.collect::<Vec<f32>>().into();
    if samples.is_empty() {
        tracing::warn!(path = %path.display(), "decoded sound was empty");
        return None;
    }
    Some(DecodedSound {
        samples,
        channels,
        sample_rate,
    })
}

impl AudioEngine for RodioEngine {
    fn play(&self, id: SoundId) {
        let Some(Some(d)) = self.decoded.get((id.get() - 1) as usize) else {
            return;
        };
        // Zero-copy: clone the Arc, not the samples.
        let source = SharedSamples {
            samples: d.samples.clone(),
            channels: d.channels,
            sample_rate: d.sample_rate,
            cursor: 0,
        };
        self.sink.mixer().add(source);
    }

    fn stop(&self) {
        // ponytail: the App owns the engine's lifetime; dropping it drops the
        // MixerDeviceSink which stops playback. No handle to pause mid-flight,
        // so shutdown-stop is a no-op. Add an explicit stop if pause/resume lands.
    }
}
