---
name: deployment
description: IEM Mixer deployment architecture — Tauri app structure, Windows self-hosted runner, NSIS installer, backup/restore system. Load when working on deploy CI, the iem-mixer app startup, or the backup/restore feature.
---

# REAPER IEM — Deployment Skill

## Deployment Architecture (DO NOT CHANGE)

**ONE unified Tauri desktop app** that does EVERYTHING:
- Embedded Axum web server (API on port 80)
- Embedded WASM frontend (UI)
- System tray icon with member menu
- Runs at user login via Windows Startup shortcut

**NEVER deploy standalone headless server** — user explicitly requires tray icon.

**Starting app from CI:** Use WMI `Win32_Process.Create()` to spawn detached process surviving runner cleanup.
- Runner in session 1 → WMI creates app in session 1 → tray icon works
- CI deploy uses `taskkill /f` to stop old process (works cross-session)

### Windows Defender

**Windows Defender quarantines `iem-mixer-app.exe`** — exclusion path set via:
```powershell
Set-MpPreference -ExclusionPath "C:\Users\newlevel\AppData\Local\IEM Mixer"
```
If app disappears after install: `Get-MpThreatDetection` for quarantine events.

### Port 80

```
netsh http add urlacl url=http://+:80/ user=USERNAME
```
(Persistent, set once. Required because binding to port 80 needs this or running elevated.)

### NSIS Installer (NOT raw exe)

- Upload `target/release/bundle/nsis/*.exe` artifact
- Run: `installer.exe /S`
- Binary installs to `%LOCALAPPDATA%\IEM Mixer\iem-mixer-app.exe`
- Config: `%APPDATA%\iem-mixer\config.yaml`
- Startup launcher: `%LOCALAPPDATA%\IEM Mixer\iem-mixer-launcher.bat`

**NEVER deploy raw exe to custom paths** — use Tauri installer for Windows integration.

---

## Self-Hosted Runner (iem-lan)

- **Label**: `iem-lan` (NOT `iem-deploy`)
- **Location**: `C:\actions-runner` on iem.lan
- **MUST run as user app in session 1** (NOT as Windows service)
- Startup shortcut: `%APPDATA%\...\Startup\GitHub Actions Runner.lnk`
- CI guard: deploy fails immediately if runner is in session 0

**Platform note:** Self-hosted runner is Windows. **Never use `shell: bash`** for iem-lan CI jobs.

---

## Mixer Backup/Restore System (v1.135.0)

Automatic mixer state backups + engineer-only restore UI. Added after CI E2E tests corrupted band member settings (47 muted sends, wrong track volumes, reset EQ bands).

**Architecture:**
- Backup daemon: tokio cron task (default 13:00, 21:00)
- Captures: sends (vol/pan/mute), track volumes, EQ bands, limiter params, customizations (pinned/hidden), PINs
- Storage: timestamped JSON in `%APPDATA%/iem-mixer/backups/`
- Restore: **name-based matching** (not index) — survives track reordering/addition/removal
- UI: "Backups" section in Settings modal, engineer-only, preview before apply

**Key files:**
- Types: `iem-core/src/backup.rs`
- Store: `iem-server/src/backup_store.rs`
- Capture: `iem-server/src/backup_capture.rs`
- Restore: `iem-server/src/backup_restore.rs`
- Routes: `iem-server/src/backup_routes.rs`
- Daemon: `iem-server/src/backup_daemon.rs`
- UI: `iem-ui/src/components/backup_section.rs`

Git nightly cron also backs up `customizations/*.json` for disaster recovery.

---

## Version Files

**Primary version source:** `iem-mixer/crates/iem-core/Cargo.toml`

Also update for consistency:
- `iem-mixer/Cargo.toml`
- `iem-mixer/crates/iem-server/Cargo.toml`
- `iem-mixer/iem-ui/Cargo.toml`
- `iem-mixer/src-tauri/Cargo.toml`
- `iem-mixer/src-tauri/tauri.conf.json` (NSIS installer version)

```bash
# Bump all files from 1.1.0 to 1.2.0
sed -i 's/version = "1.1.0"/version = "1.2.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.1.0"/"version": "1.2.0"/' iem-mixer/src-tauri/tauri.conf.json
```

---

## URLs

- **Band members (public):** `https://iem.newlevel.media/` — ONLY URL band members use (Cloudflare Tunnel PWA)
- **Internal (CI / debugging):** `http://10.77.9.231/` — direct access, never share with band
- REAPER HTTP API: `http://iem.lan:8080/`

---

## VBAN Audio Streaming (v1.66.0+)

ReaStream was replaced by VBAN IEM VST3. **Never reference ReaStream or port 58710 for audio streaming.**

```
REAPER → ENGINEER inear → VBAN IEM VST3 → UDP 127.0.0.1:6980 → iem-mixer-app → Opus → WebSocket → Browser
```

- Custom JUCE C++ VST3 (`iem-mixer/vban-vst/`), built by CI `build-vban` job
- Deployed to `C:\Program Files\Common Files\VST3\VBAN IEM.vst3`
- No startup order dependency — port 6980 doesn't conflict with REAPER
- Use `setup_vban.lua` / `check_vban.lua` for verification

**Verify streaming pipeline:**
```bash
curl "http://iem.lan:8080/_/_RS_REAPERIEM_CHECK_VBAN"
sleep 2
curl "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/vban_status"
# Expected: PRESENT:track_idx=N:fx_idx=N:enabled=yes
```
