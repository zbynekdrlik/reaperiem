# Mixer Backup & Restore — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatic scheduled mixer state backups with engineer-only restore UI in the Settings modal.

**Architecture:** A tokio cron daemon captures all mixer state (sends, volumes, EQ, limiter, customizations, PINs) into timestamped JSON files. REST API serves backup list/preview/restore. Leptos UI adds a "Backups" section to the Settings modal (engineer-only). Name-based matching ensures structural compatibility across REAPER project changes.

**Tech Stack:** Rust (Axum, Tokio, Serde), Leptos (WASM frontend), REAPER HTTP API, ReaScript EXTSTATE

**Spec:** `docs/superpowers/specs/2026-04-07-mixer-backup-restore-design.md`

---

## Context

The IEM mixer runs on production REAPER. CI E2E tests and development can corrupt band members' mixer settings. The engineer needs one-click restore to a known-good state. Backups run automatically at configured times (default 13:00, 21:00). Restore matches by track name (not index) so it works even after tracks are added/removed/reordered.

**Key codebase patterns:**
- Routes: `pub fn name_routes() -> Router<AppState>` in separate `*_routes.rs` files
- Auth: `crate::auth::verify_member_access(&headers, &member, &config.jwt_secret)?` or check `claims.engineer`
- Persistence: `atomic_write(&path, &json)` for crash-safe JSON writes
- Config: `Config` struct with `#[serde(default)]` fields, loaded from YAML
- AppState: stores added as `Arc<Store>` fields, initialized in `AppState::new()`
- UI: `{if is_engineer { Some(view! { ... }) } else { None }}` for engineer-only sections
- API calls from UI: `gloo_net::http::Request` with `Authorization: Bearer {token}` header

**REAPER API for reading state:**
- Tracks: `/_/NTRACK;TRACK` → tab-separated fields, field[1]=index, field[2]=name, field[4]=volume
- Sends: `/_/GET/TRACK/{src}/SEND/{s}` → `SEND\tsrc\tsend_idx\tmute\tvol\tpan\tdest`
- EQ: EXTSTATE `eq_read_track` + action `_RS_REAPERIEM_READ_EQ` → EXTSTATE `eq_params`
- Limiter: EXTSTATE `limiter_read_track` + action `_RS_REAPERIEM_READ_LIMITER` → EXTSTATE `limiter_params`

**REAPER API for writing state:**
- Send vol: `/_/SET/TRACK/{src}/SEND/{s}/VOL/{v}`
- Send pan: `/_/SET/TRACK/{src}/SEND/{s}/PAN/{p}`
- Send mute: `/_/SET/TRACK/{src}/SEND/{s}/MUTE/{m}` (0=unmuted, 1=muted)
- Track vol: `/_/SET/TRACK/{t}/VOL/{v}`
- EQ: EXTSTATE `eq_set` = `track=N|band=B|param=P|value=V` + action `_RS_REAPERIEM_SET_EQ`
- Limiter: EXTSTATE `limiter_set` = `track=N|param=P|value=V` + action `_RS_REAPERIEM_SET_LIMITER`
- Save project: `/_/40026`

---

## File Map

### New files
- `iem-mixer/crates/iem-core/src/backup.rs` — Backup JSON schema types (serde structs)
- `iem-mixer/crates/iem-server/src/backup_store.rs` — Backup file storage (save, list, load, prune)
- `iem-mixer/crates/iem-server/src/backup_capture.rs` — State capture logic (reads REAPER + local files)
- `iem-mixer/crates/iem-server/src/backup_restore.rs` — Restore logic (preview diff + apply)
- `iem-mixer/crates/iem-server/src/backup_routes.rs` — REST API endpoints
- `iem-mixer/crates/iem-server/src/backup_daemon.rs` — Tokio cron scheduler
- `iem-mixer/iem-ui/src/components/backup_section.rs` — Leptos UI for Settings modal

### Modified files
- `iem-mixer/crates/iem-core/src/lib.rs` — re-export backup types
- `iem-mixer/crates/iem-core/src/config.rs` — add `backup_schedule` and `backup_retention_days`
- `iem-mixer/crates/iem-server/src/lib.rs` — add `backup_store` to AppState, register module
- `iem-mixer/crates/iem-server/src/proxy.rs` — mount backup routes
- `iem-mixer/iem-ui/src/components/settings_modal.rs` — add backup section
- `iem-mixer/iem-ui/src/components/mod.rs` — register backup_section module
- `iem-mixer/iem-ui/src/api.rs` — add backup API functions
- `.github/workflows/ci.yml` — include customizations/ in git backup

### Version bump
- 5 Cargo.toml + 1 tauri.conf.json: 1.134.0 → 1.135.0

---

## Task 1: Version Bump (1.134.0 → 1.135.0)

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`

- [ ] **Step 1: Bump all version files**

```bash
sed -i 's/version = "1.134.0"/version = "1.135.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.134.0"/"version": "1.135.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify**

```bash
grep -c '1.135.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
# Both should return 1
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.135.0"
```

---

## Task 2: Backup JSON Schema Types (iem-core)

**Files:**
- Create: `iem-mixer/crates/iem-core/src/backup.rs`
- Modify: `iem-mixer/crates/iem-core/src/lib.rs`

- [ ] **Step 1: Create backup types with tests**

Create `iem-mixer/crates/iem-core/src/backup.rs`:

```rust
//! Backup JSON schema types for mixer state snapshots.
//!
//! All state keyed by track NAME (not index) for structural compatibility.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{Customization, EqBand};

/// Version of the backup format
pub const BACKUP_VERSION: u32 = 1;

/// Complete mixer state backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerBackup {
    /// Schema version for forward compatibility
    pub version: u32,
    /// ISO 8601 timestamp of when this backup was captured
    pub timestamp: String,
    /// Track index → name mapping at capture time (for diagnostics)
    pub track_layout: HashMap<u32, String>,
    /// All send routing with values, keyed by "src_name -> dest_name"
    pub sends: Vec<SendBackup>,
    /// Track output volumes, keyed by track name
    pub track_volumes: HashMap<String, f64>,
    /// EQ bands per track, keyed by track name
    pub eq: HashMap<String, Vec<EqBandBackup>>,
    /// Limiter params per track, keyed by track name
    pub limiter: HashMap<String, LimiterBackup>,
    /// Channel customizations per member ID
    pub customizations: HashMap<String, Customization>,
    /// Member PINs (raw values from pins.json)
    pub pins: HashMap<String, String>,
}

/// A single send's state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendBackup {
    pub src_name: String,
    pub dest_name: String,
    pub vol: f64,
    pub pan: f64,
    pub mute: bool,
}

/// A single EQ band's state (mirrors EqBand but with explicit serialization)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqBandBackup {
    pub band: u8,
    pub band_type: String,
    pub freq_norm: f32,
    pub gain_norm: f32,
    pub bw_norm: f32,
    pub freq_hz: f32,
    pub gain_db: f32,
    pub bw_oct: f32,
    pub enabled: bool,
}

impl From<(u8, &EqBand)> for EqBandBackup {
    fn from((band, eq): (u8, &EqBand)) -> Self {
        Self {
            band,
            band_type: eq.band_type.clone(),
            freq_norm: eq.freq_norm,
            gain_norm: eq.gain_norm,
            bw_norm: eq.bw_norm,
            freq_hz: eq.freq_hz,
            gain_db: eq.gain_db,
            bw_oct: eq.bw,
            enabled: eq.enabled,
        }
    }
}

/// Limiter state for a track
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimiterBackup {
    pub limit_db: f32,
    pub limit_norm: f32,
    pub enabled: bool,
}

/// Metadata for backup list (without full data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// Filename (e.g., "2026-04-05_130000.json")
    pub filename: String,
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// File size in bytes
    pub size_bytes: u64,
    /// Number of sends in backup
    pub send_count: usize,
    /// Number of tracks in backup
    pub track_count: usize,
}

/// Preview of what a restore would change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePreview {
    /// Values that differ and will be restored
    pub changes: Vec<RestoreChange>,
    /// Values already matching (no change needed)
    pub unchanged_count: usize,
    /// Tracks/sends in backup but not in current project
    pub skipped: Vec<SkippedEntry>,
}

/// A single value that will be changed during restore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreChange {
    pub category: RestoreCategory,
    pub description: String,
    pub current_value: String,
    pub backup_value: String,
}

/// Category of restore change (for grouping in UI)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RestoreCategory {
    Send,
    TrackVolume,
    Eq,
    Limiter,
    Customization,
    Pin,
}

/// An entry that was skipped during restore preview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedEntry {
    pub category: RestoreCategory,
    pub description: String,
    pub reason: String,
}

/// Progress update during restore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreProgress {
    pub total: usize,
    pub completed: usize,
    pub current_step: String,
}

/// Final result of a restore operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    pub restored_count: usize,
    pub skipped: Vec<SkippedEntry>,
    pub project_saved: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_roundtrip() {
        let backup = MixerBackup {
            version: BACKUP_VERSION,
            timestamp: "2026-04-05T13:00:00+02:00".to_string(),
            track_layout: HashMap::from([(1, "PETRONELA mic".to_string())]),
            sends: vec![SendBackup {
                src_name: "PETRONELA mic".to_string(),
                dest_name: "PETRONELA inear".to_string(),
                vol: 0.5,
                pan: 0.0,
                mute: false,
            }],
            track_volumes: HashMap::from([("PETRONELA inear".to_string(), 0.5011872)]),
            eq: HashMap::new(),
            limiter: HashMap::new(),
            customizations: HashMap::new(),
            pins: HashMap::new(),
        };

        let json = serde_json::to_string(&backup).unwrap();
        let parsed: MixerBackup = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, BACKUP_VERSION);
        assert_eq!(parsed.sends.len(), 1);
        assert_eq!(parsed.sends[0].src_name, "PETRONELA mic");
        assert!((parsed.track_volumes["PETRONELA inear"] - 0.5011872).abs() < 1e-6);
    }

    #[test]
    fn test_eq_band_backup_from_eq_band() {
        let eq = crate::EqBand {
            band_type: "band".to_string(),
            freq_hz: 1000.0,
            gain_db: -3.0,
            bw: 1.5,
            freq_norm: 0.5,
            gain_norm: 0.2,
            bw_norm: 0.35,
            enabled: true,
        };
        let backup = EqBandBackup::from((2, &eq));
        assert_eq!(backup.band, 2);
        assert_eq!(backup.band_type, "band");
        assert!(backup.enabled);
        assert!((backup.freq_hz - 1000.0).abs() < 0.01);
    }
}
```

- [ ] **Step 2: Register module in lib.rs**

Add to `iem-mixer/crates/iem-core/src/lib.rs`:

```rust
pub mod backup;
pub use backup::*;
```

Add this after the existing `pub mod snapshot;` line.

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-core/src/backup.rs iem-mixer/crates/iem-core/src/lib.rs
git commit -m "feat: add backup JSON schema types (iem-core)"
```

---

## Task 3: Config — Add Backup Schedule

**Files:**
- Modify: `iem-mixer/crates/iem-core/src/config.rs`

- [ ] **Step 1: Add backup config fields**

Add these fields to the `Config` struct after the `local_public_ip` field:

```rust
    /// Backup schedule times (HH:MM format, 24h), e.g. ["13:00", "21:00"]
    #[serde(default = "default_backup_schedule")]
    pub backup_schedule: Vec<String>,

    /// How many days to keep backups before pruning
    #[serde(default = "default_backup_retention_days")]
    pub backup_retention_days: u32,
```

Add the default functions after the existing default functions:

```rust
fn default_backup_schedule() -> Vec<String> {
    vec!["13:00".to_string(), "21:00".to_string()]
}

fn default_backup_retention_days() -> u32 {
    60
}
```

- [ ] **Step 2: Add test**

Add to the existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn test_backup_schedule_defaults() {
        let config = Config::default();
        assert_eq!(config.backup_schedule, vec!["13:00", "21:00"]);
        assert_eq!(config.backup_retention_days, 60);
    }

    #[test]
    fn test_backup_schedule_custom() {
        let yaml = r#"
reaper_url: "http://test:8080"
backup_schedule:
  - "09:00"
  - "13:00"
  - "18:00"
  - "22:00"
backup_retention_days: 30
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.backup_schedule.len(), 4);
        assert_eq!(config.backup_retention_days, 30);
    }
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-core/src/config.rs
git commit -m "feat: add backup_schedule and backup_retention_days to config"
```

---

## Task 4: Backup Store (file persistence)

**Files:**
- Create: `iem-mixer/crates/iem-server/src/backup_store.rs`
- Modify: `iem-mixer/crates/iem-server/src/lib.rs`

- [ ] **Step 1: Create backup store**

Create `iem-mixer/crates/iem-server/src/backup_store.rs`:

```rust
//! Backup file storage — save, list, load, prune timestamped JSON backups.

use crate::atomic_write;
use iem_core::backup::{BackupInfo, MixerBackup, BACKUP_VERSION};
use std::path::PathBuf;

pub struct BackupStore {
    backups_dir: PathBuf,
}

impl BackupStore {
    pub fn new(config_dir: &std::path::Path) -> Self {
        let backups_dir = config_dir.join("backups");
        Self { backups_dir }
    }

    /// Save a backup to disk. Returns the filename.
    pub fn save(&self, backup: &MixerBackup) -> Result<String, std::io::Error> {
        std::fs::create_dir_all(&self.backups_dir)?;

        // Parse timestamp to create filename
        let filename = backup
            .timestamp
            .replace([':', '-', '+'], "")
            .replace('T', "_")
            .chars()
            .take(15) // "YYYYMMDD_HHMMSS"
            .collect::<String>()
            + ".json";

        let path = self.backups_dir.join(&filename);
        let json = serde_json::to_string_pretty(backup).map_err(std::io::Error::other)?;
        atomic_write(&path, &json)?;

        tracing::info!(filename, sends = backup.sends.len(), "Backup saved");
        Ok(filename)
    }

    /// List all available backups (newest first)
    pub fn list(&self) -> Vec<BackupInfo> {
        let mut infos = Vec::new();

        let entries = match std::fs::read_dir(&self.backups_dir) {
            Ok(e) => e,
            Err(_) => return infos,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let filename = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            let metadata = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            // Read just the header fields without loading full backup
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let backup: MixerBackup = match serde_json::from_str(&content) {
                Ok(b) => b,
                Err(_) => continue,
            };

            infos.push(BackupInfo {
                filename,
                timestamp: backup.timestamp,
                size_bytes: metadata.len(),
                send_count: backup.sends.len(),
                track_count: backup.track_layout.len(),
            });
        }

        // Sort newest first
        infos.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        infos
    }

    /// Load a specific backup by filename
    pub fn load(&self, filename: &str) -> Result<MixerBackup, String> {
        // Path traversal protection
        if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
            return Err("Invalid filename".to_string());
        }

        let path = self.backups_dir.join(filename);
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read backup: {e}"))?;
        let backup: MixerBackup = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse backup: {e}"))?;

        if backup.version > BACKUP_VERSION {
            return Err(format!(
                "Backup version {} is newer than supported version {}",
                backup.version, BACKUP_VERSION
            ));
        }

        Ok(backup)
    }

    /// Prune backups older than retention_days
    pub fn prune(&self, retention_days: u32) -> usize {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
        let cutoff_str = cutoff.format("%Y%m%d_%H%M%S").to_string();
        let mut pruned = 0;

        let entries = match std::fs::read_dir(&self.backups_dir) {
            Ok(e) => e,
            Err(_) => return 0,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let filename = match path.file_stem().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            // Filename format: "YYYYMMDD_HHMMSS"
            if filename < cutoff_str {
                if std::fs::remove_file(&path).is_ok() {
                    tracing::info!(filename, "Pruned old backup");
                    pruned += 1;
                }
            }
        }

        pruned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_backup() -> MixerBackup {
        MixerBackup {
            version: BACKUP_VERSION,
            timestamp: "2026-04-05T13:00:00+02:00".to_string(),
            track_layout: HashMap::from([(1, "TEST mic".to_string())]),
            sends: vec![iem_core::backup::SendBackup {
                src_name: "TEST mic".to_string(),
                dest_name: "TEST inear".to_string(),
                vol: 1.0,
                pan: 0.0,
                mute: false,
            }],
            track_volumes: HashMap::from([("TEST inear".to_string(), 1.0)]),
            eq: HashMap::new(),
            limiter: HashMap::new(),
            customizations: HashMap::new(),
            pins: HashMap::new(),
        }
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let store = BackupStore::new(dir.path());
        let backup = test_backup();

        let filename = store.save(&backup).unwrap();
        assert!(filename.ends_with(".json"));

        let loaded = store.load(&filename).unwrap();
        assert_eq!(loaded.sends.len(), 1);
        assert_eq!(loaded.sends[0].src_name, "TEST mic");
    }

    #[test]
    fn test_list_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let store = BackupStore::new(dir.path());

        let mut b1 = test_backup();
        b1.timestamp = "2026-04-05T13:00:00+02:00".to_string();
        store.save(&b1).unwrap();

        let mut b2 = test_backup();
        b2.timestamp = "2026-04-05T21:00:00+02:00".to_string();
        store.save(&b2).unwrap();

        let list = store.list();
        assert_eq!(list.len(), 2);
        // Newest first
        assert!(list[0].timestamp > list[1].timestamp);
    }

    #[test]
    fn test_path_traversal_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let store = BackupStore::new(dir.path());
        assert!(store.load("../etc/passwd").is_err());
        assert!(store.load("..\\windows\\system32").is_err());
    }
}
```

- [ ] **Step 2: Add chrono and tempfile dependencies**

Check if `chrono` is already a dependency. If not, add to `iem-mixer/crates/iem-server/Cargo.toml`:

```toml
chrono = { version = "0.4", features = ["serde"] }
tempfile = { version = "3", optional = true }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Register module and add to AppState**

In `iem-mixer/crates/iem-server/src/lib.rs`, add the module declaration near the other `pub mod` lines:

```rust
pub mod backup_store;
```

Add field to `AppState` struct:

```rust
    pub backup_store: Arc<backup_store::BackupStore>,
```

Add initialization in `AppState::new()` after the `photo_store` line:

```rust
            backup_store: Arc::new(backup_store::BackupStore::new(config_dir)),
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-server/src/backup_store.rs \
  iem-mixer/crates/iem-server/src/lib.rs \
  iem-mixer/crates/iem-server/Cargo.toml
git commit -m "feat: add backup store for JSON file persistence"
```

---

## Task 5: State Capture Logic

**Files:**
- Create: `iem-mixer/crates/iem-server/src/backup_capture.rs`

This module reads all mixer state from REAPER + local files and produces a `MixerBackup`.

- [ ] **Step 1: Create capture module**

Create `iem-mixer/crates/iem-server/src/backup_capture.rs`:

```rust
//! Captures complete mixer state from REAPER + local files into a MixerBackup.

use crate::AppState;
use iem_core::backup::*;
use std::collections::HashMap;

/// Capture a complete mixer state snapshot.
///
/// Reads from:
/// 1. REAPER HTTP API — tracks, sends, volumes
/// 2. REAPER EXTSTATE — EQ bands, limiter params (via ReaScript)
/// 3. Local JSON files — customizations, PINs
pub async fn capture_mixer_state(state: &AppState) -> Result<MixerBackup, String> {
    let config = state.config.read().await;
    let reaper_url = config.reaper_url.clone();
    drop(config);

    // 1. Query all tracks
    let tracks = query_tracks(&state.http_client, &reaper_url).await?;

    let track_layout: HashMap<u32, String> = tracks
        .iter()
        .map(|t| (t.index, t.name.clone()))
        .collect();

    // 2. Query all sends
    let sends = query_all_sends(&state.http_client, &reaper_url, &tracks).await?;

    // 3. Collect track volumes (only inear + stems tracks)
    let track_volumes: HashMap<String, f64> = tracks
        .iter()
        .filter(|t| t.name.contains("inear") || t.name.contains("stems"))
        .map(|t| (t.name.clone(), t.volume))
        .collect();

    // 4. Read EQ for all tracks that have ReaEQ
    let eq = query_all_eq(state, &reaper_url, &tracks).await;

    // 5. Read limiter for all inear tracks
    let limiter = query_all_limiter(state, &reaper_url, &tracks).await;

    // 6. Read customizations from local files
    let customizations = read_customizations(state);

    // 7. Read PINs
    let pins = read_pins(state);

    let timestamp = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%:z").to_string();

    Ok(MixerBackup {
        version: BACKUP_VERSION,
        timestamp,
        track_layout,
        sends,
        track_volumes,
        eq,
        limiter,
        customizations,
        pins,
    })
}

/// Track info parsed from REAPER response
struct TrackInfo {
    index: u32,
    name: String,
    volume: f64,
    send_count: u32,
}

/// Query all tracks from REAPER
async fn query_tracks(
    client: &reqwest::Client,
    reaper_url: &str,
) -> Result<Vec<TrackInfo>, String> {
    let url = format!("{}/_/NTRACK;TRACK", reaper_url);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("REAPER unreachable: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("Read error: {e}"))?;

    let mut tracks = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() != Some(&"TRACK") || parts.len() < 12 {
            continue;
        }
        let index: u32 = parts[1].parse().unwrap_or(0);
        if index == 0 {
            continue; // Skip master track
        }
        let name = parts[2].to_string();
        let volume: f64 = parts[4].parse().unwrap_or(1.0);
        let send_count: u32 = parts[10].parse().unwrap_or(0);

        tracks.push(TrackInfo {
            index,
            name,
            volume,
            send_count,
        });
    }

    Ok(tracks)
}

/// Query all sends from all tracks that have sends
async fn query_all_sends(
    client: &reqwest::Client,
    reaper_url: &str,
    tracks: &[TrackInfo],
) -> Result<Vec<SendBackup>, String> {
    let mut sends = Vec::new();

    // Build name lookup
    let name_by_index: HashMap<u32, &str> = tracks.iter().map(|t| (t.index, t.name.as_str())).collect();

    for track in tracks {
        if track.send_count == 0 {
            continue;
        }

        for s in 0..track.send_count {
            let url = format!("{}/_/GET/TRACK/{}/SEND/{}", reaper_url, track.index, s);
            let resp = match client.get(&url).send().await {
                Ok(r) => r,
                Err(_) => continue,
            };
            let text = match resp.text().await {
                Ok(t) => t,
                Err(_) => continue,
            };

            // SEND\tsrc\tsend_idx\tmute\tvol\tpan\tdest
            let parts: Vec<&str> = text.trim().split('\t').collect();
            if parts.len() < 7 || parts[0] != "SEND" {
                continue;
            }

            let dest_index: i32 = parts[6].parse().unwrap_or(-1);
            if dest_index < 1 {
                continue; // Skip hardware outputs
            }

            let dest_name = match name_by_index.get(&(dest_index as u32)) {
                Some(n) => n.to_string(),
                None => continue,
            };

            let mute_flag: u32 = parts[3].parse().unwrap_or(0);

            sends.push(SendBackup {
                src_name: track.name.clone(),
                dest_name,
                vol: parts[4].parse().unwrap_or(1.0),
                pan: parts[5].parse().unwrap_or(0.0),
                mute: mute_flag != 0,
            });
        }
    }

    Ok(sends)
}

/// Read EQ params for all tracks via EXTSTATE
async fn query_all_eq(
    state: &AppState,
    reaper_url: &str,
    tracks: &[TrackInfo],
) -> HashMap<String, Vec<EqBandBackup>> {
    use crate::proxy::{handle_get_eq_params, reaper_api};

    let mut eq_map = HashMap::new();

    for track in tracks {
        // Use the existing handle_get_eq_params which handles locking
        let msg = handle_get_eq_params(state, track.index as usize).await;

        if let Some(iem_core::ServerMsg::EqParams { bands, .. }) = msg {
            if !bands.is_empty() {
                let backup_bands: Vec<EqBandBackup> = bands
                    .iter()
                    .enumerate()
                    .map(|(i, b)| EqBandBackup::from((i as u8, b)))
                    .collect();
                eq_map.insert(track.name.clone(), backup_bands);
            }
        }
    }

    eq_map
}

/// Read limiter params for inear tracks via EXTSTATE
async fn query_all_limiter(
    state: &AppState,
    reaper_url: &str,
    tracks: &[TrackInfo],
) -> HashMap<String, LimiterBackup> {
    use crate::proxy::handle_get_limiter_params;

    let mut limiter_map = HashMap::new();

    for track in tracks.iter().filter(|t| t.name.contains("inear")) {
        let msg = handle_get_limiter_params(state, track.index as usize).await;

        if let Some(iem_core::ServerMsg::LimiterParams {
            limit_db,
            limit_norm,
            enabled,
            ..
        }) = msg
        {
            limiter_map.insert(
                track.name.clone(),
                LimiterBackup {
                    limit_db,
                    limit_norm,
                    enabled,
                },
            );
        }
    }

    limiter_map
}

/// Read customizations from local JSON files
fn read_customizations(state: &AppState) -> HashMap<String, iem_core::Customization> {
    let mut map = HashMap::new();
    // Read for all known members
    let members = [
        "petronela", "stevo", "marek", "zuzka", "tina", "mirec", "alex", "patrika", "ani",
        "engineer",
    ];
    for member in &members {
        let custom = state.customization_store.load(member);
        if !custom.pinned.is_empty() || !custom.hidden.is_empty() {
            map.insert(member.to_string(), custom);
        }
    }
    map
}

/// Read PINs from pin store
fn read_pins(state: &AppState) -> HashMap<String, String> {
    // PinStore exposes pins via its internal HashMap
    // We read the raw pins.json file instead
    let pin_store = state.pin_store.blocking_read();
    pin_store.all_pins()
}
```

- [ ] **Step 2: Verify handle_get_eq_params and handle_get_limiter_params are pub**

Check that these functions in `proxy.rs` are `pub`. They should be based on earlier exploration (line 2041: `pub async fn handle_get_eq_params`). If `handle_get_limiter_params` is not pub, add `pub` to it.

Also check that `reaper_api` module in proxy.rs is accessible. If needed, add `pub` to the `mod reaper_api` block.

- [ ] **Step 3: Add PinStore::all_pins() method**

The PinStore needs a method to export all PINs. Check `iem-mixer/crates/iem-server/src/pin_store.rs` and add:

```rust
    /// Get all stored PINs (for backup)
    pub fn all_pins(&self) -> HashMap<String, String> {
        self.pins.clone()
    }
```

- [ ] **Step 4: Register module**

Add to `iem-mixer/crates/iem-server/src/lib.rs`:

```rust
pub mod backup_capture;
```

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/crates/iem-server/src/backup_capture.rs \
  iem-mixer/crates/iem-server/src/lib.rs \
  iem-mixer/crates/iem-server/src/pin_store.rs \
  iem-mixer/crates/iem-server/src/proxy.rs
git commit -m "feat: add backup capture — reads all mixer state from REAPER + local files"
```

---

## Task 6: Restore Logic (preview + apply)

**Files:**
- Create: `iem-mixer/crates/iem-server/src/backup_restore.rs`

- [ ] **Step 1: Create restore module**

Create `iem-mixer/crates/iem-server/src/backup_restore.rs`:

```rust
//! Restore mixer state from a backup — preview diff and apply.

use crate::AppState;
use iem_core::backup::*;
use std::collections::HashMap;

/// Build a preview of what would change if this backup were restored.
pub async fn preview_restore(
    state: &AppState,
    backup: &MixerBackup,
) -> Result<RestorePreview, String> {
    let config = state.config.read().await;
    let reaper_url = config.reaper_url.clone();
    drop(config);

    // Build current track name → index map
    let current_tracks = query_track_map(&state.http_client, &reaper_url).await?;
    // Build current send routing: src_index → {dest_index: send_idx}
    let send_routing = query_send_routing(&state.http_client, &reaper_url, &current_tracks).await?;

    let mut changes = Vec::new();
    let mut unchanged_count = 0;
    let mut skipped = Vec::new();

    // --- Compare sends ---
    for send in &backup.sends {
        let src_idx = match current_tracks.get(&send.src_name) {
            Some(&idx) => idx,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} -> {}", send.src_name, send.dest_name),
                    reason: format!("Source track '{}' not found", send.src_name),
                });
                continue;
            }
        };
        let dest_idx = match current_tracks.get(&send.dest_name) {
            Some(&idx) => idx,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} -> {}", send.src_name, send.dest_name),
                    reason: format!("Dest track '{}' not found", send.dest_name),
                });
                continue;
            }
        };

        let send_idx = match send_routing.get(&src_idx).and_then(|m| m.get(&dest_idx)) {
            Some(&idx) => idx,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} -> {}", send.src_name, send.dest_name),
                    reason: "Send route not found".to_string(),
                });
                continue;
            }
        };

        // Read current send value
        let current = query_send_value(&state.http_client, &reaper_url, src_idx, send_idx).await;
        if let Some((cur_vol, cur_pan, cur_mute)) = current {
            let vol_diff = (cur_vol - send.vol).abs() > 0.0001;
            let pan_diff = (cur_pan - send.pan).abs() > 0.001;
            let mute_diff = cur_mute != send.mute;

            if vol_diff || pan_diff || mute_diff {
                changes.push(RestoreChange {
                    category: RestoreCategory::Send,
                    description: format!("{} -> {}", send.src_name, send.dest_name),
                    current_value: format!("vol={:.4} pan={:.2} mute={}", cur_vol, cur_pan, cur_mute),
                    backup_value: format!("vol={:.4} pan={:.2} mute={}", send.vol, send.pan, send.mute),
                });
            } else {
                unchanged_count += 1;
            }
        }
    }

    // --- Compare track volumes ---
    for (name, &backup_vol) in &backup.track_volumes {
        let idx = match current_tracks.get(name) {
            Some(&idx) => idx,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::TrackVolume,
                    description: name.clone(),
                    reason: format!("Track '{}' not found", name),
                });
                continue;
            }
        };

        let current_vol = query_track_volume(&state.http_client, &reaper_url, idx).await;
        if let Some(cur) = current_vol {
            if (cur - backup_vol).abs() > 0.0001 {
                changes.push(RestoreChange {
                    category: RestoreCategory::TrackVolume,
                    description: name.clone(),
                    current_value: format!("{:.6}", cur),
                    backup_value: format!("{:.6}", backup_vol),
                });
            } else {
                unchanged_count += 1;
            }
        }
    }

    // EQ and limiter comparison would follow the same pattern
    // Count EQ bands that differ
    // (Simplified: mark all EQ as "will restore" since comparing requires ReaScript reads)
    for (track_name, bands) in &backup.eq {
        if current_tracks.contains_key(track_name) {
            let has_enabled = bands.iter().any(|b| b.enabled);
            if has_enabled {
                changes.push(RestoreChange {
                    category: RestoreCategory::Eq,
                    description: format!("{} ({} bands)", track_name, bands.len()),
                    current_value: "current".to_string(),
                    backup_value: format!("{} enabled", bands.iter().filter(|b| b.enabled).count()),
                });
            } else {
                unchanged_count += 1;
            }
        } else {
            skipped.push(SkippedEntry {
                category: RestoreCategory::Eq,
                description: track_name.clone(),
                reason: format!("Track '{}' not found", track_name),
            });
        }
    }

    // Customization comparison
    for (member, backup_custom) in &backup.customizations {
        let current = state.customization_store.load(member);
        if current.pinned != backup_custom.pinned || current.hidden != backup_custom.hidden {
            changes.push(RestoreChange {
                category: RestoreCategory::Customization,
                description: member.clone(),
                current_value: format!("pinned={:?} hidden={:?}", current.pinned, current.hidden),
                backup_value: format!("pinned={:?} hidden={:?}", backup_custom.pinned, backup_custom.hidden),
            });
        } else {
            unchanged_count += 1;
        }
    }

    Ok(RestorePreview {
        changes,
        unchanged_count,
        skipped,
    })
}

/// Apply a backup restore to live REAPER.
pub async fn apply_restore(
    state: &AppState,
    backup: &MixerBackup,
) -> Result<RestoreResult, String> {
    let config = state.config.read().await;
    let reaper_url = config.reaper_url.clone();
    drop(config);

    let current_tracks = query_track_map(&state.http_client, &reaper_url).await?;
    let send_routing = query_send_routing(&state.http_client, &reaper_url, &current_tracks).await?;

    let mut restored_count = 0;
    let mut skipped = Vec::new();

    // 1. Restore sends
    for send in &backup.sends {
        let src_idx = match current_tracks.get(&send.src_name) {
            Some(&idx) => idx,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} -> {}", send.src_name, send.dest_name),
                    reason: format!("Source '{}' not found", send.src_name),
                });
                continue;
            }
        };
        let dest_idx = match current_tracks.get(&send.dest_name) {
            Some(&idx) => idx,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} -> {}", send.src_name, send.dest_name),
                    reason: format!("Dest '{}' not found", send.dest_name),
                });
                continue;
            }
        };
        let send_idx = match send_routing.get(&src_idx).and_then(|m| m.get(&dest_idx)) {
            Some(&idx) => idx,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} -> {}", send.src_name, send.dest_name),
                    reason: "Send route not found".to_string(),
                });
                continue;
            }
        };

        // Apply vol, pan, mute
        let mute_val = if send.mute { 1 } else { 0 };
        let _ = state.http_client
            .get(format!("{}/_/SET/TRACK/{}/SEND/{}/VOL/{:.10}", reaper_url, src_idx, send_idx, send.vol))
            .send().await;
        let _ = state.http_client
            .get(format!("{}/_/SET/TRACK/{}/SEND/{}/PAN/{:.10}", reaper_url, src_idx, send_idx, send.pan))
            .send().await;
        let _ = state.http_client
            .get(format!("{}/_/SET/TRACK/{}/SEND/{}/MUTE/{}", reaper_url, src_idx, send_idx, mute_val))
            .send().await;
        restored_count += 1;
    }

    // 2. Restore track volumes
    for (name, &vol) in &backup.track_volumes {
        let idx = match current_tracks.get(name) {
            Some(&idx) => idx,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::TrackVolume,
                    description: name.clone(),
                    reason: format!("Track '{}' not found", name),
                });
                continue;
            }
        };
        let _ = state.http_client
            .get(format!("{}/_/SET/TRACK/{}/VOL/{:.10}", reaper_url, idx, vol))
            .send().await;
        restored_count += 1;
    }

    // 3. Restore EQ bands
    for (track_name, bands) in &backup.eq {
        let idx = match current_tracks.get(track_name) {
            Some(&idx) => *idx,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Eq,
                    description: track_name.clone(),
                    reason: format!("Track '{}' not found", track_name),
                });
                continue;
            }
        };

        for band in bands {
            for (param, value) in [
                ("fn", band.freq_norm),
                ("gn", band.gain_norm),
                ("bn", band.bw_norm),
                ("en", if band.enabled { 1.0 } else { 0.0 }),
            ] {
                let eq_val = format!("track={}|band={}|param={}|value={:.6}", idx, band.band, param, value);
                let set_url = crate::proxy::reaper_api::set_extstate(&reaper_url, "reaperiem", "eq_set", &eq_val);
                let _ = state.http_client.get(&set_url).send().await;
                let action_url = crate::proxy::reaper_api::trigger_action(&reaper_url, "_RS_REAPERIEM_SET_EQ");
                let _ = state.http_client.get(&action_url).send().await;
                tokio::time::sleep(std::time::Duration::from_millis(60)).await;
            }
            restored_count += 1;
        }
    }

    // 4. Restore limiter
    for (track_name, lim) in &backup.limiter {
        let idx = match current_tracks.get(track_name) {
            Some(&idx) => *idx,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Limiter,
                    description: track_name.clone(),
                    reason: format!("Track '{}' not found", track_name),
                });
                continue;
            }
        };

        for (param, value) in [("limit", lim.limit_norm), ("enabled", if lim.enabled { 1.0 } else { 0.0 })] {
            let val = format!("track={}|param={}|value={:.6}", idx, param, value);
            let set_url = crate::proxy::reaper_api::set_extstate(&reaper_url, "reaperiem", "limiter_set", &val);
            let _ = state.http_client.get(&set_url).send().await;
            let action_url = crate::proxy::reaper_api::trigger_action(&reaper_url, "_RS_REAPERIEM_SET_LIMITER");
            let _ = state.http_client.get(&action_url).send().await;
            tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        }
        restored_count += 1;
    }

    // 5. Restore customizations
    for (member, custom) in &backup.customizations {
        let _ = state.customization_store.save(member, custom);
        restored_count += 1;
    }

    // 6. Restore PINs
    for (member, pin) in &backup.pins {
        let mut pin_store = state.pin_store.write().await;
        pin_store.set_pin(member, pin);
    }

    // 7. Save REAPER project
    let project_saved = state.http_client
        .get(format!("{}/_/40026", reaper_url))
        .send()
        .await
        .is_ok();

    Ok(RestoreResult {
        restored_count,
        skipped,
        project_saved,
    })
}

// --- Helper functions ---

/// Build name → index map from live REAPER
async fn query_track_map(
    client: &reqwest::Client,
    reaper_url: &str,
) -> Result<HashMap<String, u32>, String> {
    let url = format!("{}/_/NTRACK;TRACK", reaper_url);
    let resp = client.get(&url).send().await.map_err(|e| format!("REAPER unreachable: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("Read error: {e}"))?;

    let mut map = HashMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() != Some(&"TRACK") || parts.len() < 3 {
            continue;
        }
        let idx: u32 = parts[1].parse().unwrap_or(0);
        if idx == 0 {
            continue;
        }
        map.insert(parts[2].to_string(), idx);
    }
    Ok(map)
}

/// Build send routing map: src_idx → {dest_idx: send_idx}
async fn query_send_routing(
    client: &reqwest::Client,
    reaper_url: &str,
    tracks: &HashMap<String, u32>,
) -> Result<HashMap<u32, HashMap<u32, u32>>, String> {
    let mut routing = HashMap::new();

    for &src_idx in tracks.values() {
        let mut dest_map = HashMap::new();
        for s in 0..30 {
            let url = format!("{}/_/GET/TRACK/{}/SEND/{}", reaper_url, src_idx, s);
            let resp = match client.get(&url).send().await {
                Ok(r) => r,
                Err(_) => break,
            };
            let text = match resp.text().await {
                Ok(t) => t,
                Err(_) => break,
            };
            let parts: Vec<&str> = text.trim().split('\t').collect();
            if parts.len() < 7 || parts[0] != "SEND" {
                break;
            }
            let dest: i32 = parts[6].parse().unwrap_or(-1);
            if dest > 0 {
                dest_map.insert(dest as u32, s as u32);
            }
        }
        if !dest_map.is_empty() {
            routing.insert(src_idx, dest_map);
        }
    }

    Ok(routing)
}

/// Query current send values
async fn query_send_value(
    client: &reqwest::Client,
    reaper_url: &str,
    src_idx: u32,
    send_idx: u32,
) -> Option<(f64, f64, bool)> {
    let url = format!("{}/_/GET/TRACK/{}/SEND/{}", reaper_url, src_idx, send_idx);
    let resp = client.get(&url).send().await.ok()?;
    let text = resp.text().await.ok()?;
    let parts: Vec<&str> = text.trim().split('\t').collect();
    if parts.len() < 6 {
        return None;
    }
    let mute: u32 = parts[3].parse().unwrap_or(0);
    let vol: f64 = parts[4].parse().unwrap_or(1.0);
    let pan: f64 = parts[5].parse().unwrap_or(0.0);
    Some((vol, pan, mute != 0))
}

/// Query current track volume
async fn query_track_volume(
    client: &reqwest::Client,
    reaper_url: &str,
    track_idx: u32,
) -> Option<f64> {
    let url = format!("{}/_/TRACK/{}", reaper_url, track_idx);
    let resp = client.get(&url).send().await.ok()?;
    let text = resp.text().await.ok()?;
    let parts: Vec<&str> = text.trim().split('\t').collect();
    if parts.len() < 5 {
        return None;
    }
    parts[4].parse().ok()
}
```

- [ ] **Step 2: Register module**

Add to `iem-mixer/crates/iem-server/src/lib.rs`:

```rust
pub mod backup_restore;
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-server/src/backup_restore.rs \
  iem-mixer/crates/iem-server/src/lib.rs
git commit -m "feat: add backup restore logic — preview diff and apply"
```

---

## Task 7: REST API Endpoints

**Files:**
- Create: `iem-mixer/crates/iem-server/src/backup_routes.rs`
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs`

- [ ] **Step 1: Create backup routes**

Create `iem-mixer/crates/iem-server/src/backup_routes.rs`:

```rust
//! REST API routes for backup management (engineer-only)

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use iem_core::{ApiError, backup::*};

use crate::AppState;

pub fn backup_routes() -> Router<AppState> {
    Router::new()
        .route("/api/backups", get(list_backups))
        .route("/api/backups/{filename}", get(get_backup))
        .route("/api/backups/{filename}/preview", post(preview_backup))
        .route("/api/backups/{filename}/restore", post(restore_backup))
        .route("/api/backups/capture", post(trigger_capture))
}

/// List all available backups
async fn list_backups(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<BackupInfo>>, (StatusCode, Json<ApiError>)> {
    verify_engineer(&state, &headers).await?;
    Ok(Json(state.backup_store.list()))
}

/// Get full backup data
async fn get_backup(
    State(state): State<AppState>,
    Path(filename): Path<String>,
    headers: HeaderMap,
) -> Result<Json<MixerBackup>, (StatusCode, Json<ApiError>)> {
    verify_engineer(&state, &headers).await?;
    let backup = state.backup_store.load(&filename).map_err(|e| {
        (StatusCode::NOT_FOUND, Json(ApiError::new("NOT_FOUND", &e)))
    })?;
    Ok(Json(backup))
}

/// Preview what a restore would change
async fn preview_backup(
    State(state): State<AppState>,
    Path(filename): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RestorePreview>, (StatusCode, Json<ApiError>)> {
    verify_engineer(&state, &headers).await?;
    let backup = state.backup_store.load(&filename).map_err(|e| {
        (StatusCode::NOT_FOUND, Json(ApiError::new("NOT_FOUND", &e)))
    })?;
    let preview = crate::backup_restore::preview_restore(&state, &backup).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("REAPER_ERROR", &e)))
    })?;
    Ok(Json(preview))
}

/// Apply a backup restore
async fn restore_backup(
    State(state): State<AppState>,
    Path(filename): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RestoreResult>, (StatusCode, Json<ApiError>)> {
    verify_engineer(&state, &headers).await?;
    let backup = state.backup_store.load(&filename).map_err(|e| {
        (StatusCode::NOT_FOUND, Json(ApiError::new("NOT_FOUND", &e)))
    })?;
    let result = crate::backup_restore::apply_restore(&state, &backup).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("REAPER_ERROR", &e)))
    })?;
    Ok(Json(result))
}

/// Manually trigger a backup capture (for testing)
async fn trigger_capture(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<BackupInfo>, (StatusCode, Json<ApiError>)> {
    verify_engineer(&state, &headers).await?;
    let backup = crate::backup_capture::capture_mixer_state(&state).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("CAPTURE_ERROR", &e)))
    })?;
    let filename = state.backup_store.save(&backup).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::new("SAVE_ERROR", &e.to_string())))
    })?;
    let info = BackupInfo {
        filename,
        timestamp: backup.timestamp,
        size_bytes: 0, // Approximate
        send_count: backup.sends.len(),
        track_count: backup.track_layout.len(),
    };
    Ok(Json(info))
}

/// Verify the caller is an engineer
async fn verify_engineer(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    let claims = crate::auth::verify_member_access(headers, "engineer", &config.jwt_secret)?;
    if !claims.engineer {
        return Err((StatusCode::FORBIDDEN, Json(ApiError::forbidden())));
    }
    Ok(())
}
```

- [ ] **Step 2: Mount routes in proxy.rs**

Find where other routes are merged in `proxy.rs` (look for `.merge(preset_routes())` or similar) and add:

```rust
        .merge(crate::backup_routes::backup_routes())
```

- [ ] **Step 3: Register module**

Add to `iem-mixer/crates/iem-server/src/lib.rs`:

```rust
pub mod backup_routes;
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-server/src/backup_routes.rs \
  iem-mixer/crates/iem-server/src/proxy.rs \
  iem-mixer/crates/iem-server/src/lib.rs
git commit -m "feat: add backup REST API endpoints (engineer-only)"
```

---

## Task 8: Backup Daemon (scheduled captures)

**Files:**
- Create: `iem-mixer/crates/iem-server/src/backup_daemon.rs`
- Modify: `iem-mixer/crates/iem-server/src/lib.rs`

- [ ] **Step 1: Create daemon**

Create `iem-mixer/crates/iem-server/src/backup_daemon.rs`:

```rust
//! Scheduled backup daemon — runs captures at configured times.

use crate::AppState;
use chrono::Local;
use std::collections::HashSet;

/// Spawn the backup daemon as a background tokio task.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        tracing::info!("Backup daemon started");
        let mut last_triggered: HashSet<String> = HashSet::new();

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;

            let now = Local::now();
            let current_time = now.format("%H:%M").to_string();
            let current_date = now.format("%Y-%m-%d").to_string();
            let trigger_key = format!("{} {}", current_date, current_time);

            let config = state.config.read().await;
            let schedule = config.backup_schedule.clone();
            let retention_days = config.backup_retention_days;
            drop(config);

            if schedule.contains(&current_time) && !last_triggered.contains(&trigger_key) {
                last_triggered.insert(trigger_key);
                tracing::info!(time = current_time, "Backup daemon: scheduled capture");

                match crate::backup_capture::capture_mixer_state(&state).await {
                    Ok(backup) => {
                        let send_count = backup.sends.len();
                        match state.backup_store.save(&backup) {
                            Ok(filename) => {
                                tracing::info!(filename, send_count, "Backup saved successfully");
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "Failed to save backup");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = e, "Failed to capture mixer state");
                    }
                }

                // Prune old backups
                let pruned = state.backup_store.prune(retention_days);
                if pruned > 0 {
                    tracing::info!(pruned, "Pruned old backups");
                }
            }

            // Clean up old trigger keys (keep only today's)
            last_triggered.retain(|k| k.starts_with(&current_date));
        }
    });
}
```

- [ ] **Step 2: Spawn daemon at app startup**

In the main server startup code (find where `poller` is spawned, likely in `proxy.rs` or `main.rs`), add after the poller spawn:

```rust
crate::backup_daemon::spawn(state.clone());
```

- [ ] **Step 3: Register module**

Add to `iem-mixer/crates/iem-server/src/lib.rs`:

```rust
pub mod backup_daemon;
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-server/src/backup_daemon.rs \
  iem-mixer/crates/iem-server/src/lib.rs \
  iem-mixer/crates/iem-server/src/proxy.rs
git commit -m "feat: add backup daemon — scheduled captures at configured times"
```

---

## Task 9: UI — Backup Section in Settings Modal

**Files:**
- Create: `iem-mixer/iem-ui/src/components/backup_section.rs`
- Modify: `iem-mixer/iem-ui/src/components/mod.rs`
- Modify: `iem-mixer/iem-ui/src/components/settings_modal.rs`
- Modify: `iem-mixer/iem-ui/src/api.rs`

- [ ] **Step 1: Add API functions**

Add to `iem-mixer/iem-ui/src/api.rs`:

```rust
/// List available backups (engineer-only)
pub async fn list_backups(token: &str) -> Result<Vec<iem_core::backup::BackupInfo>, String> {
    let resp = Request::get("/api/backups")
        .header("Authorization", &format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("{e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json().await.map_err(|e| format!("{e}"))
}

/// Preview a backup restore
pub async fn preview_restore(token: &str, filename: &str) -> Result<iem_core::backup::RestorePreview, String> {
    let resp = Request::post(&format!("/api/backups/{}/preview", filename))
        .header("Authorization", &format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("{e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json().await.map_err(|e| format!("{e}"))
}

/// Apply a backup restore
pub async fn apply_restore(token: &str, filename: &str) -> Result<iem_core::backup::RestoreResult, String> {
    let resp = Request::post(&format!("/api/backups/{}/restore", filename))
        .header("Authorization", &format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("{e}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json().await.map_err(|e| format!("{e}"))
}
```

- [ ] **Step 2: Create backup section component**

Create `iem-mixer/iem-ui/src/components/backup_section.rs`:

```rust
//! Backup & Restore section for the Settings modal (engineer-only)

use leptos::prelude::*;
use iem_core::backup::{BackupInfo, RestorePreview, RestoreResult, RestoreCategory};

#[component]
pub fn BackupSection(token: String) -> impl IntoView {
    let (backups, set_backups) = signal(Vec::<BackupInfo>::new());
    let (selected, set_selected) = signal(Option::<String>::None);
    let (preview, set_preview) = signal(Option::<RestorePreview>::None);
    let (restoring, set_restoring) = signal(false);
    let (result, set_result) = signal(Option::<RestoreResult>::None);
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    let token_clone = token.clone();

    // Load backups on mount
    {
        let token = token.clone();
        spawn_local(async move {
            match crate::api::list_backups(&token).await {
                Ok(list) => set_backups.set(list),
                Err(e) => set_error.set(Some(e)),
            }
        });
    }

    let on_select = move |filename: String| {
        set_selected.set(Some(filename.clone()));
        set_preview.set(None);
        set_result.set(None);
        set_error.set(None);
        set_loading.set(true);
        let token = token_clone.clone();
        spawn_local(async move {
            match crate::api::preview_restore(&token, &filename).await {
                Ok(p) => set_preview.set(Some(p)),
                Err(e) => set_error.set(Some(e)),
            }
            set_loading.set(false);
        });
    };

    let token_for_restore = token_clone.clone();
    let on_restore = move |_| {
        if let Some(filename) = selected.get_untracked() {
            set_restoring.set(true);
            set_error.set(None);
            let token = token_for_restore.clone();
            spawn_local(async move {
                match crate::api::apply_restore(&token, &filename).await {
                    Ok(r) => set_result.set(Some(r)),
                    Err(e) => set_error.set(Some(e)),
                }
                set_restoring.set(false);
            });
        }
    };

    view! {
        <div class="settings-section">
            <h3>"Backups"</h3>

            // Error display
            {move || error.get().map(|e| view! {
                <div class="backup-error">{e}</div>
            })}

            // Result display
            {move || result.get().map(|r| view! {
                <div class="backup-result">
                    <strong>"Restore complete: "</strong>
                    {format!("{} values restored", r.restored_count)}
                    {if !r.skipped.is_empty() {
                        format!(", {} skipped", r.skipped.len())
                    } else {
                        String::new()
                    }}
                    {if r.project_saved { " (project saved)" } else { " (project NOT saved!)" }}
                </div>
            })}

            // Backup list
            <div class="backup-list">
                {move || backups.get().into_iter().map(|b| {
                    let filename = b.filename.clone();
                    let is_selected = selected.get().as_deref() == Some(&filename);
                    let filename_click = filename.clone();
                    let on_select = on_select.clone();
                    view! {
                        <div
                            class="backup-item"
                            class:selected=is_selected
                            on:click=move |_| on_select(filename_click.clone())
                        >
                            <span class="backup-time">{&b.timestamp[..16]}</span>
                            <span class="backup-meta">
                                {format!("{} sends, {} tracks", b.send_count, b.track_count)}
                            </span>
                        </div>
                    }
                }).collect_view()}
            </div>

            // Preview
            {move || {
                if loading.get() {
                    return Some(view! { <div class="backup-loading">"Loading preview..."</div> }.into_any());
                }
                preview.get().map(|p| {
                    let change_count = p.changes.len();
                    let send_changes = p.changes.iter().filter(|c| c.category == RestoreCategory::Send).count();
                    let vol_changes = p.changes.iter().filter(|c| c.category == RestoreCategory::TrackVolume).count();
                    let eq_changes = p.changes.iter().filter(|c| c.category == RestoreCategory::Eq).count();

                    view! {
                        <div class="backup-preview">
                            <div class="preview-summary">
                                <span class="preview-changes">
                                    {format!("Will restore: {} sends, {} volumes, {} EQ", send_changes, vol_changes, eq_changes)}
                                </span>
                                <span class="preview-unchanged">
                                    {format!("Unchanged: {}", p.unchanged_count)}
                                </span>
                                {if !p.skipped.is_empty() {
                                    Some(view! {
                                        <span class="preview-skipped">
                                            {format!("Skipped: {} (not found)", p.skipped.len())}
                                        </span>
                                    })
                                } else {
                                    None
                                }}
                            </div>

                            <button
                                class="restore-btn"
                                disabled=move || restoring.get() || change_count == 0
                                on:click=on_restore.clone()
                            >
                                {move || if restoring.get() { "Restoring..." } else { "Restore" }}
                            </button>
                        </div>
                    }.into_any()
                })
            }}
        </div>
    }
}
```

- [ ] **Step 3: Register component module**

Add to `iem-mixer/iem-ui/src/components/mod.rs`:

```rust
pub mod backup_section;
```

- [ ] **Step 4: Add backup section to settings modal**

In `iem-mixer/iem-ui/src/components/settings_modal.rs`, find the engineer-only section (the `if is_engineer` block around line 230). Add after the existing Audio section but still inside the engineer guard:

```rust
                // Backup & Restore (engineer-only)
                <crate::components::backup_section::BackupSection token=auth_token.clone() />
```

Where `auth_token` is the JWT token string (extract from the auth context used in the component).

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/iem-ui/src/components/backup_section.rs \
  iem-mixer/iem-ui/src/components/mod.rs \
  iem-mixer/iem-ui/src/components/settings_modal.rs \
  iem-mixer/iem-ui/src/api.rs
git commit -m "feat: add backup restore UI in Settings modal (engineer-only)"
```

---

## Task 10: Git Backup — Include Customizations

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Find the backup step in CI**

Search for the nightly backup step in `ci.yml`. It should be in the deploy job, copying files from iem.lan and committing to git.

- [ ] **Step 2: Add customizations to backup**

Add `customizations/*.json` to the files being copied from iem.lan during the backup step. The exact change depends on the current backup script format, but add a line like:

```powershell
# Copy customizations (not currently backed up)
scp -r newlevel@iem.lan:"%APPDATA%\iem-mixer\customizations" data/customizations/ 2>$null
```

Or if using the win-iem-snv MCP:

```yaml
- name: Backup customizations
  run: |
    ssh newlevel@iem.lan "xcopy /Y /S \"%APPDATA%\iem-mixer\customizations\*\" \"C:\Users\newlevel\Documents\reaperiem\data\customizations\""
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: include customizations in git backup for disaster recovery"
```

---

## Task 11: Push + Monitor CI

- [ ] **Step 1: Run local checks**

```bash
cd iem-mixer && cargo fmt --all --check
```

Fix any formatting issues.

- [ ] **Step 2: Push and monitor**

```bash
git push origin dev
gh run list --limit 3
```

Monitor the run until ALL jobs reach terminal state. If any job fails, investigate with `gh run view <id> --log-failed` and fix all issues in ONE commit.

- [ ] **Step 3: If CI passes, verify on deployed system**

After deploy, verify:
1. Open Settings as engineer → "Backups" section visible
2. Trigger a manual capture via API: `curl -X POST http://10.77.9.231/api/backups/capture -H "Authorization: Bearer <token>"`
3. Verify backup file created
4. Click backup in UI → preview loads
5. Verify restore works (modify a send, restore, verify it reverted)

---

## Task Dependencies

```
Task 1 (version bump)       ─┐
Task 2 (core types)         ─┤
Task 3 (config)             ─┘
         │
         ▼
Task 4 (backup store)       ─┐
Task 5 (capture)            ─┤── Sequential (each builds on previous)
Task 6 (restore)            ─┤
Task 7 (REST routes)        ─┤
Task 8 (daemon)             ─┘
         │
         ▼
Task 9 (UI)                 ── Depends on routes (Task 7)
Task 10 (git backup)        ── Independent
         │
         ▼
Task 11 (push + verify)
```

Tasks 1-3 are independent and parallelizable. Tasks 4-8 are sequential. Task 9 depends on Task 7. Task 10 is independent.
