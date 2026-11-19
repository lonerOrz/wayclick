//! Linux backend: evdev (Wayland-safe, keyboard + mouse). Scans `/dev/input`,
//! forwards press edges, hotplugs every 3s, stops on SIGINT/SIGTERM. The queue
//! to the session is bounded: a full queue drops the newest event, counted.

use std::collections::HashSet;

use std::io::ErrorKind;
use std::path::PathBuf;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use evdev::{Device, EventStream, EventSummary, KeyCode, RelativeAxisCode};
use futures::stream::StreamExt;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_stream::wrappers::ReceiverStream;

use super::{BackendError, InputBackend};
use crate::domain::InputEvent;
use crate::input::error::InputError;
use crate::input::session::InputSession;
use crate::input::translator;

const CHANNEL_CAP: usize = 1024;
const HOTPLUG_INTERVAL: Duration = Duration::from_secs(3);

/// Why an input event was not delivered to the session queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DropReason {
    ChannelFull,
    ReceiverClosed,
}

/// Per-session drop counters, shared by every device task (warnings are
/// throttled per device, so totals live here).
#[derive(Default)]
struct DropStats {
    channel_full: AtomicU64,
    receiver_closed: AtomicU64,
}

impl DropStats {
    fn record(&self, reason: DropReason) {
        let counter = match reason {
            DropReason::ChannelFull => &self.channel_full,
            DropReason::ReceiverClosed => &self.receiver_closed,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

/// Linux evdev input backend.
pub struct EvdevBackend {
    enable_trackpads: bool,
    /// `events` spawns its driver task here; passed in so it never depends on
    /// an ambient runtime context.
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
    fn events(&mut self) -> Result<InputSession, BackendError> {
        let enable_trackpads = self.enable_trackpads;

        // Open each node directly: `evdev::enumerate` silently skips
        // unopenable nodes, which would hide "no permission" from "no devices".
        let (devices, denied) = open_devices();
        let mut interesting: Vec<(PathBuf, Device)> = Vec::new();
        for (path, dev) in devices {
            if is_interesting(&dev) {
                interesting.push((path, dev));
            } else {
                debug_skip_uninteresting(&path, &dev);
            }
        }

        if interesting.is_empty() && denied {
            return Err(BackendError::Permission);
        }

        let (tx, rx) = mpsc::channel::<InputEvent>(CHANNEL_CAP);
        let drop_stats = Arc::new(DropStats::default());
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();

        let handle = self.runtime.spawn(async move {
            let known: Arc<Mutex<HashSet<PathBuf>>> = Arc::new(Mutex::new(HashSet::new()));
            let mut tasks: JoinSet<()> = JoinSet::new();

            let ctx = DeviceContext {
                enable_trackpads,
                tx: tx.clone(),
                known: Arc::clone(&known),
                drop_stats: Arc::clone(&drop_stats),
            };

            for (path, dev) in interesting {
                add_device(&mut tasks, path, dev, &ctx, false);
            }

            let mut sigint = signal(SignalKind::interrupt()).ok();
            let mut sigterm = signal(SignalKind::terminate()).ok();
            if sigint.is_none() {
                tracing::warn!("failed to install SIGINT handler");
            }
            if sigterm.is_none() {
                tracing::warn!("failed to install SIGTERM handler");
            }

            let mut hotplug = tokio::time::interval(HOTPLUG_INTERVAL);
            hotplug.tick().await; // consume the immediate first tick
            let mut stop_rx = stop_rx;

            loop {
                tokio::select! {
                    _ = &mut stop_rx => {
                        tracing::info!("evdev backend stop requested");
                        break;
                    }
                    _ = async {
                        match sigint.as_mut() {
                            Some(s) => s.recv().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        tracing::info!("SIGINT received, shutting down evdev backend");
                        break;
                    }
                    _ = async {
                        match sigterm.as_mut() {
                            Some(s) => s.recv().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        tracing::info!("SIGTERM received, shutting down evdev backend");
                        break;
                    }
                    _ = hotplug.tick() => {
                        for (path, dev) in open_devices().0 {
                            add_device(&mut tasks, path, dev, &ctx, true);
                        }
                    }
                }
            }

            tasks.shutdown().await;
        });

        let stop: Box<dyn FnOnce() + Send> = Box::new(move || {
            let _ = stop_tx.send(());
        });
        let join: crate::input::session::JoinFn = Box::new(move || {
            Box::pin(async move {
                match handle.await {
                    Ok(()) => Ok(()),
                    Err(e) if e.is_cancelled() => Ok(()),
                    Err(_) => Err(InputError::WorkerPanicked),
                }
            })
        });
        Ok(InputSession::new(
            ReceiverStream::new(rx).boxed(),
            stop,
            join,
        ))
    }

    fn name(&self) -> &'static str {
        "evdev"
    }
}

/// Shared context passed to every device accepted by the driver task.
struct DeviceContext {
    enable_trackpads: bool,
    tx: mpsc::Sender<InputEvent>,
    known: Arc<Mutex<HashSet<PathBuf>>>,
    drop_stats: Arc<DropStats>,
}

/// Start forwarding one device, unless it is already tracked or filtered out.
/// `announce` logs newly hotplugged devices.
fn add_device(
    tasks: &mut JoinSet<()>,
    path: PathBuf,
    dev: Device,
    ctx: &DeviceContext,
    announce: bool,
) {
    let DeviceContext {
        enable_trackpads,
        tx,
        known,
        drop_stats,
    } = ctx;
    // A poisoned tracking lock must not panic the driver task.
    let Ok(mut known_guard) = known.lock() else {
        tracing::warn!("device tracking lock poisoned; skipping {path:?}");
        return;
    };
    if known_guard.contains(&path) {
        return;
    }
    if !is_interesting(&dev) {
        debug_skip_uninteresting(&path, &dev);
        return;
    }

    // Record trackpads even when disabled so hotplug does not rescan them.
    if looks_like_trackpad(&dev) && !*enable_trackpads {
        known_guard.insert(path);
        return;
    }

    let stream = match dev.into_event_stream() {
        Ok(stream) => stream,
        Err(e) => {
            if e.kind() == ErrorKind::PermissionDenied {
                tracing::warn!(
                    device = %path.display(),
                    "permission denied opening input device; check udev rules or group membership"
                );
            } else {
                tracing::warn!("cannot open {}: {e}", path.display());
            }
            return;
        }
    };
    if announce {
        tracing::info!("new device: {}", path.display());
    }
    known_guard.insert(path.clone());
    tasks.spawn(forward(
        path,
        stream,
        tx.clone(),
        known.clone(),
        Arc::clone(drop_stats),
    ));
}

/// Forward press-edge events from one device until it disappears, then drop it
/// from `known` so hotplug can pick it up again.
async fn forward(
    path: PathBuf,
    mut stream: EventStream,
    tx: mpsc::Sender<InputEvent>,
    known: Arc<Mutex<HashSet<PathBuf>>>,
    drop_stats: Arc<DropStats>,
) {
    // `None` = not warned yet; warnings throttled to one per second per device.
    let mut last_full_warn: Option<Instant> = None;

    while let Some(item) = stream.next().await {
        let event = match item {
            Ok(event) => event,
            Err(e) => {
                tracing::debug!("{} stream ended: {e}", path.display());
                break;
            }
        };

        let mapped = match event.destructure() {
            EventSummary::Key(_, code, 1) => Some(translate_key(code)),
            _ => None,
        };

        let Some(event) = mapped else { continue };

        match tx.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                drop_stats.record(DropReason::ChannelFull);
                let now = Instant::now();
                let due =
                    last_full_warn.is_none_or(|t| now.duration_since(t) >= Duration::from_secs(1));
                if due {
                    tracing::warn!(
                        device = %path.display(),
                        dropped = drop_stats.channel_full.load(Ordering::Relaxed),
                        "input queue full; dropping events (real-time priority)"
                    );
                    last_full_warn = Some(now);
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                drop_stats.record(DropReason::ReceiverClosed);
                break;
            }
        }
    }
    if let Ok(mut known_guard) = known.lock() {
        known_guard.remove(&path);
    } else {
        tracing::warn!("device tracking lock poisoned; could not remove {path:?}");
    }
}

/// Scan `/dev/input/event*` and open each node, tracking permission denials.
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
            Err(e) if e.kind() == ErrorKind::PermissionDenied => {
                tracing::debug!(
                    device = %path.display(),
                    "cannot scan input device (permission denied); check udev rules"
                );
                denied = true;
            }
            Err(_) => {}
        }
    }
    (devices, denied)
}

fn looks_like_trackpad(dev: &Device) -> bool {
    dev.name().is_some_and(is_trackpad_name)
}

/// The trackpad name heuristic.
fn is_trackpad_name(name: &str) -> bool {
    let name = name.to_lowercase();
    name.contains("touchpad") || name.contains("trackpad")
}

/// Map an evdev `KeyCode` to our `InputEvent`.
fn translate_key(code: KeyCode) -> InputEvent {
    translator::linux::from_event_code(code.code())
}

/// A device is interesting if it looks like a keyboard or a mouse (see the
/// classifiers below for the heuristic).
fn is_interesting(dev: &Device) -> bool {
    is_keyboard(dev) || is_mouse(dev)
}

/// Log why a device failed the (heuristic) keyboard/mouse classification.
fn debug_skip_uninteresting(path: &std::path::Path, dev: &Device) {
    tracing::debug!(
        device = %path.display(),
        name = dev.name().unwrap_or("<unnamed>"),
        has_keys = ?dev.supported_keys().map(|k| k.iter().count()),
        has_rel_axes = ?dev.supported_relative_axes().map(|a| a.iter().count()),
        "device classified as uninteresting; skipping"
    );
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
    use crate::domain::{MouseButton, PhysicalKey};

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
            assert_eq!(translate_key(code), InputEvent::MouseDown(button));
        }
    }

    #[test]
    fn translate_key_maps_keyboard() {
        assert_eq!(
            translate_key(KeyCode::KEY_A),
            InputEvent::KeyDown(PhysicalKey::A)
        );
    }

    #[test]
    fn detects_trackpad_names() {
        assert!(is_trackpad_name("SynPS/2 Synaptics TouchPad"));
        assert!(is_trackpad_name("Apple Internal Trackpad"));
        assert!(!is_trackpad_name("Logitech USB Keyboard"));
        assert!(!is_trackpad_name(""));
    }

    #[test]
    fn drop_stats_counts_each_reason() {
        let stats = DropStats::default();
        assert_eq!(stats.channel_full.load(Ordering::Relaxed), 0);
        assert_eq!(stats.receiver_closed.load(Ordering::Relaxed), 0);

        stats.record(DropReason::ChannelFull);
        stats.record(DropReason::ChannelFull);
        stats.record(DropReason::ReceiverClosed);

        assert_eq!(stats.channel_full.load(Ordering::Relaxed), 2);
        assert_eq!(stats.receiver_closed.load(Ordering::Relaxed), 1);
    }
}
