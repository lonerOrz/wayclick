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
    enable_trackpads: bool,
    buffer: AudioBufferSize,
    device: AudioDevice,
}

impl App {
    pub fn new(
        config_dir: PathBuf,
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
            enable_trackpads,
            buffer,
            device,
        }
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

        let sound_paths: Vec<Option<String>> = config
            .catalog
            .iter()
            .map(|(_, spec)| Some(spec.path().display().to_string()))
            .collect();
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
            spawn_config_watcher(self.config_dir.clone(), Arc::clone(&pipeline));

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

        let paths: Vec<Option<String>> = config
            .catalog
            .iter()
            .map(|(_, spec)| Some(spec.path().display().to_string()))
            .collect();
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

/// Poll `config.json` for changes and swap the pipeline's routing snapshot.
///
/// Reload is conservative: the audio engine is not rebuilt, so a config that
/// introduces sound files not already loaded is applied for routing only; the
/// new sound slots resolve to "not loaded" until a restart.
fn spawn_config_watcher(config_dir: PathBuf, pipeline: Arc<Pipeline>) {
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
