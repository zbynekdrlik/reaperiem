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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    /// Track category (mics, stems, tech)
    #[serde(default)]
    pub category: String,
    /// Stereo pair name (if part of a pair)
    #[serde(default)]
    pub stereo_pair: Option<String>,
    /// Stereo side ("L" or "R")
    #[serde(default)]
    pub stereo_side: Option<String>,
}

/// Polling response with channels and meters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollResponse {
    /// Member ID
    pub member_id: String,
    /// Channel states
    pub channels: Vec<Channel>,
    /// Meter levels (track_index -> peak level 0.0-1.0)
    pub meters: std::collections::HashMap<usize, f32>,
    /// Connection status
    pub connected: bool,
}

/// Batch control request for +Me or Reset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchControlRequest {
    /// Operation type
    pub operation: BatchOperation,
}

/// Batch operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchOperation {
    /// More Me: boost own mic +6dB, reduce others -3dB
    MoreMe,
    /// Reset: all to 0dB, unmuted, centered pan
    Reset,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_new() {
        let err = ApiError::new("TEST_CODE", "Test message");
        assert_eq!(err.code, "TEST_CODE");
        assert_eq!(err.message, "Test message");
    }

    #[test]
    fn test_api_error_unauthorized() {
        let err = ApiError::unauthorized();
        assert_eq!(err.code, "UNAUTHORIZED");
    }

    #[test]
    fn test_api_error_forbidden() {
        let err = ApiError::forbidden();
        assert_eq!(err.code, "FORBIDDEN");
    }

    #[test]
    fn test_api_error_not_found() {
        let err = ApiError::not_found("Member");
        assert_eq!(err.code, "NOT_FOUND");
        assert!(err.message.contains("Member"));
    }

    #[test]
    fn test_channel_default_values() {
        let channel = Channel {
            track_index: 1,
            name: "Test".to_string(),
            level_db: 0.0,
            pan: 0.0,
            muted: false,
            category: String::new(),
            stereo_pair: None,
            stereo_side: None,
        };
        assert_eq!(channel.track_index, 1);
        assert!(!channel.muted);
    }

    #[test]
    fn test_batch_operation_serialization() {
        // Test that BatchOperation serializes correctly
        let op = BatchOperation::MoreMe;
        let json = serde_json::to_string(&op).unwrap();
        assert_eq!(json, "\"more_me\"");

        let op = BatchOperation::Reset;
        let json = serde_json::to_string(&op).unwrap();
        assert_eq!(json, "\"reset\"");
    }

    #[test]
    fn test_batch_operation_deserialization() {
        let op: BatchOperation = serde_json::from_str("\"more_me\"").unwrap();
        assert!(matches!(op, BatchOperation::MoreMe));

        let op: BatchOperation = serde_json::from_str("\"reset\"").unwrap();
        assert!(matches!(op, BatchOperation::Reset));
    }

    #[test]
    fn test_auth_claims() {
        let claims = AuthClaims {
            sub: "marek".to_string(),
            engineer: false,
            exp: 1234567890,
            iat: 1234567800,
        };
        assert_eq!(claims.sub, "marek");
        assert!(!claims.engineer);
    }
}
