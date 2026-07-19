//! Linux backend: evdev (Wayland-safe, keyboard + mouse).
//!
//! Enumerates evdev devices, filters keyboards + mice, opens a per-device
//! event stream, and forwards press-edge events as `InputEvent`s over an mpsc
//! channel. Hotplug is handled by re-enumerating every 3s. SIGINT/SIGTERM end
//! the driver task, which drops the sender and closes the stream.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use evdev::{Device, EventSummary, KeyCode, RelativeAxisCode};
use futures::stream::{BoxStream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::{BackendError, InputBackend};
use crate::domain::{InputEvent, MouseButton};

const CHANNEL_CAP: usize = 1024;
const HOTPLUG_INTERVAL: Duration = Duration::from_secs(3);

/// Linux evdev input backend.
pub struct EvdevBackend {
    enable_trackpads: bool,
}

impl EvdevBackend {
    pub fn new(enable_trackpads: bool) -> Self {
        EvdevBackend { enable_trackpads }
    }
}

impl InputBackend for EvdevBackend {
    fn events(&mut self) -> Result<BoxStream<'static, InputEvent>, BackendError> {
        let enable_trackpads = self.enable_trackpads;

        // Probe once so we can fail fast on a permission problem: if devices
        // exist but not one is openable, that's almost certainly EACCES.
        let candidates: Vec<(PathBuf, Device)> = evdev::enumerate()
            .filter(|(_, dev)| is_interesting(dev))
            .collect();

        if !candidates.is_empty() {
            let any_openable = candidates
                .iter()
                .any(|(path, _)| Device::open(path).is_ok());
            if !any_openable {
                return Err(BackendError::Permission);
            }
        }

        let (tx, rx) = mpsc::channel::<InputEvent>(CHANNEL_CAP);

        tokio::spawn(async move {
            let known: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));

            // Open the devices we already probed.
            for (path, dev) in candidates {
                if spawn_device(
                    path.clone(),
                    dev,
                    enable_trackpads,
                    tx.clone(),
                    known.clone(),
                ) {
                    known.lock().unwrap().insert(path);
                }
            }

            let mut sigint =
                match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("failed to install SIGINT handler: {e}");
                        return;
                    }
                };
            let mut sigterm =
                match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
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
                        for (path, dev) in evdev::enumerate() {
                            let already = known.lock().unwrap().contains(&path);
                            if already || !is_interesting(&dev) {
                                continue;
                            }
                            tracing::info!("new device: {}", path.display());
                            if spawn_device(
                                path.clone(),
                                dev,
                                enable_trackpads,
                                tx.clone(),
                                known.clone(),
                            ) {
                                known.lock().unwrap().insert(path);
                            }
                        }
                    }
                }
            }
            // Dropping `tx` (and all clones held by device tasks eventually) ends the stream.
        });

        Ok(ReceiverStream::new(rx).boxed())
    }

    fn name(&self) -> &'static str {
        "evdev"
    }
}

/// Open a device stream and spawn its forwarding task. Returns `false` if the
/// device could not be opened (logged and skipped). On stream end the task
/// removes its own path from `known` so hotplug can re-open it later.
fn spawn_device(
    path: PathBuf,
    dev: Device,
    enable_trackpads: bool,
    tx: mpsc::Sender<InputEvent>,
    known: Arc<Mutex<HashSet<PathBuf>>>,
) -> bool {
    let is_trackpad = dev
        .name()
        .map(|n| {
            let n = n.to_lowercase();
            n.contains("touchpad") || n.contains("trackpad")
        })
        .unwrap_or(false);

    let mut stream = match dev.into_event_stream() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("cannot open {}: {e}", path.display());
            return false;
        }
    };

    tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            let ev = match item {
                Ok(ev) => ev,
                Err(e) => {
                    tracing::debug!("{} stream ended: {e}", path.display());
                    break;
                }
            };

            // Only the press edge (value == 1).
            let mapped = match ev.destructure() {
                EventSummary::Key(_, code, 1) => {
                    if is_trackpad && !enable_trackpads {
                        Some(InputEvent::Ignored)
                    } else {
                        Some(translate_key(code))
                    }
                }
                _ => None,
            };

            if let Some(event) = mapped
                && tx.send(event).await.is_err()
            {
                break; // receiver dropped
            }
        }
        // Stream ended (device gone / error): let hotplug re-detect it.
        known.lock().unwrap().remove(&path);
    });

    true
}

/// Map an evdev `KeyCode` to our `InputEvent`. Mouse buttons become `Mouse`,
/// everything else is a keyboard `Key(code)`.
fn translate_key(code: KeyCode) -> InputEvent {
    match code {
        KeyCode::BTN_LEFT => InputEvent::Mouse(MouseButton::Left),
        KeyCode::BTN_RIGHT => InputEvent::Mouse(MouseButton::Right),
        KeyCode::BTN_MIDDLE => InputEvent::Mouse(MouseButton::Middle),
        KeyCode::BTN_SIDE | KeyCode::BTN_BACK => InputEvent::Mouse(MouseButton::Back),
        KeyCode::BTN_EXTRA | KeyCode::BTN_FORWARD => InputEvent::Mouse(MouseButton::Forward),
        other => {
            let c = other.code();
            // evdev BTN_* codes live in 0x110..=0x117 (and nearby); treat any
            // remaining button as Mouse::Other, otherwise a keyboard key.
            // ponytail: every 0x110..=0x117 code is a named BTN_ handled above, so
            // this branch only fires for a future/un-named kernel button in range.
            if (0x110..=0x117).contains(&c) {
                InputEvent::Mouse(MouseButton::Other(c))
            } else {
                InputEvent::Key(c)
            }
        }
    }
}

/// A device is interesting if it looks like a keyboard or a mouse.
fn is_interesting(dev: &Device) -> bool {
    is_keyboard(dev) || is_mouse(dev)
}

fn is_keyboard(dev: &Device) -> bool {
    match dev.supported_keys() {
        Some(keys) => keys.contains(KeyCode::KEY_ENTER) && keys.contains(KeyCode::KEY_SPACE),
        None => false,
    }
}

fn is_mouse(dev: &Device) -> bool {
    let has_rel_x = dev
        .supported_relative_axes()
        .map(|axes| axes.contains(RelativeAxisCode::REL_X))
        .unwrap_or(false);
    let has_left = dev
        .supported_keys()
        .map(|keys| keys.contains(KeyCode::BTN_LEFT))
        .unwrap_or(false);
    has_rel_x && has_left
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_key_maps_buttons() {
        assert_eq!(
            translate_key(KeyCode::BTN_LEFT),
            InputEvent::Mouse(MouseButton::Left)
        );
        assert_eq!(
            translate_key(KeyCode::BTN_RIGHT),
            InputEvent::Mouse(MouseButton::Right)
        );
        assert_eq!(
            translate_key(KeyCode::BTN_MIDDLE),
            InputEvent::Mouse(MouseButton::Middle)
        );
        // SIDE/BACK and EXTRA/FORWARD collapse to Back/Forward.
        assert_eq!(
            translate_key(KeyCode::BTN_SIDE),
            InputEvent::Mouse(MouseButton::Back)
        );
        assert_eq!(
            translate_key(KeyCode::BTN_BACK),
            InputEvent::Mouse(MouseButton::Back)
        );
        assert_eq!(
            translate_key(KeyCode::BTN_EXTRA),
            InputEvent::Mouse(MouseButton::Forward)
        );
        assert_eq!(
            translate_key(KeyCode::BTN_FORWARD),
            InputEvent::Mouse(MouseButton::Forward)
        );
    }

    #[test]
    fn translate_key_maps_keyboard() {
        // A normal keyboard keycode maps to Key(code).
        assert_eq!(
            translate_key(KeyCode::KEY_A),
            InputEvent::Key(KeyCode::KEY_A.code())
        );
    }
}
