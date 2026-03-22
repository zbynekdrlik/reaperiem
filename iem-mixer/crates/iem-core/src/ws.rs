//! WebSocket message types for real-time mixer communication
//! Includes stems group bus control (v1.91.0)

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
    /// Set global IEM output volume for this member
    SetGlobalLevel { level_db: f32 },
    /// Set global IEM output mute for this member
    SetGlobalMute { muted: bool },
    /// Set stems group bus volume for this member
    SetStemsLevel { level_db: f32 },
    /// Set stems group bus mute for this member
    SetStemsMute { muted: bool },
    /// Update channel customization (pin/hide preferences)
    UpdateCustomization {
        pinned: Vec<usize>,
        hidden: Vec<usize>,
    },
    /// Set solo state for this member (full replacement, syncs across devices)
    SetSolo { soloed: Vec<usize> },
    /// Start listening to audio stream (engineer only)
    /// member_id specifies whose mix to listen to (e.g., "petka", "engineer")
    ListenStart { member_id: String },
    /// Stop listening to audio stream
    ListenStop,
}

/// Server → Client events (pushed via WebSocket)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", content = "data")]
pub enum ServerMsg {
    /// Full mixer state (sent on connect and periodically)
    State {
        channels: Vec<Channel>,
        connected: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        global_level_db: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        global_muted: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_track_index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stems_level_db: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stems_muted: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stems_bus_index: Option<usize>,
    },
    /// Meter levels (sent every ~150ms) — stereo [left, right] peaks
    Meters { meters: HashMap<usize, [f32; 2]> },
    /// Single channel changed (delta update)
    ChannelUpdate {
        track_index: usize,
        level_db: f32,
        muted: bool,
        pan: f32,
    },
    /// Global IEM output volume changed
    GlobalVolumeUpdate { level_db: f32, muted: bool },
    /// Stems group bus volume changed
    StemsVolumeUpdate { level_db: f32, muted: bool },
    /// REAPER connection status changed
    ConnectionChanged { connected: bool },
    /// Channel customization update (sent on connect and after changes)
    CustomizationUpdate {
        pinned: Vec<usize>,
        hidden: Vec<usize>,
    },
    /// Network mode indicator (local LAN vs remote internet)
    NetworkMode { mode: String },
    /// Solo state update (sent on connect and after changes)
    SoloUpdate { soloed: Vec<usize> },
    /// Audio streaming status update
    AudioStatus {
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
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
            global_level_db: None,
            global_muted: None,
            output_track_index: None,
            stems_level_db: None,
            stems_muted: None,
            stems_bus_index: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"State\""));
        // Optional fields should NOT appear when None
        assert!(!json.contains("global_level_db"));
        assert!(!json.contains("global_muted"));
        assert!(!json.contains("output_track_index"));
        assert!(!json.contains("stems_level_db"));
        assert!(!json.contains("stems_muted"));
        assert!(!json.contains("stems_bus_index"));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_state_with_global_volume() {
        let msg = ServerMsg::State {
            channels: vec![],
            connected: true,
            global_level_db: Some(-3.5),
            global_muted: Some(false),
            output_track_index: Some(23),
            stems_level_db: None,
            stems_muted: None,
            stems_bus_index: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"global_level_db\":-3.5"));
        assert!(json.contains("\"global_muted\":false"));
        assert!(json.contains("\"output_track_index\":23"));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_state_backwards_compat() {
        // Old State messages without global/output fields should still deserialize
        let json = r#"{"event":"State","data":{"channels":[],"connected":true}}"#;
        let decoded: ServerMsg = serde_json::from_str(json).unwrap();
        match decoded {
            ServerMsg::State {
                global_level_db,
                global_muted,
                output_track_index,
                stems_level_db,
                stems_muted,
                stems_bus_index,
                ..
            } => {
                assert_eq!(global_level_db, None);
                assert_eq!(global_muted, None);
                assert_eq!(output_track_index, None);
                assert_eq!(stems_level_db, None);
                assert_eq!(stems_muted, None);
                assert_eq!(stems_bus_index, None);
            }
            _ => panic!("Expected State variant"),
        }
    }

    #[test]
    fn test_server_msg_meters_serialization() {
        let mut meters = HashMap::new();
        meters.insert(1, [0.5, 0.4]);
        meters.insert(2, [0.3, 0.25]);
        let msg = ServerMsg::Meters { meters };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"Meters\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_meters_stereo_wire_format() {
        let mut meters = HashMap::new();
        meters.insert(1, [0.5, 0.3]);
        let msg = ServerMsg::Meters { meters };
        let json = serde_json::to_string(&msg).unwrap();
        // Verify stereo pair format: key maps to [left, right] array
        assert!(
            json.contains("[0.5,0.3]"),
            "Stereo meters should serialize as [L,R] arrays, got: {}",
            json
        );
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

    #[test]
    fn test_client_msg_set_global_level_serialization() {
        let msg = ClientMsg::SetGlobalLevel { level_db: -6.0 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"SetGlobalLevel\""));
        assert!(json.contains("\"level_db\":-6.0"));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_client_msg_set_global_mute_serialization() {
        let msg = ClientMsg::SetGlobalMute { muted: true };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"SetGlobalMute\""));
        assert!(json.contains("\"muted\":true"));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_global_volume_update_serialization() {
        let msg = ServerMsg::GlobalVolumeUpdate {
            level_db: -12.0,
            muted: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"GlobalVolumeUpdate\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_client_msg_update_customization_serialization() {
        let msg = ClientMsg::UpdateCustomization {
            pinned: vec![1, 5],
            hidden: vec![3, 7],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"UpdateCustomization\""));
        assert!(json.contains("\"pinned\":[1,5]"));
        assert!(json.contains("\"hidden\":[3,7]"));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_customization_update_serialization() {
        let msg = ServerMsg::CustomizationUpdate {
            pinned: vec![1, 5, 8],
            hidden: vec![3],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"CustomizationUpdate\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_client_msg_listen_start_serialization() {
        let msg = ClientMsg::ListenStart {
            member_id: "petka".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"ListenStart\""));
        assert!(json.contains("\"member_id\":\"petka\""));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_client_msg_listen_start_engineer_own_mix() {
        let msg = ClientMsg::ListenStart {
            member_id: "engineer".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"member_id\":\"engineer\""));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_client_msg_listen_stop_serialization() {
        let msg = ClientMsg::ListenStop;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"ListenStop\""));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_audio_status_serialization() {
        let msg = ServerMsg::AudioStatus {
            status: "no_source".to_string(),
            target: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"AudioStatus\""));
        assert!(json.contains("\"status\":\"no_source\""));
        // target should be omitted when None
        assert!(!json.contains("target"));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_audio_status_with_target() {
        let msg = ServerMsg::AudioStatus {
            status: "listening".to_string(),
            target: Some("petka".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"status\":\"listening\""));
        assert!(json.contains("\"target\":\"petka\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_audio_status_backwards_compat() {
        // Old AudioStatus without target field should still deserialize
        let json = r#"{"event":"AudioStatus","data":{"status":"stopped"}}"#;
        let decoded: ServerMsg = serde_json::from_str(json).unwrap();
        match decoded {
            ServerMsg::AudioStatus { status, target } => {
                assert_eq!(status, "stopped");
                assert_eq!(target, None);
            }
            _ => panic!("Expected AudioStatus variant"),
        }
    }

    #[test]
    fn test_server_msg_network_mode_serialization() {
        let msg = ServerMsg::NetworkMode {
            mode: "local".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"NetworkMode\""));
        assert!(json.contains("\"mode\":\"local\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_client_msg_set_solo_serialization() {
        let msg = ClientMsg::SetSolo { soloed: vec![1, 5] };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"SetSolo\""));
        assert!(json.contains("\"soloed\":[1,5]"));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_solo_update_serialization() {
        let msg = ServerMsg::SoloUpdate { soloed: vec![1, 5] };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"SoloUpdate\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_client_msg_set_stems_level_serialization() {
        let msg = ClientMsg::SetStemsLevel { level_db: -3.0 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"SetStemsLevel\""));
        assert!(json.contains("\"level_db\":-3.0"));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_client_msg_set_stems_mute_serialization() {
        let msg = ClientMsg::SetStemsMute { muted: true };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"SetStemsMute\""));
        assert!(json.contains("\"muted\":true"));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_stems_volume_update_serialization() {
        let msg = ServerMsg::StemsVolumeUpdate {
            level_db: -6.0,
            muted: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"StemsVolumeUpdate\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_state_with_stems_fields() {
        let msg = ServerMsg::State {
            channels: vec![],
            connected: true,
            global_level_db: Some(-3.5),
            global_muted: Some(false),
            output_track_index: Some(23),
            stems_level_db: Some(-6.0),
            stems_muted: Some(false),
            stems_bus_index: Some(33),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"stems_level_db\":-6.0"));
        assert!(json.contains("\"stems_muted\":false"));
        assert!(json.contains("\"stems_bus_index\":33"));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_state_stems_backwards_compat() {
        // Old State messages without stems fields should still deserialize
        let json =
            r#"{"event":"State","data":{"channels":[],"connected":true,"global_level_db":-3.5}}"#;
        let decoded: ServerMsg = serde_json::from_str(json).unwrap();
        match decoded {
            ServerMsg::State {
                stems_level_db,
                stems_muted,
                stems_bus_index,
                ..
            } => {
                assert_eq!(stems_level_db, None);
                assert_eq!(stems_muted, None);
                assert_eq!(stems_bus_index, None);
            }
            _ => panic!("Expected State variant"),
        }
    }

    #[test]
    fn test_server_msg_solo_update_empty() {
        let msg = ServerMsg::SoloUpdate { soloed: vec![] };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }
}
