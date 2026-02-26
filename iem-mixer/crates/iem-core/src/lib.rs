//! IEM Mixer Core Library
//!
//! Shared types, configuration, and constants for the IEM mixing system.

#[cfg(feature = "config")]
pub mod config;
pub mod types;
pub mod ws;

#[cfg(feature = "config")]
pub use config::{BandMember, Config, InputTrack};
pub use types::*;
pub use ws::{ClientMsg, ServerMsg};

/// Application version (from Cargo.toml)
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Git commit hash at build time (7 characters)
/// Returns "unknown" if not set during build
pub fn git_hash() -> &'static str {
    option_env!("GIT_HASH").unwrap_or("unknown")
}

/// Build timestamp (unix seconds)
/// Returns "0" if not set during build
pub fn build_time() -> &'static str {
    option_env!("BUILD_TIME").unwrap_or("0")
}

/// Full version string for display (e.g., "0.1.0 (abc1234)")
pub fn full_version() -> String {
    format!("{} ({})", VERSION, git_hash())
}
