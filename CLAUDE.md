# REAPER IEM Mixing System

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
- `set_track_volume(index, volume_db)`
- `mute_track(index, mute)`
- `solo_track(index, solo)`

### Send Control

- `set_send_level(track_index, send_index, level_db)`
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
4. **Sample rate** - Currently 44100Hz in config, should be 96000Hz for Dante
