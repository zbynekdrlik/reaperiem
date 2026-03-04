//! Mix snapshot types for history and restore
//!
//! Captures the state of a member's mixer settings at a point in time,
//! allowing restoration to previous mix states.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum number of snapshots to keep per member
pub const MAX_SNAPSHOTS: usize = 50;

/// A snapshot of a single channel's settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelSnapshot {
    /// Volume level (0.0 to 1.0 in REAPER linear scale)
    pub vol: f32,
    /// Muted state
    pub mute: bool,
    /// Pan position (-1.0 left to 1.0 right)
    pub pan: f32,
}

/// A complete mix snapshot capturing all channel states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixSnapshot {
    /// Unix timestamp (seconds) when snapshot was created
    pub timestamp: i64,
    /// Label: "auto" for automatic snapshots, user-provided for manual
    pub label: String,
    /// Whether this snapshot is pinned (protected from pruning)
    pub pinned: bool,
    /// Channel states keyed by track index
    pub channels: HashMap<usize, ChannelSnapshot>,
}

impl MixSnapshot {
    /// Create a new automatic snapshot with the current timestamp
    pub fn new_auto(channels: HashMap<usize, ChannelSnapshot>) -> Self {
        Self {
            timestamp: chrono::Utc::now().timestamp(),
            label: "auto".to_string(),
            pinned: false,
            channels,
        }
    }

    /// Create a new manual snapshot with a custom label
    pub fn new_manual(label: String, channels: HashMap<usize, ChannelSnapshot>) -> Self {
        Self {
            timestamp: chrono::Utc::now().timestamp(),
            label,
            pinned: false,
            channels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_snapshot_serde_roundtrip() {
        let channel = ChannelSnapshot {
            vol: 0.75,
            mute: false,
            pan: -0.5,
        };

        let json = serde_json::to_string(&channel).unwrap();
        let parsed: ChannelSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(channel, parsed);
    }

    #[test]
    fn test_mix_snapshot_serde_roundtrip() {
        let mut channels = HashMap::new();
        channels.insert(
            1,
            ChannelSnapshot {
                vol: 0.5,
                mute: true,
                pan: 0.0,
            },
        );
        channels.insert(
            5,
            ChannelSnapshot {
                vol: 1.0,
                mute: false,
                pan: 0.75,
            },
        );

        let snapshot = MixSnapshot {
            timestamp: 1709582400,
            label: "rehearsal".to_string(),
            pinned: true,
            channels,
        };

        let json = serde_json::to_string(&snapshot).unwrap();
        let parsed: MixSnapshot = serde_json::from_str(&json).unwrap();

        assert_eq!(snapshot.timestamp, parsed.timestamp);
        assert_eq!(snapshot.label, parsed.label);
        assert_eq!(snapshot.pinned, parsed.pinned);
        assert_eq!(snapshot.channels.len(), parsed.channels.len());
        assert_eq!(snapshot.channels[&1], parsed.channels[&1]);
        assert_eq!(snapshot.channels[&5], parsed.channels[&5]);
    }

    #[test]
    fn test_new_auto_snapshot() {
        let mut channels = HashMap::new();
        channels.insert(
            1,
            ChannelSnapshot {
                vol: 0.5,
                mute: false,
                pan: 0.0,
            },
        );

        let snapshot = MixSnapshot::new_auto(channels);

        assert_eq!(snapshot.label, "auto");
        assert!(!snapshot.pinned);
        assert!(snapshot.timestamp > 0);
    }

    #[test]
    fn test_new_manual_snapshot() {
        let channels = HashMap::new();
        let snapshot = MixSnapshot::new_manual("before soundcheck".to_string(), channels);

        assert_eq!(snapshot.label, "before soundcheck");
        assert!(!snapshot.pinned);
    }

    #[test]
    fn test_max_snapshots_constant() {
        assert_eq!(MAX_SNAPSHOTS, 50);
    }
}
