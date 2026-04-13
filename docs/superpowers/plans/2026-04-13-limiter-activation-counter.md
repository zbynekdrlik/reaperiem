# Limiter Activation Counter Implementation Plan (#145)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an `Active: M:SS` line + `Reset` button inside the existing per-track Limiter modal that shows how long that inear's safety limiter has been actively reducing gain since the last reset.

**Architecture:** Fork `MGA_JSLimiterST` JSFX to add a read-only slider5 mirroring the existing internal `gr_meter`. `meter_bridge.lua` polls slider5 each tick and accumulates active milliseconds per inear track when GR > 1 dB, writing per-track totals to a single EXTSTATE key. The server poller reads the totals into an in-memory `HashMap<usize, u64>` on `AppState`. The existing `handle_get_limiter_params` enriches its `ServerMsg::LimiterParams` reply with `active_seconds` from that HashMap. A new `ClientMsg::ResetLimiterActivity { track_index }` zeros both the server HashMap and the ReaScript-side accumulator (via an EXTSTATE round-trip so the next poller cycle does not re-overwrite the zero). UI changes are confined to `LimiterModal`.

**Tech Stack:** JSFX (REAPER plugin DSL), Lua (ReaScript), Rust (axum + tokio + serde), Leptos WASM frontend, Playwright TypeScript E2E.

**Spec:** `docs/superpowers/specs/2026-04-13-limiter-activation-counter-design.md`

---

## Hard constraints (per project CLAUDE.md + airuleset)

- No local `cargo test/build/clippy/check` — blocked by hooks. Only `cargo fmt --all --check` runs locally. All test execution lives in CI.
- All REAPER HTTP API URLs MUST use the `/_/` prefix.
- The self-hosted runner is Windows — never use `shell: bash` for iem-lan jobs.
- Every E2E spec must filter known noise and assert `consoleMessages` is empty.
- Each fix/feature row in the completion-report E2E table must reference a real committed test file.
- Last task presents the green PR URL and STOPS. Do NOT merge — wait for explicit user "merge it" / "approved" / "go ahead".

---

## File map

### Created
- `scripts/reascripts/jsfx/MGA_JSLimiterST` — local fork of upstream JSFX with slider5 added (deployed to `%APPDATA%\REAPER\Effects\loser\`)
- `iem-mixer/e2e/tests/live/limiter-activity.spec.ts` — live E2E

### Modified
- `scripts/reascripts/meter_bridge.lua` — add limiter activity polling + reset handling
- `iem-mixer/crates/iem-core/src/ws.rs:181-189` — extend `ServerMsg::LimiterParams` with `active_seconds`
- `iem-mixer/crates/iem-core/src/ws.rs:46-102` — add `ClientMsg::ResetLimiterActivity` variant
- `iem-mixer/crates/iem-server/src/lib.rs:99-154` — add `limiter_activity` field on `AppState`
- `iem-mixer/crates/iem-server/src/lib.rs:207-249` — initialize the new field in `AppState::new`
- `iem-mixer/crates/iem-server/src/poller.rs` — add EXTSTATE read for limiter activity totals each cycle
- `iem-mixer/crates/iem-server/src/proxy.rs:1230-1311` — add `ResetLimiterActivity` handler in WS recv loop
- `iem-mixer/crates/iem-server/src/proxy.rs:2505-2603` — enrich `handle_get_limiter_params` reply with `active_seconds`
- `iem-mixer/iem-ui/src/components/limiter_modal.rs` — add `active_seconds` prop + on_reset callback + new row
- `iem-mixer/iem-ui/src/pages/mixer.rs` — add active_seconds signal pair, populate from `LimiterParams` handler, pass to `LimiterModal`, wire `on_reset`
- `iem-mixer/iem-ui/style.css` — minor styling for `.limiter-activity-row` and `.limiter-reset-btn`
- `.github/workflows/ci.yml` — new "Deploy JSFX to REAPER Effects" step + register `jsfx/` path with the existing pre-deploy lint
- `README.md` — v1.149.0 changelog entry
- 5× `Cargo.toml` + `tauri.conf.json` — version bump 1.148.0 → 1.149.0

---

## Task 1: Version bump (FIRST commit, airuleset hard rule)

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`

- [ ] **Step 1: Bump all Rust + Tauri version files**

```bash
sed -i 's/version = "1.148.0"/version = "1.149.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.148.0"/"version": "1.149.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify**

```bash
grep -c '1.149.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
# Both must return 1.
grep -r '1.148.0' iem-mixer/crates iem-mixer/src-tauri iem-mixer/Cargo.toml iem-mixer/iem-ui/Cargo.toml
# Must return nothing.
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.149.0 (#145)"
```

---

## Task 2: Fork MGA_JSLimiterST with read-only GR slider

**Files:**
- Create: `scripts/reascripts/jsfx/MGA_JSLimiterST`

The upstream plugin already computes `gr_meter` continuously and writes to REAPER's
`ext_gr_meter` extension for the FX UI's own display. We add one read-only slider
that mirrors the same value, so ReaScript can poll it via `TrackFX_GetParam(track, fx, 4)`
(parameter index 4 = slider5, zero-based).

The original file lives on iem.lan at `C:\Users\newlevel\AppData\Roaming\REAPER\Effects\loser\MGA_JSLimiterST`. We copy it into the repo verbatim and add the two-line change.

- [ ] **Step 1: Pull the upstream source from iem.lan into the repo**

```bash
mkdir -p scripts/reascripts/jsfx
ssh newlevel@iem.lan 'powershell -Command "Get-Content C:\Users\newlevel\AppData\Roaming\REAPER\Effects\loser\MGA_JSLimiterST"' \
  > scripts/reascripts/jsfx/MGA_JSLimiterST
```

- [ ] **Step 2: Verify file size and shape**

```bash
wc -l scripts/reascripts/jsfx/MGA_JSLimiterST
# Should be ~80-110 lines. If 0 or unreasonably large, the SSH redirect failed.
grep -c '^slider' scripts/reascripts/jsfx/MGA_JSLimiterST
# Should be 4 (slider1..slider4).
grep -c 'ext_gr_meter' scripts/reascripts/jsfx/MGA_JSLimiterST
# Should be >= 2 (one in @init, one in @block).
```

- [ ] **Step 3: Add slider5 declaration**

Insert immediately after the existing `slider4:-0.1<-6,0,0.1>Ceiling` line:

```jsfx
slider5:0<-30,0,0.1>GR (dB read-only)
```

Apply with `Edit`:

```
old_string: slider4:-0.1<-6,0,0.1>Ceiling
new_string: slider4:-0.1<-6,0,0.1>Ceiling
slider5:0<-30,0,0.1>GR (dB read-only)
```

- [ ] **Step 4: Mirror ext_gr_meter into slider5 from @block**

In the `@block` section, the upstream file has exactly one line:

```jsfx
ext_gr_meter = gr_meter > 0 ? log(gr_meter) * (20/log(10)) : -150;
```

Replace it with:

```jsfx
ext_gr_meter = gr_meter > 0 ? log(gr_meter) * (20/log(10)) : -150;
slider5 = ext_gr_meter;
sliderchange(slider5);
```

`sliderchange()` is the JSFX call that pushes the new slider value into REAPER's
parameter automation system, making it readable from `TrackFX_GetParam`. The
audio path (`gain`, `gainO`, `spl0 *= gain;`, etc.) is untouched.

- [ ] **Step 5: Verify the edit**

```bash
grep -n '^slider\|^ext_gr_meter\|^slider5 =\|^sliderchange' scripts/reascripts/jsfx/MGA_JSLimiterST
# Expected lines (in order):
#   slider1:0<-30,0,0.1>Threshold (dB)
#   slider2:200<0,500,1>Release (ms)
#   slider3:75<0,100,1>Link Stereo (%)
#   slider4:-0.1<-6,0,0.1>Ceiling
#   slider5:0<-30,0,0.1>GR (dB read-only)
#   ext_gr_meter = gr_meter > 0 ? log(gr_meter) * (20/log(10)) : -150;
#   slider5 = ext_gr_meter;
#   sliderchange(slider5);
```

- [ ] **Step 6: Commit**

```bash
git add scripts/reascripts/jsfx/MGA_JSLimiterST
git commit -m "feat(jsfx): fork MGA_JSLimiterST to expose GR via slider5 (#145)"
```

---

## Task 3: Deploy JSFX to REAPER Effects directory in CI

**Files:**
- Modify: `.github/workflows/ci.yml:1110-1116` (insert new step immediately after "Deploy ReaScripts to REAPER")

The existing step at line 1110 copies `scripts/reascripts/*.lua` to
`%APPDATA%\REAPER\Scripts\reaperiem\`. We add a sibling step that copies the
forked JSFX file to `%APPDATA%\REAPER\Effects\loser\`, replacing the upstream
copy. Existing inserted instances pick up the new slider5 on next REAPER FX
reload — acceptable degradation, documented in changelog.

- [ ] **Step 1: Add the deploy step to ci.yml**

Insert immediately AFTER the existing block ending at `echo ReaScripts deployed`
(line 1116) and BEFORE the next step `Register new ReaScripts in reaper-kb.ini`:

```yaml
      - name: Deploy JSFX to REAPER Effects
        shell: cmd
        run: |
          if not exist "C:\Users\newlevel\AppData\Roaming\REAPER\Effects\loser\" mkdir "C:\Users\newlevel\AppData\Roaming\REAPER\Effects\loser\"
          xcopy /Y scripts\reascripts\jsfx\MGA_JSLimiterST "C:\Users\newlevel\AppData\Roaming\REAPER\Effects\loser\"
          if errorlevel 1 (echo ERROR: Failed to deploy JSFX && exit /b 1)
          echo JSFX deployed
```

Apply with `Edit`:

```
old_string:       - name: Deploy ReaScripts to REAPER
        shell: cmd
        run: |
          if not exist "C:\Users\newlevel\AppData\Roaming\REAPER\Scripts\reaperiem\" mkdir "C:\Users\newlevel\AppData\Roaming\REAPER\Scripts\reaperiem\"
          xcopy /Y scripts\reascripts\*.lua "C:\Users\newlevel\AppData\Roaming\REAPER\Scripts\reaperiem\"
          if errorlevel 1 (echo ERROR: Failed to deploy ReaScripts && exit /b 1)
          echo ReaScripts deployed

      - name: Register new ReaScripts in reaper-kb.ini
new_string:       - name: Deploy ReaScripts to REAPER
        shell: cmd
        run: |
          if not exist "C:\Users\newlevel\AppData\Roaming\REAPER\Scripts\reaperiem\" mkdir "C:\Users\newlevel\AppData\Roaming\REAPER\Scripts\reaperiem\"
          xcopy /Y scripts\reascripts\*.lua "C:\Users\newlevel\AppData\Roaming\REAPER\Scripts\reaperiem\"
          if errorlevel 1 (echo ERROR: Failed to deploy ReaScripts && exit /b 1)
          echo ReaScripts deployed

      - name: Deploy JSFX to REAPER Effects
        shell: cmd
        run: |
          if not exist "C:\Users\newlevel\AppData\Roaming\REAPER\Effects\loser\" mkdir "C:\Users\newlevel\AppData\Roaming\REAPER\Effects\loser\"
          xcopy /Y scripts\reascripts\jsfx\MGA_JSLimiterST "C:\Users\newlevel\AppData\Roaming\REAPER\Effects\loser\"
          if errorlevel 1 (echo ERROR: Failed to deploy JSFX && exit /b 1)
          echo JSFX deployed

      - name: Register new ReaScripts in reaper-kb.ini
```

- [ ] **Step 2: Verify the YAML edit (no syntax break)**

```bash
grep -n "Deploy JSFX to REAPER Effects\|Deploy ReaScripts to REAPER\|Register new ReaScripts" .github/workflows/ci.yml | head -5
# Expected output:
#   1110:      - name: Deploy ReaScripts to REAPER
#   1118:      - name: Deploy JSFX to REAPER Effects
#   1125:      - name: Register new ReaScripts in reaper-kb.ini
# (Line numbers may shift; the order is what matters.)
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: deploy MGA_JSLimiterST JSFX fork to REAPER Effects (#145)"
```

---

## Task 4: meter_bridge.lua — accumulate per-track active milliseconds + handle reset

**Files:**
- Modify: `scripts/reascripts/meter_bridge.lua`

Add a polling block inside `main()` (which already runs every defer tick). For
every track whose name matches inear AND that has the JS limiter inserted, read
slider5 via `TrackFX_GetParam(track, fx, 4)`. If value < -1.0 dB, accumulate the
elapsed time since the previous tick into a per-track local table. Write all
totals to `EXTSTATE REAPERIEM_LIMITER_ACTIVITY/totals` as `track_idx:active_ms;...`.

Also read `EXTSTATE REAPERIEM_LIMITER_ACTIVITY/reset` each tick — if set to a
track index string, zero that local entry and clear the EXTSTATE key. This is
the round-trip that lets the server-side Reset button actually zero things
durably (otherwise the next poller cycle would re-overwrite the server's zero
with the still-large local total).

- [ ] **Step 1: Add per-track activity state at the top of meter_bridge.lua**

Insert AFTER the existing `local RUNNING_KEY = "bridge_running"` line (around line 22):

```lua

-- Limiter activity tracking (#145)
-- Per-inear-track cumulative active milliseconds where GR < -1.0 dB.
-- Resets only when EXTSTATE REAPERIEM_LIMITER_ACTIVITY/reset is set to that
-- track index (the iem-mixer server writes this in response to the user
-- clicking Reset in the LimiterModal).
local LIMITER_SECTION = "REAPERIEM_LIMITER_ACTIVITY"
local LIMITER_TOTALS_KEY = "totals"
local LIMITER_RESET_KEY = "reset"
local LIMITER_GR_THRESHOLD_DB = -1.0  -- counts as "active" when slider5 < this
local LIMITER_FX_NAME_PATTERN = "MGA_JSLimiter"

-- Per-track active_ms accumulator: map<track_index_1based, integer_ms>
local limiter_active_ms = {}
-- Tick timestamp tracking (so we attribute exact wall delta, not fixed assumed dt)
local last_tick_time = nil
```

Apply with `Edit`:

```
old_string: local RUNNING_KEY = "bridge_running"
new_string: local RUNNING_KEY = "bridge_running"

-- Limiter activity tracking (#145)
-- Per-inear-track cumulative active milliseconds where GR < -1.0 dB.
-- Resets only when EXTSTATE REAPERIEM_LIMITER_ACTIVITY/reset is set to that
-- track index (the iem-mixer server writes this in response to the user
-- clicking Reset in the LimiterModal).
local LIMITER_SECTION = "REAPERIEM_LIMITER_ACTIVITY"
local LIMITER_TOTALS_KEY = "totals"
local LIMITER_RESET_KEY = "reset"
local LIMITER_GR_THRESHOLD_DB = -1.0  -- counts as "active" when slider5 < this
local LIMITER_FX_NAME_PATTERN = "MGA_JSLimiter"

-- Per-track active_ms accumulator: map<track_index_1based, integer_ms>
local limiter_active_ms = {}
-- Tick timestamp tracking (so we attribute exact wall delta, not fixed assumed dt)
local last_tick_time = nil
```

- [ ] **Step 2: Add the polling block inside main()**

The existing `main()` body iterates tracks 0..track_count-1 building the meter
parts table. AFTER the meter loop completes (after `parts[#parts + 1] = ...`
loop ends, before `reaper.SetExtState(SECTION, KEY, ...)`), add the limiter
activity loop. We use `time_precise()` so a defer hiccup is not counted as
activation time.

Insert AFTER the existing `for i = 0, track_count - 1 do ... end` block (the
one that builds `parts`) and BEFORE `reaper.SetExtState(SECTION, KEY, table.concat(parts, ";"), false)`:

```lua

  -- Limiter activity polling (#145).
  -- For every track that has our JS limiter, read slider5 (GR readout in dB,
  -- written by the JSFX from ext_gr_meter via sliderchange()), accumulate
  -- elapsed wall time into limiter_active_ms whenever slider5 < threshold.
  local now = reaper.time_precise()
  local dt_ms = 0
  if last_tick_time then
    dt_ms = math.floor((now - last_tick_time) * 1000.0 + 0.5)
    -- Clamp huge deltas (defer pause, REAPER backgrounded) so a 30 s pause
    -- doesn't show up as 30 s of limiter activity.
    if dt_ms > 250 then dt_ms = 0 end
  end
  last_tick_time = now

  -- Reset request handling — server writes track index here when user clicks Reset.
  local reset_request = reaper.GetExtState(LIMITER_SECTION, LIMITER_RESET_KEY)
  if reset_request ~= "" then
    local reset_idx = tonumber(reset_request)
    if reset_idx then
      limiter_active_ms[reset_idx] = 0
    end
    reaper.SetExtState(LIMITER_SECTION, LIMITER_RESET_KEY, "", false)
  end

  local lim_parts = {}
  for i = 0, track_count - 1 do
    local track = reaper.GetTrack(0, i)
    if track then
      local fx_count = reaper.TrackFX_GetCount(track)
      local fx_idx = -1
      for f = 0, fx_count - 1 do
        local _, fx_name = reaper.TrackFX_GetFXName(track, f)
        if fx_name and fx_name:find(LIMITER_FX_NAME_PATTERN, 1, true) then
          fx_idx = f
          break
        end
      end
      if fx_idx >= 0 then
        local track_idx = i + 1  -- 1-based to match meter convention
        local gr_db = reaper.TrackFX_GetParam(track, fx_idx, 4)  -- slider5
        if dt_ms > 0 and gr_db < LIMITER_GR_THRESHOLD_DB then
          limiter_active_ms[track_idx] = (limiter_active_ms[track_idx] or 0) + dt_ms
        end
        local total = limiter_active_ms[track_idx] or 0
        lim_parts[#lim_parts + 1] = track_idx .. ":" .. total
      end
    end
  end
  reaper.SetExtState(LIMITER_SECTION, LIMITER_TOTALS_KEY, table.concat(lim_parts, ";"), false)
```

Apply with `Edit`. The `old_string` is the existing line that writes the meter
EXTSTATE; `new_string` inserts the limiter block immediately before it:

```
old_string:   reaper.SetExtState(SECTION, KEY, table.concat(parts, ";"), false)

  -- Dynamic script registration via EXTSTATE (no REAPER restart needed)
new_string:
  -- Limiter activity polling (#145).
  -- For every track that has our JS limiter, read slider5 (GR readout in dB,
  -- written by the JSFX from ext_gr_meter via sliderchange()), accumulate
  -- elapsed wall time into limiter_active_ms whenever slider5 < threshold.
  local now = reaper.time_precise()
  local dt_ms = 0
  if last_tick_time then
    dt_ms = math.floor((now - last_tick_time) * 1000.0 + 0.5)
    -- Clamp huge deltas (defer pause, REAPER backgrounded) so a 30 s pause
    -- doesn't show up as 30 s of limiter activity.
    if dt_ms > 250 then dt_ms = 0 end
  end
  last_tick_time = now

  -- Reset request handling — server writes track index here when user clicks Reset.
  local reset_request = reaper.GetExtState(LIMITER_SECTION, LIMITER_RESET_KEY)
  if reset_request ~= "" then
    local reset_idx = tonumber(reset_request)
    if reset_idx then
      limiter_active_ms[reset_idx] = 0
    end
    reaper.SetExtState(LIMITER_SECTION, LIMITER_RESET_KEY, "", false)
  end

  local lim_parts = {}
  for i = 0, track_count - 1 do
    local track = reaper.GetTrack(0, i)
    if track then
      local fx_count = reaper.TrackFX_GetCount(track)
      local fx_idx = -1
      for f = 0, fx_count - 1 do
        local _, fx_name = reaper.TrackFX_GetFXName(track, f)
        if fx_name and fx_name:find(LIMITER_FX_NAME_PATTERN, 1, true) then
          fx_idx = f
          break
        end
      end
      if fx_idx >= 0 then
        local track_idx = i + 1  -- 1-based to match meter convention
        local gr_db = reaper.TrackFX_GetParam(track, fx_idx, 4)  -- slider5
        if dt_ms > 0 and gr_db < LIMITER_GR_THRESHOLD_DB then
          limiter_active_ms[track_idx] = (limiter_active_ms[track_idx] or 0) + dt_ms
        end
        local total = limiter_active_ms[track_idx] or 0
        lim_parts[#lim_parts + 1] = track_idx .. ":" .. total
      end
    end
  end
  reaper.SetExtState(LIMITER_SECTION, LIMITER_TOTALS_KEY, table.concat(lim_parts, ";"), false)

  reaper.SetExtState(SECTION, KEY, table.concat(parts, ";"), false)

  -- Dynamic script registration via EXTSTATE (no REAPER restart needed)
```

- [ ] **Step 3: Verify the additions did not break the existing structure**

```bash
grep -n "REAPERIEM_LIMITER_ACTIVITY\|limiter_active_ms\|main()\|reaper.atexit\|reaper.defer" scripts/reascripts/meter_bridge.lua | head -15
# Expected: see LIMITER_SECTION constant near top, limiter_active_ms init, the
# new for-loop block in main(), and the unchanged main() / atexit / defer lines.
```

- [ ] **Step 4: Commit**

```bash
git add scripts/reascripts/meter_bridge.lua
git commit -m "feat(reascript): meter_bridge accumulates limiter activity per track (#145)"
```

---

## Task 5: Extend ws.rs — LimiterParams.active_seconds + ResetLimiterActivity ClientMsg

**Files:**
- Modify: `iem-mixer/crates/iem-core/src/ws.rs`

Two changes in one type module: extend the existing `ServerMsg::LimiterParams`
variant with `#[serde(default)] active_seconds: f64`, and add a new
`ClientMsg::ResetLimiterActivity { track_index: usize }` variant. Three new
unit tests cover serde roundtrip and backwards compatibility.

- [ ] **Step 1: Write the failing serde test for backwards compatibility (RED)**

Add to the bottom of the existing `mod tests { ... }` block (after the existing
`test_server_msg_limiter_params_serialization` at line 835-849):

```rust
    #[test]
    fn test_server_msg_limiter_params_active_seconds_default() {
        // Older server may emit LimiterParams without active_seconds; new client
        // must still deserialize it with active_seconds = 0.0.
        let json = r#"{"event":"LimiterParams","data":{"track_index":23,"track_name":"PETRONELA inear","limit_db":-6.0,"limit_norm":0.0,"enabled":true}}"#;
        let decoded: ServerMsg = serde_json::from_str(json).unwrap();
        match decoded {
            ServerMsg::LimiterParams { active_seconds, .. } => {
                assert_eq!(active_seconds, 0.0);
            }
            _ => panic!("Expected LimiterParams variant"),
        }
    }

    #[test]
    fn test_server_msg_limiter_params_with_active_seconds() {
        let msg = ServerMsg::LimiterParams {
            track_index: 23,
            track_name: "PETRONELA inear".to_string(),
            limit_db: -6.0,
            limit_norm: 0.0,
            enabled: true,
            active_seconds: 83.5,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"active_seconds\":83.5"));
        let back: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn test_client_msg_reset_limiter_activity_serialization() {
        let msg = ClientMsg::ResetLimiterActivity { track_index: 23 };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"ResetLimiterActivity\""));
        assert!(json.contains("\"track_index\":23"));
        let back: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, back);
    }
```

- [ ] **Step 2: Add the new field to LimiterParams variant**

In the `ServerMsg` enum at line 181-189, modify:

```
old_string:    /// Limiter parameters for a track — single "limit" control (#72)
    LimiterParams {
        track_index: usize,
        track_name: String,
        /// Max output level in dB (-6 to 0)
        limit_db: f32,
        /// Normalized slider position (0-1)
        limit_norm: f32,
        enabled: bool,
    },
new_string:    /// Limiter parameters for a track — single "limit" control (#72)
    LimiterParams {
        track_index: usize,
        track_name: String,
        /// Max output level in dB (-6 to 0)
        limit_db: f32,
        /// Normalized slider position (0-1)
        limit_norm: f32,
        enabled: bool,
        /// Cumulative seconds the limiter has been actively reducing gain
        /// (GR < -1 dB) since the last reset or app restart (#145).
        /// Default 0.0 for backwards compatibility with older servers.
        #[serde(default)]
        active_seconds: f64,
    },
```

- [ ] **Step 3: Add the new ClientMsg variant**

In the `ClientMsg` enum at line 100-101, modify (insert AFTER the existing
`SetLimiterEnabled` variant, BEFORE the closing `}`):

```
old_string:    /// Enable/disable limiter (FX bypass toggle) (#72)
    SetLimiterEnabled { track_index: usize, enabled: bool },
}
new_string:    /// Enable/disable limiter (FX bypass toggle) (#72)
    SetLimiterEnabled { track_index: usize, enabled: bool },
    /// Reset the activity counter for one limiter track (#145).
    /// Server zeros AppState.limiter_activity[track] AND writes
    /// EXTSTATE REAPERIEM_LIMITER_ACTIVITY/reset = "<track_index>"
    /// so meter_bridge.lua zeros its local accumulator (otherwise the
    /// next poller cycle would re-overwrite the server's zero).
    ResetLimiterActivity { track_index: usize },
}
```

- [ ] **Step 4: Update the existing `test_server_msg_limiter_params_serialization` test**

The pre-existing test at line 835-849 constructs `ServerMsg::LimiterParams`
without `active_seconds`. With the new required-but-defaulted field, you must
either set it explicitly or rely on `#[serde(default)]`. Update the test to
construct the value explicitly so the test exercises the new field:

```
old_string:    #[test]
    fn test_server_msg_limiter_params_serialization() {
        let msg = ServerMsg::LimiterParams {
            track_index: 23,
            track_name: "PETRONELA inear".to_string(),
            limit_db: -6.0,
            limit_norm: 0.0,
            enabled: true,
        };
new_string:    #[test]
    fn test_server_msg_limiter_params_serialization() {
        let msg = ServerMsg::LimiterParams {
            track_index: 23,
            track_name: "PETRONELA inear".to_string(),
            limit_db: -6.0,
            limit_norm: 0.0,
            enabled: true,
            active_seconds: 0.0,
        };
```

- [ ] **Step 5: Verify formatting + commit (no local cargo build/test — runs in CI)**

```bash
cargo fmt --all --check  # MUST pass; if it complains, run `cargo fmt --all` and re-stage
git add iem-mixer/crates/iem-core/src/ws.rs
git commit -m "feat(core): LimiterParams.active_seconds + ResetLimiterActivity (#145)"
```

---

## Task 6: AppState — add limiter_activity HashMap

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/lib.rs:99-154` (struct definition)
- Modify: `iem-mixer/crates/iem-server/src/lib.rs:207-249` (`AppState::new` initializer)

The existing struct uses `tokio::sync::Mutex` for limiter locks (lines 151, 153)
but `Arc<RwLock<...>>` for the bigger config / cache state. For a small map
that we read in the WS recv path (sync) and write from the poller (async), an
`Arc<tokio::sync::Mutex<HashMap<usize, u64>>>` matches the existing limiter
mutex flavor and avoids pulling in `parking_lot`.

- [ ] **Step 1: Add the new field to the struct**

In `pub struct AppState { ... }` (line 99-154), add the field at the very end
(after `limiter_read_lock`):

```
old_string:    /// Mutex to serialize limiter EXTSTATE reads (#72)
    pub limiter_read_lock: Arc<tokio::sync::Mutex<()>>,
}
new_string:    /// Mutex to serialize limiter EXTSTATE reads (#72)
    pub limiter_read_lock: Arc<tokio::sync::Mutex<()>>,
    /// Per-inear-track cumulative active milliseconds from the limiter
    /// (#145). Populated by the background poller from EXTSTATE
    /// REAPERIEM_LIMITER_ACTIVITY/totals; zeroed entry-by-entry via the
    /// ClientMsg::ResetLimiterActivity handler.
    pub limiter_activity: Arc<tokio::sync::Mutex<std::collections::HashMap<usize, u64>>>,
}
```

- [ ] **Step 2: Initialize the field in AppState::new**

In the struct literal returned from `AppState::new` (around line 240-248), add
the new field initializer:

```
old_string:            limiter_write_lock: Arc::new(tokio::sync::Mutex::new(())),
            limiter_read_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
}
new_string:            limiter_write_lock: Arc::new(tokio::sync::Mutex::new(())),
            limiter_read_lock: Arc::new(tokio::sync::Mutex::new(())),
            limiter_activity: Arc::new(tokio::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
        }
    }
}
```

- [ ] **Step 3: Verify formatting + commit**

```bash
cargo fmt --all --check
git add iem-mixer/crates/iem-server/src/lib.rs
git commit -m "feat(server): AppState.limiter_activity HashMap (#145)"
```

---

## Task 7: poller.rs — read REAPERIEM_LIMITER_ACTIVITY/totals each cycle + parser unit test

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/poller.rs`

Add a tiny pure parser `parse_limiter_activity_totals(text: &str) -> HashMap<usize, u64>`
near the existing `parse_meter_bridge` function (line 33). Then in the cycle
body, after the existing meter bridge EXTSTATE read (~line 474), read the
limiter activity totals key and replace the AppState HashMap contents.

- [ ] **Step 1: Write the failing parser unit test (RED)**

In the existing test module at the bottom of `poller.rs` (or create one if it
doesn't exist — search for `#[cfg(test)]\nmod tests` first), add:

```rust
    #[test]
    fn parse_limiter_activity_totals_parses_pairs() {
        let text = "23:1230;24:0;25:8470";
        let parsed = super::parse_limiter_activity_totals(text);
        assert_eq!(parsed.get(&23), Some(&1230));
        assert_eq!(parsed.get(&24), Some(&0));
        assert_eq!(parsed.get(&25), Some(&8470));
        assert_eq!(parsed.len(), 3);
    }

    #[test]
    fn parse_limiter_activity_totals_empty_input() {
        assert!(super::parse_limiter_activity_totals("").is_empty());
    }

    #[test]
    fn parse_limiter_activity_totals_skips_malformed_entries() {
        let text = "23:1230;garbage;24:not_a_number;25:9999";
        let parsed = super::parse_limiter_activity_totals(text);
        assert_eq!(parsed.get(&23), Some(&1230));
        assert_eq!(parsed.get(&25), Some(&9999));
        assert_eq!(parsed.len(), 2);
    }
```

If `poller.rs` has no `#[cfg(test)] mod tests` block, add this at the very
bottom of the file:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn parse_limiter_activity_totals_parses_pairs() {
        let text = "23:1230;24:0;25:8470";
        let parsed = super::parse_limiter_activity_totals(text);
        assert_eq!(parsed.get(&23), Some(&1230));
        assert_eq!(parsed.get(&24), Some(&0));
        assert_eq!(parsed.get(&25), Some(&8470));
        assert_eq!(parsed.len(), 3);
    }

    #[test]
    fn parse_limiter_activity_totals_empty_input() {
        assert!(super::parse_limiter_activity_totals("").is_empty());
    }

    #[test]
    fn parse_limiter_activity_totals_skips_malformed_entries() {
        let text = "23:1230;garbage;24:not_a_number;25:9999";
        let parsed = super::parse_limiter_activity_totals(text);
        assert_eq!(parsed.get(&23), Some(&1230));
        assert_eq!(parsed.get(&25), Some(&9999));
        assert_eq!(parsed.len(), 2);
    }
}
```

- [ ] **Step 2: Add the parser implementation**

Add immediately AFTER the existing `pub fn parse_meter_bridge(...)` (which
ends around line 56):

```rust
/// Parse limiter activity EXTSTATE response into per-track active_ms map.
/// Input format: "23:1230;24:0;25:8470" (track_idx_1based:active_ms_u64)
/// Skips malformed entries silently.
pub fn parse_limiter_activity_totals(text: &str) -> HashMap<usize, u64> {
    let mut totals = HashMap::new();
    for entry in text.split(';') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        if let Some((idx_str, ms_str)) = entry.split_once(':')
            && let Ok(track_idx) = idx_str.parse::<usize>()
            && let Ok(ms) = ms_str.parse::<u64>()
        {
            totals.insert(track_idx, ms);
        }
    }
    totals
}
```

- [ ] **Step 3: Add the EXTSTATE read in the poller cycle**

Find the existing block at lines 474-489 (the meter bridge EXTSTATE read inside
`if connected { ... }`). Add a sibling read for limiter activity totals
immediately AFTER that block, BEFORE the closing `}` of the `if connected`
scope.

The existing block ends with:

```rust
        let extstate_url = reaper_api::get_extstate(&reaper_url, "REAPERIEM_METERS", "peaks");
        if let Ok(resp) = state.http_client.get(&extstate_url).send().await
            && let Ok(text) = resp.text().await
        {
            // REAPER EXTSTATE response: "EXTSTATE\tSECTION\tKEY\tvalue"
            if let Some(value) = text.split('\t').nth(3)
                && !value.is_empty()
            {
                let bridge_meters = parse_meter_bridge(value);
                if !bridge_meters.is_empty() {
                    // Override TRACK field meters with true L/R data
                    meters = bridge_meters;
                }
            }
        }
    }
```

Apply with `Edit`:

```
old_string:        let extstate_url = reaper_api::get_extstate(&reaper_url, "REAPERIEM_METERS", "peaks");
        if let Ok(resp) = state.http_client.get(&extstate_url).send().await
            && let Ok(text) = resp.text().await
        {
            // REAPER EXTSTATE response: "EXTSTATE\tSECTION\tKEY\tvalue"
            if let Some(value) = text.split('\t').nth(3)
                && !value.is_empty()
            {
                let bridge_meters = parse_meter_bridge(value);
                if !bridge_meters.is_empty() {
                    // Override TRACK field meters with true L/R data
                    meters = bridge_meters;
                }
            }
        }
    }
new_string:        let extstate_url = reaper_api::get_extstate(&reaper_url, "REAPERIEM_METERS", "peaks");
        if let Ok(resp) = state.http_client.get(&extstate_url).send().await
            && let Ok(text) = resp.text().await
        {
            // REAPER EXTSTATE response: "EXTSTATE\tSECTION\tKEY\tvalue"
            if let Some(value) = text.split('\t').nth(3)
                && !value.is_empty()
            {
                let bridge_meters = parse_meter_bridge(value);
                if !bridge_meters.is_empty() {
                    // Override TRACK field meters with true L/R data
                    meters = bridge_meters;
                }
            }
        }

        // Limiter activity totals (#145) — meter_bridge.lua writes per-track
        // cumulative active milliseconds where the limiter is reducing gain.
        let lim_url = reaper_api::get_extstate(
            &reaper_url,
            "REAPERIEM_LIMITER_ACTIVITY",
            "totals",
        );
        if let Ok(resp) = state.http_client.get(&lim_url).send().await
            && let Ok(text) = resp.text().await
            && let Some(value) = text.split('\t').nth(3)
        {
            let totals = parse_limiter_activity_totals(value);
            let mut guard = state.limiter_activity.lock().await;
            *guard = totals;
        }
    }
```

- [ ] **Step 4: Verify formatting + commit**

```bash
cargo fmt --all --check
git add iem-mixer/crates/iem-server/src/poller.rs
git commit -m "feat(server): poller reads limiter activity totals from EXTSTATE (#145)"
```

---

## Task 8: proxy.rs — handle ResetLimiterActivity command + enrich GetLimiterParams reply

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs:1230-1311` (WS recv loop limiter handlers)
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs:2505-2555` (`handle_get_limiter_params` reply)

Two distinct integrations:

(a) Add `ClientMsg::ResetLimiterActivity` handler in the WS recv loop next to
the existing `GetLimiterParams` / `SetLimiterParam` / `SetLimiterEnabled`
handlers. Reuse the existing `owns_limiter_track` closure for the
member-vs-engineer authorization check. On allow, zero the AppState HashMap
entry AND write `EXTSTATE REAPERIEM_LIMITER_ACTIVITY/reset = "<track_index>"`
so meter_bridge.lua zeros its local accumulator.

(b) In `handle_get_limiter_params`, after building the `LimiterParams` reply
via `parse_limiter_params_response`, replace the constructed value with one
that includes `active_seconds` looked up from `state.limiter_activity[track_index]`.

- [ ] **Step 1: Write the failing serde test for the new ClientMsg already done in Task 5 — confirm it still passes**

The serde roundtrip lives in `iem-core/src/ws.rs`. No new test fixture is needed
here. Move on.

- [ ] **Step 2: Add the `ResetLimiterActivity` handler in the WS recv loop**

Add immediately AFTER the existing `if let iem_core::ClientMsg::SetLimiterEnabled { ... }` block (which ends at line 1310 with `continue;` and `}`):

```
old_string:                            if let iem_core::ClientMsg::SetLimiterEnabled {
                                track_index,
                                enabled,
                            } = cmd
                            {
                                if owns_limiter_track(track_index) {
                                    let state_clone = state.clone();
                                    tokio::spawn(async move {
                                        handle_set_limiter_param(
                                            &state_clone,
                                            track_index,
                                            "enabled",
                                            if enabled { 1.0 } else { 0.0 },
                                        )
                                        .await;
                                    });
                                }
                                continue;
                            }
new_string:                            if let iem_core::ClientMsg::SetLimiterEnabled {
                                track_index,
                                enabled,
                            } = cmd
                            {
                                if owns_limiter_track(track_index) {
                                    let state_clone = state.clone();
                                    tokio::spawn(async move {
                                        handle_set_limiter_param(
                                            &state_clone,
                                            track_index,
                                            "enabled",
                                            if enabled { 1.0 } else { 0.0 },
                                        )
                                        .await;
                                    });
                                }
                                continue;
                            }
                            if let iem_core::ClientMsg::ResetLimiterActivity { track_index } = cmd
                            {
                                if owns_limiter_track(track_index) {
                                    let state_clone = state.clone();
                                    tokio::spawn(async move {
                                        handle_reset_limiter_activity(&state_clone, track_index)
                                            .await;
                                    });
                                }
                                continue;
                            }
```

- [ ] **Step 3: Add the `handle_reset_limiter_activity` function**

Add immediately AFTER the existing `handle_set_limiter_param` function (which
ends around line 2648 with the closing `}`):

```rust
/// Reset the activity counter for one limiter track (#145).
/// Zeros AppState.limiter_activity[track] AND writes
/// EXTSTATE REAPERIEM_LIMITER_ACTIVITY/reset = "<track_index>"
/// so meter_bridge.lua zeros its local accumulator on its next tick
/// (otherwise the next poller cycle would re-overwrite the server zero).
pub async fn handle_reset_limiter_activity(state: &AppState, track_index: usize) {
    {
        let mut guard = state.limiter_activity.lock().await;
        guard.insert(track_index, 0);
    }

    let config = state.config.read().await;
    let reaper_url = config.reaper_url.clone();
    drop(config);

    let set_url = reaper_api::set_extstate(
        &reaper_url,
        "REAPERIEM_LIMITER_ACTIVITY",
        "reset",
        &track_index.to_string(),
    );
    if state.http_client.get(&set_url).send().await.is_err() {
        tracing::error!(
            track_index,
            "Limiter activity reset: failed to write reset EXTSTATE"
        );
    }
}
```

Apply with `Edit` — paste the function immediately AFTER the closing `}` of
`handle_set_limiter_param` and BEFORE the `/// REAPER HTTP API URL builder`
doc comment that introduces `pub(crate) mod reaper_api`:

```
old_string:/// REAPER HTTP API URL builder
/// CRITICAL: All REAPER API commands MUST use the `/_/` prefix!
new_string:/// Reset the activity counter for one limiter track (#145).
/// Zeros AppState.limiter_activity[track] AND writes
/// EXTSTATE REAPERIEM_LIMITER_ACTIVITY/reset = "<track_index>"
/// so meter_bridge.lua zeros its local accumulator on its next tick
/// (otherwise the next poller cycle would re-overwrite the server zero).
pub async fn handle_reset_limiter_activity(state: &AppState, track_index: usize) {
    {
        let mut guard = state.limiter_activity.lock().await;
        guard.insert(track_index, 0);
    }

    let config = state.config.read().await;
    let reaper_url = config.reaper_url.clone();
    drop(config);

    let set_url = reaper_api::set_extstate(
        &reaper_url,
        "REAPERIEM_LIMITER_ACTIVITY",
        "reset",
        &track_index.to_string(),
    );
    if state.http_client.get(&set_url).send().await.is_err() {
        tracing::error!(
            track_index,
            "Limiter activity reset: failed to write reset EXTSTATE"
        );
    }
}

/// REAPER HTTP API URL builder
/// CRITICAL: All REAPER API commands MUST use the `/_/` prefix!
```

- [ ] **Step 4: Enrich `handle_get_limiter_params` with active_seconds**

In `handle_get_limiter_params` (line 2505), replace the bare
`parse_limiter_params_response(track_index, value)` call at the end with a
version that injects `active_seconds`. The parser today returns
`Some(ServerMsg::LimiterParams { ..., enabled, active_seconds: 0.0 })` — we
overwrite `active_seconds` from the AppState HashMap before returning.

```
old_string:    parse_limiter_params_response(track_index, value)
}

/// Parse limiter EXTSTATE response into ServerMsg
new_string:    let mut reply = parse_limiter_params_response(track_index, value)?;
    if let iem_core::ServerMsg::LimiterParams {
        ref mut active_seconds,
        ..
    } = reply
    {
        let guard = state.limiter_activity.lock().await;
        let ms = guard.get(&track_index).copied().unwrap_or(0);
        *active_seconds = (ms as f64) / 1000.0;
    }
    Some(reply)
}

/// Parse limiter EXTSTATE response into ServerMsg
```

- [ ] **Step 5: Update the existing parser to default `active_seconds: 0.0`**

The function `parse_limiter_params_response` at line 2558 builds a `LimiterParams`
in two places (NO_LIMITER branch around line 2562, and the OK branch around line
2596). Both must include `active_seconds: 0.0` after the new field is added to
the variant.

```
old_string:    if value.starts_with("NO_LIMITER:") {
        let track_name = value.strip_prefix("NO_LIMITER:").unwrap_or("").to_string();
        return Some(iem_core::ServerMsg::LimiterParams {
            track_index,
            track_name,
            limit_db: 0.0,
            limit_norm: 0.0,
            enabled: false,
        });
    }
new_string:    if value.starts_with("NO_LIMITER:") {
        let track_name = value.strip_prefix("NO_LIMITER:").unwrap_or("").to_string();
        return Some(iem_core::ServerMsg::LimiterParams {
            track_index,
            track_name,
            limit_db: 0.0,
            limit_norm: 0.0,
            enabled: false,
            active_seconds: 0.0,
        });
    }
```

```
old_string:    Some(iem_core::ServerMsg::LimiterParams {
        track_index,
        track_name,
        limit_db: get_field("limit="),
        limit_norm: get_field("limit_n="),
        enabled: get_field("enabled=") >= 0.5,
    })
}
new_string:    Some(iem_core::ServerMsg::LimiterParams {
        track_index,
        track_name,
        limit_db: get_field("limit="),
        limit_norm: get_field("limit_n="),
        enabled: get_field("enabled=") >= 0.5,
        active_seconds: 0.0,
    })
}
```

- [ ] **Step 6: Verify formatting + commit**

```bash
cargo fmt --all --check
git add iem-mixer/crates/iem-server/src/proxy.rs
git commit -m "feat(server): ResetLimiterActivity handler + enrich LimiterParams (#145)"
```

---

## Task 9: LimiterModal — add active_seconds + Reset button

**Files:**
- Modify: `iem-mixer/iem-ui/src/components/limiter_modal.rs`

Add two new component props (`active_seconds: ReadSignal<f64>` and `on_reset:
Callback<()>`) plus a small `format_active(secs: f64) -> String` helper. Render
a new row between the existing toggle row and the closing `</div>`.

- [ ] **Step 1: Add the helper function**

Insert immediately AFTER the existing `norm_to_db` helper (around line 19),
BEFORE the `LimiterModal` component definition:

```rust
/// Format active seconds for display in the modal: "never" when zero,
/// otherwise "M:SS" with seconds floored. (#145)
fn format_active(secs: f64) -> String {
    if secs <= 0.0 {
        return "never".to_string();
    }
    let total_secs = secs.floor() as u64;
    let m = total_secs / 60;
    let s = total_secs % 60;
    format!("{}:{:02}", m, s)
}

#[cfg(test)]
mod tests {
    use super::format_active;

    #[test]
    fn format_active_zero_is_never() {
        assert_eq!(format_active(0.0), "never");
    }

    #[test]
    fn format_active_negative_is_never() {
        assert_eq!(format_active(-1.0), "never");
    }

    #[test]
    fn format_active_under_one_minute() {
        assert_eq!(format_active(0.5), "0:00");
        assert_eq!(format_active(1.0), "0:01");
        assert_eq!(format_active(59.99), "0:59");
    }

    #[test]
    fn format_active_minutes() {
        assert_eq!(format_active(60.0), "1:00");
        assert_eq!(format_active(83.5), "1:23");
        assert_eq!(format_active(125.0), "2:05");
        assert_eq!(format_active(3661.0), "61:01");
    }
}
```

Apply with `Edit`:

```
old_string:/// Convert norm (0-1) to dB (-6 to 0), handling negative zero
fn norm_to_db(norm: f32) -> f32 {
    let db = norm * 6.0 - 6.0;
    if db == 0.0 { 0.0 } else { db } // eliminate -0.0
}
new_string:/// Convert norm (0-1) to dB (-6 to 0), handling negative zero
fn norm_to_db(norm: f32) -> f32 {
    let db = norm * 6.0 - 6.0;
    if db == 0.0 { 0.0 } else { db } // eliminate -0.0
}

/// Format active seconds for display in the modal: "never" when zero,
/// otherwise "M:SS" with seconds floored. (#145)
fn format_active(secs: f64) -> String {
    if secs <= 0.0 {
        return "never".to_string();
    }
    let total_secs = secs.floor() as u64;
    let m = total_secs / 60;
    let s = total_secs % 60;
    format!("{}:{:02}", m, s)
}

#[cfg(test)]
mod tests {
    use super::format_active;

    #[test]
    fn format_active_zero_is_never() {
        assert_eq!(format_active(0.0), "never");
    }

    #[test]
    fn format_active_negative_is_never() {
        assert_eq!(format_active(-1.0), "never");
    }

    #[test]
    fn format_active_under_one_minute() {
        assert_eq!(format_active(0.5), "0:00");
        assert_eq!(format_active(1.0), "0:01");
        assert_eq!(format_active(59.99), "0:59");
    }

    #[test]
    fn format_active_minutes() {
        assert_eq!(format_active(60.0), "1:00");
        assert_eq!(format_active(83.5), "1:23");
        assert_eq!(format_active(125.0), "2:05");
        assert_eq!(format_active(3661.0), "61:01");
    }
}
```

- [ ] **Step 2: Add the new props on the `LimiterModal` component signature**

In the `#[component] pub fn LimiterModal(...)` signature (line 22-40), add two
new parameters AFTER `loading: ReadSignal<bool>` and BEFORE `on_param_change`:

```
old_string:    /// Whether limiter FX is enabled (not bypassed)
    enabled: ReadSignal<bool>,
    /// Whether data is loading
    loading: ReadSignal<bool>,
    /// Callback when a parameter changes: (param_name, normalized_value)
    on_param_change: Callback<(String, f32)>,
new_string:    /// Whether limiter FX is enabled (not bypassed)
    enabled: ReadSignal<bool>,
    /// Whether data is loading
    loading: ReadSignal<bool>,
    /// Cumulative seconds the limiter has actively reduced gain (#145).
    active_seconds: ReadSignal<f64>,
    /// Callback when a parameter changes: (param_name, normalized_value)
    on_param_change: Callback<(String, f32)>,
    /// Callback when the user clicks the Reset activity button (#145).
    on_reset: Callback<()>,
```

- [ ] **Step 3: Render the new row inside the loaded view**

Inside the `else` branch (the loaded params view, line 56-95), add a new row
AFTER the `<div class="limiter-toggle-row">...</div>` block and BEFORE the
closing `</div>` of `<div class="limiter-params">`:

```
old_string:                                <div class="limiter-toggle-row">
                                    <span>"Limiter"</span>
                                    <button
                                        class=move || {
                                            if enabled.get() {
                                                "limiter-toggle-btn on"
                                            } else {
                                                "limiter-toggle-btn off"
                                            }
                                        }
                                        on:click=move |_| {
                                            on_enabled_change.run(!enabled.get_untracked())
                                        }
                                    >
                                        {move || if enabled.get() { "ON" } else { "OFF" }}
                                    </button>
                                    {move || {
                                        if !enabled.get() {
                                            view! {
                                                <span class="limiter-warning">
                                                    "HEARING PROTECTION OFF"
                                                </span>
                                            }
                                                .into_any()
                                        } else {
                                            view! {}.into_any()
                                        }
                                    }}
                                </div>
                            </div>
new_string:                                <div class="limiter-toggle-row">
                                    <span>"Limiter"</span>
                                    <button
                                        class=move || {
                                            if enabled.get() {
                                                "limiter-toggle-btn on"
                                            } else {
                                                "limiter-toggle-btn off"
                                            }
                                        }
                                        on:click=move |_| {
                                            on_enabled_change.run(!enabled.get_untracked())
                                        }
                                    >
                                        {move || if enabled.get() { "ON" } else { "OFF" }}
                                    </button>
                                    {move || {
                                        if !enabled.get() {
                                            view! {
                                                <span class="limiter-warning">
                                                    "HEARING PROTECTION OFF"
                                                </span>
                                            }
                                                .into_any()
                                        } else {
                                            view! {}.into_any()
                                        }
                                    }}
                                </div>
                                <div class="limiter-activity-row">
                                    <span class="limiter-activity-label">
                                        "Active: "
                                        {move || format_active(active_seconds.get())}
                                    </span>
                                    <button
                                        class="limiter-reset-btn"
                                        on:click=move |_| on_reset.run(())
                                    >
                                        "Reset"
                                    </button>
                                </div>
                            </div>
```

- [ ] **Step 4: Verify formatting + commit**

```bash
cargo fmt --all --check
git add iem-mixer/iem-ui/src/components/limiter_modal.rs
git commit -m "feat(ui): LimiterModal active counter + Reset button (#145)"
```

---

## Task 10: mixer.rs — wire active_seconds signal + on_reset callback end-to-end

**Files:**
- Modify: `iem-mixer/iem-ui/src/pages/mixer.rs`

Add the new signal pair, populate it from the existing `LimiterParams` handler,
extend `connect_websocket`'s parameter list, and pass the signal + callback to
the rendered `LimiterModal`. Two callsites of `connect_websocket` (initial
connect + reconnect closure) must be updated symmetrically.

- [ ] **Step 1: Add the signal pair declaration**

Around line 747, AFTER the existing `let (limiter_loading, set_limiter_loading) = signal(false);`
line, add:

```
old_string:    let (limiter_loading, set_limiter_loading) = signal(false);
new_string:    let (limiter_loading, set_limiter_loading) = signal(false);
    // Limiter activity counter (#145) — cumulative seconds since last reset.
    let (limiter_active_seconds, set_limiter_active_seconds) = signal(0.0_f64);
```

- [ ] **Step 2: Extend `connect_websocket`'s parameter list**

The function signature is around line 70-115 (search for `fn connect_websocket(`).
Add a new parameter `set_limiter_active_seconds: WriteSignal<f64>` immediately
after the existing `set_limiter_loading: WriteSignal<bool>,`:

```
old_string:    // Limiter signals (#72) — single "max level" control
    set_limiter_limit_db: WriteSignal<f32>,
    set_limiter_limit_norm: WriteSignal<f32>,
    set_limiter_enabled: WriteSignal<bool>,
    set_limiter_loading: WriteSignal<bool>,
new_string:    // Limiter signals (#72) — single "max level" control
    set_limiter_limit_db: WriteSignal<f32>,
    set_limiter_limit_norm: WriteSignal<f32>,
    set_limiter_enabled: WriteSignal<bool>,
    set_limiter_loading: WriteSignal<bool>,
    /// Limiter activity counter (#145)
    set_limiter_active_seconds: WriteSignal<f64>,
```

- [ ] **Step 3: Populate the signal in the `LimiterParams` match arm**

The handler is around line 391-402. Update the destructuring AND the body:

```
old_string:                    iem_core::ServerMsg::LimiterParams {
                        track_index: _,
                        track_name: _,
                        limit_db,
                        limit_norm,
                        enabled,
                    } => {
                        let _ = set_limiter_limit_db.try_set(limit_db);
                        let _ = set_limiter_limit_norm.try_set(limit_norm);
                        let _ = set_limiter_enabled.try_set(enabled);
                        let _ = set_limiter_loading.try_set(false);
                    }
new_string:                    iem_core::ServerMsg::LimiterParams {
                        track_index: _,
                        track_name: _,
                        limit_db,
                        limit_norm,
                        enabled,
                        active_seconds,
                    } => {
                        let _ = set_limiter_limit_db.try_set(limit_db);
                        let _ = set_limiter_limit_norm.try_set(limit_norm);
                        let _ = set_limiter_enabled.try_set(enabled);
                        let _ = set_limiter_active_seconds.try_set(active_seconds);
                        let _ = set_limiter_loading.try_set(false);
                    }
```

- [ ] **Step 4: Pass the new signal at BOTH connect_websocket call sites**

The first call site is around line 822-863 (initial connect inside `Effect::new`):

```
old_string:            set_limiter_limit_db,
            set_limiter_limit_norm,
            set_limiter_enabled,
            set_limiter_loading,
            set_alert_data,
            alert_data,
            set_alert_active,
            set_talk_state,
            set_engineer_talking,
            page_visible_effect.clone(),
        );
    });
new_string:            set_limiter_limit_db,
            set_limiter_limit_norm,
            set_limiter_enabled,
            set_limiter_loading,
            set_limiter_active_seconds,
            set_alert_data,
            alert_data,
            set_alert_active,
            set_talk_state,
            set_engineer_talking,
            page_visible_effect.clone(),
        );
    });
```

The second call site is around line 917-958 (reconnect closure). Apply the same
single-line insertion:

```
old_string:                set_limiter_limit_db,
                set_limiter_limit_norm,
                set_limiter_enabled,
                set_limiter_loading,
                set_alert_data,
                alert_data,
                set_alert_active,
                set_talk_state,
                set_engineer_talking,
                page_visible.clone(),
            );
        }
    }) as Box<dyn FnMut()>);
new_string:                set_limiter_limit_db,
                set_limiter_limit_norm,
                set_limiter_enabled,
                set_limiter_loading,
                set_limiter_active_seconds,
                set_alert_data,
                alert_data,
                set_alert_active,
                set_talk_state,
                set_engineer_talking,
                page_visible.clone(),
            );
        }
    }) as Box<dyn FnMut()>);
```

- [ ] **Step 5: Pass active_seconds + on_reset to the rendered LimiterModal**

In the `<Show when=move || limiter_open.get().is_some() ...>` block (around
line 1572-1616), add the two new attributes. The existing block already binds
`ti` from `limiter_open.get_untracked()` inside callbacks — reuse that pattern:

```
old_string:                        <LimiterModal
                            track_name=track_name
                            limit_db=limiter_limit_db
                            limit_norm=limiter_limit_norm
                            enabled=limiter_enabled
                            loading=limiter_loading
                            on_param_change=Callback::new(move |(param, value): (String, f32)| {
new_string:                        <LimiterModal
                            track_name=track_name
                            limit_db=limiter_limit_db
                            limit_norm=limiter_limit_norm
                            enabled=limiter_enabled
                            loading=limiter_loading
                            active_seconds=limiter_active_seconds
                            on_reset=Callback::new(move |_: ()| {
                                if let Some((ti, _)) = limiter_open.get_untracked() {
                                    ws_send(ws_for_lim, &iem_core::ClientMsg::ResetLimiterActivity {
                                        track_index: ti,
                                    });
                                    // Optimistic local update so the user sees "never" immediately.
                                    let _ = set_limiter_active_seconds.try_set(0.0);
                                }
                            })
                            on_param_change=Callback::new(move |(param, value): (String, f32)| {
```

- [ ] **Step 6: Verify formatting + commit**

```bash
cargo fmt --all --check
git add iem-mixer/iem-ui/src/pages/mixer.rs
git commit -m "feat(ui): wire LimiterModal active_seconds + reset to WS (#145)"
```

---

## Task 11: style.css — minor styling for the new row

**Files:**
- Modify: `iem-mixer/iem-ui/style.css`

Add CSS rules for `.limiter-activity-row`, `.limiter-activity-label`, and
`.limiter-reset-btn` matching the existing `.limiter-toggle-row` family.

- [ ] **Step 1: Find the existing limiter modal CSS block**

```bash
grep -n "limiter-toggle-row\|limiter-toggle-btn\|limiter-warning" iem-mixer/iem-ui/style.css | head -10
# Note the line range — the new rules go immediately after that block.
```

- [ ] **Step 2: Append new rules to style.css**

Add the following at the end of the limiter section (immediately after the
last `.limiter-warning {...}` block, or if you can't easily locate it, at the
very end of the file):

```css
.limiter-activity-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 0;
  border-top: 1px solid rgba(255, 255, 255, 0.08);
  margin-top: 6px;
  font-size: 14px;
}

.limiter-activity-label {
  color: #ddd;
}

.limiter-reset-btn {
  background: rgba(255, 255, 255, 0.08);
  color: #fff;
  border: 1px solid rgba(255, 255, 255, 0.2);
  border-radius: 4px;
  padding: 4px 12px;
  font-size: 13px;
  cursor: pointer;
}

.limiter-reset-btn:hover {
  background: rgba(255, 255, 255, 0.15);
}

.limiter-reset-btn:active {
  background: rgba(255, 255, 255, 0.25);
}
```

Use the `Edit` tool to insert at the appropriate location, or `Write` if
appending to a long file.

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/iem-ui/style.css
git commit -m "style: limiter activity row + reset button (#145)"
```

---

## Task 12: Live E2E — limiter-activity.spec.ts

**Files:**
- Create: `iem-mixer/e2e/tests/live/limiter-activity.spec.ts`

End-to-end test that drives a hot signal into a member's mix via the existing
`tone_generator` ReaScript, verifies the `Active:` line in their LimiterModal
shows ≥ 5 s of accumulated activity, clicks Reset, and verifies the counter
goes back to "never". This is the regression test for the entire feature.

- [ ] **Step 1: Confirm the existing tone-generator pattern**

```bash
grep -n "tone_generator\|TONE_GEN\|_RS_REAPERIEM_TONE_GEN" iem-mixer/e2e/tests/live/*.spec.ts | head -10
# Note which existing live spec uses tone_generator and how it triggers/stops it.
# audio-pipeline.spec.ts is the most likely reference; copy that pattern.
```

- [ ] **Step 2: Create the spec file**

Create `iem-mixer/e2e/tests/live/limiter-activity.spec.ts` with:

```typescript
/**
 * Limiter Activity Counter — Issue #145
 *
 * Verifies that the per-track Active counter inside the LimiterModal
 * accumulates time when the safety limiter is reducing gain, and that
 * the Reset button zeros the counter (both server- and ReaScript-side).
 *
 * Requires REAPER on iem.lan with the modified MGA_JSLimiterST exposing
 * slider5 (deployed by CI).  Uses the tone_generator ReaScript to drive
 * a hot signal into the engineer's mix bus, which then exceeds the
 * limiter ceiling (-6 dBFS by default) on the engineer's inear track.
 */

import { test, expect, Page } from "@playwright/test";

const REAPER_URL = "http://iem.lan:8080";
const TONE_GEN_ACTION = "_RS_REAPERIEM_TONE_GEN";

async function loginAsEngineer(page: Page) {
  const response = await page.request.post("/api/auth", {
    data: { member: "engineer", pin: "1177" },
  });
  expect(response.status()).toBe(200);
  const data = await response.json();
  await page.evaluate(
    ({ token, member, engineer }) => {
      localStorage.setItem(
        "iem_token",
        JSON.stringify({ token, member, engineer }),
      );
    },
    { token: data.token, member: data.member, engineer: data.engineer },
  );
}

async function triggerToneGenerator(page: Page) {
  // Toggles the test tone on the engineer track. First call = on, second = off.
  await page.request.get(`${REAPER_URL}/_/${TONE_GEN_ACTION}`).catch(() => {});
}

async function readActiveText(page: Page): Promise<string> {
  const label = page.locator(".limiter-activity-label");
  await expect(label).toBeVisible({ timeout: 5000 });
  return (await label.innerText()).trim();
}

function parseActiveSeconds(text: string): number {
  // Format: "Active: never" or "Active: M:SS"
  const stripped = text.replace(/^Active:\s*/, "").trim();
  if (stripped === "never") return 0;
  const match = stripped.match(/^(\d+):(\d{2})$/);
  if (!match) {
    throw new Error(`Unparseable active text: '${text}'`);
  }
  return parseInt(match[1], 10) * 60 + parseInt(match[2], 10);
}

test.describe("Limiter Activity Counter — Issue #145", () => {
  const consoleMessages: string[] = [];

  test.beforeEach(async ({ page }) => {
    consoleMessages.length = 0;
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        if (msg.text().includes("subscribe await failed")) return;
        if (msg.text().includes("Push API in incognito")) return;
        if (msg.text().includes("vapid-key fetch error")) return;
        if (msg.text().includes("navigator.vibrate")) return;
        if (msg.text().includes("closure invoked recursively")) return;
        if (msg.text().includes("[vite]")) return;
        if (msg.text().includes("favicon")) return;
        if (msg.text().includes("integrity")) return;
        if (msg.text().includes("WebSocket connection")) return;
        consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
      }
    });
  });

  test.afterEach(async () => {
    expect(consoleMessages).toEqual([]);
  });

  test("counter accumulates while limiter is reducing gain, Reset zeros it", async ({
    page,
  }) => {
    test.setTimeout(60_000);

    // Login + navigate to engineer's own mixer
    await page.goto("/");
    await loginAsEngineer(page);
    await page.goto("/engineer");
    await expect(page.locator(".mixer-header").first()).toBeVisible({
      timeout: 10_000,
    });

    // Make sure no test tone is leaking from a previous run; toggle off-then-on
    // to ensure we start in a known state. The TONE_GEN action toggles, so we
    // call once to whatever the current state was, wait, then trigger again to
    // explicitly turn it ON.
    await triggerToneGenerator(page);
    await page.waitForTimeout(800);

    // Find the LIM button on the engineer's own channel strip.
    // ENGINEER inear is the engineer's own output bus; the modal we open is
    // for that track.  We open the FIRST limiter button on the page, which
    // for the engineer's mixer is their own.
    const limitBtn = page.locator(".limiter-btn-small").first();
    await expect(limitBtn).toBeVisible({ timeout: 10_000 });

    // Reset first so we measure ONLY this test's accumulation.
    await limitBtn.click();
    await expect(page.locator(".limiter-modal")).toBeVisible({ timeout: 5000 });
    const resetBtnPre = page.locator(".limiter-reset-btn");
    await expect(resetBtnPre).toBeVisible();
    await resetBtnPre.click();
    // Close + reopen so we re-fetch active_seconds from the server.
    await page.locator(".limiter-close-btn").click();
    await expect(page.locator(".limiter-modal")).not.toBeVisible({
      timeout: 2000,
    });

    // Now turn the tone ON (it was a no-op previously if it was already off).
    await triggerToneGenerator(page);

    // Hold the hot signal long enough for the limiter to engage and accumulate
    // measurable active time.  meter_bridge polls per defer tick (~30 ms);
    // 6 s of audible signal should produce well over 5 s of accumulated activity.
    await page.waitForTimeout(6000);

    // Open the modal again and read the counter
    await limitBtn.click();
    await expect(page.locator(".limiter-modal")).toBeVisible({ timeout: 5000 });
    const activeText = await readActiveText(page);
    const activeSecs = parseActiveSeconds(activeText);
    expect(
      activeSecs,
      `Expected >= 5 s of limiter activity after a 6 s hot tone, got '${activeText}'`,
    ).toBeGreaterThanOrEqual(5);

    // Reset
    const resetBtn = page.locator(".limiter-reset-btn");
    await resetBtn.click();

    // Stop the tone before the next assertion, otherwise meter_bridge will
    // immediately accumulate again on the next tick and the counter will not
    // remain at zero.
    await triggerToneGenerator(page);
    await page.waitForTimeout(800);

    // Close + reopen modal to re-fetch active_seconds from the server.
    await page.locator(".limiter-close-btn").click();
    await expect(page.locator(".limiter-modal")).not.toBeVisible({
      timeout: 2000,
    });
    await limitBtn.click();
    await expect(page.locator(".limiter-modal")).toBeVisible({ timeout: 5000 });

    const afterReset = await readActiveText(page);
    expect(
      parseActiveSeconds(afterReset),
      `After Reset + tone-off + reopen, expected 'Active: never' or 'Active: 0:00'-'0:01', got '${afterReset}'`,
    ).toBeLessThanOrEqual(1);

    // Cleanly close before afterEach runs the console-error check.
    await page.locator(".limiter-close-btn").click();
  });
});
```

- [ ] **Step 3: Verify the file was created**

```bash
ls -la iem-mixer/e2e/tests/live/limiter-activity.spec.ts
wc -l iem-mixer/e2e/tests/live/limiter-activity.spec.ts
# Should exist, ~150 lines.
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/e2e/tests/live/limiter-activity.spec.ts
git commit -m "test(e2e): live limiter activity counter spec (#145)"
```

---

## Task 13: README changelog

**Files:**
- Modify: `README.md`

Per project CLAUDE.md, the changelog MUST be updated for every user-facing
change. Add a v1.149.0 entry.

- [ ] **Step 1: Locate the changelog block**

```bash
grep -n "^### v1.148\|^### v1.149\|^## Changelog" README.md | head -5
# The new entry goes immediately after the "## Changelog" header line, BEFORE
# the existing "### v1.148.0" entry.
```

- [ ] **Step 2: Insert the new entry**

Replace the existing "### v1.148.0" header with the new entry placed above it:

```
old_string: ### v1.148.0
new_string: ### v1.149.0 (2026-04-13)

- **Feature**: Per-inear-track limiter activation counter (#145). Open the LIM dialog on any channel to see how long that inear's safety limiter has been actively reducing gain since the last reset, plus a Reset button to zero it. Visible to engineer (any track) and to band members on their own track.
- **Note**: Existing limiter instances pick up the new GR readout on next REAPER FX reload (next REAPER restart or project reload).

### v1.148.0
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: changelog entry for v1.149.0 limiter activity counter (#145)"
```

---

## Task 14: Push, monitor CI, fix forward if needed

- [ ] **Step 1: Run local format check (only allowed local check)**

```bash
cd iem-mixer && cargo fmt --all --check
cd ..
# Expected: clean exit. If it fails, run `cargo fmt --all`, re-stage, amend the
# affected commit (NOT the version bump — make a fix-fmt commit if needed).
```

- [ ] **Step 2: Push**

```bash
git push origin dev
```

- [ ] **Step 3: Monitor CI to terminal state**

Find the latest run:

```bash
gh run list --branch dev --limit 3
```

Watch it (background, single sleep — never use `gh run watch`):

```bash
RUN_ID=$(gh run list --branch dev --limit 1 --json databaseId --jq '.[0].databaseId')
echo "Monitoring run $RUN_ID"
# Use `gh run view $RUN_ID` to check status. ALL 10 jobs (lint, test,
# build-wasm, e2e CI, build-tauri, test-integrity, deploy to iem.lan,
# post-deploy E2E, etc.) MUST reach success.
```

- [ ] **Step 4: If any job fails, investigate and fix in ONE follow-up commit**

```bash
gh run view $RUN_ID --log-failed | head -200
# Diagnose. Common failure modes for this PR:
# - cargo fmt: re-format and commit
# - Backwards-compat test in iem-core: confirm #[serde(default)] is on active_seconds
# - Limiter activity E2E: tone_generator may need different action handling;
#   check the action exists with `curl http://iem.lan:8080/_/_RS_REAPERIEM_TONE_GEN`
# - Post-deploy: check `curl http://10.77.9.231/api/version` reports 1.149.0
# - Limiter E2E times out reading slider5: existing limiter instances need
#   REAPER FX reload — manually trigger setup_output_limiter on iem.lan to
#   re-insert the FX and pick up slider5 (this is documented as the v1.149.0
#   one-time degradation in the README changelog).
# Fix all issues in one commit:
git add <fixes>
git commit -m "fix: <describe what was broken and how>"
git push origin dev
# Re-monitor.
```

---

## Task 15: Open PR `dev → main`, verify mergeable, present URL, STOP

- [ ] **Step 1: Confirm dev is fully ahead of main with green CI**

```bash
gh run list --branch dev --limit 3
# Latest run on dev MUST be success.

git fetch origin
git log --oneline origin/main..origin/dev
# Should list every commit added in this plan: version bump, JSFX fork,
# CI deploy step, meter_bridge update, ws.rs, AppState, poller, proxy,
# LimiterModal, mixer.rs, style.css, E2E spec, README changelog.
```

- [ ] **Step 2: Open the PR**

```bash
gh pr create --base main --head dev --title "feat: limiter activation counter (#145)" --body "$(cat <<'EOF'
## Summary
- Adds a per-inear-track active-time counter and Reset button inside the existing LimiterModal.
- Forks `MGA_JSLimiterST` to expose its existing internal `gr_meter` via a read-only slider5, which `meter_bridge.lua` polls every defer tick to accumulate active milliseconds whenever GR > 1 dB.
- Server poller reads totals from a single EXTSTATE key into an in-memory `HashMap<usize, u64>` on `AppState`.
- `ServerMsg::LimiterParams` extended with `active_seconds: f64` (default 0.0 for stale-PWA backwards compat). New `ClientMsg::ResetLimiterActivity { track_index }` zeros both server HashMap and ReaScript-side accumulator (via EXTSTATE round-trip).
- Counter is visible to whoever can open the LimiterModal: member for own track, engineer for any track.
- v1.149.0.

## Test plan
- [x] Unit: `iem-core::test_server_msg_limiter_params_active_seconds_default` — older server JSON without `active_seconds` deserializes with default 0.0.
- [x] Unit: `iem-core::test_server_msg_limiter_params_with_active_seconds` — full roundtrip including the new field.
- [x] Unit: `iem-core::test_client_msg_reset_limiter_activity_serialization` — ResetLimiterActivity serde roundtrip.
- [x] Unit: `iem-server::poller::tests::parse_limiter_activity_totals_*` — three tests cover happy path, empty, and malformed input.
- [x] Unit: `iem-ui::components::limiter_modal::tests::format_active_*` — four tests cover zero, negative, sub-minute, and minute-spanning formatting.
- [x] Live E2E: `iem-mixer/e2e/tests/live/limiter-activity.spec.ts` — drives 6 s of hot tone via `_RS_REAPERIEM_TONE_GEN` against the real iem.lan REAPER, asserts Active counter ≥ 5 s, clicks Reset, asserts counter back to ≤ 1 s.
- [x] CI: all 10 jobs green including Deploy to iem.lan and post-deploy E2E.
- [x] Manual post-deploy: open the LIM modal on a member's mixer, observe the new "Active: ..." row.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2.5: Verify PR is mergeable (no conflicts, all checks green)**

```bash
PR_NUM=$(gh pr list --base main --head dev --json number --jq '.[0].number')
gh api repos/zbynekdrlik/reaperiem/pulls/$PR_NUM \
  --jq '{mergeable: .mergeable, mergeable_state: .mergeable_state}'
# Required for green-light: mergeable: true, mergeable_state: "clean"
# If "behind", `git fetch origin && git push origin dev` after rebasing or merging main.
# If "blocked" or "dirty", investigate the failing check or conflict.
```

- [ ] **Step 3: Present the URL and STOP**

Output the PR URL to the user along with:
- Latest run ID and its `success` status
- Confirmation that `mergeable_state == "clean"`
- A note that you are NOT merging — waiting for explicit user instruction.

DO NOT call `gh pr merge`. The airuleset rule is absolute: explicit user
"merge it" / "approved" / "go ahead" is required.

---

## Task dependencies

```
T1 (version bump) → MUST be first commit
  ↓
T2 (JSFX fork) ──┐
T3 (CI deploy) ──┤  Independent — order T2 before T3 because T3 references the file path
T4 (meter_bridge) ──┐
T5 (ws.rs) ─────────┤
T6 (AppState) ──────┤
T7 (poller) — depends on T5 (uses HashMap field name) and T6 (state.limiter_activity)
T8 (proxy) — depends on T5 (ResetLimiterActivity variant) and T6 (state.limiter_activity)
T9 (LimiterModal) — depends on T5 (ServerMsg field for type checking)
T10 (mixer.rs) — depends on T5, T9
T11 (style.css) — independent
T12 (E2E spec) — should be added before push so CI exercises it
T13 (README) — independent
  ↓
T14 (push + monitor CI) — depends on all of the above
  ↓
T15 (PR + STOP) — depends on T14
```

Tasks T2 through T13 may run in any order once T1 is on disk; subagent driver
should dispatch them sequentially to avoid Cargo.toml lockfile contention.

---

## Verification (after CI is green)

1. **All CI jobs pass** including Deploy to iem.lan and post-deploy E2E
2. **Post-deploy `/api/version`** returns `1.149.0`
3. **Limiter modal** opens on a real member's mixer and shows the "Active:" row
4. **PR `dev → main`** is mergeable and clean
5. **Plan-fulfillment audit:** every `[ ]` checkbox in this plan is `[x]`
