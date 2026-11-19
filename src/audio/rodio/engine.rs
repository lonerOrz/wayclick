//! Rodio-backed audio engine.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::source::Source;
use rodio::stream::MixerDeviceSink;
use rodio::{ChannelCount, SampleRate};

use crate::audio::buffer::DEFAULT_BUFFER_FRAMES;
use crate::audio::decoder;
use crate::audio::rodio::source::SharedSamples;
use crate::audio::{AudioBufferSize, AudioDevice, AudioEngine, AudioError, SoundBank};
use crate::domain::SoundId;

/// Default cap on concurrent sounds when the config does not specify one.
pub const MAX_VOICES: usize = 32;

/// A rodio-backed [`AudioEngine`].
pub struct RodioEngine {
    sink: MixerDeviceSink,
    bank: SoundBank,
    /// Voices currently queued or playing; capped at `max_voices`.
    voices: Arc<AtomicUsize>,
    /// Per-voice output gain (`1.0` = unity).
    volume: f32,
    /// Concurrent-voice cap.
    max_voices: usize,
}

/// Wraps a source and releases one voice slot when rodio drops it (which
/// happens once the source yields `None`).
struct VoiceGuard<S> {
    inner: S,
    voices: Arc<AtomicUsize>,
}

impl<S> VoiceGuard<S> {
    fn new(inner: S, voices: Arc<AtomicUsize>) -> Self {
        Self { inner, voices }
    }
}

impl<S> Drop for VoiceGuard<S> {
    fn drop(&mut self) {
        let previous = self.voices.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "voice counter underflow");
    }
}

impl<S: Iterator> Iterator for VoiceGuard<S> {
    type Item = S::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for VoiceGuard<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

/// Log a stream error without panicking. The device layer may recover on its
/// own (e.g. after a device re-plug); if it does not, subsequent `play` calls
/// fail and are reported by the pipeline's rate-limited logger.
fn log_stream_error(err: rodio::cpal::StreamError) {
    tracing::error!(error = %err, "audio output stream error");
}

impl RodioEngine {
    /// Open an output device and take ownership of the decoded sounds.
    ///
    /// The device is chosen by [`AudioDevice`]; a named device falls back to
    /// the default when no match is found. The builder is retried with the
    /// device's other supported configs if the requested buffer size is
    /// rejected, so a bad buffer choice never leaves the app silent.
    ///
    /// [`AudioBufferSize::LowLatency`] asks the device for the smallest period
    /// it advertises, bypassing the library's ~50 ms safe default.
    pub fn new(
        bank: SoundBank,
        buffer: AudioBufferSize,
        device: AudioDevice,
        volume: f32,
        max_voices: usize,
    ) -> Result<Self, AudioError> {
        if !bank.has_any() {
            return Err(AudioError::NoSounds);
        }

        let device_handle = select_device(&device)?;
        let builder = rodio::stream::DeviceSinkBuilder::from_device(device_handle.clone())?;
        let builder = match buffer {
            AudioBufferSize::LowLatency => match min_supported_period(&device_handle) {
                Some(frames) => {
                    let frames = low_latency_period(frames);
                    tracing::info!(frames, "requesting minimum device period for low latency");
                    builder.with_buffer_size(rodio::cpal::BufferSize::Fixed(frames))
                }
                None => {
                    tracing::warn!(
                        "device does not report a supported buffer range; using its default"
                    );
                    builder
                }
            },
            AudioBufferSize::DeviceDefault => builder,
            AudioBufferSize::Fixed(frames) => {
                builder.with_buffer_size(rodio::cpal::BufferSize::Fixed(frames.get()))
            }
        };
        let mut sink = builder
            .with_error_callback(log_stream_error)
            .open_sink_or_fallback()?;
        sink.log_on_drop(false);

        Ok(Self {
            sink,
            bank,
            voices: Arc::new(AtomicUsize::new(0)),
            volume: volume.clamp(0.0, 1.0),
            max_voices: max_voices.max(1),
        })
    }

    /// Decode every named sound and build an engine.
    ///
    /// A missing or invalid file warns and leaves an empty slot; if all slots
    /// are empty, [`AudioError::NoSounds`] is returned.
    pub fn from_paths(
        dir: &Path,
        paths: &[Option<String>],
        buffer: AudioBufferSize,
        device: AudioDevice,
        volume: f32,
        max_voices: usize,
    ) -> Result<Self, AudioError> {
        let bank = Self::decode_bank(dir, paths);
        Self::new(bank, buffer, device, volume, max_voices)
    }

    /// Decode every named sound without opening a device (for `check`).
    pub fn decode_bank(dir: &Path, paths: &[Option<String>]) -> SoundBank {
        let slots: Vec<Option<Arc<crate::audio::DecodedSound>>> = paths
            .iter()
            .map(|slot| {
                let name = slot.as_deref()?;
                match decoder::decode(&dir.join(name)) {
                    Ok(sound) => Some(Arc::new(sound)),
                    Err(e) => {
                        tracing::warn!(path = name, error = %e, "failed to load sound");
                        None
                    }
                }
            })
            .collect();
        SoundBank::from_slots(slots)
    }
}

/// Resolve the requested device to a concrete cpal device.
///
/// A named device is matched (case-insensitively) against each output device's
/// description; if none matches, the default device is used and a warning is
/// logged so the user knows the selection did not take effect.
fn select_device(device: &AudioDevice) -> Result<rodio::cpal::Device, AudioError> {
    let host = rodio::cpal::default_host();
    match device {
        AudioDevice::Default => host
            .default_output_device()
            .ok_or_else(|| AudioError::Device("no default output device".into())),
        AudioDevice::Named(name) => {
            let devices = host
                .output_devices()
                .map_err(|e| AudioError::Device(format!("cannot enumerate output devices: {e}")))?;
            for candidate in devices {
                let description = candidate
                    .description()
                    .map(|d| d.name().to_owned())
                    .unwrap_or_default();
                if device.matches(&description) {
                    tracing::info!(device = %description, "using audio output device");
                    return Ok(candidate);
                }
            }
            tracing::warn!(
                requested = name,
                "audio output device not found; falling back to default"
            );
            host.default_output_device()
                .ok_or_else(|| AudioError::Device("no default output device".into()))
        }
    }
}

/// The smallest period (in frames) the device advertises, if it reports a
/// range. Returns `None` when the backend cannot say before opening a stream.
fn min_supported_period(device: &rodio::cpal::Device) -> Option<u32> {
    let configs = device.supported_output_configs().ok()?;
    let mut min = u32::MAX;
    for config in configs {
        if let rodio::cpal::SupportedBufferSize::Range { min: lo, .. } = *config.buffer_size() {
            min = min.min(lo);
        }
    }
    (min != u32::MAX).then_some(min)
}

/// Lower bound for the low-latency period, in frames.
///
/// ALSA happily reports a 1-frame minimum, which no real workload can
/// sustain: the callback would underrun on the first scheduling hiccup.
/// 64 frames (1.33 ms at 48 kHz) is the lowest value the latency guidance
/// recommends; below it the audible gain is nil and clicks are guaranteed.
const LOW_LATENCY_MIN_PERIOD: u32 = 64;

/// Clamp the device's advertised minimum to a sustainable floor.
fn low_latency_period(device_min: u32) -> u32 {
    device_min.max(LOW_LATENCY_MIN_PERIOD)
}

/// Reserve a voice slot (`false` at the cap); the CAS loop makes
/// check-then-increment atomic across concurrent `play` callers.
fn reserve_voice(voices: &AtomicUsize, cap: usize) -> bool {
    let mut current = voices.load(Ordering::Acquire);
    loop {
        if current >= cap {
            return false;
        }
        match voices.compare_exchange_weak(
            current,
            current + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return true,
            Err(observed) => current = observed,
        }
    }
}

#[must_use]
pub fn default_buffer() -> AudioBufferSize {
    AudioBufferSize::Fixed(
        crate::audio::buffer::BufferFrames::new(DEFAULT_BUFFER_FRAMES)
            .expect("DEFAULT_BUFFER_FRAMES is non-zero"),
    )
}

impl AudioEngine for RodioEngine {
    fn play(&self, id: SoundId) -> Result<(), AudioError> {
        let sound = self.bank.get(id).ok_or(AudioError::SoundNotLoaded(id))?;

        if !reserve_voice(&self.voices, self.max_voices) {
            return Err(AudioError::TooManyVoices(self.max_voices));
        }

        let source = SharedSamples::new(
            sound.shared_samples(),
            sound.channels(),
            sound.sample_rate(),
        );
        // Per-voice gain applied via the Source trait.
        let source = source.amplify(self.volume);
        self.sink
            .mixer()
            .add(VoiceGuard::new(source, Arc::clone(&self.voices)));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::{NonZeroU16, NonZeroU32};

    fn tiny_source() -> rodio::buffer::SamplesBuffer {
        rodio::buffer::SamplesBuffer::new(
            NonZeroU16::new(1).unwrap(),
            NonZeroU32::new(48_000).unwrap(),
            vec![0.0f32, 0.0],
        )
    }

    #[test]
    fn guard_releases_slot_on_drop() {
        let voices = Arc::new(AtomicUsize::new(1));
        {
            let _guard = VoiceGuard::new(tiny_source(), Arc::clone(&voices));
            assert_eq!(voices.load(Ordering::Acquire), 1);
        }
        assert_eq!(voices.load(Ordering::Acquire), 0);
    }

    #[test]
    fn guard_forwards_samples_and_span() {
        let voices = Arc::new(AtomicUsize::new(1));
        let mut guard = VoiceGuard::new(tiny_source(), Arc::clone(&voices));
        assert_eq!(guard.channels().get(), 1);
        assert_eq!(guard.sample_rate().get(), 48_000);
        assert_eq!(guard.next(), Some(0.0));
        assert_eq!(guard.next(), Some(0.0));
        assert_eq!(guard.next(), None);
        drop(guard);
        assert_eq!(voices.load(Ordering::Acquire), 0);
    }

    #[test]
    fn reserve_voice_stops_at_cap() {
        let voices = AtomicUsize::new(0);
        for _ in 0..MAX_VOICES {
            assert!(reserve_voice(&voices, MAX_VOICES));
        }
        assert_eq!(voices.load(Ordering::Acquire), MAX_VOICES);
        assert!(!reserve_voice(&voices, MAX_VOICES));
        assert_eq!(voices.load(Ordering::Acquire), MAX_VOICES);
        voices.fetch_sub(1, Ordering::AcqRel);
        assert!(reserve_voice(&voices, MAX_VOICES));
        assert_eq!(voices.load(Ordering::Acquire), MAX_VOICES);
    }

    #[test]
    fn reserve_voice_respects_custom_cap() {
        let voices = AtomicUsize::new(0);
        assert!(reserve_voice(&voices, 2));
        assert!(reserve_voice(&voices, 2));
        assert!(!reserve_voice(&voices, 2));
    }

    #[test]
    fn low_latency_period_has_a_sustainable_floor() {
        // ALSA reports a 1-frame minimum; never request it.
        assert_eq!(low_latency_period(1), LOW_LATENCY_MIN_PERIOD);
        assert_eq!(low_latency_period(0), LOW_LATENCY_MIN_PERIOD);
        // A device whose real minimum is above the floor is respected.
        assert_eq!(low_latency_period(256), 256);
        assert_eq!(low_latency_period(LOW_LATENCY_MIN_PERIOD), 64);
    }
}
