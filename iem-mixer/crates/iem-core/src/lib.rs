//! IEM Mixer Core Library
//!
//! Shared types, configuration, and constants for the IEM mixing system.

pub mod config;
pub mod types;

pub use config::{BandMember, Config, InputTrack};
pub use types::*;

/// Application version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
