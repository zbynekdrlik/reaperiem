# REAPER IEM Mixing System

<!-- Global rules inherited from ~/.claude/CLAUDE.md (managed by airuleset) -->
<!-- PR merge policy, CI monitoring, TDD, autonomous verification, git workflow, test strictness, deploy patterns -->

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

## Git Branching Model

### CI Job Matrix

| Job                | `dev` push | PR to `main` | `main` push |
| ------------------ | :--------: | :----------: | :---------: |
| test-integrity     |    yes     |     yes      |     yes     |
| lint               |    yes     |     yes      |     yes     |
| test               |    yes     |     yes      |     yes     |
| build-wasm         |    yes     |     yes      |     yes     |
| e2e                |    yes     |     yes      |     yes     |
| check-version-bump |     -      |   **yes**    |      -      |
| build-tauri        |  **yes**   |   **yes**    |   **yes**   |
| deploy             |  **yes**   |      -       |   **yes**   |

### ⚠️ CHANGELOG MAINTENANCE (MANDATORY)

**After EVERY PR merge to main, you MUST update the changelog in README.md.**

Include ALL user-facing changes:

- New features and settings (e.g., double-tap disable option)
- Bug fixes that affect user experience
- UI/UX improvements
- New access methods or URLs

**NEVER skip changelog updates.** If you merged a PR, the changelog must be updated in the same session.

**Changelog format in README.md:**

```markdown
## Changelog

### v1.X.0 (YYYY-MM-DD)

- **Feature**: Description
- **Fix**: Description
- **Setting**: New option in Settings modal
```

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
2. **ReaScript registration** - New scripts are registered dynamically via meter_bridge (no restart). CI also adds them to `reaper-kb.ini` as fallback for next startup.
3. **Git on iem.lan** - Repository cloned there, commits track REAPER project changes
4. **Sample rate** - Network runs at 96kHz (REAPER follows ASIO driver rate)
5. **Input metering** - Tracks must be record-armed (I_RECARM=1) to show input levels
6. **⚠️ ALWAYS SAVE BEFORE RESTART** - Run `curl "http://iem.lan:8080/_/40026"` before any REAPER restart!

---

## ⚠️ REAPER Data Verification (MANDATORY)

Before ANY change to REAPER HTTP API parsing, you MUST:

1. `curl -s "http://iem.lan:8080/_/NTRACK;TRACK"` — see real field layout
2. `curl -s "http://iem.lan:8080/_/GET/TRACK/1/SEND/0"` — see real SEND format
3. Capture the actual values and use them in tests
4. NEVER assume field counts, value ranges, or sentinel values

**REAPER meter values are dB×10** (NOT centibels!). The official docs say: "last_meter_peak and last_meter_pos are integers that are dB\*10, so -100 would be -10dB." Convert: `10^(value / 10.0 / 20.0)`. Floor: `-1500` = -150 dB = digital silence. Values like -925 = -92.5 dB = preamp noise floor (invisible on meters).

Integration tests exist: `cargo test -p iem-server --features integration`

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
   - Deploy via CI (push to `dev`)
   - Dynamic registration via meter_bridge (no restart needed — see below)

### Dynamic Script Registration (v1.62.0+)

**New scripts can be registered at runtime without REAPER restart.**

`meter_bridge.lua` (running continuously via `defer()`) checks `EXTSTATE reaperiem/register_scripts` each tick. When set with pipe-delimited filenames, it calls `reaper.AddRemoveReaScript()` to register them instantly.

```bash
# Register scripts dynamically (CI does this automatically):
curl "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/register_scripts/setup_vban.lua|check_vban.lua"
sleep 3
curl "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/register_result"
# Expected: OK:2
```

**Filenames only** (not full paths) — meter_bridge constructs the path via `reaper.GetResourcePath() .. "/Scripts/reaperiem/" .. filename`.

### Stereo Tracks Convention:

```
Input tracks from FOH: Single stereo track (DRUMS, BASS, INST, OTHER, BGVS)
  - NOT separate L/R tracks
  - Use consecutive Dante input channels as stereo pair
  - NCHAN=2, stereo input mode (channel + 1024)
```

---

## ⚠️ OPERATIONAL PLAYBOOK — HOW TO WORK WITH REAPER

**This section documents the patterns Claude MUST follow. Read it EVERY session.**

### MCP Tools — MANDATORY FIRST PRIORITY

You have a `reaperiem` MCP server with 24+ tools for controlling REAPER. **MCP tools are the MANDATORY first choice for ALL REAPER operations. NEVER use curl or SSH when an MCP tool can do the job.**

**If an MCP tool is missing, incomplete, or cannot handle an operation you need — you MUST extend the MCP server to add that capability BEFORE proceeding with the task.** Do not work around missing MCP functionality with curl/SSH. Instead:

1. Add the new tool to `mcp/reaperiem_mcp/tools/` (or create a new module)
2. Add the corresponding ReaScript if needed (`scripts/reascripts/`)
3. Register the tool in `server.py`
4. Deploy via CI (push to `dev`)
5. Then use the new MCP tool for the operation

**This is non-negotiable.** Every curl workaround is technical debt. Every SSH hack is a missed MCP improvement. The MCP server should be the complete, authoritative interface for REAPER operations.

```
DECISION TREE:
  Need to read/write REAPER state? → Use MCP tool (list_tracks, set_send_level, etc.)
  MCP tool missing for operation?  → STOP. Add MCP tool first, then use it.
  Need to trigger a ReaScript?     → Add MCP tool wrapper, OR use curl as LAST RESORT
  Need EXTSTATE read/write?         → Add MCP tool wrapper, OR use curl as LAST RESORT
  Need to run Windows commands?     → Use win-iem-snv MCP tools (Shell, Snapshot, FileRead, FileWrite, etc.)
  Need screenshots/desktop?         → Use win-iem-snv Snapshot (NEVER ssh + screenshot script)
  Need git ops on iem.lan?          → Use MCP git tools (git_status, git_commit, git_push, git_log)
  Need to add REAPER capability?    → Extend MCP server (see "Adding New MCP Tools" below)
```

**NEVER DO:**

```
❌ Use curl to REAPER when an MCP tool exists for the same operation
❌ Use SSH to iem.lan for REAPER operations (SSH is for Windows system ops only)
❌ Leave a curl/SSH workaround in place — always follow up by adding the MCP tool
❌ Say "I'll add the MCP tool later" — add it NOW before proceeding
```

### How to Talk to REAPER (3 Methods)

**Method 1: MCP Tools (preferred for standard operations)**

```
mcp__reaperiem__list_tracks          # See all tracks
mcp__reaperiem__set_send_level       # Control mix levels
mcp__reaperiem__get_track_meter      # Read meters
mcp__reaperiem__set_hardware_output  # Route audio outputs
```

**Method 2: HTTP API via curl (for EXTSTATE and actions)**

```bash
# ALL commands MUST use /_/ prefix!
curl "http://iem.lan:8080/_/NTRACK;TRACK"                           # Query all tracks
curl "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/key/value"       # Set EXTSTATE
curl "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/key"             # Read EXTSTATE
curl "http://iem.lan:8080/_/_RS_REAPERIEM_SETUP_REASTREAM"          # Trigger ReaScript action
curl "http://iem.lan:8080/_/40026"                                  # Save project (action 40026)
```

**Method 3: SSH to iem.lan (for Windows system operations)**

```bash
ssh newlevel@iem.lan "command"                                       # Run command on Windows
ssh newlevel@iem.lan "tasklist | findstr iem-mixer"                  # Check process
ssh newlevel@iem.lan "type C:\path\to\file"                         # Read file on Windows
```

### EXTSTATE Communication Pattern

EXTSTATE is the bridge between HTTP API and ReaScripts. This is the standard pattern:

```
1. Set parameters:  curl "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/param_key/param_value"
2. Trigger script:  curl "http://iem.lan:8080/_/_RS_REAPERIEM_SCRIPT_NAME"
3. Wait:            sleep 2-3 seconds (script needs time to execute)
4. Read result:     curl "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/result_key"
```

Every ReaScript writes results to EXTSTATE. Never assume a script succeeded — always read the result.

### ReaScript Lifecycle

```
scripts/reascripts/*.lua  →  CI deploys to iem.lan REAPER/Scripts/reaperiem/
                          →  CI registers in reaper-kb.ini (startup fallback)
                          →  CI triggers dynamic registration via meter_bridge EXTSTATE
                          →  Scripts become callable via HTTP API action IDs
```

**Two script types:**

- **One-shot** (setup*vban.lua, check_vban.lua): Triggered via `/*/_RS_REAPERIEM_\*`, run once, write result to EXTSTATE
- **Deferred** (meter_bridge.lua): Run continuously via `reaper.defer()`, must NOT use `ShowConsoleMsg` (steals focus)

### Adding New MCP Tools

When REAPER needs a new capability not covered by existing tools:

```
1. Create ReaScript:       scripts/reascripts/new_script.lua
   - Use EXTSTATE for params/results (not ShowConsoleMsg!)
   - One-shot scripts: read EXTSTATE, do work, write result
   - Name action: _RS_REAPERIEM_NEW_SCRIPT

2. Add MCP tool wrapper:   mcp/reaperiem_mcp/tools/<module>.py
   - Import in server.py
   - Use reaper_http.send_command() for HTTP API calls
   - Use ssh_client for system operations

3. Add to config:          config/reaper_config.yaml (action IDs)

4. Push to dev:            CI deploys script, registers it dynamically

5. Test via MCP:           Use the new tool immediately (no restart)
```

**MCP server structure:**

```
mcp/reaperiem_mcp/
├── server.py              # FastMCP server, tool registration
├── lib/
│   ├── reaper_http.py     # HTTP client (send_command, _build_url with /_/ prefix)
│   ├── ssh_client.py      # SSH to iem.lan (paramiko, PowerShell commands)
│   └── config.py          # YAML config loader
└── tools/
    ├── tracks.py          # Track control tools
    ├── mix.py             # Send/mix control tools
    ├── git.py             # Git operations on iem.lan
    ├── band.py            # Band member config
    ├── routing.py         # Hardware output routing
    └── presets.py         # Mix preset save/load
```

### Registered ReaScripts (all in scripts/reascripts/)

| Script                      | Action ID                    | Type     | Purpose                                             |
| --------------------------- | ---------------------------- | -------- | --------------------------------------------------- |
| meter_bridge.lua            | `_RS_REAPERIEM_METER_BRIDGE` | Deferred | L/R stereo meters + dynamic script registration     |
| set_hardware_output.lua     | `_RS_REAPERIEM_SET_HW_OUT`   | One-shot | Set Dante output routing                            |
| rename_track.lua            | `_RS_REAPERIEM_RENAME_TRACK` | One-shot | Rename tracks live                                  |
| check_send_modes.lua        | `_RS_REAPERIEM_CHECK_SENDS`  | One-shot | Verify all sends are pre-fader                      |
| fix_send_mode.lua           | `_RS_REAPERIEM_FIX_SENDS`    | One-shot | Fix sends to pre-fader post-FX                      |
| setup_vban.lua              | `_RS_REAPERIEM_SETUP_VBAN`   | One-shot | Insert VBAN IEM VST3 on engineer track              |
| check_vban.lua              | `_RS_REAPERIEM_CHECK_VBAN`   | One-shot | Verify VBAN IEM VST3 status                         |
| setup_iem_project.lua       | -                            | One-shot | Initial project setup                               |
| merge_stereo_inputs.lua     | -                            | One-shot | Merge mono inputs to stereo                         |
| set_colors.lua              | -                            | One-shot | Set track colors                                    |
| create_sends_for_member.lua | -                            | One-shot | Create sends for new member                         |
| tone_generator.lua          | `_RS_REAPERIEM_TONE_GEN`     | One-shot | Toggle test tone on engineer track (CI audio tests) |

### Common Operational Tasks

**Check if REAPER is responding:**

```bash
curl -sf "http://iem.lan:8080/_/NTRACK" && echo "REAPER OK" || echo "REAPER DOWN"
```

**Check if app is running:**

```bash
curl -sf "http://10.77.9.231/api/version" | python3 -m json.tool
```

**Verify audio streaming pipeline:**

```bash
curl "http://iem.lan:8080/_/_RS_REAPERIEM_CHECK_VBAN"
sleep 2
curl "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/vban_status"
# Expected: PRESENT:track_idx=N:fx_idx=N:enabled=yes
```

### VBAN Audio Streaming Configuration

```
REAPER → ENGINEER inear track → VBAN IEM VST3 → UDP 127.0.0.1:6980 → iem-mixer-app → Opus → WebSocket → Browser

VBAN IEM VST3 (custom, built in CI from iem-mixer/vban-vst/):
  - Hardcoded: Send to 127.0.0.1:6980, stream name "engineer"
  - Format: VBAN protocol, INT16 interleaved PCM
  - No GUI configuration needed — auto-activates on insert
  - Deployed to: C:\Program Files\Common Files\VST3\VBAN IEM.vst3 (dir has user write perms via icacls)

App listens on: 127.0.0.1:6980 (standard VBAN port)
No startup order dependency — port 6980 is not shared with REAPER.
No SO_REUSEADDR needed — plain bind.
```

**Post-deploy verification (CI does this automatically):**

The deploy job sends synthetic UDP packets and connects to the live WebSocket to verify binary Opus frames arrive. This test FAILS the deploy if audio pipeline is broken — no `continue-on-error`.

**Register new scripts dynamically (no restart):**

```bash
curl "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/register_scripts/script1.lua|script2.lua"
sleep 3
curl "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/register_result"
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

---

## ⚠️ VERSION FILES

**Version file (primary):** `iem-mixer/crates/iem-core/Cargo.toml` (this is where VERSION constant comes from)

**Also update for consistency:**

- `iem-mixer/Cargo.toml`
- `iem-mixer/crates/iem-server/Cargo.toml`
- `iem-mixer/iem-ui/Cargo.toml`
- `iem-mixer/src-tauri/Cargo.toml`
- `iem-mixer/src-tauri/tauri.conf.json` (NSIS installer version)

**Example version bump:**

```bash
# Bump all Cargo.toml files + tauri.conf.json from 1.1.0 to 1.2.0
sed -i 's/version = "1.1.0"/version = "1.2.0"/' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.1.0"/"version": "1.2.0"/' iem-mixer/src-tauri/tauri.conf.json
```

---

## ⚠️ PRE-PUSH VALIDATION CHECKLIST (MANDATORY)

**Before EVERY push, verify these project-specific rules:**

1. **Dead code** - Any new function/module used somewhere? If only in tests, mark `#[cfg(test)]`. **NEVER use `#[allow(dead_code)]`** — if code isn't used, remove it entirely.
2. **Platform** - Self-hosted runner is Windows. Never use `shell: bash` for iem-lan jobs!
3. **REAPER API** - All URLs must have `/_/` prefix. All SEND parsing must use field index 4/3/5 for vol/mute/pan
4. **Feature flags** - `--features standalone` needed for server binary

**NEVER DO:**

```
❌ Push code that introduces dead_code warnings (remove unused code, don't suppress with #[allow(dead_code)])
❌ Use `shell: bash` on Windows self-hosted runner
❌ Change REAPER parsing without adding tests that verify real response format
❌ Push multiple "fix CI" commits in a row - think before pushing!
```

---

## ⚠️ PROJECT-SPECIFIC TEST NOTES

### Known Test Gaps

Existing tests are mostly rendering checks. Green CI does NOT guarantee features work. Write behavior tests, not just DOM checks.

- E2E tests run without REAPER — most mixer functionality is assume()-skipped
- No WebSocket message flow integration tests
- No settings persistence tests (save → reload → verify)
- Mute, pan, fader commands not verified against REAPER

After every deploy, manually verify: fader → REAPER value changes, mute → channel mutes in REAPER.

### E2E Against Real System

When the user reports a bug, write a failing E2E test against the REAL deployed system first.

```
Phase 1: Write E2E test → run against iem.lan/10.77.9.231 → confirm it FAILS (catches the bug)
Phase 2: Write fix → deploy → run E2E test on live system → confirm it PASSES → report success
```

CI uses synthetic data. User-reported issues MUST be verified against the real deployed system.

### Visual Changes

For icon/image fixes: write pixel-level automated test BEFORE generating the asset, verify with CI screenshot artifact. Never claim visual fixes without downloading and checking the artifact yourself.

### Test File Locations

- `iem-mixer/crates/*/src/*.rs` - Unit tests (inline #[cfg(test)])
- `iem-mixer/tests/` - Integration tests
- `iem-mixer/e2e/` - Playwright e2e tests

**Run locally before push:**

```bash
cd iem-mixer
cargo test --workspace           # Unit + integration
npx playwright test              # E2E (requires trunk serve)
```

---

## CI Policy

### GitHub Secrets

- `IEM_LAN_SSH_KEY` - SSH key for deploy@iem.lan
- `TAURI_SIGNING_PRIVATE_KEY` - For updater signatures (optional)

---

## IEM Mixer Desktop App

The `iem-mixer/` directory contains the new Tauri + Leptos WASM desktop application.

### Structure:

```
 iem-mixer/
├── crates/
│   ├── iem-core/     # Shared types, config
│   └── iem-server/   # Axum API server
├── iem-ui/           # Leptos WASM frontend
└── src-tauri/        # Tauri desktop shell
```

### Build Commands (run on GitHub Actions, not locally):

```bash
# WASM frontend
cd iem-mixer/iem-ui && trunk build --release

# Tauri app
cd iem-mixer/src-tauri && cargo tauri build
```

### URLs:

- **User-facing (band members):** `https://iem.newlevel.media/` — this is the ONLY URL band members use (Cloudflare Tunnel PWA)
- **Internal server (infrastructure):** `http://10.77.9.231/` — direct access to iem.lan server, for CI verification and debugging
- Routes: `/` (landing), `/login` (PIN auth), `/<member>` (mixer, e.g., /petka)

**IMPORTANT: Band members always access via `iem.newlevel.media`. The IP `10.77.9.231` and hostname `iem.lan` are internal infrastructure — never give these to band members.**

---

## ⚠️ END OF EACH PROMPT PROCESS

After completing any task that affects the IEM Mixer application, always provide the deployment status:

```
PR: <url> | CI: green | Deploy: verified | Dashboard: http://10.77.9.231/ (internal) / https://iem.newlevel.media/ (user-facing)
```

This ensures the user can immediately access and verify the changes.
