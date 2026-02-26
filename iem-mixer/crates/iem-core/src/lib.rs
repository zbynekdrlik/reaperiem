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

/// Full version string for display (e.g., "1.1.0 (2026-02-26 14:30)")
/// Shows human-readable deployment time instead of git hash
pub fn full_version() -> String {
    let timestamp = build_time().parse::<i64>().unwrap_or(0);
    if timestamp == 0 {
        format!("{} (dev)", VERSION)
    } else {
        // Convert unix timestamp to human-readable format
        let datetime = chrono::DateTime::from_timestamp(timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        format!("{} ({})", VERSION, datetime)
    }
}

/// Get formatted deployment timestamp (e.g., "2026-02-26 14:30:00 UTC")
pub fn deployed_at() -> String {
    let timestamp = build_time().parse::<i64>().unwrap_or(0);
    if timestamp == 0 {
        "unknown".to_string()
    } else {
        chrono::DateTime::from_timestamp(timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }
}
