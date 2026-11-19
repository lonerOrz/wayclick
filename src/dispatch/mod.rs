//! Dispatch: route input events to actions and execute them.
//!
//! Split into focused components so each can be tested without real devices:
//! [`Router`] matches events, [`Executor`] runs actions, [`RandomSelector`]
//! chooses defaults, and [`Metrics`] records counters.

pub mod executor;
pub mod log_limiter;
pub mod metrics;
pub mod pipeline;
pub mod random;
pub mod router;

pub use metrics::{Metrics, MetricsSnapshot};
pub use pipeline::{Pipeline, RuntimeSnapshot};
pub use random::RandomSelector;
pub use router::Router;
