//! App: composition root — owns the audio engine, wires the platform backend to
//! the pipeline, and runs until shutdown.
//!
//! Lifecycle: `RodioEngine` (and its `MixerDeviceSink`) is owned here, so
//! playback stops only when `App` is dropped. The backend spawns its own tasks
//! and pushes `InputEvent`s down a channel; `App` drives the pipeline until the
//! stream ends (the backend's own signal handling stops it).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::audio::{AudioEngine, RodioEngine};
use crate::backend::for_current_platform;
use crate::config::{CompiledConfig, ConfigSource, FileConfigSource};
use crate::domain::SoundId;
use crate::pipeline::Pipeline;

pub struct App {
    config_dir: PathBuf,
    enable_trackpads: bool,
    buffer_frames: Option<u32>,
}

impl App {
    pub fn new(config_dir: PathBuf, enable_trackpads: bool, buffer_frames: Option<u32>) -> App {
        App {
            config_dir,
            enable_trackpads,
            buffer_frames,
        }
    }

    /// Load config, build the engine + pipeline, and run the input listener.
    /// Returns the process exit code.
    pub fn run(self) -> i32 {
        let Some(config) = self.load_config() else {
            return 1;
        };

        let engine = match RodioEngine::from_dir(
            &self.config_dir,
            &config.sounds,
            self.buffer_frames,
        ) {
            Ok(engine) => Arc::new(engine) as Arc<dyn AudioEngine>,
            Err(e) => {
                tracing::error!(error = %e, "failed to start audio engine");
                eprintln!(
                    "wayclick: cannot open an audio output device. \
                     Is a sound card / PulseAudio running? (try `wayclick check` for a headless self-test)"
                );
                return 1;
            }
        };

        let pipeline = Pipeline::new(config, engine);
        let metrics = pipeline.metrics();

        // Build the runtime first and hand it to the backend: the Linux backend
        // spawns its driver task on it, so `events()` must not rely on an ambient
        // runtime context. The pipeline is then driven on the same runtime.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");

        let mut backend = for_current_platform(self.enable_trackpads, rt.handle().clone());

        let stream = match backend.events() {
            Ok(stream) => stream,
            Err(e) => {
                tracing::error!(backend = backend.name(), error = %e, "failed to start input backend");
                return 1;
            }
        };

        tracing::info!(backend = backend.name(), "wayclick listening");
        rt.block_on(async move {
            pipeline.run(stream).await;
        });

        tracing::info!(
            received = metrics.received.load(Ordering::Relaxed),
            played = metrics.played.load(Ordering::Relaxed),
            "shutdown"
        );
        0
    }

    /// Headless self-check: load config + decode audio, report, exit. Decoding
    /// needs no audio device, so this works on a headless machine.
    pub fn check(self) -> i32 {
        let Some(config) = self.load_config() else {
            return 1;
        };

        let decoded = RodioEngine::decode_dir(&self.config_dir, &config.sounds);
        let total = config.sound_count();
        let loaded = decoded.iter().filter(|slot| slot.is_some()).count();
        let rules = config.rules.len();
        let defaults = config.default_ids.len();

        tracing::info!(loaded, total, rules, defaults, "audio decoded");
        println!("wayclick check:");
        println!("  sounds: {loaded}/{total} decoded");
        println!("  rules:  {rules}");
        println!("  defaults: {defaults}");
        for idx in 1..=total {
            if let Some(sid) = SoundId::new(idx) {
                match (config.filename(sid), decoded.get((idx - 1) as usize)) {
                    (Some(name), Some(Some(_))) => println!("  sound {idx}: {name} [ok]"),
                    (Some(name), _) => println!("  sound {idx}: {name} [MISSING/INVALID]"),
                    _ => {}
                }
            }
        }
        0
    }

    /// Load and log the config, shared by `run` and `check`.
    fn load_config(&self) -> Option<CompiledConfig> {
        let source = FileConfigSource::new(&self.config_dir);
        match source.load() {
            Ok(config) => {
                tracing::info!(
                    dir = %self.config_dir.display(),
                    rules = config.rules.len(),
                    sounds = config.sounds.len(),
                    "config loaded"
                );
                Some(config)
            }
            Err(e) => {
                tracing::error!(dir = %self.config_dir.display(), error = %e, "failed to load config");
                None
            }
        }
    }
}
