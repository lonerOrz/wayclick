//! Linux backend: evdev (Wayland-safe, keyboard + mouse).
//!
//! Scans `/dev/input`, opens each keyboard/mouse once, and forwards press-edge
//! events as `InputEvent`s over an mpsc channel. Trackpad presses are never
//! forwarded unless `enable_trackpads` is set. Hotplug re-scans every 3s.
//! SIGINT/SIGTERM end the driver task, which stops the device tasks so every
//! sender drops and the channel closes.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use evdev::{Device, EventStream, EventSummary, KeyCode, RelativeAxisCode};
use futures::stream::{BoxStream, StreamExt};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_stream::wrappers::ReceiverStream;

use super::{BackendError, InputBackend};
use crate::domain::InputEvent;

const CHANNEL_CAP: usize = 1024;
const HOTPLUG_INTERVAL: Duration = Duration::from_secs(3);

/// Linux evdev input backend.
pub struct EvdevBackend {
    enable_trackpads: bool,
    /// The runtime the driver task is spawned on. Held explicitly so `events`
    /// never depends on an ambient runtime context.
    runtime: tokio::runtime::Handle,
}

impl EvdevBackend {
    pub fn new(enable_trackpads: bool, runtime: tokio::runtime::Handle) -> Self {
        EvdevBackend {
            enable_trackpads,
            runtime,
        }
    }
}

impl InputBackend for EvdevBackend {
    fn events(&mut self) -> Result<BoxStream<'static, InputEvent>, BackendError> {
        let enable_trackpads = self.enable_trackpads;

        // Open every device once, up front. Doing the open here (rather than via
        // `evdev::enumerate`, which silently skips unopenable nodes) lets us tell
        // "no input devices" from "no permission", and hands the already-open
        // devices to the driver task without a second open or a probe/use race.
        let (devices, denied) = open_devices();
        let interesting: Vec<(PathBuf, Device)> = devices
            .into_iter()
            .filter(|(_, dev)| is_interesting(dev))
            .collect();

        if interesting.is_empty() && denied {
            return Err(BackendError::Permission);
        }

        let (tx, rx) = mpsc::channel::<InputEvent>(CHANNEL_CAP);

        self.runtime.spawn(async move {
            let known: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));
            let mut tasks: JoinSet<()> = JoinSet::new();

            for (path, dev) in interesting {
                add_device(&mut tasks, path, dev, enable_trackpads, &tx, &known, false);
            }

            let mut sigint = match signal(SignalKind::interrupt()) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("failed to install SIGINT handler: {e}");
                    return;
                }
            };
            let mut sigterm = match signal(SignalKind::terminate()) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("failed to install SIGTERM handler: {e}");
                    return;
                }
            };
            let mut hotplug = tokio::time::interval(HOTPLUG_INTERVAL);
            hotplug.tick().await; // consume the immediate first tick

            loop {
                tokio::select! {
                    _ = sigint.recv() => {
                        tracing::info!("SIGINT received, shutting down evdev backend");
                        break;
                    }
                    _ = sigterm.recv() => {
                        tracing::info!("SIGTERM received, shutting down evdev backend");
                        break;
                    }
                    _ = hotplug.tick() => {
                        for (path, dev) in open_devices().0 {
                            add_device(&mut tasks, path, dev, enable_trackpads, &tx, &known, true);
                        }
                    }
                }
            }

            // Stop forwarding and drop every sender so the receiver sees the
            // channel close and the stream ends.
            tasks.shutdown().await;
        });

        Ok(ReceiverStream::new(rx).boxed())
    }

    fn name(&self) -> &'static str {
        "evdev"
    }
}

/// Start forwarding one device, unless it is already tracked or filtered out.
/// `announce` logs newly hotplugged devices.
fn add_device(
    tasks: &mut JoinSet<()>,
    path: PathBuf,
    dev: Device,
    enable_trackpads: bool,
    tx: &mpsc::Sender<InputEvent>,
    known: &Arc<Mutex<HashSet<PathBuf>>>,
    announce: bool,
) {
    if known.lock().unwrap().contains(&path) || !is_interesting(&dev) {
        return;
    }

    // Trackpad input is off by default. Drop it here — the one place that knows
    // the device — so nothing downstream has to filter. Still record the path so
    // hotplug doesn't keep rescanning it.
    if looks_like_trackpad(&dev) && !enable_trackpads {
        known.lock().unwrap().insert(path);
        return;
    }

    let stream = match dev.into_event_stream() {
        Ok(stream) => stream,
        Err(e) => {
            tracing::warn!("cannot open {}: {e}", path.display());
            return;
        }
    };
    if announce {
        tracing::info!("new device: {}", path.display());
    }
    known.lock().unwrap().insert(path.clone());
    tasks.spawn(forward(path, stream, tx.clone(), known.clone()));
}

/// Forward press-edge events from one device until it disappears, then drop the
/// device from `known` so hotplug can pick it up again.
async fn forward(
    path: PathBuf,
    mut stream: EventStream,
    tx: mpsc::Sender<InputEvent>,
    known: Arc<Mutex<HashSet<PathBuf>>>,
) {
    while let Some(item) = stream.next().await {
        let event = match item {
            Ok(event) => event,
            Err(e) => {
                tracing::debug!("{} stream ended: {e}", path.display());
                break;
            }
        };

        // Only the press edge (value == 1).
        let mapped = match event.destructure() {
            EventSummary::Key(_, code, 1) => Some(translate_key(code)),
            _ => None,
        };

        if let Some(event) = mapped
            && tx.send(event).await.is_err()
        {
            break; // receiver dropped
        }
    }
    known.lock().unwrap().remove(&path);
}

/// Scan `/dev/input/event*`, opening each node. Returns the open devices plus
/// whether any open was denied by permissions.
fn open_devices() -> (Vec<(PathBuf, Device)>, bool) {
    let mut devices = Vec::new();
    let mut denied = false;

    let Ok(entries) = std::fs::read_dir("/dev/input") else {
        return (devices, denied);
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        let is_event = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("event"));
        if !is_event {
            continue;
        }
        match Device::open(&path) {
            Ok(dev) => devices.push((path, dev)),
            Err(e) if e.kind() == ErrorKind::PermissionDenied => denied = true,
            Err(_) => {}
        }
    }
    (devices, denied)
}

/// Trackpads are identified by device name (see [`is_trackpad_name`]).
fn looks_like_trackpad(dev: &Device) -> bool {
    dev.name().is_some_and(is_trackpad_name)
}

/// The trackpad name heuristic, split out so the policy has a test surface.
fn is_trackpad_name(name: &str) -> bool {
    let name = name.to_lowercase();
    name.contains("touchpad") || name.contains("trackpad")
}

/// Map an evdev `KeyCode` to our `InputEvent`. Mouse buttons become `Mouse`,
/// everything else is a keyboard `Key(code)`.
fn translate_key(code: KeyCode) -> InputEvent {
    InputEvent::from_evdev_code(code.code())
}

/// A device is interesting if it looks like a keyboard or a mouse.
fn is_interesting(dev: &Device) -> bool {
    is_keyboard(dev) || is_mouse(dev)
}

fn is_keyboard(dev: &Device) -> bool {
    dev.supported_keys()
        .is_some_and(|keys| keys.contains(KeyCode::KEY_ENTER) && keys.contains(KeyCode::KEY_SPACE))
}

fn is_mouse(dev: &Device) -> bool {
    let has_rel_x = dev
        .supported_relative_axes()
        .is_some_and(|axes| axes.contains(RelativeAxisCode::REL_X));
    let has_left = dev
        .supported_keys()
        .is_some_and(|keys| keys.contains(KeyCode::BTN_LEFT));
    has_rel_x && has_left
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MouseButton;

    #[test]
    fn translate_key_maps_buttons() {
        for (code, button) in [
            (KeyCode::BTN_LEFT, MouseButton::Left),
            (KeyCode::BTN_RIGHT, MouseButton::Right),
            (KeyCode::BTN_MIDDLE, MouseButton::Middle),
            (KeyCode::BTN_SIDE, MouseButton::Back),
            (KeyCode::BTN_BACK, MouseButton::Back),
            (KeyCode::BTN_EXTRA, MouseButton::Forward),
            (KeyCode::BTN_FORWARD, MouseButton::Forward),
        ] {
            assert_eq!(translate_key(code), InputEvent::Mouse(button));
        }
    }

    #[test]
    fn translate_key_maps_keyboard() {
        assert_eq!(
            translate_key(KeyCode::KEY_A),
            InputEvent::Key(KeyCode::KEY_A.code())
        );
    }

    #[test]
    fn detects_trackpad_names() {
        assert!(is_trackpad_name("SynPS/2 Synaptics TouchPad"));
        assert!(is_trackpad_name("Apple Internal Trackpad"));
        assert!(!is_trackpad_name("Logitech USB Keyboard"));
        assert!(!is_trackpad_name(""));
    }
}
