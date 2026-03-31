//! Push subscription persistence for Web Push notifications (#133)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PushSubscription {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

pub struct PushStore {
    subscriptions: Vec<PushSubscription>,
    path: PathBuf,
}

impl PushStore {
    pub fn load(config_dir: &std::path::Path) -> Self {
        let path = config_dir.join("push_subscriptions.json");
        let subscriptions = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Self {
            subscriptions,
            path,
        }
    }

    pub fn all(&self) -> &[PushSubscription] {
        &self.subscriptions
    }

    /// Add or update a subscription (dedup by endpoint URL).
    pub fn add(&mut self, sub: PushSubscription) -> Result<(), std::io::Error> {
        if let Some(existing) = self
            .subscriptions
            .iter_mut()
            .find(|s| s.endpoint == sub.endpoint)
        {
            existing.p256dh = sub.p256dh;
            existing.auth = sub.auth;
        } else {
            self.subscriptions.push(sub);
        }
        self.save()
    }

    /// Remove a subscription by endpoint (called when push returns 404/410).
    pub fn remove_endpoint(&mut self, endpoint: &str) {
        self.subscriptions.retain(|s| s.endpoint != endpoint);
        let _ = self.save();
    }

    fn save(&self) -> Result<(), std::io::Error> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json =
            serde_json::to_string_pretty(&self.subscriptions).map_err(std::io::Error::other)?;
        crate::atomic_write(&self.path, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_store_crud() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = PushStore::load(dir.path());
        assert!(store.all().is_empty());

        let sub = PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/abc123".into(),
            p256dh: "BPK_key".into(),
            auth: "auth_secret".into(),
        };

        store.add(sub.clone()).unwrap();
        assert_eq!(store.all().len(), 1);

        // Dedup by endpoint
        let sub2 = PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/abc123".into(),
            p256dh: "new_key".into(),
            auth: "new_auth".into(),
        };
        store.add(sub2).unwrap();
        assert_eq!(store.all().len(), 1);
        assert_eq!(store.all()[0].p256dh, "new_key");

        store.remove_endpoint("https://fcm.googleapis.com/fcm/send/abc123");
        assert!(store.all().is_empty());
    }

    #[test]
    fn test_push_store_persistence() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut store = PushStore::load(dir.path());
            store
                .add(PushSubscription {
                    endpoint: "https://example.com/push/1".into(),
                    p256dh: "key1".into(),
                    auth: "auth1".into(),
                })
                .unwrap();
        }
        let store = PushStore::load(dir.path());
        assert_eq!(store.all().len(), 1);
        assert_eq!(store.all()[0].endpoint, "https://example.com/push/1");
    }
}
