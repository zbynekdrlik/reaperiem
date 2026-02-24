# REAPER IEM Mixing System

## ⚠️ CRITICAL: Git Workflow Rules

**READ THIS FIRST - ENFORCED BY GIT HOOKS**

```
┌─────────────────────────────────────────────────────────────┐
│  THIS IS DEV MACHINE - You can ONLY commit CODE here        │
│                                                             │
│  ✅ ALLOWED: mcp/*, web/*, scripts/*, config/*, *.py, *.md  │
│  ❌ BLOCKED: projects/*.RPP (git hook will reject)          │
│                                                             │
│  To deploy code to iem.lan: ./scripts/deploy.sh             │
│  To check status:           ./scripts/deploy.sh --status    │
└─────────────────────────────────────────────────────────────┘
```

**File Ownership (ENFORCED):**
| Files | Edit On | Commit On | Hook Blocks |
|-------|---------|-----------|-------------|
| Code (_.py, _.html, _.lua) | Dev machine | Dev machine | iem.lan |
| REAPER projects (_.RPP) | iem.lan (REAPER) | iem.lan | Dev machine |

**NEVER DO:**

- ❌ `git add projects/*.RPP` on dev machine (hook blocks it)
- ❌ Edit code files on iem.lan (hook blocks commits)
- ❌ Manual SCP/rsync to sync files (use deploy.sh)
- ❌ Direct push from iem.lan for code changes

**ALWAYS DO:**

- ✅ Use `./scripts/deploy.sh` to deploy code
- ✅ Let REAPER save projects on iem.lan, commit there
- ✅ Pull on dev machine if you need latest RPP: `git pull`

---

## Project Overview

MCP server for personal monitor mixing using REAPER's HTTP Web API for a church band.
REAPER runs on `iem.lan` with Yamaha Dante Accelerator (128ch @ 96kHz).

## Key Commands

- `pytest` - Run tests
- `python -m reaperiem_mcp.server` - Run MCP server locally
- `/mcp` - Restart MCP connection in Claude Code

## Architecture

```
Claude Code → MCP Server → HTTP API → REAPER (iem.lan:8080)
                        → SSH      → Git operations on iem.lan
```

- `mcp/reaperiem_mcp/` - FastMCP server code
- `config/` - YAML configuration files (reaper_config.yaml has secrets)
- `scripts/reascripts/` - Lua scripts deployed to REAPER
- `web/` - Custom REAPER web interface files
- `projects/` - REAPER project files (version controlled)

## REAPER Control Methods

### HTTP Web API (real-time, no restart)

- `SET/TRACK/index/VOL/value` - Set track volume
- `SET/TRACK/x/SEND/y/VOL/value` - Set send volume
- `SET/EXTSTATE/section/key/value` - Pass parameters to ReaScripts
- `_ACTION_ID` - Trigger registered ReaScript actions

### ReaScripts (for advanced operations)

- Registered in `reaper-kb.ini` on iem.lan
- Triggered via HTTP API action IDs
- Can do ANYTHING including hardware output routing

### Key ReaScript: set_hardware_output.lua

- Location: `REAPER/Scripts/reaperiem/set_hardware_output.lua`
- Action ID: `_RS_REAPERIEM_SET_HW_OUT`
- Sets hardware output channels for tracks LIVE

## REAPER Files on iem.lan

- Config: `C:\Users\newlevel\AppData\Roaming\REAPER\`
  - `REAPER.ini` - Main config (ASIO, sample rate)
  - `reaper-kb.ini` - Registered scripts/actions
- Project: `C:\Users\newlevel\Documents\reaperiem\`
- Scripts: `...\REAPER\Scripts\reaperiem\`

## Conventions

- Track names: UPPERCASE first word, lowercase second (e.g., "MAREK mic", "MAREK inear")
- Band member IDs: 1-indexed integers
- Dante outputs: Stereo pairs (L/R), 1-indexed in config, 0-indexed in API

## MCP Tools Available

### Track Control

- `list_tracks` - List all REAPER tracks
- `get_track(index)` - Get track details
- `get_track_meter(track_index)` - Get peak/RMS levels in dB
- `set_track_volume(index, volume_db)`
- `mute_track(index, mute)`
- `solo_track(index, solo)`

### Send Control

- `set_send_level(track_index, send_index, level_db)`
- `set_send_pan(track_index, send_index, pan)` - Pan position (-1.0 left to 1.0 right)
- `set_send_mute(track_index, send_index, mute)` - Mute/unmute send (bool)
- `adjust_send_level(track_index, send_index, adjustment_db)`

### Hardware Routing (LIVE, no restart)

- `set_hardware_output(track_index, channel_l, channel_r)`

### Git Operations (on iem.lan)

- `git_status` - Show changed files
- `git_commit(message)` - Commit changes
- `git_push` - Push to GitHub
- `git_log(count)` - Show history

### Band Config

- `list_band_members` - Show members and outputs
- `add_band_member(name, dante_output_l, dante_output_r)`
- `list_input_tracks` - Show input routing
- `add_input_track(name, dante_input, default_level_db)`

## Important Notes

1. **No REAPER restarts** - All MCP operations work live via HTTP API + ReaScripts
2. **ReaScript registration** - New scripts need one REAPER restart to load from reaper-kb.ini
3. **Git on iem.lan** - Repository cloned there, commits track REAPER project changes
4. **Sample rate** - Network runs at 96kHz (REAPER follows ASIO driver rate)
5. **Input metering** - Tracks must be record-armed (I_RECARM=1) to show input levels
6. **⚠️ ALWAYS SAVE BEFORE RESTART** - Run `curl "http://iem.lan:8080/_/40026"` before any REAPER restart!

---

## ⚠️ REAPER Development Rules

### NEVER DO:

```
❌ Edit .RPP files directly (requires reload, causes conflicts)
❌ Restart REAPER to apply changes
❌ Create one-off scripts - improve existing MCP tools instead
❌ Hardcode track indices or names that may change
❌ Reinvent functionality that exists in MCP tools
```

### ALWAYS DO:

```
✅ Use HTTP API + ReaScripts for ALL REAPER operations (live, no restart)
✅ Continuously improve scripts/reascripts/*.lua to handle new operations
✅ Add new MCP tools when needed (mcp/reaperiem_mcp/server.py)
✅ Use EXTSTATE to pass parameters between HTTP API and ReaScripts
✅ Track operations by name pattern matching, not hardcoded indices
```

### Adding New REAPER Capabilities:

1. **Check if MCP tool exists** - Use existing tools first
2. **If new capability needed:**
   - Create/modify ReaScript in `scripts/reascripts/`
   - Register in `reaper-kb.ini` with action ID `_RS_REAPERIEM_*`
   - Add MCP tool wrapper in `mcp/reaperiem_mcp/server.py`
   - Add action ID to `config/reaper_config.yaml`
   - Deploy via `./scripts/deploy.sh`
   - ONE restart to register new action, then it works live forever

### Stereo Tracks Convention:

```
Input tracks from FOH: Single stereo track (DRUMS, BASS, INST, OTHER, BGVS)
  - NOT separate L/R tracks
  - Use consecutive Dante input channels as stereo pair
  - NCHAN=2, stereo input mode (channel + 1024)
```

---

## ⚠️ DANTE NETWORK SAFETY

The Dante network has 3 devices by role. **Only the IEM Accelerator is safe to modify.**
Device names may change. Run `netaudio device list` to get current names.

```
Device Roles:
  IEM Accelerator (128ch)  ← Claude controls (connected to iem.lan REAPER)
  Stagebox (32ch)          ← DO NOT MODIFY (mics/DIs)
  FOH Accelerator (128ch)  ← DO NOT MODIFY (main PA)
```

**ALLOWED:**

```bash
netaudio device list                                              # List online devices
netaudio channel list --device-name <any>                         # Read channels
netaudio subscription list                                        # Read routing
netaudio config --device-name <IEM_DEVICE> --set-channel-name <ch> <name>
```

**NEVER DO:**

```bash
# ❌ NEVER modify subscriptions (breaks audio routing!)
netaudio subscription add/remove ...

# ❌ NEVER change device settings
netaudio config --set-sample-rate/encoding/latency ...

# ❌ NEVER modify stagebox or FOH devices
```

See `.claude/skills/dante.md` for full Dante documentation and `config/dante_network.yaml` for topology.
