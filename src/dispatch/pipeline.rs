//! Runtime event flow: route → execute, with metrics.
//!
//! The routing table and default policy live in an immutable [`RuntimeSnapshot`]
//! held behind an `RwLock`. A hot-reload replaces the snapshot atomically while
//! in-flight events keep using the snapshot they read, so a reload never tears
//! a dispatch. The audio engine and its decoded sounds are reused across
//! reloads (see [`Pipeline::replace_snapshot`]).

use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures::StreamExt;

use crate::audio::AudioEngine;
use crate::config::CompiledConfig;
use crate::dispatch::executor::Executor;
use crate::dispatch::log_limiter::RateLimitedLog;
use crate::dispatch::metrics::Metrics;
use crate::dispatch::random::RandomSelector;
use crate::dispatch::router::Router;
use crate::domain::{Action, DefaultPolicy, InputEvent, SoundId};

/// Minimum interval between "playback failed" log lines. Failures inside the
/// window are counted and folded into the next emitted line.
const FAILURE_LOG_INTERVAL: Duration = Duration::from_secs(5);

/// Immutable routing state swapped atomically on config reload.
#[derive(Debug)]
pub struct RuntimeSnapshot {
    router: Router,
    default_policy: DefaultPolicy,
}

impl RuntimeSnapshot {
    /// Build a snapshot from compiled rules, default ids, and policy. Returns
    /// `Err` with the offending event if two rules share a trigger.
    pub fn new(
        rules: Vec<crate::domain::CompiledRule>,
        default_ids: Vec<SoundId>,
        default_policy: DefaultPolicy,
    ) -> Result<Self, InputEvent> {
        Ok(Self {
            router: Router::new(rules, default_ids)?,
            default_policy,
        })
    }

    /// Build a snapshot from a fully compiled config.
    pub fn from_config(config: &CompiledConfig) -> Self {
        Self {
            router: Router::new(config.rules.clone(), config.default_ids.clone())
                .expect("duplicate triggers must be rejected by config compilation"),
            default_policy: config.default_policy,
        }
    }
}

/// The runtime flow controller: routes each event and executes the action.
pub struct Pipeline {
    snapshot: RwLock<Arc<RuntimeSnapshot>>,
    executor: Executor,
    random: RandomSelector,
    metrics: Arc<Metrics>,
    failure_log: RateLimitedLog,
}

/// Start-of-dispatch timer. A zero-sized no-op when `latency-metrics` is off,
/// so the two clock reads vanish from the default hot path.
struct LatencyTimer {
    #[cfg(feature = "latency-metrics")]
    started: std::time::Instant,
}

impl LatencyTimer {
    #[inline]
    fn start() -> Self {
        #[cfg(feature = "latency-metrics")]
        return Self {
            started: std::time::Instant::now(),
        };
        #[cfg(not(feature = "latency-metrics"))]
        Self {}
    }

    #[inline]
    fn record_to(&self, metrics: &Metrics) {
        #[cfg(feature = "latency-metrics")]
        metrics.record_dispatch_latency(self.started.elapsed().as_nanos() as u64);
        #[cfg(not(feature = "latency-metrics"))]
        let _ = metrics;
    }
}

impl Pipeline {
    pub fn new(config: CompiledConfig, engine: Arc<dyn AudioEngine>) -> Pipeline {
        let snapshot = Arc::new(RuntimeSnapshot::from_config(&config));
        Pipeline {
            snapshot: RwLock::new(snapshot),
            executor: Executor::new(engine),
            random: RandomSelector::new(),
            metrics: Arc::new(Metrics::default()),
            failure_log: RateLimitedLog::new(FAILURE_LOG_INTERVAL),
        }
    }

    /// A handle to the counters, cloned out before the pipeline moves into the runtime.
    pub fn metrics(&self) -> Arc<Metrics> {
        Arc::clone(&self.metrics)
    }

    /// Swap in a new routing snapshot and optionally a rebuilt engine.
    ///
    /// In-flight events keep their old snapshot/engine; subsequent events use
    /// the new ones. The old snapshot and engine are reclaimed once their last
    /// user drops them — sounds still playing finish naturally.
    pub fn replace_snapshot(
        &self,
        snapshot: Arc<RuntimeSnapshot>,
        engine: Option<Arc<dyn AudioEngine>>,
    ) {
        if let Some(engine) = engine {
            self.executor.replace_engine(engine);
        }
        if let Ok(mut guard) = self.snapshot.write() {
            *guard = snapshot;
        }
    }

    /// Drive one event through route → execute; unmatched events fall through
    /// to a randomly chosen default.
    pub fn handle(&self, event: InputEvent) {
        let timer = LatencyTimer::start();
        self.metrics.inc_received();

        // Clone the Arc out of the lock so dispatch never holds it.
        let snapshot = match self.snapshot.read() {
            Ok(guard) => Arc::clone(&guard),
            Err(_) => {
                timer.record_to(&self.metrics);
                return;
            }
        };

        let action = if let Some(action) = snapshot.router.dispatch(event) {
            self.metrics.inc_matched();
            *action
        } else if snapshot.default_policy == DefaultPolicy::Random
            && let Some(id) = self.random.choose(snapshot.router.defaults())
        {
            Action::PlaySound(id)
        } else {
            timer.record_to(&self.metrics);
            return;
        };

        self.metrics.inc_play_request();
        if let Err(e) = self.executor.execute(action) {
            self.metrics.inc_failed_play();
            if let Some(suppressed) = self.failure_log.record() {
                if suppressed == 0 {
                    tracing::warn!(error = %e, "sound playback failed");
                } else {
                    tracing::warn!(
                        error = %e,
                        suppressed,
                        "sound playback failed (further failures suppressed)"
                    );
                }
            }
        }
        timer.record_to(&self.metrics);
    }

    /// Consume a backend event stream until it ends.
    pub async fn run<S>(&self, mut stream: S)
    where
        S: futures::Stream<Item = InputEvent> + Unpin,
    {
        while let Some(event) = stream.next().await {
            self.handle(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioError;
    use crate::config::{CompiledConfig, SoundCatalog, SoundSpec};
    use crate::domain::{Action, CompiledRule, MouseButton, PhysicalKey, SoundId};
    use std::sync::Mutex;

    struct FakeAudioEngine {
        played: Mutex<Vec<SoundId>>,
        fail: bool,
    }

    impl FakeAudioEngine {
        fn new(fail: bool) -> Self {
            Self {
                played: Mutex::new(Vec::new()),
                fail,
            }
        }
    }

    impl AudioEngine for FakeAudioEngine {
        fn play(&self, id: SoundId) -> Result<(), AudioError> {
            if self.fail {
                return Err(AudioError::SoundNotLoaded(id));
            }
            self.played.lock().expect("lock poisoned").push(id);
            Ok(())
        }
    }

    fn cfg(rules: Vec<CompiledRule>, defaults: Vec<SoundId>) -> CompiledConfig {
        let mut catalog = SoundCatalog::default();
        catalog.push(SoundSpec::new("a.wav".into()));
        catalog.push(SoundSpec::new("b.wav".into()));
        CompiledConfig {
            catalog,
            rules,
            default_ids: defaults,
            default_policy: DefaultPolicy::Random,
            volume_milli: 1000,
            max_voices: 32,
        }
    }

    fn rule(event: InputEvent, slot: usize) -> CompiledRule {
        CompiledRule::new(event, Action::PlaySound(SoundId::from_index(slot).unwrap()))
    }

    #[test]
    fn matched_event_plays_its_sound() {
        let engine = Arc::new(FakeAudioEngine::new(false));
        let event = InputEvent::KeyDown(PhysicalKey::A);
        let pipeline = Pipeline::new(cfg(vec![rule(event, 0)], vec![]), engine.clone());
        pipeline.handle(event);

        assert_eq!(
            *engine.played.lock().unwrap(),
            vec![SoundId::from_index(0).unwrap()]
        );
        let m = pipeline.metrics().snapshot();
        assert_eq!(m.received_events, 1);
        assert_eq!(m.matched_events, 1);
        assert_eq!(m.play_requests, 1);
        assert_eq!(m.failed_plays, 0);
    }

    #[test]
    fn unmatched_event_uses_default() {
        let engine = Arc::new(FakeAudioEngine::new(false));
        let default = SoundId::from_index(1).unwrap();
        let pipeline = Pipeline::new(cfg(vec![], vec![default]), engine.clone());
        pipeline.handle(InputEvent::KeyDown(PhysicalKey::Z));

        assert_eq!(*engine.played.lock().unwrap(), vec![default]);
        let m = pipeline.metrics().snapshot();
        assert_eq!(m.matched_events, 0);
        assert_eq!(m.play_requests, 1);
    }

    #[test]
    fn unmatched_mouse_event_uses_default() {
        let engine = Arc::new(FakeAudioEngine::new(false));
        let default = SoundId::from_index(1).unwrap();
        let pipeline = Pipeline::new(cfg(vec![], vec![default]), engine.clone());
        pipeline.handle(InputEvent::MouseDown(MouseButton::Left));

        assert_eq!(*engine.played.lock().unwrap(), vec![default]);
        let m = pipeline.metrics().snapshot();
        assert_eq!(m.matched_events, 0);
        assert_eq!(m.play_requests, 1);
    }

    #[test]
    fn playback_failure_is_counted() {
        let engine = Arc::new(FakeAudioEngine::new(true));
        let event = InputEvent::KeyDown(PhysicalKey::A);
        let pipeline = Pipeline::new(cfg(vec![rule(event, 0)], vec![]), engine);
        pipeline.handle(event);

        let m = pipeline.metrics().snapshot();
        assert_eq!(m.play_requests, 1);
        assert_eq!(m.failed_plays, 1);
    }

    #[test]
    fn no_rules_no_defaults_is_a_noop() {
        let engine = Arc::new(FakeAudioEngine::new(false));
        let pipeline = Pipeline::new(cfg(vec![], vec![]), engine.clone());
        pipeline.handle(InputEvent::KeyDown(PhysicalKey::A));

        assert!(engine.played.lock().unwrap().is_empty());
        assert_eq!(pipeline.metrics().snapshot().play_requests, 0);
    }

    #[test]
    fn ignore_policy_does_not_play_default() {
        let engine = Arc::new(FakeAudioEngine::new(false));
        let mut config = cfg(vec![], vec![SoundId::from_index(1).unwrap()]);
        config.default_policy = DefaultPolicy::Ignore;
        let pipeline = Pipeline::new(config, engine.clone());
        pipeline.handle(InputEvent::KeyDown(PhysicalKey::Z));

        assert!(engine.played.lock().unwrap().is_empty());
        let m = pipeline.metrics().snapshot();
        assert_eq!(m.received_events, 1);
        assert_eq!(m.matched_events, 0);
        assert_eq!(m.play_requests, 0);
    }

    #[test]
    fn ignore_policy_still_plays_matched_rules() {
        let engine = Arc::new(FakeAudioEngine::new(false));
        let event = InputEvent::KeyDown(PhysicalKey::A);
        let mut config = cfg(vec![rule(event, 0)], vec![]);
        config.default_policy = DefaultPolicy::Ignore;
        let pipeline = Pipeline::new(config, engine.clone());
        pipeline.handle(event);

        assert_eq!(
            *engine.played.lock().unwrap(),
            vec![SoundId::from_index(0).unwrap()]
        );
        assert_eq!(pipeline.metrics().snapshot().play_requests, 1);
    }

    #[cfg(feature = "latency-metrics")]
    #[test]
    fn dispatch_latency_is_recorded() {
        let engine = Arc::new(FakeAudioEngine::new(false));
        let event = InputEvent::KeyDown(PhysicalKey::A);
        let pipeline = Pipeline::new(cfg(vec![rule(event, 0)], vec![]), engine);
        pipeline.handle(event);
        assert!(pipeline.metrics().snapshot().dispatch_latency.total() >= 1);
    }

    #[test]
    fn replace_snapshot_switches_routing() {
        let engine = Arc::new(FakeAudioEngine::new(false));
        let a = InputEvent::KeyDown(PhysicalKey::A);
        let b = InputEvent::KeyDown(PhysicalKey::B);
        // Start with only A mapped to sound 0.
        let pipeline = Pipeline::new(cfg(vec![rule(a, 0)], vec![]), engine.clone());
        pipeline.handle(a);
        assert_eq!(
            *engine.played.lock().unwrap(),
            vec![SoundId::from_index(0).unwrap()]
        );

        // Swap to a snapshot that maps B to sound 1 and ignores unmatched.
        let snapshot = Arc::new(
            RuntimeSnapshot::new(vec![rule(b, 1)], vec![], DefaultPolicy::Ignore).unwrap(),
        );
        pipeline.replace_snapshot(snapshot, None);
        engine.played.lock().unwrap().clear();

        pipeline.handle(b);
        assert_eq!(
            *engine.played.lock().unwrap(),
            vec![SoundId::from_index(1).unwrap()]
        );
    }

    #[test]
    fn snapshot_swap_is_safe_under_concurrent_dispatch() {
        use std::sync::Barrier;

        const READERS: usize = 4;
        const ITERATIONS: usize = 500;
        const SWAPS: usize = 200;

        let engine = Arc::new(FakeAudioEngine::new(false));
        let a = InputEvent::KeyDown(PhysicalKey::A);
        let b = InputEvent::KeyDown(PhysicalKey::B);
        let pipeline = Arc::new(Pipeline::new(cfg(vec![rule(a, 0)], vec![]), engine));

        // Readers run a fixed number of dispatches rather than until a stop
        // flag, so the expected event count is exact and the test cannot pass
        // or fail depending on thread scheduling.
        let start = Arc::new(Barrier::new(READERS + 1));
        let mut handles = Vec::new();
        for _ in 0..READERS {
            let pipeline = Arc::clone(&pipeline);
            let start = Arc::clone(&start);
            handles.push(std::thread::spawn(move || {
                start.wait();
                for _ in 0..ITERATIONS {
                    pipeline.handle(a);
                    pipeline.handle(b);
                }
            }));
        }
        start.wait();
        for i in 0..SWAPS {
            let snapshot = Arc::new(
                RuntimeSnapshot::new(
                    vec![if i % 2 == 0 { rule(a, 0) } else { rule(b, 1) }],
                    vec![],
                    DefaultPolicy::Ignore,
                )
                .unwrap(),
            );
            pipeline.replace_snapshot(snapshot, None);
        }
        for handle in handles {
            handle.join().unwrap();
        }
        // Exactly one received event per handle call: no lost events, no
        // double counts, and no panic from a torn snapshot.
        let expected = (READERS * ITERATIONS * 2) as u64;
        assert_eq!(pipeline.metrics().snapshot().received_events, expected);
    }
}
