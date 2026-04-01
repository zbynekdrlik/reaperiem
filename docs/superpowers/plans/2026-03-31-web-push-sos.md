# Web Push SOS Notifications Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Send Web Push notifications to the engineer's phone when a band member presses SOS, even when the app is closed.

**Architecture:** Server generates VAPID P-256 keys, stores engineer push subscriptions in JSON, encrypts payloads per RFC 8291, and POSTs to browser push services. Service worker handles `push` events to show notifications. All crypto uses pure-Rust crates (no OpenSSL).

**Tech Stack:** p256 (ECDH + key gen), aes-gcm (encryption), hkdf/sha2 (key derivation), jsonwebtoken (VAPID JWT, already a dep), reqwest (HTTP POST, already a dep)

---

## File Map

| File | Action | Purpose |
|------|--------|---------|
| `iem-mixer/crates/iem-server/Cargo.toml` | Modify | Add crypto dependencies |
| `iem-mixer/crates/iem-core/src/config.rs` | Modify | Add `vapid_private_key` field + auto-generation |
| `iem-mixer/crates/iem-server/src/push_store.rs` | Create | Push subscription persistence (PinStore pattern) |
| `iem-mixer/crates/iem-server/src/push.rs` | Create | RFC 8291 encryption + VAPID JWT + send |
| `iem-mixer/crates/iem-server/src/lib.rs` | Modify | Add PushStore to AppState |
| `iem-mixer/crates/iem-server/src/routes.rs` | Modify | Add push API endpoints |
| `iem-mixer/crates/iem-server/src/proxy.rs` | Modify | Send push on CallEngineer |
| `iem-mixer/iem-ui/sw.js` | Modify | Add `push` event handler |
| `iem-mixer/iem-ui/src/pages/mixer.rs` | Modify | Subscribe to push on engineer login |

---

### Task 1: Add Dependencies

**Files:**
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`

- [ ] **Step 1: Add crypto + push dependencies**

Add after the existing `# Audio streaming` section in `[dependencies]`:

```toml
# Web Push notifications (RFC 8291 + VAPID) — pure Rust, no OpenSSL
p256 = { version = "0.13", features = ["ecdh", "pkcs8"] }
aes-gcm = "0.10"
hkdf = "0.12"
sha2 = "0.10"
rand_core = { version = "0.6", features = ["getrandom"] }
base64 = "0.22"
url = "2"
```

These are all pure-Rust RustCrypto crates — no native/C dependencies, no OpenSSL. Safe for Windows cross-compilation.

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/crates/iem-server/Cargo.toml
git commit -m "chore: add pure-Rust Web Push dependencies (p256, aes-gcm, hkdf)"
```

---

### Task 2: VAPID Key Generation in Config

**Files:**
- Modify: `iem-mixer/crates/iem-core/src/config.rs`

**Context:** The Config struct already has `jwt_secret` with auto-generation via `validate_security()`. The pattern: field has a serde default, `validate_security()` checks if it's the default value, generates a random one, and persists to config.yaml via string manipulation. The VAPID key follows the same pattern but stores a base64url-encoded P-256 private key (32 bytes).

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` module in `config.rs`:

```rust
#[test]
fn test_vapid_key_auto_generation() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.yaml");
    let mut f = std::fs::File::create(&config_path).unwrap();
    writeln!(f, "reaper_url: http://localhost:8080").unwrap();

    let mut config = Config::load(config_path.to_str().unwrap()).unwrap();
    assert_eq!(config.vapid_private_key, "");

    config.validate_security(Some(&config_path));
    assert!(!config.vapid_private_key.is_empty());

    // Key should be valid base64url and decode to 32 bytes
    use base64::Engine;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&config.vapid_private_key)
        .unwrap();
    assert_eq!(decoded.len(), 32);

    // Should persist to config file
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("vapid_private_key:"));

    // Reload should preserve the key
    let reloaded = Config::load(config_path.to_str().unwrap()).unwrap();
    assert_eq!(reloaded.vapid_private_key, config.vapid_private_key);
}

#[test]
fn test_vapid_public_key_derivation() {
    use base64::Engine;
    // Generate a key
    let sk = p256::SecretKey::random(&mut rand_core::OsRng);
    let raw = sk.to_bytes();
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&raw);

    let pk = Config::vapid_public_key_base64url(&encoded).unwrap();
    let pk_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&pk)
        .unwrap();
    // Uncompressed P-256 point = 65 bytes (0x04 prefix + 32 x + 32 y)
    assert_eq!(pk_bytes.len(), 65);
    assert_eq!(pk_bytes[0], 0x04);
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd iem-mixer && cargo test -p iem-core test_vapid -- --nocapture
```

Expected: FAIL — `vapid_private_key` field doesn't exist, `vapid_public_key_base64url` doesn't exist.

- [ ] **Step 3: Add iem-core dependencies**

In `iem-mixer/crates/iem-core/Cargo.toml`, add:

```toml
# VAPID key generation (Web Push)
p256 = { version = "0.13", features = ["pkcs8"] }
rand_core = { version = "0.6", features = ["getrandom"] }
base64 = "0.22"
```

- [ ] **Step 4: Add vapid_private_key field to Config struct**

In `config.rs`, add the field to the `Config` struct (after `jwt_secret`):

```rust
/// VAPID private key for Web Push (base64url-encoded P-256 scalar, 32 bytes)
#[serde(default)]
pub vapid_private_key: String,
```

- [ ] **Step 5: Add VAPID key generation to validate_security()**

In `validate_security()`, after the jwt_secret block, add:

```rust
// Auto-generate VAPID key pair for Web Push if not set
if self.vapid_private_key.is_empty() {
    let sk = p256::SecretKey::random(&mut rand_core::OsRng);
    use base64::Engine;
    self.vapid_private_key =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sk.to_bytes());

    if let Some(path) = config_path {
        let key = "vapid_private_key";
        let new_line = format!("{}: \"{}\"", key, self.vapid_private_key);
        if let Ok(content) = std::fs::read_to_string(path) {
            let updated = if content.contains(&format!("{}:", key)) {
                content
                    .lines()
                    .map(|line| {
                        if line.trim_start().starts_with(&format!("{}:", key)) {
                            new_line.as_str()
                        } else {
                            line
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n"
            } else {
                let mut result = content;
                if !result.ends_with('\n') {
                    result.push('\n');
                }
                result.push_str(&new_line);
                result.push('\n');
                result
            };
            let _ = std::fs::write(path, updated);
        }
    }

    eprintln!("INFO: Auto-generated VAPID key pair for Web Push notifications.");
}
```

- [ ] **Step 6: Add public key derivation helper**

Add as a static method on `Config`:

```rust
/// Derive the VAPID public key (base64url, uncompressed P-256 point) from the private key.
pub fn vapid_public_key_base64url(private_key_b64: &str) -> Result<String, ConfigError> {
    use base64::Engine;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(private_key_b64)
        .map_err(|e| ConfigError::Io(format!("invalid VAPID key base64: {}", e)))?;
    let sk = p256::SecretKey::from_slice(&raw)
        .map_err(|e| ConfigError::Io(format!("invalid VAPID P-256 key: {}", e)))?;
    let pk = sk.public_key();
    let point = pk.to_encoded_point(false); // uncompressed, 65 bytes
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(point.as_bytes()))
}
```

- [ ] **Step 7: Run tests to verify they pass**

```bash
cd iem-mixer && cargo test -p iem-core test_vapid -- --nocapture
```

Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/crates/iem-core/src/config.rs
git commit -m "feat: auto-generate VAPID P-256 key pair for Web Push (#133)"
```

---

### Task 3: Push Subscription Store

**Files:**
- Create: `iem-mixer/crates/iem-server/src/push_store.rs`
- Modify: `iem-mixer/crates/iem-server/src/lib.rs`

**Context:** Follow the PinStore pattern (`pin_store.rs`): struct with data + path, `load()` from JSON, `save()` with `atomic_write()`, wrapped in `Arc<RwLock<>>` in AppState. The `atomic_write()` helper is defined in `lib.rs`.

- [ ] **Step 1: Write the failing test**

Create `push_store.rs` with only the test module first:

```rust
//! Push subscription persistence for Web Push notifications (#133)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

        // Add
        store.add(sub.clone()).unwrap();
        assert_eq!(store.all().len(), 1);

        // Dedup by endpoint
        let sub2 = PushSubscription {
            endpoint: "https://fcm.googleapis.com/fcm/send/abc123".into(),
            p256dh: "new_key".into(),
            auth: "new_auth".into(),
        };
        store.add(sub2.clone()).unwrap();
        assert_eq!(store.all().len(), 1);
        assert_eq!(store.all()[0].p256dh, "new_key"); // Updated

        // Remove by endpoint
        store.remove_endpoint("https://fcm.googleapis.com/fcm/send/abc123");
        assert!(store.all().is_empty());
    }

    #[test]
    fn test_push_store_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let sub = PushSubscription {
            endpoint: "https://example.com/push/1".into(),
            p256dh: "key1".into(),
            auth: "auth1".into(),
        };

        {
            let mut store = PushStore::load(dir.path());
            store.add(sub.clone()).unwrap();
        }

        // Reload from disk
        let store = PushStore::load(dir.path());
        assert_eq!(store.all().len(), 1);
        assert_eq!(store.all()[0].endpoint, "https://example.com/push/1");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd iem-mixer && cargo test -p iem-server test_push_store -- --nocapture
```

Expected: FAIL — `load()`, `all()`, `add()`, `remove_endpoint()` not implemented.

- [ ] **Step 3: Implement PushStore**

Add the implementation above the test module in `push_store.rs`:

```rust
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
        Self { subscriptions, path }
    }

    pub fn all(&self) -> &[PushSubscription] {
        &self.subscriptions
    }

    /// Add or update a subscription (dedup by endpoint URL).
    pub fn add(&mut self, sub: PushSubscription) -> Result<(), std::io::Error> {
        if let Some(existing) = self.subscriptions.iter_mut().find(|s| s.endpoint == sub.endpoint) {
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
```

- [ ] **Step 4: Register module in lib.rs**

Add `pub mod push_store;` to `iem-server/src/lib.rs` (near the other store modules).

- [ ] **Step 5: Add PushStore to AppState**

In `lib.rs`, add the field to `AppState`:

```rust
/// Push subscription storage for Web Push notifications (#133)
pub push_store: Arc<RwLock<push_store::PushStore>>,
```

In `AppState::new()`, add initialization:

```rust
push_store: Arc::new(RwLock::new(push_store::PushStore::load(config_dir))),
```

- [ ] **Step 6: Run tests to verify they pass**

```bash
cd iem-mixer && cargo test -p iem-server test_push_store -- --nocapture
```

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add iem-mixer/crates/iem-server/src/push_store.rs iem-mixer/crates/iem-server/src/lib.rs
git commit -m "feat: add PushStore for Web Push subscription persistence (#133)"
```

---

### Task 4: Web Push Encryption + Delivery Module

**Files:**
- Create: `iem-mixer/crates/iem-server/src/push.rs`
- Modify: `iem-mixer/crates/iem-server/src/lib.rs`

**Context:** This module handles RFC 8291 payload encryption and VAPID JWT signing. It uses `reqwest` (already a dependency) to POST to the browser push service. All crypto uses pure-Rust crates from Task 1.

- [ ] **Step 1: Write the failing test**

Create `push.rs` with test first:

```rust
//! Web Push encryption (RFC 8291) and VAPID delivery (#133)

use crate::push_store::PushSubscription;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_payload_produces_valid_output() {
        // Generate a fake subscriber key pair (simulating browser)
        let subscriber_sk = p256::SecretKey::random(&mut rand_core::OsRng);
        let subscriber_pk = subscriber_sk.public_key();
        let subscriber_pub_bytes = subscriber_pk.to_encoded_point(false);

        let mut auth_secret = [0u8; 16];
        rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut auth_secret);

        use base64::Engine;
        let p256dh = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(subscriber_pub_bytes.as_bytes());
        let auth = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&auth_secret);

        let body = encrypt_payload(b"test payload", &p256dh, &auth).unwrap();

        // aes128gcm header: 16 (salt) + 4 (rs) + 1 (idlen) + 65 (keyid) = 86 bytes
        assert!(body.len() > 86);
        // Salt is 16 bytes
        let rs = u32::from_be_bytes([body[16], body[17], body[18], body[19]]);
        assert_eq!(rs, 4096);
        // keyid length = 65
        assert_eq!(body[20], 65);
    }

    #[test]
    fn test_vapid_jwt_structure() {
        let sk = p256::SecretKey::random(&mut rand_core::OsRng);
        use base64::Engine;
        let key_b64 =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sk.to_bytes());

        let (jwt, pub_key) = build_vapid_header(
            &key_b64,
            "https://fcm.googleapis.com/fcm/send/test",
        )
        .unwrap();

        // JWT has 3 dot-separated parts
        assert_eq!(jwt.split('.').count(), 3);
        // Public key is base64url
        assert!(!pub_key.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd iem-mixer && cargo test -p iem-server test_encrypt_payload test_vapid_jwt -- --nocapture
```

Expected: FAIL — functions not implemented.

- [ ] **Step 3: Implement RFC 8291 encryption**

Add to `push.rs` above the test module:

```rust
use aes_gcm::{aead::Aead, Aes128Gcm, KeyInit, Nonce};
use base64::Engine;
use hkdf::Hkdf;
use sha2::Sha256;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Encrypt a push message payload per RFC 8291 (aes128gcm content encoding).
pub fn encrypt_payload(
    plaintext: &[u8],
    subscriber_p256dh_b64: &str,
    subscriber_auth_b64: &str,
) -> anyhow::Result<Vec<u8>> {
    let subscriber_pub_bytes = B64.decode(subscriber_p256dh_b64)?;
    let auth_secret = B64.decode(subscriber_auth_b64)?;

    let subscriber_pk = p256::PublicKey::from_sec1_bytes(&subscriber_pub_bytes)?;

    // Generate ephemeral ECDH key pair
    let ephemeral = p256::ecdh::EphemeralSecret::random(&mut rand_core::OsRng);
    let ephemeral_pk = p256::PublicKey::from(&ephemeral);
    let ephemeral_pub_bytes = ephemeral_pk.to_encoded_point(false);

    // ECDH shared secret
    let shared = ephemeral.diffie_hellman(&subscriber_pk);

    // Random salt
    let mut salt = [0u8; 16];
    rand_core::RngCore::fill_bytes(&mut rand_core::OsRng, &mut salt);

    // Derive IKM: HKDF(salt=auth_secret, ikm=shared, info="WebPush: info\0" || ua_pub || as_pub)
    let mut info_ikm = Vec::with_capacity(131);
    info_ikm.extend_from_slice(b"WebPush: info\0");
    info_ikm.extend_from_slice(&subscriber_pub_bytes);
    info_ikm.extend_from_slice(ephemeral_pub_bytes.as_bytes());

    let hkdf_auth = Hkdf::<Sha256>::new(Some(&auth_secret), shared.raw_secret_bytes().as_slice());
    let mut ikm = [0u8; 32];
    hkdf_auth.expand(&info_ikm, &mut ikm).map_err(|e| anyhow::anyhow!("{}", e))?;

    // Derive CEK and nonce from IKM + salt
    let hkdf_cek = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut cek = [0u8; 16];
    hkdf_cek
        .expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let mut nonce_bytes = [0u8; 12];
    hkdf_cek
        .expand(b"Content-Encoding: nonce\0", &mut nonce_bytes)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Pad plaintext (0x02 = final record delimiter)
    let mut padded = plaintext.to_vec();
    padded.push(0x02);

    // Encrypt with AES-128-GCM
    let cipher = Aes128Gcm::new_from_slice(&cek)?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, padded.as_slice())
        .map_err(|e| anyhow::anyhow!("AES-GCM encrypt: {}", e))?;

    // Build aes128gcm body: salt(16) + rs(4) + idlen(1) + keyid(65) + ciphertext
    let rs: u32 = 4096;
    let mut body = Vec::with_capacity(86 + ciphertext.len());
    body.extend_from_slice(&salt);
    body.extend_from_slice(&rs.to_be_bytes());
    body.push(65); // uncompressed P-256 point length
    body.extend_from_slice(ephemeral_pub_bytes.as_bytes());
    body.extend_from_slice(&ciphertext);

    Ok(body)
}
```

- [ ] **Step 4: Implement VAPID JWT builder**

Add to `push.rs`:

```rust
/// Build VAPID Authorization header components: (jwt_token, base64url_public_key).
pub fn build_vapid_header(
    vapid_private_key_b64: &str,
    endpoint: &str,
) -> anyhow::Result<(String, String)> {
    let raw = B64.decode(vapid_private_key_b64)?;
    let sk = p256::SecretKey::from_slice(&raw)?;

    // Audience = origin of the push endpoint
    let parsed = url::Url::parse(endpoint)?;
    let audience = format!("{}://{}", parsed.scheme(), parsed.host_str().unwrap_or(""));

    // Build JWT claims
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let claims = serde_json::json!({
        "aud": audience,
        "exp": now + 12 * 3600, // 12 hours
        "sub": "mailto:iem@newlevel.media",
    });

    // Sign with ES256 using jsonwebtoken
    use p256::pkcs8::EncodePrivateKey;
    let pkcs8_der = sk.to_pkcs8_der()?;
    let encoding_key = jsonwebtoken::EncodingKey::from_ec_der(pkcs8_der.as_bytes());
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
    header.typ = Some("JWT".into());
    let jwt = jsonwebtoken::encode(&header, &claims, &encoding_key)?;

    // Public key for k= parameter
    let pk = sk.public_key();
    let pub_b64 = B64.encode(pk.to_encoded_point(false).as_bytes());

    Ok((jwt, pub_b64))
}
```

- [ ] **Step 5: Implement push send function**

Add to `push.rs`:

```rust
/// Send a Web Push notification to a single subscription.
/// Returns Ok(true) if sent, Ok(false) if subscription expired (should be removed).
pub async fn send_push(
    client: &reqwest::Client,
    vapid_private_key_b64: &str,
    sub: &PushSubscription,
    payload: &[u8],
) -> anyhow::Result<bool> {
    let body = encrypt_payload(payload, &sub.p256dh, &sub.auth)?;
    let (jwt, pub_key) = build_vapid_header(vapid_private_key_b64, &sub.endpoint)?;

    let resp = client
        .post(&sub.endpoint)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Encoding", "aes128gcm")
        .header("TTL", "86400")
        .header(
            "Authorization",
            format!("vapid t={}, k={}", jwt, pub_key),
        )
        .body(body)
        .send()
        .await?;

    let status = resp.status().as_u16();
    match status {
        200 | 201 | 202 => Ok(true),
        404 | 410 => Ok(false), // Subscription expired — caller should remove
        _ => {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("push failed ({}): {}", status, text);
        }
    }
}

/// Send push to all engineer subscriptions. Removes expired ones.
pub async fn send_push_to_engineers(
    client: &reqwest::Client,
    vapid_key: &str,
    push_store: &std::sync::Arc<tokio::sync::RwLock<crate::push_store::PushStore>>,
    payload: &[u8],
) {
    let subs = {
        let store = push_store.read().await;
        store.all().to_vec()
    };

    if subs.is_empty() {
        return;
    }

    let mut expired = Vec::new();
    for sub in &subs {
        match send_push(client, vapid_key, sub, payload).await {
            Ok(true) => {
                tracing::debug!("push sent to {}", &sub.endpoint[..50.min(sub.endpoint.len())]);
            }
            Ok(false) => {
                tracing::info!("push subscription expired, removing");
                expired.push(sub.endpoint.clone());
            }
            Err(e) => {
                tracing::warn!("push send error: {}", e);
            }
        }
    }

    if !expired.is_empty() {
        let mut store = push_store.write().await;
        for endpoint in expired {
            store.remove_endpoint(&endpoint);
        }
    }
}
```

- [ ] **Step 6: Register module in lib.rs**

Add `pub mod push;` to `iem-server/src/lib.rs`.

- [ ] **Step 7: Run tests to verify they pass**

```bash
cd iem-mixer && cargo test -p iem-server test_encrypt_payload test_vapid_jwt -- --nocapture
```

Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add iem-mixer/crates/iem-server/src/push.rs iem-mixer/crates/iem-server/src/lib.rs
git commit -m "feat: RFC 8291 Web Push encryption + VAPID delivery (#133)"
```

---

### Task 5: API Endpoints

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/routes.rs`

**Context:** Routes are registered in `api_routes()` function. Handlers use `axum::extract::State(state): axum::extract::State<AppState>` to access state. Engineer-only access is checked via `auth::extract_claims` from JWT in Authorization header.

- [ ] **Step 1: Add route registrations**

In `routes.rs`, in the `api_routes()` function, add before the `// WebSocket` comment:

```rust
// Web Push notification subscription (#133)
.route("/api/push/vapid-key", get(get_vapid_key))
.route("/api/push/subscribe", post(push_subscribe))
```

- [ ] **Step 2: Implement GET /api/push/vapid-key handler**

Add to `routes.rs`:

```rust
/// Return the VAPID public key for browser push subscription.
async fn get_vapid_key(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl axum::response::IntoResponse {
    let config = state.config.read().await;
    match iem_core::config::Config::vapid_public_key_base64url(&config.vapid_private_key) {
        Ok(pub_key) => (axum::http::StatusCode::OK, axum::Json(serde_json::json!({ "key": pub_key }))),
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({ "error": "VAPID key not configured" })),
        ),
    }
}
```

- [ ] **Step 3: Implement POST /api/push/subscribe handler**

Add to `routes.rs`:

```rust
/// Store a push subscription (engineer-only).
async fn push_subscribe(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> impl axum::response::IntoResponse {
    // Verify engineer token
    let config = state.config.read().await;
    let claims = match auth::extract_claims(
        headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .unwrap_or(""),
        &config.jwt_secret,
    ) {
        Some(c) if c.engineer => c,
        _ => {
            return (
                axum::http::StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({ "error": "engineer access required" })),
            );
        }
    };
    drop(config);

    // Parse subscription
    let endpoint = body["endpoint"].as_str().unwrap_or("").to_string();
    let p256dh = body["keys"]["p256dh"].as_str().unwrap_or("").to_string();
    let auth = body["keys"]["auth"].as_str().unwrap_or("").to_string();

    if endpoint.is_empty() || p256dh.is_empty() || auth.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": "missing endpoint, p256dh, or auth" })),
        );
    }

    let sub = crate::push_store::PushSubscription {
        endpoint,
        p256dh,
        auth,
    };
    let mut store = state.push_store.write().await;
    match store.add(sub) {
        Ok(()) => (
            axum::http::StatusCode::OK,
            axum::Json(serde_json::json!({ "ok": true })),
        ),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({ "error": format!("save failed: {}", e) })),
        ),
    }
}
```

- [ ] **Step 4: Run build check**

```bash
cd iem-mixer && cargo check -p iem-server
```

Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/crates/iem-server/src/routes.rs
git commit -m "feat: add /api/push/vapid-key and /api/push/subscribe endpoints (#133)"
```

---

### Task 6: Send Push on CallEngineer

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs`

**Context:** The `CallEngineer` handler is in the WebSocket message processing loop (~line 1144). After the existing `event_tx.send()` broadcasts, we spawn a background task to send push notifications. The push task is fire-and-forget — it must not block the WS handler.

- [ ] **Step 1: Add push delivery after WS broadcast**

In `proxy.rs`, find the `CallEngineer` match arm. After the two `event_tx.send()` calls and before `continue;`, add:

```rust
// Send Web Push to all engineer devices (fire-and-forget)
{
    let push_store = state.push_store.clone();
    let http_client = state.http_client.clone();
    let vapid_key = state.config.read().await.vapid_private_key.clone();
    let push_name = display_name.clone();
    let push_member = member_id.clone();
    if !vapid_key.is_empty() {
        tokio::spawn(async move {
            let payload = serde_json::json!({
                "type": "SOS",
                "name": push_name,
                "member": push_member,
            });
            crate::push::send_push_to_engineers(
                &http_client,
                &vapid_key,
                &push_store,
                payload.to_string().as_bytes(),
            )
            .await;
        });
    }
}
```

- [ ] **Step 2: Run build check**

```bash
cd iem-mixer && cargo check -p iem-server
```

Expected: Compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-server/src/proxy.rs
git commit -m "feat: send Web Push to engineers on CallEngineer (#133)"
```

---

### Task 7: Service Worker Push Event Handler

**Files:**
- Modify: `iem-mixer/iem-ui/sw.js`

**Context:** The service worker already has a `message` event handler for in-app alerts and a `notificationclick` handler that opens `/engineer`. The new `push` event handler is separate and works even when the app is completely closed.

- [ ] **Step 1: Add push event listener**

In `sw.js`, add after the existing `notificationclick` listener (at the end of the file):

```javascript
// Handle Web Push notifications (works even when app is fully closed)
self.addEventListener("push", (event) => {
  let data = {};
  try {
    data = event.data?.json() ?? {};
  } catch {
    data = {};
  }

  if (data.type === "SOS") {
    event.waitUntil(
      self.registration.showNotification(`IEM Alert: ${data.name || "Member"}`, {
        body: `${data.name || "Someone"} needs help!`,
        requireInteraction: true,
        tag: "iem-alert",
        vibrate: [500, 200, 500, 200, 500],
      }),
    );
  } else {
    // Generic fallback for unknown push types
    event.waitUntil(
      self.registration.showNotification("IEM Mixer", {
        body: "New alert — tap to open",
        requireInteraction: true,
        tag: "iem-generic",
      }),
    );
  }
});
```

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/iem-ui/sw.js
git commit -m "feat: service worker push event handler for SOS alerts (#133)"
```

---

### Task 8: Frontend Push Subscription

**Files:**
- Modify: `iem-mixer/iem-ui/src/pages/mixer.rs`
- Modify: `iem-mixer/iem-ui/Cargo.toml` (add web-sys features)

**Context:** The mixer page detects engineers via `crate::auth::get_auth().map(|a| a.engineer).unwrap_or(false)` (line 884). The WebSocket is established in `connect_websocket()`. After the WS connects successfully, we subscribe to push if the session is an engineer. The subscription is sent to `POST /api/push/subscribe`.

- [ ] **Step 1: Add web-sys features for Push API**

In `iem-mixer/iem-ui/Cargo.toml`, add to the `web-sys` features list:

```toml
# Web Push subscription API (#133)
"ServiceWorkerContainer",
"ServiceWorkerRegistration",
"PushManager",
"PushSubscription",
"PushSubscriptionOptionsInit",
```

- [ ] **Step 2: Add push subscription helper function**

In `mixer.rs`, add a standalone function (outside any component, near the top-level helpers):

```rust
/// Subscribe to Web Push for engineer SOS alerts (#133).
/// Fetches VAPID key, subscribes via Push API, sends subscription to server.
fn subscribe_to_push() {
    let is_engineer = crate::auth::get_auth().map(|a| a.engineer).unwrap_or(false);
    if !is_engineer {
        return;
    }

    wasm_bindgen_futures::spawn_local(async move {
        // 1. Fetch VAPID public key from server
        let vapid_key = match crate::api::api_get("/api/push/vapid-key").await {
            Ok(resp) => match resp.get("key").and_then(|k| k.as_str()) {
                Some(k) => k.to_string(),
                None => return,
            },
            Err(_) => return,
        };

        // 2. Convert base64url key to Uint8Array for pushManager.subscribe()
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let navigator = window.navigator();
        let sw_container: web_sys::ServiceWorkerContainer = match js_sys::Reflect::get(
            &navigator,
            &wasm_bindgen::JsValue::from_str("serviceWorker"),
        )
        .ok()
        .and_then(|v| v.dyn_into().ok())
        {
            Some(c) => c,
            None => return,
        };

        // Wait for SW to be ready
        let ready_promise = sw_container.ready().unwrap();
        let registration: web_sys::ServiceWorkerRegistration =
            match wasm_bindgen_futures::JsFuture::from(ready_promise).await {
                Ok(r) => match r.dyn_into() {
                    Ok(reg) => reg,
                    Err(_) => return,
                },
                Err(_) => return,
            };

        // 3. Subscribe to push
        let push_manager = match registration.push_manager() {
            Ok(pm) => pm,
            Err(_) => return,
        };

        // Decode base64url to Uint8Array
        let key_bytes = match base64url_decode(&vapid_key) {
            Some(b) => b,
            None => return,
        };
        let key_array = js_sys::Uint8Array::new_with_length(key_bytes.len() as u32);
        key_array.copy_from(&key_bytes);

        let mut opts = web_sys::PushSubscriptionOptionsInit::new();
        opts.user_visible_only(true);
        opts.application_server_key(Some(&key_array.into()));

        let sub_promise = match push_manager.subscribe_with_options(&opts) {
            Ok(p) => p,
            Err(e) => {
                web_sys::console::warn_1(
                    &format!("[push] subscribe failed: {:?}", e).into(),
                );
                return;
            }
        };
        let sub: web_sys::PushSubscription = match wasm_bindgen_futures::JsFuture::from(sub_promise)
            .await
            .ok()
            .and_then(|v| v.dyn_into().ok())
        {
            Some(s) => s,
            None => return,
        };

        // 4. Extract subscription fields and send to server
        let endpoint = sub.endpoint();
        let json_str = match js_sys::JSON::stringify(&sub.to_json())
            .ok()
            .and_then(|s| s.as_string())
        {
            Some(s) => s,
            None => return,
        };

        // POST to server
        let _ = crate::api::api_post("/api/push/subscribe", &json_str).await;
        web_sys::console::log_1(&"[push] engineer subscribed to Web Push".into());
    });
}

/// Decode base64url (no padding) to bytes.
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut s = input.replace('-', "+").replace('_', "/");
    while s.len() % 4 != 0 {
        s.push('=');
    }
    web_sys::window()?
        .atob(&s)
        .ok()
        .map(|decoded| decoded.bytes().collect())
}
```

- [ ] **Step 3: Call subscribe_to_push on engineer session**

In the `MixerPage` component body, after the line where `is_engineer` is set (line ~884), add:

```rust
// Subscribe to Web Push for SOS alerts (engineer only, one-time)
if is_engineer {
    subscribe_to_push();
}
```

- [ ] **Step 4: Verify api_get and api_post exist in api.rs**

Check that `crate::api::api_get` and `crate::api::api_post` helper functions exist and have the right signatures. If `api_post` doesn't accept a raw JSON string, adapt the call to use the existing API pattern (likely `api_post(url, &serde_json::Value)`). The subscription JSON from `sub.to_json()` already matches the expected format: `{ "endpoint": "...", "keys": { "p256dh": "...", "auth": "..." } }`.

- [ ] **Step 5: Run build check**

```bash
cd iem-mixer && cargo check -p iem-ui --target wasm32-unknown-unknown
```

Expected: Compiles. If `api_post` signature doesn't match, adapt the call.

- [ ] **Step 6: Commit**

```bash
git add iem-mixer/iem-ui/Cargo.toml iem-mixer/iem-ui/src/pages/mixer.rs
git commit -m "feat: frontend push subscription for engineer SOS alerts (#133)"
```

---

### Task 9: Version Bump + Lint + Final Commit

**Files:**
- Modify: 6 `Cargo.toml` files + `tauri.conf.json`
- Modify: `README.md`

- [ ] **Step 1: Check current version**

```bash
grep '^version' iem-mixer/crates/iem-core/Cargo.toml
```

- [ ] **Step 2: Bump version (1.124.0 → 1.125.0)**

```bash
sed -i 's/version = "1.124.0"/version = "1.125.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.124.0"/"version": "1.125.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 3: Update changelog in README.md**

Add under `## Changelog`:

```markdown
### v1.125.0 (2026-03-31)
- **Feature**: Web Push notifications for SOS alert — engineer gets notified even when app is closed (#133)
```

- [ ] **Step 4: Run lint checks**

```bash
cd iem-mixer && cargo fmt --all --check
cd iem-mixer && cargo clippy --workspace --all-targets -- -D warnings
```

Fix any issues before proceeding.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: bump version to 1.125.0 + changelog for Web Push SOS (#133)"
```

- [ ] **Step 6: Push and monitor CI**

```bash
git push origin dev
gh run list --branch dev --limit 3
# Monitor until all jobs pass
```

---

## Verification

After CI deploys:

1. **VAPID key generated**: SSH to iem.lan, check `config.yaml` has `vapid_private_key` field
2. **API endpoint works**: `curl -s http://10.77.9.231/api/push/vapid-key` returns `{ "key": "..." }`
3. **Engineer subscribe**: Log in as engineer on phone, check browser console for `[push] engineer subscribed to Web Push`
4. **End-to-end push**: Band member taps SOS → engineer phone shows system notification (even with app closed)
5. **Notification click**: Tapping notification opens the app at `/engineer`
