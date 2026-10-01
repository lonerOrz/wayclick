//! Pipeline: map `InputEvent`s to actions, with lightweight counters.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::StreamExt;

use crate::audio::AudioEngine;
use crate::config::CompiledConfig;
use crate::domain::{Action, CompiledRule, InputEvent, SoundId};

/// Counters for the whole pipeline; logged once on shutdown.
#[derive(Debug, Default)]
pub struct Metrics {
    pub received: AtomicU64,
    pub played: AtomicU64,
}

/// The runtime flow controller.
pub struct Pipeline {
    rules: Vec<CompiledRule>,
    default_ids: Vec<SoundId>,
    engine: Arc<dyn AudioEngine>,
    metrics: Arc<Metrics>,
}

impl Pipeline {
    pub fn new(config: CompiledConfig, engine: Arc<dyn AudioEngine>) -> Pipeline {
        let default_ids = config.default_ids.clone();
        Pipeline {
            rules: config.rules,
            default_ids,
            engine,
            metrics: Arc::new(Metrics::default()),
        }
    }

    /// A handle to the counters, cloned out before the pipeline moves into the runtime.
    pub fn metrics(&self) -> Arc<Metrics> {
        self.metrics.clone()
    }

    /// Drive one event through map -> execute.
    pub fn handle(&self, event: InputEvent) {
        self.metrics.received.fetch_add(1, Ordering::Relaxed);

        // First matching rule wins; otherwise a random default.
        if let Some(rule) = self.rules.iter().find(|rule| rule.trigger == event) {
            for action in &rule.actions {
                let Action::PlaySound(id) = action;
                self.metrics.played.fetch_add(1, Ordering::Relaxed);
                self.engine.play(*id);
            }
        } else if !self.default_ids.is_empty() {
            self.metrics.played.fetch_add(1, Ordering::Relaxed);
            let id = self.default_ids[fastrand(self.default_ids.len())];
            self.engine.play(id);
        }
    }

    /// Consume a backend event stream until it ends.
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

/// Pick a random index in `0..n` from a thread-local RNG, so the pipeline holds
/// no shared RNG state.
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
    use crate::config::CompiledConfig;
    use crate::domain::{CompiledRule, SoundId};

    struct StubEngine {
        played: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl AudioEngine for StubEngine {
        fn play(&self, _id: SoundId) {
            self.played.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn cfg_with(mappings: Vec<(InputEvent, SoundId)>) -> CompiledConfig {
        CompiledConfig {
            sounds: vec![Some("a.wav".into()), Some("b.wav".into())],
            rules: mappings
                .into_iter()
                .map(|(event, id)| CompiledRule::new(event, vec![Action::PlaySound(id)]))
                .collect(),
            default_ids: vec![],
        }
    }

    fn stub() -> (Arc<StubEngine>, Arc<std::sync::atomic::AtomicUsize>) {
        let played = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let engine = Arc::new(StubEngine {
            played: played.clone(),
        });
        (engine, played)
    }

    #[test]
    fn mapped_key_plays() {
        let (engine, played) = stub();
        let cfg = cfg_with(vec![(InputEvent::Key(1), SoundId::new(1).unwrap())]);
        let pipeline = Pipeline::new(cfg, engine);
        pipeline.handle(InputEvent::Key(1));
        assert_eq!(played.load(Ordering::Relaxed), 1);
        assert_eq!(pipeline.metrics().played.load(Ordering::Relaxed), 1);
    }
}
