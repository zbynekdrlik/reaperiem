//! Configuration types for band members, inputs, and PINs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use subtle::ConstantTimeEq;

/// Default member PIN when no custom PIN is configured
pub const DEFAULT_MEMBER_PIN: &str = "7711";

/// Default engineer PIN (full access to any member's mixer)
pub const DEFAULT_ENGINEER_PIN: &str = "1177";

/// Constant-time string comparison to prevent timing attacks on PIN verification.
/// Returns true if both strings are equal, false otherwise.
#[inline]
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// REAPER server URL
    #[serde(default = "default_reaper_url")]
    pub reaper_url: String,

    /// Server port
    #[serde(default = "default_port")]
    pub port: u16,

    /// Band members with their output assignments (LEGACY - will be removed)
    /// Members are now discovered from REAPER tracks ending in " inear"
    #[serde(default)]
    pub members: Vec<BandMember>,

    /// Dante output channel mappings, keyed by REAPER track name prefix.
    /// Example: "PETKA" -> [3, 4] maps REAPER track "PETKA inear" to Dante outputs 3 (L) and 4 (R).
    /// REAPER is the source of truth for member names; this config only provides Dante routing.
    #[serde(default)]
    pub dante_outputs: HashMap<String, [u8; 2]>,

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

    /// Public IP of the local network (for LAN/WAN detection via Cloudflare Tunnel).
    /// When a request comes through Cloudflare with CF-Connecting-IP matching this IP,
    /// the client is on the local church WiFi. Different IP = remote.
    #[serde(default)]
    pub local_public_ip: Option<String>,
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
            dante_outputs: HashMap::new(),
            inputs: Vec::new(),
            pins: HashMap::new(),
            engineer_pin: None,
            jwt_secret: default_jwt_secret(),
            tls: false,
            https_port: default_https_port(),
            tls_cert: default_tls_cert(),
            tls_key: default_tls_key(),
            https_domain: None,
            local_public_ip: None,
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

    /// YAML key for the JWT signing token
    const JWT_CONFIG_KEY: &'static str = "jwt_secret";

    /// Validate that critical security settings are configured.
    /// If jwt_secret is still the default placeholder, generates a random one
    /// and persists it to the config file so tokens survive restarts.
    pub fn validate_security(&mut self, config_path: Option<&Path>) {
        if self.jwt_secret == "change-me-in-production" || self.jwt_secret.is_empty() {
            // Generate a random value
            use std::time::{SystemTime, UNIX_EPOCH};
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            self.jwt_secret = format!(
                "auto-{:x}-{:x}",
                seed,
                seed.wrapping_mul(0x517cc1b727220a95)
            );

            // Persist so tokens survive app restarts
            if let Some(path) = config_path {
                if let Err(e) = self.persist_jwt_to_config(path) {
                    eprintln!("WARNING: Failed to save generated JWT config: {}", e);
                }
            }

            eprintln!(
                "INFO: Auto-generated JWT signing key and saved to config file. \
                 Tokens will now persist across restarts."
            );
        }
    }

    /// Write the current jwt_secret back to the config file.
    fn persist_jwt_to_config(&self, path: &Path) -> Result<(), ConfigError> {
        let content = std::fs::read_to_string(path).map_err(|e| ConfigError::Io(e.to_string()))?;

        let key = Self::JWT_CONFIG_KEY;
        let new_line = format!("{}: \"{}\"", key, self.jwt_secret);
        let updated = if content.contains(&format!("{}:", key)) {
            let mut result = String::new();
            for line in content.lines() {
                if line.trim_start().starts_with(&format!("{}:", key)) {
                    result.push_str(&new_line);
                } else {
                    result.push_str(line);
                }
                result.push('\n');
            }
            result
        } else {
            let mut result = content;
            if !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(&new_line);
            result.push('\n');
            result
        };

        std::fs::write(path, updated).map_err(|e| ConfigError::Io(e.to_string()))?;
        Ok(())
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

    /// Validate a PIN for a member or engineer.
    /// Uses constant-time comparison to prevent timing attacks.
    pub fn validate_pin(&self, member_id: &str, pin: &str) -> PinValidation {
        // Check engineer PIN (config override or default "1177")
        let eng_pin = self.engineer_pin.as_deref().unwrap_or(DEFAULT_ENGINEER_PIN);
        if constant_time_eq(pin, eng_pin) {
            return PinValidation::Engineer;
        }

        // Check member-specific PIN from config
        if let Some(expected_pin) = self.pins.get(member_id) {
            if constant_time_eq(pin, expected_pin) {
                return PinValidation::Member(member_id.to_string());
            }
            return PinValidation::Invalid;
        }

        // No config PIN for this member — check default PIN "7711"
        if self.find_member(member_id).is_some() {
            if constant_time_eq(pin, DEFAULT_MEMBER_PIN) {
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

/// A band member discovered from REAPER at runtime.
/// Created by querying REAPER for tracks ending in " inear".
/// REAPER is the source of truth for member names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredMember {
    /// Member name extracted from REAPER track (e.g., "PETKA" from "PETKA inear")
    pub name: String,

    /// REAPER track index (1-based)
    pub track_index: usize,

    /// Left Dante output channel (1-indexed)
    pub dante_output_l: u8,

    /// Right Dante output channel (1-indexed)
    pub dante_output_r: u8,

    /// 0-based index in discovered members list (matches send index)
    pub send_index: usize,

    /// Send index on this member's inear track that routes to ENGINEER.
    /// Discovered dynamically by querying REAPER send destinations.
    /// None if no send to engineer exists (e.g., engineer's own track).
    #[serde(default)]
    pub mix_send_index: Option<usize>,
}

impl DiscoveredMember {
    /// Get lowercase ID for URL routing and API
    pub fn id(&self) -> String {
        self.name.to_lowercase()
    }

    /// Get REAPER track name for this member's output
    pub fn track_name(&self) -> String {
        format!("{} inear", self.name)
    }

    /// Create a discovered member from a REAPER track name and config.
    /// Returns None if the track doesn't end in " inear" or has no Dante mapping.
    pub fn from_reaper_track(
        track_name: &str,
        track_index: usize,
        send_index: usize,
        config: &Config,
    ) -> Option<Self> {
        // Extract member name from track name (e.g., "PETKA" from "PETKA inear")
        let name = track_name.strip_suffix(" inear")?;

        // Look up Dante outputs from config
        let dante_channels = config.dante_outputs.get(name)?;

        Some(Self {
            name: name.to_string(),
            track_index,
            dante_output_l: dante_channels[0],
            dante_output_r: dante_channels[1],
            send_index,
            mix_send_index: None, // Discovered later by querying REAPER send destinations
        })
    }
}

/// Validate that a member_id is safe for use in filesystem paths.
/// Rejects path traversal attempts and special characters.
/// Returns Ok(()) if valid, Err with message if invalid.
pub fn validate_member_id(member_id: &str) -> Result<(), String> {
    if member_id.is_empty() {
        return Err("member_id cannot be empty".to_string());
    }
    // Only allow alphanumeric, underscore, and hyphen
    if !member_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!(
            "member_id '{}' contains invalid characters (only a-z, A-Z, 0-9, _, - allowed)",
            member_id
        ));
    }
    Ok(())
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
    fn test_track_name_uses_uppercase() {
        // Track name should use uppercase display name
        // REAPER tracks must match config - no aliases needed
        let member = BandMember {
            name: "Petronela".to_string(),
            dante_output_l: 3,
            dante_output_r: 4,
        };
        assert_eq!(member.id(), "petronela");
        assert_eq!(member.track_name(), "PETRONELA inear");
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

    // === NEW: Tests for REAPER as source of truth architecture ===

    #[test]
    fn test_dante_outputs_lookup() {
        // Config should have dante_outputs map keyed by REAPER track name prefix
        let mut config = Config::default();
        config.dante_outputs.insert("PETKA".to_string(), [3, 4]);
        config.dante_outputs.insert("STEVO".to_string(), [5, 6]);

        // Lookup should return Dante channels for a given REAPER track prefix
        assert_eq!(config.dante_outputs.get("PETKA"), Some(&[3, 4]));
        assert_eq!(config.dante_outputs.get("STEVO"), Some(&[5, 6]));
        assert_eq!(config.dante_outputs.get("UNKNOWN"), None);
    }

    #[test]
    fn test_dante_outputs_yaml_parsing() {
        // Config YAML should parse dante_outputs map correctly
        let yaml = r#"
reaper_url: "http://iem.lan:8080"
port: 80
dante_outputs:
  PETKA: [3, 4]
  STEVO: [5, 6]
  MAREK: [7, 8]
inputs: []
"#;
        let config: Config = serde_yaml::from_str(yaml).expect("YAML should parse");
        assert_eq!(config.dante_outputs.get("PETKA"), Some(&[3, 4]));
        assert_eq!(config.dante_outputs.get("MAREK"), Some(&[7, 8]));
    }

    #[test]
    fn test_discovered_member_from_reaper_track() {
        // DiscoveredMember should be created from REAPER track name
        let mut config = Config::default();
        config.dante_outputs.insert("PETKA".to_string(), [3, 4]);

        let member = DiscoveredMember::from_reaper_track("PETKA inear", 23, 0, &config)
            .expect("should parse");
        assert_eq!(member.name, "PETKA");
        assert_eq!(member.id(), "petka");
        assert_eq!(member.track_index, 23);
        assert_eq!(member.send_index, 0);
        assert_eq!(member.dante_output_l, 3);
        assert_eq!(member.dante_output_r, 4);
        assert_eq!(member.track_name(), "PETKA inear");
    }

    #[test]
    fn test_discovered_member_no_dante_mapping() {
        // Should return None if no Dante mapping exists
        let config = Config::default();
        let result = DiscoveredMember::from_reaper_track("PETKA inear", 23, 0, &config);
        assert!(
            result.is_none(),
            "Should return None when no Dante mapping exists"
        );
    }

    #[test]
    fn test_discovered_member_not_inear_track() {
        // Should return None for tracks that don't end in " inear"
        let mut config = Config::default();
        config.dante_outputs.insert("PETKA".to_string(), [3, 4]);

        let result = DiscoveredMember::from_reaper_track("PETKA mic", 1, 0, &config);
        assert!(
            result.is_none(),
            "Non-inear tracks should not be discovered"
        );
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;

    #[test]
    fn test_validate_member_id_accepts_valid_names() {
        assert!(validate_member_id("petka").is_ok());
        assert!(validate_member_id("engineer").is_ok());
        assert!(validate_member_id("STEVO").is_ok());
        assert!(validate_member_id("marek-2").is_ok());
        assert!(validate_member_id("band_member").is_ok());
    }

    #[test]
    fn test_validate_member_id_rejects_path_traversal() {
        assert!(validate_member_id("../etc").is_err());
        assert!(validate_member_id("..").is_err());
    }

    #[test]
    fn test_validate_member_id_rejects_empty() {
        assert!(validate_member_id("").is_err());
    }

    #[test]
    fn test_validate_member_id_rejects_special_chars() {
        assert!(validate_member_id("foo bar").is_err());
        assert!(validate_member_id("foo.json").is_err());
    }

    #[test]
    fn test_validate_security_generates_on_default() {
        let mut config = Config::default();
        config.validate_security(None);
        assert_ne!(config.jwt_secret, "change-me-in-production");
        assert!(config.jwt_secret.starts_with("auto-"));
    }

    #[test]
    fn test_validate_security_keeps_custom() {
        let mut config = Config::default();
        let val = "not-the-default-placeholder".to_string();
        config.jwt_secret = val.clone();
        config.validate_security(None);
        assert_eq!(config.jwt_secret, val);
    }

    #[test]
    fn test_validate_security_persists_to_file() {
        let dir = std::env::temp_dir().join(format!("iem-persist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.yaml");
        std::fs::write(&path, "port: 80\n").unwrap();

        let mut cfg = Config::load(&path).unwrap();
        cfg.validate_security(Some(&path));
        let generated = cfg.jwt_secret.clone();
        assert!(generated.starts_with("auto-"));

        // Reload from file — must be persisted
        let reloaded = Config::load(&path).unwrap();
        assert_eq!(reloaded.jwt_secret, generated);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_validate_security_no_overwrite_custom() {
        let dir = std::env::temp_dir().join(format!("iem-nowrite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.yaml");
        std::fs::write(&path, "port: 80\n").unwrap();

        let mut cfg = Config::default();
        let val = "my-custom-jwt-value".to_string();
        cfg.jwt_secret = val.clone();
        cfg.validate_security(Some(&path));
        assert_eq!(cfg.jwt_secret, val);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
