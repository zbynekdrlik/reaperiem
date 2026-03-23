# REAPER IEM Mixing System

MCP server for controlling REAPER as a personal monitor (IEM) mixer for church band.

**URL:** https://iem.newlevel.media/

## Changelog

### v1.92.0 (2026-03-23)

- **Fix**: Stems fader on Main tab now renders after ME mic channel (was incorrectly before it)

### v1.91.0 (2026-03-23)

- **Feature**: Stems group volume fader — control all stem channels (DRUMS, BASS, INST, CLICK, GUIDE, BGVS, OTHER) together with a single fader while preserving individual relative mix levels (#87)
- **Feature**: Stems fader visible on both Main and Stems tabs for quick access
- **Feature**: Stems volume saved/restored with presets (backwards-compatible with old presets)
- **Fix**: CI deploy now reliably kills app processes across sessions (multi-method fallback)
- **Fix**: CI deploy picks correct version installer when multiple are present

### v1.90.0 (2026-03-22)

- **Fix**: Preset names now accept digits and backspace works correctly — login page keyboard listener was leaking globally and intercepting keys on all pages (#110)

### v1.89.0 (2026-03-22)

- **Fix**: Band member Listen now works — mutes other member sends to ENGINEER for true audio isolation (solo-based approach didn't affect send routing)
- **Fix**: Listen mute states fully restored after stopping Listen on band member pages
- **Fix**: CI deploy hardened — REAPER restart verifies project loaded (NTRACK > 0), creates StartREAPER task with correct project path, fails deploy if REAPER doesn't come back

### v1.79.0 (2026-03-21)

- **Feature**: Listen volume boost setting (0-24 dB in 3 dB steps) in engineer Settings modal
- **Fix**: Listen boost applies immediately while listening — no need to stop and restart Listen mode
- **Feature**: Keyboard PIN entry on desktop — type digits directly instead of tapping number pad

### v1.75.0 (2026-03-20)

- **Feature**: Solo state now syncs across devices — solo a channel on your phone and your laptop shows it too
- **Feature**: New connections receive current solo state immediately (no stale UI on second tab)

### v1.74.0 (2026-03-20)

- **Fix**: Phones no longer stuck on infinite spinner after app restart — JWT signing key now persists to config.yaml so cached tokens remain valid across restarts
- **Fix**: Stale tokens auto-detected — after 3 consecutive WebSocket failures, the app verifies the token with the server and redirects to login if rejected (instead of spinning forever)

### v1.73.0 (2026-03-20)

- **Security**: REAPER proxy endpoint now requires engineer authentication
- **Security**: JWT secret auto-generated at startup when not configured (with warning)
- **Security**: Member ID validated against path traversal in all file stores
- **Fix**: MCP meter readings corrected — was dividing dB\*10 by 100 instead of 10 (10x error)
- **Fix**: REST endpoints now use REAPER-discovered members instead of static config
- **Fix**: Batch Reset uses name-based track lookup instead of sequential indices
- **Fix**: WebSocket closure memory leak on reconnect (closures stored instead of forgotten)
- **Perf**: Memoized channel display list to avoid recomputation on every meter update
- **Perf**: E2E tests use pre-built binary (30s startup vs 120s)
- **Robustness**: Atomic file writes (tmp+rename) in all JSON stores prevent corruption on crash
- **Robustness**: Poisoned mutex handled gracefully in audio diagnostics
- **CI**: Nightly backup uses git worktree (no working tree modification while REAPER runs)
- **CI**: Cargo cache cross-job fallback with restore-keys

### v1.72.0 (2026-03-20)

- **Hardening**: App now retries member discovery when REAPER is temporarily unavailable at startup — engineer mix controls auto-recover within 10 seconds instead of staying broken for the entire session
- **Fix**: Engineer mix monitoring now uses post-fader sends so the engineer hears members' actual output volumes (fader adjustments reflected in real-time)
- **Perf**: Fixed PWA freezing on Android — consolidated meter animations, throttled WebSocket updates, added server-side change detection to reduce unnecessary broadcasts

### v1.71.0 (2026-03-19)

- **Fix**: Blank page after deploy — removed service worker caching that served stale WASM/JS assets; all band members' phones auto-fix on next app open (no manual cache clear needed)
- **Fix**: AudioData.copyTo RangeError — use allocationSize + f32-planar format for correct buffer sizing
- **Fix**: Mobile audio playback — AudioContext.resume() now called during user gesture to unblock audio on mobile browsers

### v1.59.0 (2026-03-16)

- **Fix**: Mute/fader controls no longer target wrong member after track insertion/removal — frontend now detects track index shifts and fully replaces channel state instead of merging by stale index
- **Fix**: `<For>` key changed to compound (name + track_index) so Leptos destroys stale closures when tracks shift, preventing captured values from targeting the wrong REAPER track

### v1.58.0 (2026-03-15)

- **Fix**: Engineer mixer now shows all member mix faders — hardware output destination (-1) was breaking send discovery, preventing mix channels from appearing
- **Fix**: Engineer mute no longer shuts down member hardware outputs — mute now targets the correct mix send index instead of hardcoded send 0
- **Fix**: Rate-limited REAPER discovery requests (50ms delay) to prevent HTTP API crashes on startup
- **Fix**: Removed test_setup.lua script that could create random tracks in REAPER
- **Fix**: CI backup step now uses git worktrees to prevent project file deletion

### v1.56.0 (2026-03-14)

- **Fix**: Fader now reaches exact whole-number dB values (e.g., -4.0) — switched to 0.2 dB steps with integer boundary snapping
- **Fix**: Bottom toolbar (Mute All, Snapshots, Presets) no longer disappears on mobile when address bar shows/hides

### v1.54.0 (2026-03-14)

- **Fix**: Mute All button on engineer mixer now mutes all 31 channels (previously only muted 22 input channels, leaving 9 mix channels unmuted)

### v1.52.0 (2026-03-14)

- **Feature**: Auto-redirect — returning users skip the member grid and go straight to their mixer (valid token) or PIN login (expired token)
- **UX**: Back button still works — navigating back shows the member grid within the same session

### v1.51.0 (2026-03-13)

- **Fix**: Channel name truncation — long names like "Petronela" no longer get cut off when muted or stereo-paired (replaced `border-left` with `box-shadow: inset`)

### v1.49.0 (2026-03-13)

- **Feature**: Engineer Mixes tab — monitor each band member's in-ear mix with individual faders
- **Fix**: All engineer channels default-muted (engineer unmutes selectively)
- **Fix**: CI backup handles dirty REAPER project without failing deploy

### v1.47.0 (2026-03-11)

- **Fix**: Engineer PIN change — engineers on member phones can now change the member's PIN (was returning 403)
- **Fix**: Token expiry enforcement — expired tokens are now detected every 60s and redirect to login (was silently failing)
- **UI**: PIN change modal hides "Current PIN" field for engineers (they don't know the member's PIN)

### v1.46.0 (2026-03-11)

- **Security**: Enforce member access control — members can only access their own mixer, engineers can access any (Issue #77)
- **Fix**: Hide button now works on muted channels (Issue #78)
- **Fix**: Cross-member navigation redirects to login page with correct member pre-selected instead of blinking back to landing

### v1.44.0 (2026-03-10)

- **Fix**: Applied pre-fader sends to REAPER — 199 sends corrected from post-fader to pre-fader post-FX mode (Issue #7)
- **CI**: Added "Verify send modes" step that fails pipeline if any send regresses to post-fader
- **Script**: New `check_send_modes.lua` for automated send mode verification via EXTSTATE

### v1.43.0 (2026-03-10)

- **Fix**: Own channel always appears first on Main tab (above pinned channels)
- **Fix**: Removed "MY MIC" label clutter from Main tab
- **Fix**: Kebab menu visual polish — wider dB column, menu positioned left of label
- **Fix**: Channel header dB overflow, kebab visibility, name truncation improvements

### v1.36.0 (2026-03-09)

- **Feature**: Server-side presets — presets now sync across all devices (#70)
- **Feature**: Nightly git backup of snapshots and presets (#64, #71)
- **Feature**: CI deploy backs up snapshots and presets to git repository

### v1.35.0 (2026-03-08)

- **Fix**: Remove explorer-killing icon cache clear from CI deploy (was causing taskbar crash-loop on iem.lan)
- **Fix**: Headphones icon anti-aliased rendering with correct headband arc direction

### v1.34.0 (2026-03-08)

- **Fix**: Version/datetime text contrast improved for better readability (#63)
- **Fix**: Snapshot history shows absolute date with Slovak day name first (#69)
- **Fix**: App icon shows headphones matching tray icon instead of blue rectangle (#2)
- **Feature**: Band changelog skill for user-oriented Slovak changelogs

### v1.33.0 (2026-03-07)

- **Feature**: Access from mobile data via Cloudflare Tunnel - single URL works everywhere
- **Fix**: Tray menu shows correct HTTPS URL

### v1.32.0 (2026-03-06)

- **Feature**: `rename_track` MCP tool for renaming REAPER tracks

### v1.31.0 (2026-03-06)

- **Fix**: Member sees own fader in main section (#51)
- **Fix**: Higher contrast for version/datetime text (#63)
- **Fix**: Comprehensive name changes across REAPER, Dante, and mixer

### v1.30.0 (2026-03-06)

- **Feature**: REAPER as single source of truth for band members
- **Feature**: Version and datetime displayed on landing page
- **Feature**: Global volume persistence across page reloads

### v1.28.0 (2026-03-04)

- **Feature**: Daily preset snapshots - automatic server-side backups of mixer settings
- **Feature**: Snapshot history modal with restore, pin, and delete
- **Feature**: Network error UX improvements with clear feedback
- **Feature**: PIN re-authentication for sensitive operations
- **Fix**: Preset modal responsive on mobile devices
- **Security**: Constant-time PIN comparison

### v1.27.0 (2026-03-03)

- **Feature**: NEWLEVEL IEM MIXER branding
- **Feature**: New app icon

### v1.25.0 (2026-03-02)

- **Fix**: Silent meter bridge (removed console window popup)
- **Fix**: Auto-restart meter bridge on REAPER reconnect

### v1.23.0 (2026-03-02)

- **Feature**: ReaScript meter bridge for true L/R stereo peaks
- **Fix**: Meters show correct L/R stereo levels
- **Fix**: Meters display raw input levels (not affected by fader/pan)
- **Fix**: Correct dB×10 conversion formula

### v1.21.0 (2026-03-01)

- **Feature**: Settings modal with configurable options
- **Setting**: Fader double-tap toggle (enable/disable double-tap to 0 dB)
- **Feature**: Pan slider smooth animation with double-tap to center
- **Feature**: Rename band members from UI
- **Feature**: Change PIN from Settings modal
- **Feature**: Logout button in Settings modal
- **UI**: Category tabs: Main, Mics, Stems, Tech
- **UI**: Global volume (master) fader on Main tab
- **UI**: Presets modal - save/load/delete named presets with timestamps

### v1.20.0 (2026-03-01)

- **Feature**: Full codebase security review (P0+P1+P2 fixes)
- **Feature**: WebSocket real-time communication (replaced HTTP polling)

## Features

- HTTP-based control of REAPER tracks and sends
- Per-band-member "More Me" web interface
- Git version control of REAPER projects via SSH
- Claude Code integration via MCP

## Architecture

- **MCP Server**: Python + FastMCP on dev machine
- **REAPER**: Running on iem.lan with Web Interface enabled
- **Control**: HTTP Web API (port 8080)
- **Version Control**: Git on iem.lan via SSH

## Quick Start

```bash
# Install dependencies
pip install -e ./mcp/reaperiem_mcp

# Configure
cp config/reaper_config.yaml.example config/reaper_config.yaml
# Edit with your settings

# Run MCP server
python -m reaperiem_mcp.server
```
