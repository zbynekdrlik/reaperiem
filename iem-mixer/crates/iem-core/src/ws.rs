//! WebSocket message types for real-time mixer communication

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::Channel;

/// Client → Server commands (sent via WebSocket)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "cmd")]
pub enum ClientMsg {
    /// Set send level for a track
    SetLevel { track_index: usize, level_db: f32 },
    /// Set send mute for a track
    SetMute { track_index: usize, muted: bool },
    /// Set send pan for a track
    SetPan { track_index: usize, pan: f32 },
}

/// Server → Client events (pushed via WebSocket)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event")]
pub enum ServerMsg {
    /// Full mixer state (sent on connect and periodically)
    State {
        channels: Vec<Channel>,
        connected: bool,
    },
    /// Meter levels (sent every ~150ms)
    Meters { meters: HashMap<usize, f32> },
    /// Single channel changed (delta update)
    ChannelUpdate {
        track_index: usize,
        level_db: f32,
        muted: bool,
        pan: f32,
    },
    /// REAPER connection status changed
    ConnectionChanged { connected: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_msg_set_level_serialization() {
        let msg = ClientMsg::SetLevel {
            track_index: 1,
            level_db: -6.0,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"SetLevel\""));
        assert!(json.contains("\"track_index\":1"));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_client_msg_set_mute_serialization() {
        let msg = ClientMsg::SetMute {
            track_index: 3,
            muted: true,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"SetMute\""));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_client_msg_set_pan_serialization() {
        let msg = ClientMsg::SetPan {
            track_index: 2,
            pan: 0.75,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"SetPan\""));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_state_serialization() {
        let msg = ServerMsg::State {
            channels: vec![Channel {
                track_index: 1,
                name: "MAREK mic".to_string(),
                level_db: 0.0,
                pan: 0.5,
                muted: false,
                category: "mics".to_string(),
                stereo_pair: None,
                stereo_side: None,
            }],
            connected: true,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"State\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_meters_serialization() {
        let mut meters = HashMap::new();
        meters.insert(1, 0.5);
        meters.insert(2, 0.3);
        let msg = ServerMsg::Meters { meters };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"Meters\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_channel_update_serialization() {
        let msg = ServerMsg::ChannelUpdate {
            track_index: 5,
            level_db: -12.0,
            muted: true,
            pan: 0.3,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"ChannelUpdate\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_connection_changed_serialization() {
        let msg = ServerMsg::ConnectionChanged { connected: false };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"ConnectionChanged\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }
}
