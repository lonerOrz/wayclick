//! Library crate: all business logic; `src/main.rs` is a thin CLI wrapper. The
//! split also keeps the cfg-gated backends visible to clippy as public surface,
//! so cross-platform code isn't flagged dead.

pub mod app;
pub mod audio;
pub mod backend;
pub mod config;
pub mod domain;
pub mod pipeline;
