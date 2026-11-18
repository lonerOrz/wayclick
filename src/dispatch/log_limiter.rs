//! Rate-limited logging for hot-path failures.
//!
//! Playback can fail repeatedly under a misconfiguration (e.g. an unloaded
//! sound). Logging every failure would flood the output and add cost on the
//! dispatch path. This limiter emits at most one message per interval and
//! reports how many failures were suppressed since the previous message.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Emits at most one log line per interval, folding suppressed failures into
/// the next line's count.
pub struct RateLimitedLog {
    interval: Duration,
    /// Nanoseconds since `epoch` at the last emitted line, plus one. Zero means
    /// "never emitted"; the offset keeps a t=0 emission distinct from that.
    last_emit_ns: AtomicU64,
    /// Failures seen since the last emitted line.
    suppressed: AtomicU64,
    epoch: Instant,
}

impl RateLimitedLog {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_emit_ns: AtomicU64::new(0),
            suppressed: AtomicU64::new(0),
            epoch: Instant::now(),
        }
    }

    /// Record one failure. Returns `Some(suppressed_since_last)` when the
    /// caller should log now, or `None` when the failure is being folded into
    /// the running count.
    pub fn record(&self) -> Option<u64> {
        self.suppressed.fetch_add(1, Ordering::Relaxed);
        let now_ns = self.epoch.elapsed().as_nanos() as u64;
        let interval_ns = self.interval.as_nanos() as u64;
        let last = self.last_emit_ns.load(Ordering::Relaxed);
        let should_log = last == 0 || now_ns.saturating_sub(last - 1) >= interval_ns;
        if !should_log {
            return None;
        }
        // Store `now + 1`; a lost race means another thread logged this window.
        if self
            .last_emit_ns
            .compare_exchange(last, now_ns + 1, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return None;
        }
        // The count includes this failure; report the ones before it.
        Some(self.suppressed.swap(0, Ordering::Relaxed).saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_failure_logs_immediately() {
        let limiter = RateLimitedLog::new(Duration::from_secs(60));
        assert_eq!(limiter.record(), Some(0));
    }

    #[test]
    fn subsequent_failures_within_interval_are_suppressed() {
        let limiter = RateLimitedLog::new(Duration::from_secs(60));
        assert_eq!(limiter.record(), Some(0));
        assert_eq!(limiter.record(), None);
        assert_eq!(limiter.record(), None);
    }

    #[test]
    fn zero_interval_logs_every_failure() {
        let limiter = RateLimitedLog::new(Duration::ZERO);
        assert!(limiter.record().is_some());
        assert!(limiter.record().is_some());
    }

    #[test]
    fn suppressed_count_reported_on_next_emit() {
        let limiter = RateLimitedLog::new(Duration::ZERO);
        // First emit reports 0 prior failures.
        assert_eq!(limiter.record(), Some(0));
        // No interval, so each call emits; the counter resets each time.
        assert_eq!(limiter.record(), Some(0));
    }
}
