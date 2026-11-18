//! Hot-path benchmarks: router lookup, full pipeline dispatch, and config
//! compilation.
//!
//! Run with: cargo bench --bench dispatch

use std::hint::black_box;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use wayclick::audio::AudioEngine;
use wayclick::audio::AudioError;
use wayclick::config::{CompiledConfig, SoundCatalog, SoundSpec};
use wayclick::dispatch::{Pipeline, Router};
use wayclick::domain::{
    Action, CompiledRule, DefaultPolicy, InputEvent, PhysicalKey, Platform, SoundId,
};

/// Generate `n` distinct events using tagged `Unknown` keys so any `n` works.
fn make_events(n: usize) -> Vec<InputEvent> {
    (0..n)
        .map(|i| {
            InputEvent::KeyDown(PhysicalKey::Unknown {
                platform: Platform::Linux,
                code: i as u32,
            })
        })
        .collect()
}

fn make_config(events: &[InputEvent]) -> CompiledConfig {
    let mut catalog = SoundCatalog::default();
    for _ in 0..events.len() {
        catalog.push(SoundSpec::new("a.wav".into()));
    }
    let rules = events
        .iter()
        .enumerate()
        .map(|(i, &event)| {
            CompiledRule::new(event, Action::PlaySound(SoundId::from_index(i).unwrap()))
        })
        .collect();
    CompiledConfig {
        catalog,
        rules,
        default_ids: vec![],
        default_policy: DefaultPolicy::Random,
        volume_milli: 1000,
        max_voices: 32,
    }
}

/// A no-op audio engine so pipeline benches isolate dispatch cost from device I/O.
struct NullEngine;

impl AudioEngine for NullEngine {
    fn play(&self, _id: SoundId) -> Result<(), AudioError> {
        Ok(())
    }
}

fn bench_router(c: &mut Criterion) {
    for &n in &[10usize, 100, 500, 1000] {
        let events = make_events(n);
        let config = make_config(&events);
        let router = Router::new(config.rules, config.default_ids).unwrap();

        // The last rule was the worst case for the old Vec::find() scan.
        let probe = *events.last().unwrap();
        let mut group = c.benchmark_group(format!("router-{n}"));
        group.bench_function("hashmap_get", |b| {
            b.iter(|| black_box(router.dispatch(black_box(probe))));
        });
        group.finish();
    }
}

fn bench_pipeline(c: &mut Criterion) {
    for &n in &[100usize, 1000] {
        let events = make_events(n);
        let config = make_config(&events);
        let engine = Arc::new(NullEngine) as Arc<dyn AudioEngine>;
        let pipeline = Pipeline::new(config, engine);
        let probe = *events.last().unwrap();

        let mut group = c.benchmark_group(format!("pipeline-{n}"));
        group.bench_function("handle_matched", |b| {
            b.iter(|| pipeline.handle(black_box(probe)));
        });
        group.finish();
    }
}

fn bench_compile(c: &mut Criterion) {
    // Build a representative validated config once, then time compilation.
    let text = {
        let mut mappings = String::new();
        // Evdev keyboard codes 1..=767, skipping the mouse button range
        // (0x110..=0x117) that parses to MouseDown and would collide.
        let codes = (1u16..=767).filter(|c| !(0x110..=0x117).contains(c));
        for code in codes {
            if !mappings.is_empty() {
                mappings.push(',');
            }
            mappings.push_str(&format!("\"{code}\":\"a.wav\""));
        }
        format!("{{\"defaults\":[\"a.wav\"],\"mappings\":{{{mappings}}}}}")
    };
    let compiled = wayclick::config::from_str(&text).unwrap();
    black_box(&compiled);

    c.bench_function("config_compile_751", |b| {
        b.iter(|| {
            let config = wayclick::config::from_str(black_box(&text)).unwrap();
            black_box(config);
        });
    });
}

criterion_group!(benches, bench_router, bench_pipeline, bench_compile);
criterion_main!(benches);
