//! App: composition root — wires the platform backend to the pipeline.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::audio::buffer::BufferFrames;
use crate::audio::rodio::engine::default_buffer;
use crate::audio::{AudioBufferSize, AudioDevice, AudioEngine, AudioError, RodioEngine};
use crate::backend::for_current_platform;
use crate::config::{
    CompiledConfig, ConfigDiagnostic, ConfigError, ConfigSource, FileConfigSource,
};
use crate::dispatch::{Pipeline, RuntimeSnapshot};
use crate::input::error::InputError;
use futures::StreamExt;

/// How often the config file is polled for hot-reload changes.
const RELOAD_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Errors from application startup or shutdown.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("configuration failed: {0}")]
    Config(#[from] ConfigError),
    #[error("audio initialization failed: {0}")]
    Audio(#[from] AudioError),
    #[error("input initialization failed: {0}")]
    #[allow(dead_code)]
    Input(#[from] InputError),
    #[error("runtime initialization failed: {0}")]
    Runtime(#[from] std::io::Error),
    #[error("input session has no event stream")]
    NoEventStream,
    #[error("{failed}/{total} sounds failed to decode")]
    AudioDecodeFailures { failed: usize, total: usize },
}

pub struct App {
    config_dir: PathBuf,
    /// Directories searched for relative sound paths (config dir first, then
    /// the bundled dir).
    sound_dirs: Vec<PathBuf>,
    enable_trackpads: bool,
    buffer: AudioBufferSize,
    device: AudioDevice,
}

impl App {
    pub fn new(
        config_dir: PathBuf,
        sound_dirs: Vec<PathBuf>,
        enable_trackpads: bool,
        buffer_frames: Option<u32>,
        audio_device: Option<String>,
    ) -> App {
        // CLI semantics: None keeps the historical fixed 512-frame default;
        // `Some(0)` requests the device's own buffer size; any other value
        // fixes that many frames.
        let buffer = match buffer_frames {
            None => default_buffer(),
            Some(0) => AudioBufferSize::DeviceDefault,
            Some(frames) => match BufferFrames::new(frames) {
                Some(frames) => AudioBufferSize::Fixed(frames),
                None => default_buffer(),
            },
        };
        let device = match audio_device {
            Some(name) => AudioDevice::Named(name),
            None => AudioDevice::Default,
        };
        App {
            config_dir,
            sound_dirs,
            enable_trackpads,
            buffer,
            device,
        }
    }

    /// Resolve each compiled sound path against the search directories.
    ///
    /// Absolute paths pass through untouched; relative paths return the first
    /// existing match, falling back to the config-dir join (so decode errors
    /// name a sensible path instead of silently vanishing).
    fn resolve_sound_paths(&self, config: &CompiledConfig) -> Vec<Option<String>> {
        resolve_sound_paths(&self.sound_dirs, config)
    }

    /// Run the input listener until a shutdown signal or the stream ends.
    ///
    /// Shutdown order: signal → request backend stop → drain the stream (the
    /// worker closes it on exit) → join the worker. This order works on all
    /// platforms.
    pub fn run(self) -> Result<(), AppError> {
        let (config, warnings) = self.load_config()?;
        for warning in &warnings {
            tracing::warn!("config diagnostic: {warning}");
        }

        let sound_paths = self.resolve_sound_paths(&config);
        let engine = RodioEngine::from_paths(
            &self.config_dir,
            &sound_paths,
            self.buffer,
            self.device,
            config.volume(),
            config.max_voices as usize,
        )?;
        let engine = Arc::new(engine) as Arc<dyn AudioEngine>;

        let pipeline = Arc::new(Pipeline::new(config, engine));
        let metrics = pipeline.metrics();

        // Evdev spawns its driver task on this handle, so the backend must be
        // constructed after the runtime exists.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;

        let mut backend = for_current_platform(self.enable_trackpads, rt.handle().clone());
        let backend_name = backend.name();
        let mut session = backend.events().map_err(|e| {
            tracing::error!(backend = backend_name, error = %e, "failed to start input backend");
            AppError::Input(InputError::Start(e.to_string()))
        })?;
        let stream = session.take_events().ok_or(AppError::NoEventStream)?;

        tracing::info!(backend = backend_name, "wayclick listening");
        rt.block_on(async {
            let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
            tokio::spawn(async move {
                wait_for_shutdown_signal().await;
                let _ = stop_tx.send(());
            });

            // Hot reload: poll the config file's mtime and swap the routing
            // snapshot when it changes.
            spawn_config_watcher(
                self.config_dir.clone(),
                self.sound_dirs.clone(),
                sound_paths,
                Arc::clone(&pipeline),
            );

            let mut stream = stream;
            // `Some` keeps the select branch live; a completed oneshot polled
            // again would panic, so it is set to `None` after firing.
            let mut stop_rx = Some(stop_rx);
            loop {
                tokio::select! {
                    maybe_event = stream.next() => {
                        match maybe_event {
                            Some(event) => pipeline.handle(event),
                            None => break,
                        }
                    }
                    _ = async {
                        match stop_rx.as_mut() {
                            Some(rx) => { let _ = rx.await; }
                            None => std::future::pending().await,
                        }
                    } => {
                        stop_rx = None;
                        tracing::info!("shutdown requested; stopping backend");
                        session.request_stop();
                    }
                }
            }
        });

        rt.block_on(session.shutdown()).map_err(AppError::Input)?;

        let snapshot = metrics.snapshot();
        let latency = snapshot.dispatch_latency;
        tracing::info!(
            received = snapshot.received_events,
            matched = snapshot.matched_events,
            play_requests = snapshot.play_requests,
            failed_plays = snapshot.failed_plays,
            dispatch_p50_ns = latency.percentile_bound(50),
            dispatch_p95_ns = latency.percentile_bound(95),
            dispatch_p99_ns = latency.percentile_bound(99),
            "shutdown"
        );
        Ok(())
    }

    /// Headless self-check: load config + decode audio. Needs no audio device.
    pub fn check(self) -> Result<(), AppError> {
        let (config, warnings) = self.load_config()?;

        let paths = self.resolve_sound_paths(&config);
        let bank = RodioEngine::decode_bank(&self.config_dir, &paths);
        let total = config.sound_count();
        let loaded = config
            .catalog
            .iter()
            .filter(|(sid, _)| bank.get(*sid).is_some())
            .count();
        let rules = config.rules.len();
        let defaults = config.default_ids.len();

        tracing::info!(loaded, total, rules, defaults, "audio decoded");
        println!("wayclick check:");
        println!("  sounds: {loaded}/{total} decoded");
        println!("  rules:  {rules}");
        println!("  defaults: {defaults}");
        println!("  warnings: {}", warnings.len());
        for warning in &warnings {
            println!("    {warning}");
        }
        for (sid, spec) in config.catalog.iter() {
            let line = format!("  sound {}: {}", sid.index() + 1, spec.path().display());
            match bank.get(sid) {
                Some(_) => println!("{line} [ok]"),
                None => println!("{line} [MISSING/INVALID]"),
            }
        }
        if loaded < total {
            return Err(AppError::AudioDecodeFailures {
                failed: total - loaded,
                total,
            });
        }
        Ok(())
    }

    /// Load the config and return its non-fatal diagnostics.
    ///
    /// Errors are rejected inside the loader. Display of warnings is left to
    /// the caller: `run` logs them via tracing, `check` prints them in its
    /// human-readable report.
    fn load_config(&self) -> Result<(CompiledConfig, Vec<ConfigDiagnostic>), AppError> {
        let source = FileConfigSource::new(&self.config_dir);
        let (config, validated) = source.load_with_diagnostics()?;
        let warnings: Vec<ConfigDiagnostic> = validated
            .diagnostics
            .iter()
            .filter(|d| d.severity == crate::config::diagnostic::DiagnosticSeverity::Warning)
            .cloned()
            .collect();
        tracing::info!(
            dir = %self.config_dir.display(),
            rules = config.rules.len(),
            sounds = config.sound_count(),
            warnings = warnings.len(),
            "config loaded"
        );
        Ok((config, warnings))
    }
}

/// Free-standing path resolution so the hot-reload watcher can compare the
/// new config's sound catalog against the engine's startup catalog.
fn resolve_sound_paths(sound_dirs: &[PathBuf], config: &CompiledConfig) -> Vec<Option<String>> {
    config
        .catalog
        .iter()
        .map(|(_, spec)| {
            let path = spec.path();
            if path.is_absolute() {
                return Some(path.display().to_string());
            }
            for dir in sound_dirs {
                let candidate = dir.join(path);
                if candidate.exists() {
                    // Absolute so downstream `config_dir.join(path)` in the
                    // decoder passes the path through untouched.
                    let absolute = std::fs::canonicalize(&candidate)
                        .unwrap_or_else(|_| std::path::absolute(&candidate).unwrap_or(candidate));
                    return Some(absolute.display().to_string());
                }
            }
            // Not found anywhere: keep the config-dir join so the decoder
            // reports the intended location.
            Some(sound_dirs[0].join(path).display().to_string())
        })
        .collect()
}

/// Poll `config.json` for changes and swap the pipeline's routing snapshot.
///
/// Reload only applies when the sound catalog is unchanged: the audio engine
/// keeps the bank decoded at startup, and `SoundId`s are bank positions, so a
/// config that adds, removes, or reorders sounds would shift every id and
/// trigger wrong sounds. Such reloads are rejected with a restart hint.
/// Routing-only changes (keys, defaults, policy, mappings to existing sounds)
/// apply immediately.
fn spawn_config_watcher(
    config_dir: PathBuf,
    sound_dirs: Vec<PathBuf>,
    sound_paths: Vec<Option<String>>,
    pipeline: Arc<Pipeline>,
) {
    tokio::spawn(async move {
        let source = FileConfigSource::new(&config_dir);
        let mut last_mtime = source.last_modified();
        loop {
            tokio::time::sleep(RELOAD_POLL_INTERVAL).await;
            let current = source.last_modified();
            if current == last_mtime {
                continue;
            }
            last_mtime = current;
            match source.load_with_diagnostics() {
                Ok((config, validated)) => {
                    for d in &validated.diagnostics {
                        if d.severity == crate::config::diagnostic::DiagnosticSeverity::Warning {
                            tracing::warn!("config reload diagnostic: {d}");
                        }
                    }
                    // Compare the *resolved* sound paths: identical catalogs
                    // mean identical SoundId assignment.
                    let new_paths = resolve_sound_paths(&sound_dirs, &config);
                    if new_paths != sound_paths {
                        tracing::warn!(
                            "config reload skipped: sound set changed (restart to load new sounds)"
                        );
                        continue;
                    }
                    let snapshot = Arc::new(RuntimeSnapshot::from_config(&config));
                    pipeline.replace_snapshot(snapshot);
                    tracing::info!(
                        rules = config.rules.len(),
                        sounds = config.sound_count(),
                        "config reloaded"
                    );
                }
                Err(e) => {
                    // Keep the old snapshot: a broken edit must not stop playback.
                    tracing::warn!(error = %e, "config reload failed; keeping previous config");
                }
            }
        }
    });
}

/// Wait for SIGINT or SIGTERM.
#[cfg(unix)]
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut sigint = signal(SignalKind::interrupt()).ok();
    let mut sigterm = signal(SignalKind::terminate()).ok();
    tokio::select! {
        _ = async {
            match sigint.as_mut() {
                Some(s) => s.recv().await,
                None => std::future::pending().await,
            }
        } => {
            tracing::info!("SIGINT received");
        }
        _ = async {
            match sigterm.as_mut() {
                Some(s) => s.recv().await,
                None => std::future::pending().await,
            }
        } => {
            tracing::info!("SIGTERM received");
        }
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_ok() {
        tracing::info!("Ctrl+C received");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::from_str;

    /// The hot-reload guard: a config whose sound set differs from the engine's
    /// startup catalog must be detected as changed, because SoundIds are bank
    /// positions and would silently shift onto wrong sounds.
    #[test]
    fn sound_set_change_is_detectable() {
        let dirs = vec![PathBuf::from("assets/default")];
        let before =
            from_str(r#"{"defaults":["key1.wav","key3.wav"],"mappings":{"30":"key1.wav"}}"#)
                .unwrap();
        let after_reordered =
            from_str(r#"{"defaults":["key3.wav","key1.wav"],"mappings":{"30":"key1.wav"}}"#)
                .unwrap();
        let after_added =
            from_str(r#"{"defaults":["key1.wav","key3.wav"],"mappings":{"30":"key2.wav"}}"#)
                .unwrap();

        let baseline = resolve_sound_paths(&dirs, &before);
        // These files exist in assets/default, so resolution succeeds.
        assert!(baseline.iter().all(|p| p.is_some()));

        assert_ne!(
            resolve_sound_paths(&dirs, &after_reordered),
            baseline,
            "reordering defaults changes SoundId assignment and must be detected"
        );
        assert_ne!(
            resolve_sound_paths(&dirs, &after_added),
            baseline,
            "a different sound sequence changes SoundId assignment and must be detected"
        );

        // Same sound sequence, different routing: must NOT be flagged.
        let rerouted =
            from_str(r#"{"defaults":["key1.wav","key3.wav"],"mappings":{"31":"key1.wav"}}"#)
                .unwrap();
        assert_eq!(
            resolve_sound_paths(&dirs, &rerouted),
            baseline,
            "routing-only changes keep the catalog and are safe to hot-reload"
        );
    }
}
