//! Common types used across the IEM mixer

use serde::{Deserialize, Serialize};

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
    /// Meter levels (track_index -> [left, right] peak levels 0.0-1.0)
    pub meters: std::collections::HashMap<usize, [f32; 2]>,
    /// Connection status
    pub connected: bool,
}

/// Batch control request for Reset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchControlRequest {
    /// Operation type
    pub operation: BatchOperation,
}

/// Batch operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchOperation {
    /// Reset: all to 0dB, unmuted, centered pan
    Reset,
    /// MuteAll: mute all input channels (engineer default state — safe, produces silence)
    MuteAll,
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

/// Per-member channel customization (pin/hide preferences)
/// Stored server-side so preferences follow the member to any device
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Customization {
    /// Track indices that are pinned to the Main tab
    #[serde(default)]
    pub pinned: Vec<usize>,
    /// Track indices that are hidden from all category tabs
    #[serde(default)]
    pub hidden: Vec<usize>,
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

    pub fn bad_request(msg: &str) -> Self {
        Self::new("BAD_REQUEST", msg)
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
    fn test_api_error_bad_request() {
        let err = ApiError::bad_request("invalid input");
        assert_eq!(err.code, "BAD_REQUEST");
        assert!(err.message.contains("invalid input"));
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
        let op = BatchOperation::Reset;
        let json = serde_json::to_string(&op).unwrap();
        assert_eq!(json, "\"reset\"");
    }

    #[test]
    fn test_batch_operation_deserialization() {
        let op: BatchOperation = serde_json::from_str("\"reset\"").unwrap();
        assert!(matches!(op, BatchOperation::Reset));
    }

    #[test]
    fn test_batch_operation_mute_all_serialization() {
        let op = BatchOperation::MuteAll;
        let json = serde_json::to_string(&op).unwrap();
        assert_eq!(json, "\"mute_all\"");
    }

    #[test]
    fn test_batch_operation_mute_all_deserialization() {
        let op: BatchOperation = serde_json::from_str("\"mute_all\"").unwrap();
        assert!(matches!(op, BatchOperation::MuteAll));
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

    // ================================================================
    // Customization type tests
    // ================================================================

    #[test]
    fn test_customization_default_is_empty() {
        let cust = Customization::default();
        assert!(cust.pinned.is_empty());
        assert!(cust.hidden.is_empty());
    }

    #[test]
    fn test_customization_serialization_roundtrip() {
        let cust = Customization {
            pinned: vec![1, 5, 8],
            hidden: vec![3, 7, 12],
        };
        let json = serde_json::to_string(&cust).unwrap();
        let decoded: Customization = serde_json::from_str(&json).unwrap();
        assert_eq!(cust, decoded);
    }

    #[test]
    fn test_customization_deserialize_missing_fields() {
        // Old data without pinned/hidden should default to empty
        let json = "{}";
        let cust: Customization = serde_json::from_str(json).unwrap();
        assert!(cust.pinned.is_empty());
        assert!(cust.hidden.is_empty());
    }

    // ================================================================
    // "Me" fader input detection - case-sensitive comparison regression
    // ================================================================

    #[test]
    fn test_my_input_detection_case_match() {
        // Reproduces the bug: format must produce uppercase "MIC" to match
        // channel names that are compared via to_uppercase()
        let member = "petka";
        let my_input = format!("{} MIC", member.to_uppercase());
        let channel_name = "PETKA mic";

        assert_eq!(
            channel_name.to_uppercase(),
            my_input,
            "is_my_input must match when channel is member's mic"
        );
    }

    #[test]
    fn test_my_input_detection_non_member() {
        let member = "petka";
        let my_input = format!("{} MIC", member.to_uppercase());
        let other_channel = "STEVO mic";

        assert_ne!(
            other_channel.to_uppercase(),
            my_input,
            "Other member's mic must NOT match"
        );
    }
}
