//! Input layer: platform translators and the input session. Platform-native
//! codes become [`crate::domain`] types here, so no platform constant leaks
//! into the domain or the pipeline.

pub mod error;
pub mod session;
pub mod translator;
