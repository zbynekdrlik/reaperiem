//! Configuration types for band members, inputs, and PINs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Default member PIN when no custom PIN is configured
pub const DEFAULT_MEMBER_PIN: &str = "7711";

/// Default engineer PIN (full access to any member's mixer)
pub const DEFAULT_ENGINEER_PIN: &str = "1177";

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

    /// Enable HTTPS (for PWA installability on phones)
    #[serde(default)]
    pub tls: bool,

    /// HTTPS port (default 443)
    #[serde(default = "default_https_port")]
    pub https_port: u16,

    /// TLS certificate file path (relative to config dir)
    #[serde(default = "default_tls_cert")]
    pub tls_cert: String,

    /// TLS private key file path (relative to config dir)
    #[serde(default = "default_tls_key")]
    pub tls_key: String,

    /// Domain for HTTPS redirect (HTTP requests to this domain → HTTPS)
    #[serde(default)]
    pub https_domain: Option<String>,
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

fn default_https_port() -> u16 {
    443
}

fn default_tls_cert() -> String {
    "cert.pem".to_string()
}

fn default_tls_key() -> String {
    "key.pem".to_string()
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
            tls: false,
            https_port: default_https_port(),
            tls_cert: default_tls_cert(),
            tls_key: default_tls_key(),
            https_domain: None,
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

    /// Get member's send index (0-based) for REAPER HTTP API
    ///
    /// REAPER HTTP API sends are 0-based: Send 0 = first member, Send 1 = second, etc.
    /// This matches the position in the config `members` array directly.
    pub fn member_index(&self, id: &str) -> Option<usize> {
        self.members.iter().position(|m| m.id() == id)
    }

    /// Find member and return both the 0-based send index and a reference.
    /// Eliminates the unwrap-after-find pattern in proxy.rs.
    pub fn find_member_with_index(&self, id: &str) -> Option<(usize, &BandMember)> {
        self.members.iter().enumerate().find(|(_, m)| m.id() == id)
    }

    /// Validate a PIN for a member or engineer
    pub fn validate_pin(&self, member_id: &str, pin: &str) -> PinValidation {
        // Check engineer PIN (config override or default "1177")
        let eng_pin = self.engineer_pin.as_deref().unwrap_or(DEFAULT_ENGINEER_PIN);
        if pin == eng_pin {
            return PinValidation::Engineer;
        }

        // Check member-specific PIN from config
        if let Some(expected_pin) = self.pins.get(member_id) {
            if pin == expected_pin {
                return PinValidation::Member(member_id.to_string());
            }
            return PinValidation::Invalid;
        }

        // No config PIN for this member — check default PIN "7711"
        if self.find_member(member_id).is_some() {
            if pin == DEFAULT_MEMBER_PIN {
                return PinValidation::Member(member_id.to_string());
            }
            return PinValidation::Invalid;
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

    fn make_test_member(name: &str) -> BandMember {
        BandMember {
            name: name.to_string(),
            dante_output_l: 3,
            dante_output_r: 4,
        }
    }

    #[test]
    fn test_default_pin_required_when_no_pin_configured() {
        let mut config = Config::default();
        config.members.push(make_test_member("Petka"));
        // Default PIN "7711" should work
        assert_eq!(
            config.validate_pin("petka", "7711"),
            PinValidation::Member("petka".to_string())
        );
        // Empty PIN should NOT work anymore
        assert_eq!(config.validate_pin("petka", ""), PinValidation::Invalid);
        // Wrong PIN should NOT work
        assert_eq!(config.validate_pin("petka", "0000"), PinValidation::Invalid);
    }

    #[test]
    fn test_engineer_pin_default_1177() {
        let mut config = Config::default();
        config.members.push(make_test_member("Petka"));
        // Default engineer PIN "1177" works on any member
        assert_eq!(
            config.validate_pin("petka", "1177"),
            PinValidation::Engineer
        );
    }

    #[test]
    fn test_engineer_pin_config_overrides_default() {
        let mut config = Config::default();
        config.engineer_pin = Some("9999".to_string());
        config.members.push(make_test_member("Petka"));
        // Config engineer PIN works
        assert_eq!(
            config.validate_pin("petka", "9999"),
            PinValidation::Engineer
        );
        // Default "1177" should NOT work when overridden
        assert_eq!(config.validate_pin("petka", "1177"), PinValidation::Invalid);
    }

    #[test]
    fn test_find_member_with_index() {
        let mut config = Config::default();
        config.members.push(make_test_member("Petka"));
        config.members.push(make_test_member("Stevo"));
        config.members.push(make_test_member("Marek"));

        let (idx, member) = config.find_member_with_index("stevo").unwrap();
        assert_eq!(idx, 1);
        assert_eq!(member.name, "Stevo");

        let (idx, member) = config.find_member_with_index("marek").unwrap();
        assert_eq!(idx, 2);
        assert_eq!(member.name, "Marek");

        assert!(config.find_member_with_index("unknown").is_none());
    }

    #[test]
    fn test_config_pin_overrides_default() {
        let mut config = Config::default();
        config.members.push(make_test_member("Petka"));
        config.pins.insert("petka".to_string(), "5555".to_string());
        // Config-specific PIN works
        assert_eq!(
            config.validate_pin("petka", "5555"),
            PinValidation::Member("petka".to_string())
        );
        // Default PIN "7711" should NOT work when member has config PIN
        assert_eq!(config.validate_pin("petka", "7711"), PinValidation::Invalid);
    }
}
