//! Common types used across the IEM mixer

use serde::{Deserialize, Serialize};

/// Track information from REAPER
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Track index (1-based)
    pub index: usize,
    /// Track name
    pub name: String,
    /// Volume in dB
    pub volume_db: f32,
    /// Pan position (-1.0 to 1.0)
    pub pan: f32,
    /// Muted state
    pub muted: bool,
    /// Solo state
    pub solo: bool,
}

/// Send information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Send {
    /// Send index (1-based)
    pub index: usize,
    /// Target track name
    pub target: String,
    /// Level in dB
    pub level_db: f32,
    /// Pan position (-1.0 to 1.0)
    pub pan: f32,
    /// Muted state
    pub muted: bool,
}

/// Mixer state for a band member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerState {
    /// Member ID
    pub member_id: String,
    /// Channels with their levels
    pub channels: Vec<Channel>,
}

/// A single channel in the mixer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    /// Input track index (1-based)
    pub track_index: usize,
    /// Input track name
    pub name: String,
    /// Level in dB
    pub level_db: f32,
    /// Pan position (-1.0 to 1.0)
    pub pan: f32,
    /// Muted state
    pub muted: bool,
}

/// Authentication token payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    /// Subject (member ID or "engineer")
    pub sub: String,
    /// Is engineer (full access)
    pub engineer: bool,
    /// Expiration timestamp (Unix seconds)
    pub exp: u64,
    /// Issued at timestamp
    pub iat: u64,
}

/// API error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    /// Error code
    pub code: String,
    /// Human-readable message
    pub message: String,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn unauthorized() -> Self {
        Self::new("UNAUTHORIZED", "Authentication required")
    }

    pub fn forbidden() -> Self {
        Self::new("FORBIDDEN", "Access denied")
    }

    pub fn not_found(what: &str) -> Self {
        Self::new("NOT_FOUND", format!("{} not found", what))
    }
}
