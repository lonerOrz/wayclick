//! Rodio-backed audio engine.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::source::Source;
use rodio::stream::MixerDeviceSink;
use rodio::{ChannelCount, SampleRate};

use crate::audio::decoder;
use crate::audio::rodio::source::SharedSamples;
use crate::audio::{AudioBufferSize, AudioDevice, AudioEngine, AudioError, SoundBank};
use crate::domain::SoundId;

/// Default cap on concurrent sounds when the config does not specify one.
pub const MAX_VOICES: usize = 32;

/// A rodio-backed [`AudioEngine`].
pub struct RodioEngine {
    /// Owns the output stream and mixer.
    sink: MixerDeviceSink,
    /// Decoded PCM sound bank.
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

impl RodioEngine {
    /// Open an output device and take ownership of the decoded sounds.
    ///
    /// The device is chosen by [`AudioDevice`]; a named device falls back to
    /// the default when no match is found. The buffer mode selects either an
    /// automatic device-clamped period, the lowest safe period, the device
    /// default, or an explicit frame count.
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
        let period_range = default_period_range(&device_handle);

        let builder = rodio::stream::DeviceSinkBuilder::from_device(device_handle)?;
        let builder = match buffer {
            AudioBufferSize::Auto => match choose_auto_period(period_range) {
                Some(frames) => {
                    tracing::info!(frames, "automatically selected audio period");
                    builder.with_buffer_size(rodio::cpal::BufferSize::Fixed(frames))
                }
                None => {
                    tracing::warn!(
                        "cannot determine supported period range; using Rodio safe default"
                    );
                    builder
                }
            },
            AudioBufferSize::LowLatency => match choose_low_latency_period(period_range) {
                Some(frames) => {
                    tracing::info!(frames, "requesting lowest safe device period");
                    builder.with_buffer_size(rodio::cpal::BufferSize::Fixed(frames))
                }
                None => {
                    tracing::warn!("no usable low-latency period; using Rodio safe default");
                    builder
                }
            },
            AudioBufferSize::DeviceDefault => builder,
            AudioBufferSize::Fixed(frames) => {
                builder.with_buffer_size(rodio::cpal::BufferSize::Fixed(frames.get()))
            }
        };

        // Log each distinct terminal error once. CPAL 0.18 already recovers
        // from ALSA polling errors via underrun recovery instead of looping;
        // this flag only suppresses duplicate log lines for repeated reports.
        let reported = Arc::new(AtomicBool::new(false));
        let mut sink = builder
            .with_error_callback({
                let reported = Arc::clone(&reported);
                move |err: rodio::CpalError| {
                    if !reported.swap(true, Ordering::Relaxed) {
                        tracing::error!(
                            error = %err,
                            kind = ?err.kind(),
                            "audio output stream error"
                        );
                    }
                }
            })
            .open_sink_or_fallback()?;
        sink.log_on_drop(false);

        tracing::info!(config = ?sink.config(), "audio output initialized");

        Ok(Self {
            sink,
            bank,
            voices: Arc::new(AtomicUsize::new(0)),
            volume: volume.clamp(0.0, 1.0),
            max_voices: max_voices.max(1),
        })
    }

    /// Decode every named sound and build an engine.
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
        let source = source.amplify(self.volume);
        self.sink
            .mixer()
            .add(VoiceGuard::new(source, Arc::clone(&self.voices)));
        Ok(())
    }
}

/// Resolve the requested device to a concrete cpal device.
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

/// Read the buffer-size range for the device's actual default output config.
fn default_period_range(device: &rodio::cpal::Device) -> Option<(u32, u32)> {
    let config = device.default_output_config().ok()?;
    match *config.buffer_size() {
        rodio::cpal::SupportedBufferSize::Range { min, max } => Some((min, max)),
        _ => None,
    }
}

/// Initial automatic target: 5.33 ms per period at 48 kHz.
const AUTO_TARGET_PERIOD: u32 = 256;

/// Do not blindly use pathological hardware minima.
const LOW_LATENCY_MIN_PERIOD: u32 = 64;

/// Select a conservative period within the supported range.
fn choose_auto_period(range: Option<(u32, u32)>) -> Option<u32> {
    let (min, max) = range?;
    if min > max {
        return None;
    }
    Some(AUTO_TARGET_PERIOD.clamp(min, max))
}

/// Select the lowest period that respects our safety floor.
fn choose_low_latency_period(range: Option<(u32, u32)>) -> Option<u32> {
    let (min, max) = range?;
    if min > max {
        return None;
    }
    let frames = min.max(LOW_LATENCY_MIN_PERIOD);
    (frames <= max).then_some(frames)
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

/// Default buffer mode: automatic, device-clamped period selection.
pub fn default_buffer() -> AudioBufferSize {
    AudioBufferSize::Auto
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
    fn auto_period_prefers_256_frames() {
        assert_eq!(choose_auto_period(Some((64, 1024))), Some(256));
    }

    #[test]
    fn auto_period_respects_device_minimum() {
        assert_eq!(choose_auto_period(Some((512, 2048))), Some(512));
    }

    #[test]
    fn auto_period_respects_device_maximum() {
        assert_eq!(choose_auto_period(Some((64, 128))), Some(128));
    }

    #[test]
    fn auto_period_handles_unknown_range() {
        assert_eq!(choose_auto_period(None), None);
    }

    #[test]
    fn low_latency_period_has_a_sustainable_floor() {
        assert_eq!(choose_low_latency_period(Some((1, 1024))), Some(64));
        assert_eq!(choose_low_latency_period(Some((256, 1024))), Some(256));
        assert_eq!(choose_low_latency_period(Some((1, 32))), None);
    }
}
