//! Audio buffer size and output-device selection.

/// Requested output buffer size.
///
/// cpal/ALSA interprets a fixed size as the *period* with a 2-period ring
/// buffer, so the audible latency is roughly `2 × frames / sample_rate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioBufferSize {
    /// Automatically select a conservative low-latency period.
    ///
    /// Prefer 256 frames, clamped to the default output device's advertised
    /// period range. If the range is unknown, use Rodio's safe default.
    Auto,
    /// Request the smallest advertised period, with a 64-frame safety floor.
    /// This is what `--buffer-frames 0` selects.
    ///
    /// Unlike [`Self::DeviceDefault`], it bypasses the library's ~50 ms
    /// "safe" default, which would add tens of milliseconds of delay.
    LowLatency,
    /// Let the driver pick (cpal `BufferSize::Default`). On PipeWire/ALSA
    /// this is usually a few hundred frames; when the library opens the sink
    /// itself it may substitute its own ~50 ms safe size.
    DeviceDefault,
    /// Request a fixed number of frames (must be non-zero).
    Fixed(BufferFrames),
}

/// A non-zero frame count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferFrames(std::num::NonZeroU32);

impl BufferFrames {
    /// Build a frame count; `0` is rejected.
    pub fn new(frames: u32) -> Option<Self> {
        std::num::NonZeroU32::new(frames).map(Self)
    }

    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// Which output device to open.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AudioDevice {
    /// The system's default output device.
    #[default]
    Default,
    /// A device whose name contains this substring (case-insensitive).
    Named(String),
}

impl AudioDevice {
    /// Whether `description` (a device name) matches this selection.
    #[must_use]
    pub fn matches(&self, description: &str) -> bool {
        match self {
            Self::Default => true,
            Self::Named(needle) => description.to_lowercase().contains(&needle.to_lowercase()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_frames_rejected() {
        assert!(BufferFrames::new(0).is_none());
    }

    #[test]
    fn nonzero_frames_accepted() {
        assert_eq!(BufferFrames::new(512).unwrap().get(), 512);
    }

    #[test]
    fn device_name_matching_is_case_insensitive() {
        assert!(AudioDevice::Named("usb".into()).matches("Speakers (USB Audio)"));
        assert!(!AudioDevice::Named("usb".into()).matches("HDMI Output"));
        assert!(AudioDevice::Default.matches("anything"));
    }
}
