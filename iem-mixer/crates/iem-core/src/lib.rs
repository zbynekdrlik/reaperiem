//! IEM Mixer Core Library
//!
//! Shared types, configuration, and constants for the IEM mixing system.

pub mod config;
pub mod types;

pub use config::{BandMember, Config, InputTrack};
pub use types::*;

/// Application version (from Cargo.toml)
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Git commit hash at build time (7 characters)
pub const GIT_HASH: &str = option_env!("GIT_HASH").unwrap_or("unknown");

/// Build timestamp (unix seconds)
pub const BUILD_TIME: &str = option_env!("BUILD_TIME").unwrap_or("0");

/// Full version string for display (e.g., "0.1.0 (abc1234)")
pub fn full_version() -> String {
    format!("{} ({})", VERSION, GIT_HASH)
}
