//! App: owns the audio engine (and its sink), wires the platform backend to the
//! pipeline, and runs until shutdown.
//!
//! Ownership/lifecycle (the rewrite plan's "Ownership" chapter):
//! - `RodioEngine` (and its `MixerDeviceSink`) is owned by `App` → playback only
//!   stops when `App` is dropped.
//! - The backend borrows nothing; it spawns its own tasks and pushes `InputEvent`s
//!   down an mpsc channel.
//! - `Pipeline` holds an `Arc<dyn AudioEngine>` (the `RodioEngine`) + the compiled
//!   config + metrics. The backend stream is consumed by `Pipeline::run`.
//! - Signal handling (SIGINT/SIGTERM) lives in the backend's own task for Linux;
//!   `App::run` awaits the stream to completion, which ends when the backend stops.

use std::path::PathBuf;
use std::sync::Arc;

use crate::audio::{AudioEngine, NullEngine, RodioEngine};
use crate::backend::for_current_platform;
use crate::config::{ConfigSource, FileConfigSource};
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
    ///
    /// Returns the process exit code.
    pub fn run(self) -> i32 {
        let source = FileConfigSource::new(&self.config_dir);
        let config = match source.load() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(dir = %self.config_dir.display(), error = %e, "failed to load config");
                return 1;
            }
        };

        let engine =
            match RodioEngine::from_dir(&self.config_dir, &config.sounds, self.buffer_frames) {
                Ok(e) => Arc::new(e) as Arc<dyn AudioEngine>,
                Err(e) => {
                    // No audio device (e.g. headless WSL, unplugged card). Degrade to
                    // silent monitoring instead of crashing the whole input listener.
                    tracing::warn!(
                        error = %e,
                        "failed to start audio engine; running in SILENT MONITORING mode (no sound)"
                    );
                    eprintln!(
                        "wayclick: no audio output device — running silent. \
                         Set up ALSA→PulseAudio (WSLg) or a sound card for sound."
                    );
                    Arc::new(NullEngine) as Arc<dyn AudioEngine>
                }
            };

        let pipeline = Arc::new(Pipeline::new(config, engine, self.enable_trackpads));

        tracing::info!("wayclick listening");
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(async move {
            let mut backend = for_current_platform(self.enable_trackpads);
            let stream = match backend.events() {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(backend = backend.name(), error = %e, "failed to start input backend");
                    return 1;
                }
            };
            pipeline.run(stream).await;
            0
        })
    }

    /// Headless self-check: load config + decode audio, report, exit.
    ///
    /// Proves "it really runs" without capturing input or needing root. Decoding
    /// needs no audio device, so this works on a headless machine.
    pub fn check(self) -> i32 {
        let source = FileConfigSource::new(&self.config_dir);
        let config = match source.load() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(dir = %self.config_dir.display(), error = %e, "failed to load config");
                return 1;
            }
        };

        // Decode every wav (no audio device required) to prove the asset pack
        // + decoder path works headlessly.
        let decoded = RodioEngine::decode_dir(&self.config_dir, &config.sounds);
        let total = config.sound_count();
        let loaded = decoded.iter().filter(|s| s.is_some()).count();

        let rules = config.rules.len();
        let defaults = config.default_ids.len();
        tracing::info!(
            sounds.total = total,
            sounds.loaded = loaded,
            rules = rules,
            defaults = defaults,
            "config loaded"
        );
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
}
