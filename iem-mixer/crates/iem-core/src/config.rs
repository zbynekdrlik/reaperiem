//! Configuration types for band members, inputs, and PINs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// REAPER server URL
    #[serde(default = "default_reaper_url")]
    pub reaper_url: String,

    /// Server port
    #[serde(default = "default_port")]
    pub port: u16,

    /// Band members with their output assignments
    #[serde(default)]
    pub members: Vec<BandMember>,

    /// Input tracks
    #[serde(default)]
    pub inputs: Vec<InputTrack>,

    /// PIN codes for authentication (member_id -> PIN)
    #[serde(default)]
    pub pins: HashMap<String, String>,

    /// Engineer PIN (full access)
    #[serde(default)]
    pub engineer_pin: Option<String>,

    /// JWT secret for token signing
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,
}

fn default_reaper_url() -> String {
    "http://iem.lan:8080".to_string()
}

fn default_port() -> u16 {
    80
}

fn default_jwt_secret() -> String {
    // In production, this should be set via config file or env var
    "change-me-in-production".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            reaper_url: default_reaper_url(),
            port: default_port(),
            members: Vec::new(),
            inputs: Vec::new(),
            pins: HashMap::new(),
            engineer_pin: None,
            jwt_secret: default_jwt_secret(),
        }
    }
}

impl Config {
    /// Load configuration from a YAML file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content =
            std::fs::read_to_string(path.as_ref()).map_err(|e| ConfigError::Io(e.to_string()))?;
        serde_yaml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// Find a band member by their ID (lowercase name)
    pub fn find_member(&self, id: &str) -> Option<&BandMember> {
        self.members.iter().find(|m| m.id() == id)
    }

    /// Get member index (1-based) for REAPER track
    pub fn member_index(&self, id: &str) -> Option<usize> {
        self.members
            .iter()
            .position(|m| m.id() == id)
            .map(|i| i + 1)
    }

    /// Validate a PIN for a member or engineer
    pub fn validate_pin(&self, member_id: &str, pin: &str) -> PinValidation {
        // Check engineer PIN first
        if let Some(ref eng_pin) = self.engineer_pin {
            if pin == eng_pin {
                return PinValidation::Engineer;
            }
        }

        // Check member PIN
        if let Some(expected_pin) = self.pins.get(member_id) {
            if pin == expected_pin {
                return PinValidation::Member(member_id.to_string());
            }
        }

        PinValidation::Invalid
    }
}

/// Result of PIN validation
#[derive(Debug, Clone, PartialEq)]
pub enum PinValidation {
    /// Valid member PIN
    Member(String),
    /// Valid engineer PIN (full access)
    Engineer,
    /// Invalid PIN
    Invalid,
}

/// Band member configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandMember {
    /// Display name (e.g., "Marek")
    pub name: String,

    /// Left Dante output channel (1-indexed)
    pub dante_output_l: u8,

    /// Right Dante output channel (1-indexed)
    pub dante_output_r: u8,
}

impl BandMember {
    /// Get lowercase ID for URL routing
    pub fn id(&self) -> String {
        self.name.to_lowercase()
    }

    /// Get REAPER track name for this member's output
    pub fn track_name(&self) -> String {
        format!("{} inear", self.name.to_uppercase())
    }
}

/// Input track configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputTrack {
    /// Track name (e.g., "MAREK mic")
    pub name: String,

    /// Dante input channel (1-indexed)
    pub dante_input: u8,

    /// Default send level in dB
    #[serde(default)]
    pub default_level_db: f32,
}

/// Configuration errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_member_id() {
        let member = BandMember {
            name: "Marek".to_string(),
            dante_output_l: 25,
            dante_output_r: 26,
        };
        assert_eq!(member.id(), "marek");
        assert_eq!(member.track_name(), "MAREK inear");
    }

    #[test]
    fn test_pin_validation() {
        let mut config = Config::default();
        config.pins.insert("marek".to_string(), "1234".to_string());
        config.engineer_pin = Some("9999".to_string());

        assert_eq!(
            config.validate_pin("marek", "1234"),
            PinValidation::Member("marek".to_string())
        );
        assert_eq!(
            config.validate_pin("marek", "9999"),
            PinValidation::Engineer
        );
        assert_eq!(
            config.validate_pin("marek", "wrong"),
            PinValidation::Invalid
        );
    }
}
