//! Elevated member storage with JSON persistence
//!
//! Tracks which band members have elevated access (can view/control other
//! members' mixes via mix channels, like the engineer).

use crate::atomic_write;
use std::collections::HashSet;
use std::path::PathBuf;

/// Runtime elevated-member storage backed by a JSON file
pub struct ElevatedStore {
    members: HashSet<String>,
    path: PathBuf,
}

impl ElevatedStore {
    /// Load from elevated.json in the given directory (or create empty if not found)
    pub fn load(config_dir: &std::path::Path) -> Self {
        let path = config_dir.join("elevated.json");
        let members = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            HashSet::new()
        };
        Self { members, path }
    }

    /// Check if a member has elevated access
    pub fn is_elevated(&self, member_id: &str) -> bool {
        self.members.contains(member_id)
    }

    /// Set or remove elevated access for a member and persist to disk
    pub fn set_elevated(&mut self, member_id: &str, elevated: bool) -> Result<(), std::io::Error> {
        iem_core::config::validate_member_id(member_id).map_err(std::io::Error::other)?;
        if elevated {
            self.members.insert(member_id.to_string());
        } else {
            self.members.remove(member_id);
        }
        self.save()
    }

    /// List all elevated member IDs
    pub fn list(&self) -> &HashSet<String> {
        &self.members
    }

    fn save(&self) -> Result<(), std::io::Error> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.members).map_err(std::io::Error::other)?;
        atomic_write(&self.path, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_store_not_elevated() {
        let dir = std::env::temp_dir().join("iem_test_elevated_empty");
        let _ = std::fs::remove_dir_all(&dir);
        let store = ElevatedStore::load(&dir);
        assert!(!store.is_elevated("petronela"));
        assert!(store.list().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_set_elevated_true() {
        let dir = std::env::temp_dir().join("iem_test_elevated_set");
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = ElevatedStore::load(&dir);
        store.set_elevated("petronela", true).unwrap();
        assert!(store.is_elevated("petronela"));
        assert!(!store.is_elevated("marek"));
        assert_eq!(store.list().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_set_elevated_false_removes() {
        let dir = std::env::temp_dir().join("iem_test_elevated_unset");
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = ElevatedStore::load(&dir);
        store.set_elevated("petronela", true).unwrap();
        assert!(store.is_elevated("petronela"));
        store.set_elevated("petronela", false).unwrap();
        assert!(!store.is_elevated("petronela"));
        assert!(store.list().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_persists_to_disk() {
        let dir = std::env::temp_dir().join("iem_test_elevated_persist");
        let _ = std::fs::remove_dir_all(&dir);

        // Save
        {
            let mut store = ElevatedStore::load(&dir);
            store.set_elevated("petronela", true).unwrap();
            store.set_elevated("marek", true).unwrap();
        }

        // Reload
        let store = ElevatedStore::load(&dir);
        assert!(store.is_elevated("petronela"));
        assert!(store.is_elevated("marek"));
        assert!(!store.is_elevated("stevo"));
        assert_eq!(store.list().len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_idempotent_set() {
        let dir = std::env::temp_dir().join("iem_test_elevated_idempotent");
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = ElevatedStore::load(&dir);
        store.set_elevated("petronela", true).unwrap();
        store.set_elevated("petronela", true).unwrap();
        assert_eq!(store.list().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
