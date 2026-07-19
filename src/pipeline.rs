//! Pipeline: filter -> map -> execute, with lightweight metrics.
//!
//! `Pipeline` owns the `CompiledConfig` rules, the default sound ids, and the
//! metrics counters. It pulls `InputEvent`s from a backend stream and drives
//! them to the `Executor`. Metrics live HERE (not in the executor) so a single
//! counter set covers the whole flow. The audio engine owns sample decoding, so
//! the pipeline only deals in `SoundId`s (no `AudioCache` lookups on the hot path).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::StreamExt;

use crate::audio::AudioEngine;
use crate::config::CompiledConfig;
use crate::domain::{Action, CompiledRule, InputEvent, SoundId};
use crate::executor::Executor;

/// Counters for the whole pipeline. `AtomicU64` — no locks, cheap to bump from
/// many tasks. Read by `check` / a future metrics endpoint.
#[derive(Debug, Default)]
pub struct Metrics {
    pub received: AtomicU64,
    pub filtered: AtomicU64,
    pub mapped: AtomicU64,
    pub played: AtomicU64,
}

/// The runtime flow controller.
pub struct Pipeline {
    rules: Vec<CompiledRule>,
    default_ids: Vec<SoundId>,
    executor: Executor,
    metrics: Arc<Metrics>,
    enable_trackpads: bool,
}

impl Pipeline {
    pub fn new(
        config: CompiledConfig,
        engine: Arc<dyn AudioEngine>,
        enable_trackpads: bool,
    ) -> Pipeline {
        let default_ids = config.default_ids.clone();
        Pipeline {
            rules: config.rules,
            default_ids,
            executor: Executor::new(engine),
            metrics: Arc::new(Metrics::default()),
            enable_trackpads,
        }
    }

    #[allow(dead_code)]
    pub fn metrics(&self) -> Arc<Metrics> {
        self.metrics.clone()
    }

    /// Drive one event through filter -> map -> execute. Pure (no I/O beyond play).
    pub fn handle(&self, event: InputEvent) {
        self.metrics.received.fetch_add(1, Ordering::Relaxed);

        // Filter: trackpads and `Ignored` events are dropped unless enabled.
        if event == InputEvent::Ignored {
            self.metrics.filtered.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if !self.enable_trackpads && is_trackpad(event) {
            self.metrics.filtered.fetch_add(1, Ordering::Relaxed);
            return;
        }

        // Map: find the first rule whose trigger matches.
        if let Some(rule) = self.rules.iter().find(|r| r.trigger == event) {
            self.metrics.mapped.fetch_add(1, Ordering::Relaxed);
            for action in &rule.actions {
                let Action::PlaySound(id) = action;
                self.metrics.played.fetch_add(1, Ordering::Relaxed);
                self.executor.play(*id);
            }
        } else if !self.default_ids.is_empty() {
            // No explicit mapping: play a random default.
            self.metrics.mapped.fetch_add(1, Ordering::Relaxed);
            self.metrics.played.fetch_add(1, Ordering::Relaxed);
            let id = self.default_ids[fastrand(self.default_ids.len())];
            self.executor.play(id);
        }
    }

    /// Consume a backend event stream to completion (or until the stream ends).
    pub async fn run<S>(&self, stream: S)
    where
        S: futures::Stream<Item = InputEvent> + Unpin,
    {
        let mut stream = stream;
        while let Some(event) = stream.next().await {
            self.handle(event);
        }
    }
}

/// Heuristic: does this event look like a trackpad? Backends already tag
/// trackpads by emitting `Ignored`; this is a secondary guard. v1: only
/// `Ignored` reaches here for trackpads, so this is a no-op fallback kept
/// intentionally tiny.
fn is_trackpad(_event: InputEvent) -> bool {
    false
}

/// Tiny fast pseudo-random index pick (no external dep surface on Pipeline).
/// Uses a thread-local `SmallRng` via `rand`.
fn fastrand(n: usize) -> usize {
    use rand::Rng;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;
    thread_local! {
        static RNG: std::cell::RefCell<SmallRng> = std::cell::RefCell::new(SmallRng::from_entropy());
    }
    if n == 0 {
        return 0;
    }
    RNG.with(|r| (*r.borrow_mut()).gen_range(0..n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioEngine;
    use crate::config::CompiledConfig;
    use crate::domain::{CompiledRule, SoundId};

    struct StubEngine {
        played: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }
    impl AudioEngine for StubEngine {
        fn play(&self, _id: SoundId) {
            self.played.fetch_add(1, Ordering::Relaxed);
        }
        fn stop(&self) {}
    }

    fn cfg_with(mappings: Vec<(InputEvent, SoundId)>) -> CompiledConfig {
        CompiledConfig {
            sounds: vec![Some("a.wav".into()), Some("b.wav".into())],
            rules: mappings
                .into_iter()
                .map(|(e, id)| CompiledRule::new(e, vec![Action::PlaySound(id)]))
                .collect(),
            default_ids: vec![],
        }
    }

    #[test]
    fn mapped_key_plays() {
        let played = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let engine = Arc::new(StubEngine {
            played: played.clone(),
        });
        let cfg = cfg_with(vec![(InputEvent::Key(1), SoundId::new(1).unwrap())]);
        let p = Pipeline::new(cfg, engine, false);
        p.handle(InputEvent::Key(1));
        assert_eq!(played.load(Ordering::Relaxed), 1);
        assert_eq!(p.metrics().played.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn ignored_is_filtered() {
        let played = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let engine = Arc::new(StubEngine {
            played: played.clone(),
        });
        let cfg = cfg_with(vec![]);
        let p = Pipeline::new(cfg, engine, false);
        p.handle(InputEvent::Ignored);
        assert_eq!(p.metrics().filtered.load(Ordering::Relaxed), 1);
        assert_eq!(played.load(Ordering::Relaxed), 0);
    }
}
