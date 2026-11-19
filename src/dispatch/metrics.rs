//! Pipeline metrics: cheap atomic counters and a latency histogram, logged
//! on shutdown.

use std::sync::atomic::{AtomicU64, Ordering};

/// Counters for the event pipeline.
#[derive(Debug, Default)]
pub struct Metrics {
    pub received_events: AtomicU64,
    pub matched_events: AtomicU64,
    pub play_requests: AtomicU64,
    pub failed_plays: AtomicU64,
    /// Dispatch-stage latency in nanoseconds, bucketed by power-of-two upper
    /// bounds. Fixed size and allocation-free so it is safe on the hot path.
    dispatch_latency: LatencyHistogram,
}

impl Metrics {
    pub fn inc_received(&self) {
        self.received_events.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_matched(&self) {
        self.matched_events.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_play_request(&self) {
        self.play_requests.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_failed_play(&self) {
        self.failed_plays.fetch_add(1, Ordering::Relaxed);
    }

    /// Record dispatch latency when the `latency-metrics` feature is on.
    /// No-op otherwise, so callers can stay unconditional on the hot path.
    #[inline]
    pub fn record_dispatch_latency(&self, nanos: u64) {
        #[cfg(feature = "latency-metrics")]
        self.dispatch_latency.record(nanos);
        #[cfg(not(feature = "latency-metrics"))]
        let _ = nanos;
    }

    /// Read a consistent-enough snapshot (Relaxed counters; values may drift).
    #[must_use]
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            received_events: self.received_events.load(Ordering::Relaxed),
            matched_events: self.matched_events.load(Ordering::Relaxed),
            play_requests: self.play_requests.load(Ordering::Relaxed),
            failed_plays: self.failed_plays.load(Ordering::Relaxed),
            dispatch_latency: self.dispatch_latency.snapshot(),
        }
    }
}

/// Upper bounds (inclusive) for latency buckets: 1µs, 10µs, 100µs, 1ms,
/// 10ms, 100ms, and +inf. Each bucket counts samples `<= bound` and greater
/// than the previous bound.
const LATENCY_BOUNDS_NS: [u64; 7] = [
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    u64::MAX,
];

/// A fixed-size, allocation-free latency histogram backed by atomics.
#[derive(Debug, Default)]
pub struct LatencyHistogram {
    buckets: [AtomicU64; LATENCY_BOUNDS_NS.len()],
}

impl LatencyHistogram {
    #[cfg(feature = "latency-metrics")]
    fn record(&self, nanos: u64) {
        let idx = LATENCY_BOUNDS_NS
            .iter()
            .position(|&bound| nanos <= bound)
            .unwrap_or(LATENCY_BOUNDS_NS.len() - 1);
        self.buckets[idx].fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> LatencySnapshot {
        LatencySnapshot {
            buckets: std::array::from_fn(|i| self.buckets[i].load(Ordering::Relaxed)),
        }
    }
}

/// Point-in-time copy of the latency buckets, aligned with [`LATENCY_BOUNDS_NS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LatencySnapshot {
    pub buckets: [u64; LATENCY_BOUNDS_NS.len()],
}

impl LatencySnapshot {
    pub fn total(&self) -> u64 {
        self.buckets.iter().sum()
    }

    /// The smallest bucket bound containing the p-th percentile (p in 0..=100),
    /// or `None` when no samples exist.
    #[must_use]
    pub fn percentile_bound(&self, pct: u64) -> Option<u64> {
        let total = self.total();
        if total == 0 {
            return None;
        }
        // Rank of the percentile sample, 1-based and clamped.
        let target = ((total * pct) / 100).max(1);
        let mut cumulative = 0u64;
        for (count, &bound) in self.buckets.iter().zip(LATENCY_BOUNDS_NS.iter()) {
            cumulative += count;
            if cumulative >= target {
                return Some(bound);
            }
        }
        None
    }
}

/// Point-in-time copy of the counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MetricsSnapshot {
    pub received_events: u64,
    pub matched_events: u64,
    pub play_requests: u64,
    pub failed_plays: u64,
    pub dispatch_latency: LatencySnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "latency-metrics")]
    #[test]
    fn histogram_records_buckets() {
        let h = LatencyHistogram::default();
        h.record(500); // <= 1us
        h.record(5_000); // <= 10us
        h.record(5_000); // <= 10us
        h.record(50_000_000); // <= 100ms
        let s = h.snapshot();
        assert_eq!(s.buckets, [1, 2, 0, 0, 0, 1, 0]);
        assert_eq!(s.total(), 4);
    }

    #[test]
    fn percentile_bounds() {
        let mut s = LatencySnapshot::default();
        assert_eq!(s.percentile_bound(50), None);
        // 90 samples in the first bucket, 10 in the last.
        s.buckets[0] = 90;
        s.buckets[6] = 10;
        assert_eq!(s.percentile_bound(50), Some(LATENCY_BOUNDS_NS[0]));
        assert_eq!(s.percentile_bound(95), Some(LATENCY_BOUNDS_NS[6]));
        assert_eq!(s.percentile_bound(100), Some(LATENCY_BOUNDS_NS[6]));
    }

    #[test]
    fn metrics_snapshot_is_independent() {
        let m = Metrics::default();
        m.inc_received();
        m.record_dispatch_latency(2_000);
        let first = m.snapshot();
        m.inc_received();
        assert_eq!(first.received_events, 1);
        assert_eq!(m.snapshot().received_events, 2);
    }
}
