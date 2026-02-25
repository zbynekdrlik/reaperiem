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

---

## ⚠️ CI/CD MONITORING REQUIREMENTS

**CRITICAL: Always commit, push, and monitor CI automatically - DO NOT wait for user confirmation.**

When implementation is complete and tests pass locally:

1. Commit immediately with a descriptive message
2. Push to trigger CI
3. Monitor CI until all jobs pass
4. If CI fails, fix and repeat
5. For releases: verify deployment to iem.lan

**CRITICAL: After EVERY push, you MUST monitor CI until ALL jobs are GREEN.**

### ⚠️ PRE-PUSH VALIDATION CHECKLIST (MANDATORY)

**Before EVERY push, mentally verify:**

1. **Dead code** - Any new function/module used somewhere? If only in tests, mark `#[cfg(test)]`
2. **Format** - Code must match rustfmt (no trailing whitespace, proper line lengths)
3. **Platform** - Self-hosted runner is Windows. Never use `shell: bash` for iem-lan jobs!
4. **REAPER API** - All URLs must have `/_/` prefix. All SEND parsing must use field index 4/3/5 for vol/mute/pan
5. **Feature flags** - `--features standalone` needed for server binary

**NEVER DO:**

```
❌ Push code that introduces dead_code warnings (use #[cfg(test)] or call the function)
❌ Use `shell: bash` on Windows self-hosted runner
❌ Change REAPER parsing without adding tests that verify real response format
❌ Push multiple "fix CI" commits in a row - think before pushing!
```

**THE RULE:** One push should work. If CI fails, the fix should be ONE commit that addresses ALL issues found, not a stream of partial fixes.

### After Pushing Code:

```bash
# 1. Check CI status immediately
gh run list --limit 3

# 2. Watch the run until complete
gh run watch <run-id>

# 3. If any job fails, check logs and fix
gh run view <run-id> --log-failed
```

### CI Must Pass:

- ✅ Lint & Format (cargo fmt, clippy)
- ✅ Unit Tests (cargo test)
- ✅ Integration Tests (API endpoints)
- ✅ **E2E Tests (Playwright)** - Full browser testing of web UI
- ✅ Build WASM (trunk build)
- ✅ Build Tauri (Windows)
- ✅ CI Success (all jobs)

### ⚠️ MANDATORY: Comprehensive Testing

**Every feature MUST have full test coverage before merging:**

```
Unit Tests:
  - All Rust functions in iem-core, iem-server
  - Edge cases and error handling

Integration Tests:
  - API endpoints (/api/members, /api/mixer/*, /api/auth)
  - REAPER proxy functionality
  - Authentication flow

E2E Tests (Playwright):
  - Landing page loads with member cards
  - Login flow (PIN entry, JWT storage)
  - Mixer page renders with faders
  - Fader controls actually work
  - Navigation between pages
  - Mobile viewport testing
  - Error states and loading spinners
```

### ⚠️ E2E TESTS ARE HISTORICALLY WEAK - RADICAL IMPROVEMENT REQUIRED

**CRITICAL: E2E tests have repeatedly allowed broken apps to deploy!**

E2E tests MUST verify:

1. **Faders actually change REAPER values** - not just "page loads"
2. **Mute button state persists** - click mute, verify state after poll refresh
3. **Pan slider works end-to-end** - move slider, verify REAPER receives command
4. **Meters show real audio** - if track has audio, meter must be > 0
5. **Connection status accurate** - disconnected banner when REAPER unreachable
6. **Presets persist** - save preset, reload page, preset still exists

**After EVERY deploy, manually verify:**

- Open mixer in browser
- Move a fader → REAPER value changes (check REAPER directly)
- Click mute → channel mutes in REAPER
- If controls don't work, CI HAS FAILED even if green!

**E2E tests must be expanded aggressively** - if a feature exists, it needs an E2E test that verifies it works end-to-end with REAPER, not just that "the UI renders".

**Test files location:**

- `iem-mixer/crates/*/src/*.rs` - Unit tests (inline #[cfg(test)])
- `iem-mixer/tests/` - Integration tests
- `iem-mixer/e2e/` - Playwright e2e tests

**Run locally before push:**

```bash
cd iem-mixer
cargo test --workspace           # Unit + integration
npx playwright test              # E2E (requires trunk serve)
```

### After Release (tags):

```bash
# Verify deployment to iem.lan
curl -sf http://iem.lan/ && echo "Deploy OK" || echo "Deploy FAILED"

# Check release artifacts exist
gh release view <tag>
```

### NEVER DO:

```
❌ Push and walk away without checking CI
❌ Ignore failing CI jobs
❌ Skip verifying deployment after release
❌ Merge/release with failing tests
```

---

## ⚠️ ZERO TOLERANCE CI POLICY

**FUNDAMENTAL RULE: If it's not tested, it's broken.**

### CI MUST:

- ❌ NEVER skip tests (no `#[ignore]`, no `skip`, no conditional `if`)
- ❌ NEVER have conditional test execution (no `if: always()` bypass)
- ❌ NEVER pass with 0 tests (must verify test count > 0)
- ❌ NEVER deploy without E2E verification
- ✅ ALWAYS run ALL tests on EVERY push
- ✅ ALWAYS verify deployed app responds correctly
- ✅ ALWAYS fail if any test is skipped or ignored

### Required Test Coverage:

| Component     | Test Type   | Must Test                                        |
| ------------- | ----------- | ------------------------------------------------ |
| API endpoints | Integration | Every endpoint returns expected data             |
| REAPER proxy  | Integration | Commands reach REAPER and return valid responses |
| Auth flow     | Integration | Login/logout/token refresh                       |
| Mixer UI      | E2E         | Page loads, faders work, presets save            |
| Deploy        | Smoke       | HTTP 200 from http://iem.lan/                    |

### Meta-Test Requirement:

CI must include a "test-integrity" job that:

1. Counts total tests and fails if < minimum threshold
2. Scans for `#[ignore]` and fails if any found
3. Scans for `skip` patterns and fails if any found
4. Verifies no `if:` conditions bypass test execution

### GitHub Secrets Required:

- `IEM_LAN_SSH_KEY` - SSH key for deploy@iem.lan (set via `gh secret set`)
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

- `http://10.77.9.231/` - Landing page (member selection)
- `http://10.77.9.231/login` - PIN authentication
- `http://10.77.9.231/<member>` - Member's mixer (e.g., /petka)

**IMPORTANT: Always use IP address (10.77.9.231) instead of hostname when providing URLs to the user.**

---

## ⚠️ END OF EACH PROMPT PROCESS

After completing any task that affects the IEM Mixer application, always provide the user with the relevant URL in IP format:

```
✅ Deployed: http://10.77.9.231/
```

This ensures the user can immediately access and verify the changes.
