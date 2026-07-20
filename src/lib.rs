//! wayclick — low-latency input sound engine (Rust rewrite).
//!
//! Library crate holding all business logic. The binary in `src/main.rs` is a
//! thin wrapper. Splitting into lib/bin lets `cargo clippy --all-targets` treat
//! the platform backends as public library surfaces, so cross-platform code that
//! is only reachable via `cfg`-gated dynamic dispatch is not flagged dead-code.

pub mod app;
pub mod audio;
pub mod backend;
pub mod config;
pub mod domain;
pub mod pipeline;
