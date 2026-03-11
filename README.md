# REAPER IEM Mixing System

MCP server for controlling REAPER as a personal monitor (IEM) mixer for church band.

**URL:** https://iem.newlevel.media/

## Changelog

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
