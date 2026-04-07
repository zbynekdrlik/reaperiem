# Mixer Backup & Restore System — Design Spec

## Problem

The IEM mixer runs on production REAPER alongside active CI/E2E testing. Development changes (deploys, E2E tests) can corrupt band members' mixer settings — send levels, track volumes, EQ, mutes. When this happens, the engineer needs a fast, reliable way to restore all mixer state to a known-good point (e.g., after the last rehearsal or service).

Today's incident: CI E2E tests muted 47 sends, corrupted 3 track volumes, and reset all EQ bands. Manual restore took hours of RPP parsing, send mapping, and 2680 EQ API calls. This must be a one-click operation.

Additional gap: channel customizations (hidden/pinned per member) are not backed up at all and would be lost if the server disk failed.

## Goals

1. **Zero-effort automatic backups** on a configurable schedule (default: 13:00 and 21:00 daily)
2. **Engineer-only restore UI** in the Settings modal — select a backup, preview changes, confirm
3. **Structural compatibility** — restore works even after tracks are added/removed/reordered during development
4. **Complete state capture** — sends, track volumes, EQ, limiter, customizations, PINs
5. **Git disaster recovery** — continue nightly git commits for drive-failure scenarios

## Non-Goals

- Manual/named checkpoints (engineer doesn't want extra work)
- Full RPP file restore (too destructive when project structure changed)
- Undo/redo for individual mixer operations
- Backup of REAPER project structure (tracks, FX chain layout, routing topology)

## Architecture

Two independent subsystems:

```
Backup Daemon (tokio cron)          Restore UI (engineer Settings modal)
        |                                       |
        v                                       v
  Query REAPER state               Load backup JSON from disk
  Read local files                 Compare against live REAPER
  Write JSON to backups/           Show preview (green/yellow/gray)
  Prune old backups                Engineer confirms → apply via API
        |                                       |
        v                                       v
  %APPDATA%/iem-mixer/backups/     REAPER HTTP API + EXTSTATE
  2026-04-05_130000.json           + local file writes
```

Git backup (nightly cron from iem.lan) continues unchanged as disaster recovery layer.

## Backup Format

Each backup is a single self-contained JSON file:

**Location:** `%APPDATA%/iem-mixer/backups/<YYYY-MM-DD_HHMMSS>.json`

**Schema (version 1):**

```json
{
  "version": 1,
  "timestamp": "2026-04-05T13:00:00+02:00",
  "track_layout": {
    "1": "PETRONELA mic",
    "2": "STEVO mic"
  },
  "sends": [
    {
      "src_name": "PETRONELA mic",
      "dest_name": "PETRONELA inear",
      "vol": 1.0,
      "pan": 0.0,
      "mute": false
    }
  ],
  "track_volumes": {
    "PETRONELA inear": 0.5011872,
    "STEVO inear": 1.0
  },
  "eq": {
    "MIREC mic": [
      {
        "band": 0,
        "type": "highpass",
        "freq_norm": 0.0,
        "gain_norm": 0.25,
        "bw_norm": 0.5,
        "freq_hz": 20.0,
        "gain_db": 0.0,
        "bw_oct": 2.0,
        "enabled": false
      }
    ]
  },
  "limiter": {
    "PETRONELA inear": {
      "limit_db": -6.0,
      "limit_norm": 0.0,
      "enabled": true
    }
  },
  "customizations": {
    "petronela": { "pinned": [1], "hidden": [] },
    "stevo": { "pinned": [], "hidden": [] }
  },
  "pins": {
    "petronela": "7711",
    "stevo": "7711"
  }
}
```

**Key design decision:** All sends and EQ keyed by **track name**, not index. This survives track reordering, insertion, and deletion between backup and restore.

**Sends** use `src_name + dest_name` pairs (e.g., "PETRONELA mic -> PETRONELA inear") for matching. At restore time, the app finds the current track indices for both names, then locates the send index by querying REAPER's send routing.

**EQ** is keyed by the track name where the ReaEQ plugin lives. At restore time, the app scans the track's FX chain for ReaEQ (by plugin name) regardless of FX slot position.

## Backup Schedule

Configured in `config.yaml`:

```yaml
backup_schedule:
  - "13:00"
  - "21:00"
```

The app runs a tokio task that checks the schedule. At each configured time:

1. Query `/_/NTRACK;TRACK` for all track names, volumes, indices
2. For each track with sends: query `/_/GET/TRACK/{t}/SEND/{s}` to get vol/pan/mute/dest
3. For each track with ReaEQ: read EQ via EXTSTATE `eq_read_track` + `_RS_REAPERIEM_READ_EQ`
4. For each inear track with limiter: read via EXTSTATE `limiter_read_track` + `_RS_REAPERIEM_READ_LIMITER`
5. Read customization JSON files from `%APPDATA%/iem-mixer/customizations/*.json`
6. Read PINs from `%APPDATA%/iem-mixer/pins.json`
7. Write single JSON backup file to `%APPDATA%/iem-mixer/backups/`
8. Prune backups older than 60 days (configurable via `backup_retention_days`)

**Backup must not interfere with live mixing.** All REAPER reads are non-destructive (GET only, no SET). EQ/limiter reads use the existing read locks to avoid conflicts with user operations.

## Backup Pruning

- Keep all backups from the last 60 days (configurable)
- Never delete backups while restore UI is open
- Log pruned backups for audit

## Restore Flow

### UI Location

Engineer-only section in Settings modal. Hidden when not logged in as engineer.

### List View

Shows available backups sorted newest-first:

```
Backups
─────────────────────────────
  Today 13:00         (8h ago)
  Yesterday 21:00    (24h ago)
  Yesterday 13:00    (32h ago)
  Apr 5 21:00      (2 days ago)
  Apr 5 13:00      (2 days ago)
  ...
  [older backups collapsed]
```

Each row is clickable.

### Preview View

Clicking a backup loads its JSON and compares every value against live REAPER state. Shows a summary and detail breakdown:

**Summary bar:**
```
Will restore: 47 sends, 3 volumes, 120 EQ bands, 0 limiters, 2 customizations
Unchanged: 201 sends, 7 volumes, 0 EQ, 10 limiters
Skipped (not found): 2 tracks (PETKA mic, TRANSLATOR)
```

**Color coding:**
- **Green**: value differs from current — will be restored
- **Gray**: value matches current — no change needed
- **Yellow**: structural mismatch — track/FX not found in current project, will be skipped

**Detail sections** (collapsible):
- Sends: grouped by member, showing src → dest with old/new values
- Track volumes: showing track name with old/new
- EQ: grouped by track, showing band params
- Customizations: showing member with old/new hidden/pinned lists

### Apply

Engineer clicks "Restore" button (only enabled when preview is loaded).

**Restore order:**
1. Sends (vol, pan, mute) via `SET/TRACK/{t}/SEND/{s}/VOL|PAN|MUTE`
2. Track volumes via `SET/TRACK/{t}/VOL/{v}`
3. EQ bands via EXTSTATE `eq_set` + `_RS_REAPERIEM_SET_EQ` (serialized, 60ms per param)
4. Limiter params via EXTSTATE `limiter_set` + `_RS_REAPERIEM_SET_LIMITER`
5. Customizations: write JSON files directly
6. PINs: write pins.json directly
7. Save REAPER project (action 40026)

**Progress indicator** during apply (EQ is slow — ~2680 commands for full restore).

**Final report:**
```
Restored: 47 sends, 3 volumes, 120 EQ bands, 2 customizations
Skipped: PETKA mic (not found), TRANSLATOR (not found)
Project saved.
```

### Matching Algorithm

At restore time, for each backup entry:

1. **Track matching:** Build a `name → current_index` map from live REAPER `NTRACK;TRACK` response. Backup entry's track name is looked up in this map. If not found → yellow (skipped).

2. **Send matching:** For each backup send (src_name → dest_name):
   - Look up src and dest track indices by name
   - Query src track's sends to find which send_idx routes to dest track
   - If send route not found → yellow (skipped)
   - Apply vol/pan/mute to the correct send_idx

3. **EQ matching:** For each backup EQ entry (track_name → bands):
   - Look up track index by name
   - Scan track's FX chain for ReaEQ plugin (by name, not by slot index)
   - If ReaEQ not found on that track → yellow (skipped)
   - Apply band params via EXTSTATE

4. **Limiter matching:** Same as EQ — find JS limiter by name, not slot.

### Structural Compatibility Matrix

| Development change | Restore behavior |
|---|---|
| Track reordered | No impact — matched by name |
| Track renamed | Old name not found → yellow, skipped |
| Track added after backup | New track untouched (not in backup) |
| Track removed after backup | Old track skipped → yellow warning |
| Send route added | New send untouched |
| Send route removed | Old send skipped → yellow warning |
| FX reordered | EQ/limiter found by plugin name scan |
| FX added | No impact — existing FX found by name |
| FX removed | EQ/limiter skipped → yellow warning |
| New member added | New member untouched |
| Member removed | Old member's data skipped |

## Git Disaster Recovery (unchanged)

The existing nightly backup cron on iem.lan continues:
- Saves REAPER project (action 40026)
- Copies RPP + data files
- Commits to git with timestamp

**Enhancement:** also include `customizations/*.json` files in the git backup. These are currently NOT committed and would be lost on disk failure.

## API Endpoints

New REST endpoints (engineer-auth required):

- `GET /api/backups` — list available backups (timestamp, file size)
- `GET /api/backups/:timestamp` — load a specific backup's JSON
- `POST /api/backups/:timestamp/preview` — compare backup against live state, return diff
- `POST /api/backups/:timestamp/restore` — apply the restore (returns progress via SSE or WebSocket)

## File Changes

### New files
- `iem-mixer/crates/iem-server/src/backup.rs` — backup daemon (schedule, capture, prune)
- `iem-mixer/crates/iem-server/src/backup_routes.rs` — REST endpoints for list/preview/restore
- `iem-mixer/crates/iem-core/src/backup.rs` — backup JSON schema types
- `iem-mixer/iem-ui/src/components/backup_restore.rs` — Leptos UI component

### Modified files
- `iem-mixer/crates/iem-server/src/lib.rs` — add backup daemon to app startup
- `iem-mixer/crates/iem-server/src/proxy.rs` — mount backup routes
- `iem-mixer/crates/iem-core/src/config.rs` — add `backup_schedule` and `backup_retention_days`
- `iem-mixer/iem-ui/src/pages/settings.rs` — add backup section (engineer-only)
- `.github/workflows/ci.yml` — include customizations/ in git backup step
- `config/iem-mixer-config.yaml` — add default backup schedule

## Testing

- **Unit tests:** backup JSON serialization/deserialization, name matching algorithm, schedule parsing, pruning logic
- **Integration tests:** full capture → restore round-trip with mock REAPER responses
- **E2E tests (CI runner):** backup list UI renders, preview shows correct diff counts
- **E2E tests (deploy runner):** full backup → modify sends → restore → verify sends match original
