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

## ⚠️ CLAUDE AUTONOMOUS WINDOWS ENGINEER (CAWE) DIRECTIVE

**You are "Claude Autonomous Windows Engineer" (CAWE).**

You have the tools and skills to autonomously verify ALL Windows deployments. You MUST:

1. **Deploy to iem.lan** via CI
2. **Capture screenshot** of the deployed result (CI artifact)
3. **Download and verify** the screenshot yourself
4. **Report ONLY verified facts** (not hopes, not "should work")

### CAWE Verification Protocol

```
For ANY visual change (icons, UI, colors):
  1. Push code to trigger CI
  2. Wait for CI to complete
  3. Download the taskbar-screenshot artifact
  4. View it yourself (Read tool on downloaded file)
  5. Report: "VERIFIED: [what you actually saw]"
     OR: "FAILED: [what was wrong]"

NEVER: "It should work now" / "The icon should appear"
```

### CAWE Capabilities

- **SSH to iem.lan**: You can run commands on the Windows host
- **Screenshot capture**: CI uploads taskbar screenshot artifact
- **Icon pixel verification**: verify_icons.py tests in CI
- **Process control**: Start/stop apps remotely

### CAWE Forbidden Behaviors

```
❌ Claim success without downloading/viewing verification artifacts
❌ Use speculative language ("should", "will probably", "might")
❌ Ask user to verify something you can verify yourself
❌ Ignore CI artifacts and rely on user reports
❌ Treat user as your testing tool
```

### CAWE Success Reporting

**Only use these phrases:**

- `VERIFIED: I downloaded the screenshot and saw headphones icon`
- `FAILED: Screenshot shows blue rectangle, not headphones`
- `NOT VERIFIED: CI still running, artifact not yet available`

**Never use these phrases:**

- "The icon should now appear correctly"
- "This should fix the issue"
- "It will probably work after cache clear"
- "I believe the fix is correct"

---

## Git Branching Model

**Two branches only: `main` + `dev`** (enforced by GitHub rulesets)

| Branch | Purpose     | Direct push | Force push | Delete  |
| ------ | ----------- | :---------: | :--------: | :-----: |
| `main` | Production  |   BLOCKED   |  BLOCKED   | BLOCKED |
| `dev`  | Development |   allowed   |  BLOCKED   | BLOCKED |

### Development Workflow

1. Push all code to `dev`
2. Create PR `dev` → `main` when ready to deploy
3. CI runs all checks on PR (lint, tests, build, e2e, version bump)
4. **Wait for explicit user approval** (user must say "approved" or equivalent in the conversation)
5. Merge commit (only method allowed — no squash, no rebase)
6. Merge triggers auto-deploy to iem.lan

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

### PR Delivery Requirement

**You are responsible for delivering a GREEN, mergeable PR.** When creating a PR from `dev` → `main`:

1. Push to `dev`, wait for CI to pass (including deploy to iem.lan)
2. Fix any failures on `dev` before creating the PR
3. Create PR only when `dev` CI is fully green
4. Monitor PR CI until ALL required checks pass
5. Provide the PR URL to the user only after it is confirmed green and mergeable
6. **The PR URL you give the user must be ready to merge — no exceptions**
7. **⚠️ NEVER merge the PR yourself — wait for the user's explicit approval in the conversation** (e.g., "approved, do it", "merge it", "go ahead"). Only then run `gh pr merge`.
8. **After merge: Update README.md changelog** with user-facing changes from the PR

If CI fails on the PR, fix the issue, push to `dev`, and wait for the PR to go green before reporting.

### ⚠️ PR MERGE REQUIRES EXPLICIT USER APPROVAL

**CRITICAL: You MUST NOT merge any PR without the user's explicit approval in the conversation.**

- Present the green PR URL to the user
- Wait for the user to explicitly approve (e.g., "approved", "merge it", "do it", "go ahead")
- Only THEN run `gh pr merge`
- **NEVER auto-merge, even if all CI checks pass**
- **NEVER assume approval — silence is NOT approval**

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

### NEVER DO:

```
❌ Push directly to main (blocked by GitHub ruleset)
❌ Create feature branches (blocked by GitHub ruleset)
❌ Force push to main or dev (blocked)
❌ Squash or rebase merge (only merge commits allowed)
❌ Give the user a PR URL that has failing CI checks
❌ Ask the user to merge a PR that isn't green
❌ Merge a PR without explicit user approval in the conversation (NEVER auto-merge!)
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
2. **ReaScript registration** - New scripts need one REAPER restart to load from reaper-kb.ini
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

**FULL CYCLE REQUIREMENT: You MUST complete the ENTIRE pipeline before reporting to the user:**

1. Commit immediately with a descriptive message
2. Push to trigger CI
3. Monitor CI until all jobs pass
4. If CI fails, fix and repeat
5. After merge to main: **monitor the deploy CI run** until Deploy job completes
6. After deploy: **verify the live app** at http://10.77.9.231/ responds correctly
7. **Only THEN** report success to the user

**DO NOT interrupt the user mid-pipeline.** The full cycle is: code → commit → push → CI green → merge → deploy CI green → verify live app. Complete all steps autonomously.

**CRITICAL: After EVERY push, you MUST monitor CI until ALL jobs are GREEN.**

### ⚠️ VERSION BUMP REQUIREMENT

**BUMP VERSION AT THE START OF EVERY DEVELOPMENT SESSION that will deploy to production.**

The CI version check runs FIRST to fail fast (within seconds) rather than after expensive builds.

**Version file:** `iem-mixer/crates/iem-core/Cargo.toml` (this is where VERSION constant comes from)

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

### ⚠️ PRE-PUSH VALIDATION CHECKLIST (MANDATORY)

**Before EVERY push, mentally verify:**

1. **Dead code** - Any new function/module used somewhere? If only in tests, mark `#[cfg(test)]`. **NEVER use `#[allow(dead_code)]`** — if code isn't used, remove it entirely.
2. **Format** - Code must match rustfmt (no trailing whitespace, proper line lengths)
3. **Platform** - Self-hosted runner is Windows. Never use `shell: bash` for iem-lan jobs!
4. **REAPER API** - All URLs must have `/_/` prefix. All SEND parsing must use field index 4/3/5 for vol/mute/pan
5. **Feature flags** - `--features standalone` needed for server binary

**NEVER DO:**

```
❌ Push code that introduces dead_code warnings (remove unused code, don't suppress with #[allow(dead_code)])
❌ Use `#[allow(dead_code)]` to suppress warnings — remove the unused code instead
❌ Use `shell: bash` on Windows self-hosted runner
❌ Change REAPER parsing without adding tests that verify real response format
❌ Push multiple "fix CI" commits in a row - think before pushing!
```

**THE RULE:** One push should work. If CI fails, the fix should be ONE commit that addresses ALL issues found, not a stream of partial fixes.

### ⚠️ ZERO TOLERANCE FOR TRANSIENT CI FAILURES

**The iem.lan network and GitHub runners are on strong infrastructure. There is NO excuse for transient failures.**

Every CI failure must be treated as a real bug and hardened against:

- **Network downloads** (wasm-bindgen, trunk, crates) — must use caching, retries, or pre-installed binaries
- **Timeouts** — increase timeouts or add proper wait-for-ready logic
- **Flaky tests** — fix the root cause, never re-run and hope
- **Resource exhaustion** — size runners appropriately

If a "transient" failure happens, the CI pipeline itself has a bug. Fix the pipeline, don't just re-run.

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

- ✅ Test Integrity Check (no ignored/skipped tests)
- ✅ Lint & Format (cargo fmt, clippy)
- ✅ Unit Tests (cargo test)
- ✅ **E2E Tests (Playwright)** - Full browser testing of web UI
- ✅ Build WASM (trunk build)
- ✅ Build Tauri (Windows) — PRs and main only
- ✅ Verify Version Bump — PRs only

### ⚠️ TDD MANDATORY — NO EXCEPTIONS, NO EXCUSES

**THIS IS THE #1 MOST VIOLATED RULE IN THIS PROJECT. Claude has repeatedly ignored TDD and shipped broken features. This stops now.**

**STRICT ENFORCEMENT:**

- **Every implementation plan MUST have test steps BEFORE code steps.** A plan without test steps is rejected.
- **Every bug fix starts with a failing test.** No test = no fix = no commit.
- **Every new feature starts with tests describing expected behavior.** No tests = no implementation.
- **The user is NOT your test suite.** You must catch regressions yourself through automated tests.

**For bug fixes — REPRODUCE BEFORE FIXING:**

1. **Write a failing test that captures the EXACT reported bug** — if you can't reproduce the bug in a test, you don't understand it yet
2. Run the test, confirm it FAILS (proving the bug exists)
3. ONLY THEN write the fix
4. Run the test again, confirm it PASSES
5. Run ALL tests to verify nothing else broke

**For new features — TESTS FIRST:**

1. Write E2E tests that describe the expected behavior BEFORE writing implementation
2. Write unit tests for new functions BEFORE implementing them
3. Implement until all tests pass
4. If tests pass but the feature is broken in the real app, THE TESTS ARE WRONG — fix the tests first

**For EVERY implementation plan — MANDATORY test steps:**

```
EVERY plan must follow this structure:
  Step 1: Write failing tests (E2E + unit) for the feature/bug
  Step 2: Confirm tests fail (proving they test the right thing)
  Step 3: Implement the feature/fix
  Step 4: Confirm tests pass
  Step 5: Run ALL existing tests to catch regressions
  Step 6: Push and monitor CI

A plan that goes straight to "implement X" without "write tests for X" first
is WRONG and must be rewritten.
```

**Why this is non-negotiable:** Without reproducing the bug first, fixes routinely introduce NEW bugs or don't actually fix the reported issue. A test that captures the bug is PROOF you understand the problem. Code without a failing test first is guessing. Claude has shipped broken pan animations, broken settings that don't persist, and broken meters — all because tests were skipped.

```
❌ NEVER: Read bug report → write code fix → hope it works → push
❌ NEVER: Write a plan with only implementation steps and no test steps
❌ NEVER: Use the user as a tester — "verify on live app" is NOT a substitute for automated tests
✅ ALWAYS: Read bug report → write failing test → confirm failure → write fix → confirm pass → push
✅ ALWAYS: Plan = test steps first, then implementation steps
✅ ALWAYS: New E2E/integration tests for every feature, covering actual behavior not just rendering
```

**Test comprehensiveness requirements:**

- **E2E tests must test REAL behavior** — not just "element exists" but "element does X when interacted with"
- **Integration tests must verify server-side logic** — WebSocket messages, REAPER API calls, cache behavior
- **Unit tests must cover edge cases** — string comparisons (case sensitivity!), category classification, conversion formulas
- **Every bug that reaches production gets a regression test** — so it never happens again
- **Animation tests must verify intermediate values** — not just "animation class exists" but "value changed over time"
- **Settings tests must verify persistence** — save setting, reload page, verify setting is still there
- **Visual/icon tests must verify actual pixel data** — not just "file exists" but verify specific pixel values match expected

### ⚠️ VISUAL CHANGES REQUIRE PIXEL-LEVEL VERIFICATION

**NEVER claim a visual fix (icons, images, colors) is done without automated verification.**

Claude has repeatedly:

1. Generated icon files and claimed "headphones icon is ready"
2. Ignored test results showing the icon had wrong transparency (alpha=1 instead of alpha=0)
3. Made the user report the same issue 3+ times before actually fixing it

**For icon/image fixes — MANDATORY verification:**

```python
# Example: Verify icon is headphones (not solid rectangle)
from PIL import Image
img = Image.open("icon.png").convert("RGBA")

# Check center is transparent (gap between ear cups)
center = img.getpixel((16, 16))
assert center[3] == 0, f"Center not transparent: alpha={center[3]}"

# Check ear area is blue
ear = img.getpixel((7, 20))
assert ear[2] > 200 and ear[3] == 255, f"Ear not blue: {ear}"

# Check corner is transparent
corner = img.getpixel((0, 0))
assert corner[3] == 0, f"Corner not transparent: alpha={corner[3]}"
```

**NEVER DO:**

```
❌ "The icon looks correct to me" — your visual inspection means nothing without pixel tests
❌ "The icon should now appear correctly" — NEVER use "should", only verified facts
❌ "It should work now" — this is FORBIDDEN language, you must VERIFY
❌ Claim fix is done after seeing a thumbnail — verify with automated test
❌ Ignore test results showing alpha=1 (almost transparent but wrong)
❌ Use the user as your visual verification tool
❌ Report success without downloading and checking the CI screenshot artifact
```

**ALWAYS DO:**

```
✅ Write pixel-level test BEFORE generating icon
✅ Run test and confirm it FAILS on old icon
✅ Generate new icon
✅ Run test and confirm it PASSES
✅ Download the CI taskbar screenshot artifact and visually verify
✅ If Windows icon cache is persistent, add reboot step to CI
✅ Only claim "VERIFIED: icon shows headphones" after checking screenshot
```

**FORBIDDEN PHRASES (never use these):**

- "should work" / "should appear" / "should be fixed"
- "it will probably" / "it might"
- "I believe" / "I think it's fixed"
- "The fix should take effect"

**REQUIRED PHRASES (use these instead):**

- "VERIFIED: I checked the screenshot and saw [X]"
- "NOT YET VERIFIED: CI is still running"
- "FAILED: screenshot shows [X] instead of [Y]"

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

### ⚠️ TESTS ARE WEAK — TREAT AS UNRELIABLE UNTIL PROVEN OTHERWISE

**STRICT RULE: Current E2E and integration tests are known to be weak. They have repeatedly allowed broken features to deploy while showing green CI. Every feature you implement or claim as "done" MUST be verified beyond what the existing tests check.**

**THE PROBLEM:** Tests mostly verify "UI renders" and "element exists" — they do NOT verify that features actually work. A green CI run does NOT mean the feature works. assume() guards hide failures instead of catching them.

**MANDATORY for every feature/fix:**

1. **Do NOT trust existing tests** — they are superficial. Read them critically.
2. **Write NEW tests that verify actual behavior**, not just rendering:
   - Does the fader actually send a value to REAPER? (not just "fader exists")
   - Does mute actually mute? (not just "button renders")
   - Does the animation actually animate? (not just "class exists")
   - Does the setting actually persist? (not just "modal opens")
3. **Verify on the live app** after deploy — open http://10.77.9.231/ in a browser and manually test every feature you changed
4. **If you cannot write a meaningful test** (e.g., REAPER not available in CI), explicitly document what is NOT tested and flag it to the user
5. **Never claim a feature is "done"** based solely on CI passing — CI passing means the code compiles and superficial checks pass, nothing more

**Current test gaps (known):**

- E2E tests run without REAPER — most mixer functionality is assume()-skipped
- No integration tests verify WebSocket message flow end-to-end
- No tests verify that settings actually persist across page reloads
- No tests verify pan/fader animations actually animate (timing, intermediate values)
- No tests verify meter values change in response to send controls
- Mute, pan, fader commands are not verified against REAPER

**After EVERY deploy, manually verify:**

- Open mixer in browser
- Move a fader → REAPER value changes (check REAPER directly)
- Click mute → channel mutes in REAPER
- If controls don't work, CI HAS FAILED even if green!

**E2E tests must be expanded aggressively** — if a feature exists, it needs an E2E test that verifies it works end-to-end with REAPER, not just that "the UI renders".

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

- **User-facing (band members):** `https://iem.newlevel.media/` — this is the ONLY URL band members use (Cloudflare Tunnel PWA)
- **Internal server (infrastructure):** `http://10.77.9.231/` — direct access to iem.lan server, for CI verification and debugging
- Routes: `/` (landing), `/login` (PIN auth), `/<member>` (mixer, e.g., /petka)

**IMPORTANT: Band members always access via `iem.newlevel.media`. The IP `10.77.9.231` and hostname `iem.lan` are internal infrastructure — never give these to band members.**

---

## ⚠️ END OF EACH PROMPT PROCESS

After completing any task that affects the IEM Mixer application, always provide the deployment status:

```
✅ Deployed: http://10.77.9.231/ (internal) / https://iem.newlevel.media/ (user-facing)
```

This ensures the user can immediately access and verify the changes.
