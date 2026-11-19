//! Library crate: all business logic; `src/main.rs` is a thin CLI wrapper.
//! The split keeps cfg-gated backends visible to clippy as public surface.

pub mod app;
pub mod audio;
pub mod backend;
pub mod config;
pub mod dispatch;
pub mod domain;
pub mod input;
