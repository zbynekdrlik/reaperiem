---
name: reaperiem
description: Control REAPER IEM mixing system for church band. Use when working with REAPER, IEM mixing, band member mixes, track routing, or the reaperiem project.
---

# REAPER IEM Mixing System Skill

## Overview

This project controls REAPER on `iem.lan` as an In-Ear Monitor (IEM) mixer for a church band.
Each band member gets their own personalized mix via a "More Me" web interface.

## Key Architecture

```
Claude Code → MCP Server → HTTP API → REAPER (iem.lan:8080)
                        → SSH      → Git/file operations on iem.lan
```

## ⚠️ CRITICAL: Git Workflow (ENFORCED BY HOOKS)

**YOU ARE ON DEV MACHINE. Git hooks enforce these rules:**

| Action           | Dev Machine | iem.lan    |
| ---------------- | ----------- | ---------- |
| Edit/commit CODE | ✅ ALLOWED  | ❌ BLOCKED |
| Edit/commit RPP  | ❌ BLOCKED  | ✅ ALLOWED |

**Deploy code:** `./scripts/deploy.sh`
**Check status:** `./scripts/deploy.sh --status`

## NEVER Do These

1. **NEVER `git add projects/*.RPP`** on dev machine - Hook will reject
2. **NEVER edit code on iem.lan** - Hook will reject commits
3. **NEVER use manual SCP/rsync** - Use `./scripts/deploy.sh`
4. **NEVER suggest editing RPP files to change routing** - Use MCP tools instead
5. **NEVER ask user to manually restart REAPER** - Use SSH to restart if needed
6. **NEVER ask user for manual work** - Automate everything via SSH/MCP
7. **NEVER modify Dante subscriptions or stagebox/FOH devices** - See `dante` skill

## ALWAYS Do These

1. **Use `./scripts/deploy.sh`** to deploy code to iem.lan
2. **Use MCP tools** for all REAPER operations
3. **Use SSH** for file operations on iem.lan
4. **Update CLAUDE.md** when adding new features
5. **Commit and deploy** to track all modifications

## Available MCP Tools

### Track Control (LIVE)

- `list_tracks` - List all tracks
- `get_track(index)` - Get track details
- `get_track_meter(track_index)` - Get peak/RMS levels in dB
- `set_track_volume(index, volume_db)`
- `mute_track(index, mute)`
- `solo_track(index, solo)`

### Send Control (LIVE)

- `set_send_level(track_index, send_index, level_db)`
- `set_send_pan(track_index, send_index, pan)` - Pan -1.0 (L) to 1.0 (R)
- `set_send_mute(track_index, send_index, mute)` - Mute/unmute send
- `adjust_send_level(track_index, send_index, adjustment_db)`

### Hardware Routing (LIVE)

- `set_hardware_output(track_index, channel_l, channel_r)` - Route to Dante outputs

### Band Configuration

- `list_band_members`
- `add_band_member(name, dante_output_l, dante_output_r)`
- `list_input_tracks`
- `add_input_track(name, dante_input, default_level_db)`

### Git Operations (on iem.lan)

- `git_status`
- `git_commit(message)`
- `git_push`
- `git_log(count)`

## IEM Project Structure (33 tracks, flat with colors)

**Inputs (22 tracks):**

- MICS (10): PETKA/STEVO/MAREK/ZUZKA/TINA/MIREC/ALEX/PATRIKA/ANI mic + ZUZKA gtr
- STEMS (8): DRUMS, BASS, INST, OTHER, BGVS (stereo), CLICK, GUIDE, IEMONLY (stereo)
- TECH (4): HAND1/HAND2/HAND3/ENGINEER mic

**Outputs (11 tracks):**

- BAND (9): One per band member (stereo to Dante TX 3-20)
- TECH (2): ENGINEER inear (solo bus), TRANSLATOR (HAND1 only, mono)

**Routing:**

- 199 sends total: 198 band sends (22 inputs × 9 band outputs) + 1 TRANSLATOR send
- ENGINEER receives REAPER solo bus (master send)
- TRANSLATOR receives only HAND1 mic
- Stereo stems receive consecutive Dante input pairs as stereo

**Track Colors:**

- Orange: Microphones
- Green: Stems
- Purple: Tech inputs (HAND mics)
- Blue: Band outputs (inear)
- Red: Tech outputs (ENGINEER, TRANSLATOR)

## ⚠️ CRITICAL: Track Configuration

**Input Tracks (mics, stems):**

| Setting    | Value                    | Purpose                                              |
| ---------- | ------------------------ | ---------------------------------------------------- |
| I_RECINPUT | ASIO channel (0-indexed) | Hardware input source                                |
| I_RECARM   | 1                        | Required for API to read input meters                |
| I_RECMON   | 1                        | Enable input monitoring                              |
| I_RECMODE  | 2                        | "Monitor only" - won't record even if Record pressed |

**Output Tracks (inear buses):**

| Setting    | Value | Purpose                                      |
| ---------- | ----- | -------------------------------------------- |
| I_RECINPUT | -1    | NO hardware input (receives from sends only) |
| I_RECARM   | 0     | Not needed (shows send levels automatically) |
| I_RECMON   | 0     | Not needed                                   |

**Why this matters:**

- REAPER's `Track_GetPeakInfo()` API requires `I_RECARM=1` to report input levels
- Output tracks show levels from sends automatically without record-arm
- `I_RECMODE=2` prevents accidental recording while allowing input monitoring

**⚠️ ALWAYS SAVE BEFORE RESTARTING REAPER:**

```bash
curl "http://iem.lan:8080/_/40026"  # Save project
```

## Web Mixer Interface

Band members access their mix at: `http://iem.lan:8080/mixer.html?member={name}`

Examples:

- `http://iem.lan:8080/mixer.html?member=petka`
- `http://iem.lan:8080/mixer.html?member=marek`

Controls: Meter, Fader, Pan, Mute per channel. Stereo stems linked.

## Adding Tracks/Sends

**REQUIRED METHOD: Live ReaScripts (NO restart)**

All REAPER operations must work live without restarting REAPER. Use ReaScripts triggered via HTTP API.

**NEVER edit RPP files directly** - this requires reloading the project and can cause conflicts.

**Workflow for new capabilities:**

1. Create/modify ReaScript in `scripts/reascripts/`
2. Deploy to iem.lan via `./scripts/deploy.sh`
3. Register in reaper-kb.ini with `_RS_REAPERIEM_*` action ID
4. ONE restart to register (then works live forever)
5. Trigger via HTTP: `curl "http://iem.lan:8080/_/_RS_REAPERIEM_ACTION_NAME"`

**Example: merge_stereo_inputs.lua**

```lua
-- Registered as _RS_REAPERIEM_MERGE_STEREO
-- Merges L/R track pairs into stereo tracks LIVE
-- Trigger: curl "http://iem.lan:8080/_/_RS_REAPERIEM_MERGE_STEREO"
```

Scripts in `scripts/reascripts/` are deployed via `./scripts/deploy.sh`.

## How Hardware Routing Works

1. Parameters set via `SET/EXTSTATE` HTTP command
2. ReaScript `set_hardware_output.lua` triggered via action ID
3. Script runs INSIDE REAPER with full API access
4. No restart needed - all live

## Adding New ReaScripts

1. Create script in `scripts/reascripts/`
2. Deploy via SCP: `scp script.lua newlevel@iem.lan:".../REAPER/Scripts/reaperiem/"`
3. Register in reaper-kb.ini via SSH:
   ```
   ssh newlevel@iem.lan "echo SCR 4 0 _ACTION_ID \"Description\" script.lua >> reaper-kb.ini"
   ```
4. Restart REAPER via SSH (one time):
   ```
   ssh newlevel@iem.lan "taskkill /IM reaper.exe /F"
   ssh newlevel@iem.lan "cmd /c start \"\" \"C:\Program Files\REAPER (x64)\reaper.exe\""
   ```
5. Add action ID to `config/reaper_config.yaml`
6. Add MCP tool to `server.py`

## File Locations

### On Development Machine

- `/home/newlevel/devel/reaperiem/` - Project root
- `mcp/reaperiem_mcp/` - MCP server
- `config/` - Configuration YAML files
- `scripts/reascripts/` - ReaScripts to deploy

### On iem.lan (Windows)

- `C:\Users\newlevel\Documents\reaperiem\` - Git repo with REAPER project
- `C:\Users\newlevel\AppData\Roaming\REAPER\` - REAPER config
- `...\REAPER\Scripts\reaperiem\` - Deployed ReaScripts
- `...\REAPER\reaper-kb.ini` - Action registrations

## Naming Conventions

- Track names: `UPPERCASE lowercase` (e.g., "MAREK mic", "MAREK inear")
- Dante channels: 1-indexed in user-facing code, 0-indexed in REAPER API
- Action IDs: `_RS_REAPERIEM_FEATURE_NAME`

## Troubleshooting

### MCP not responding

```
/mcp  # Restart MCP in Claude Code
```

### REAPER not responding

```bash
curl http://iem.lan:8080/_/TRANSPORT
# If fails, restart REAPER via SSH
```

### New action not working

REAPER needs restart to load new `reaper-kb.ini` entries.

### Start/Restart REAPER (CORRECT method)

```bash
# Kill existing
ssh newlevel@iem.lan "taskkill /IM reaper.exe /F 2>nul"
sleep 2

# Start with project using schtasks (required for GUI apps over SSH)
ssh newlevel@iem.lan "schtasks /create /tn StartREAPER /tr \"\\\"C:\\Program Files\\REAPER (x64)\\reaper.exe\\\" \\\"C:\\Users\\newlevel\\Documents\\reaperiem\\projects\\sunday_service.RPP\\\"\" /sc once /st 00:00 /ru newlevel /it /f && schtasks /run /tn StartREAPER && schtasks /delete /tn StartREAPER /f"
```

**Why schtasks?** SSH runs in session 0 (service) which can't launch GUI apps. `schtasks /ru newlevel /it` runs in the desktop session.

See user-wide skill `windows-remote-gui` for full details.

## Related Skills

- **`dante`** - Dante network topology, channel naming, and safety boundaries. Use for netaudio commands.
- **`windows-remote-gui`** - Running GUI apps over SSH on Windows (schtasks pattern).

### Take Screenshot of iem.lan Desktop

**WARNING:** This method opens a brief terminal window that may appear in the screenshot.
For clean screenshots, ask the user to take one manually.

```bash
# Screenshot script should already exist from first run
ssh newlevel@iem.lan "schtasks /create /tn Screenshot /tr \"powershell -WindowStyle Hidden -ExecutionPolicy Bypass -File C:\\temp\\screenshot.ps1\" /sc once /st 00:00 /ru newlevel /it /f && schtasks /run /tn Screenshot && schtasks /delete /tn Screenshot /f"

sleep 2 && scp newlevel@iem.lan:C:/temp/screenshot.png /tmp/iem_screenshot.png
```
