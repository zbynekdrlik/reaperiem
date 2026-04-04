# IEM Output Limiter (Hearing Protection) — Design Spec

**Issue:** #72 (sub-issue 1 of 3: Limiter → Compression → Reverb)

**Goal:** Add a brick-wall limiter (ReaLimit) on every member's IEM output bus to protect against sudden loud transients (dropped mics, feedback, accidental volume spikes). Controllable by engineers via the mixer UI.

**Why:** Without a limiter, a single transient event goes directly into a member's ear canal at full force. This can cause permanent hearing damage in one incident. Every professional IEM rig has a brick-wall limiter on every output — it is a safety requirement, not an optional feature.

---

## Architecture

Follows the existing EQ pattern: ReaScripts communicate via EXTSTATE, the Axum server proxies between WebSocket and REAPER, and the Leptos frontend provides real-time controls.

```
ReaScripts (Lua)  ←→  EXTSTATE  ←→  Axum Server (proxy.rs)  ←→  WebSocket  ←→  Leptos UI
```

### Signal Chain Position

ReaLimit is inserted as the **last FX** on each output track (after any existing EQ or other processing). This is non-negotiable — the limiter must be the final stage before audio reaches the member's ears.

```
[Input Tracks] → [TRIM IN] → [ReaEQ] → [Sends] → [Output Bus] → [Bus EQ] → [ReaLimit] → Dante Output
```

### Target Tracks

The 11 output tracks (member IEM buses):
- PETRONELA inear, STEVO inear, MAREK inear, ZUZKA inear, TINA inear
- MIREC inear, ALEX inear, PATRIKA inear, ANI inear
- ENGINEER inear, TRANSLATOR inear

Input tracks do NOT get limiters — limiting happens at the output stage only.

---

## ReaLimit Parameters

ReaLimit is REAPER's built-in brickwall limiter. It has a small parameter set:

| Parameter | User Range | Default | Description |
|-----------|-----------|---------|-------------|
| **Threshold** | -30 to 0 dB | -12 dB | Level where limiting begins (gain reduction starts here) |
| **Ceiling** | -20 to 0 dB | -6 dB | Absolute maximum output level — nothing passes above this |
| **Release** | 1–500 ms | 50 ms | How quickly the limiter stops reducing gain after a peak |
| **Enabled** | on/off | on | FX bypass toggle |

**Fixed (not user-adjustable):** Attack is effectively 0 ms (brickwall behavior). Lookahead is enabled if available. These are not exposed in the UI to prevent members or engineers from accidentally weakening protection.

**Parameter discovery:** The setup ReaScript will enumerate ReaLimit's parameters by name using `TrackFX_GetParamName()` and map them to indices dynamically, rather than hardcoding indices. This is the same approach used for ReaEQ.

---

## ReaScripts

### `setup_output_limiter.lua`

- **Trigger:** `_RS_REAPERIEM_SETUP_LIMITER`
- **Action:** Insert ReaLimit as the last FX on each output track (tracks with "inear" in name)
- **Idempotent:** Skip tracks that already have a "LIMITER" FX
- **Rename:** FX is renamed to "LIMITER" for consistent identification
- **Defaults:** Ceiling = -6 dB, Threshold = -12 dB, Release = 50 ms
- **Result:** Writes count to EXTSTATE `reaperiem/limiter_setup_result` (e.g., "OK:11")

### `read_limiter_params.lua`

- **Trigger:** `_RS_REAPERIEM_READ_LIMITER`
- **Input:** EXTSTATE `reaperiem/limiter_read_track` = track index
- **Action:** Find "LIMITER" FX on track, read all parameter values
- **Output:** EXTSTATE `reaperiem/limiter_params` with format:
  ```
  OK:track=N,name=TRACKNAME,fx=IDX|threshold=-12.0,ceiling=-6.0,release=50.0,enabled=1
  ```
- **Error:** `NO_LIMITER` if track has no ReaLimit FX

### `set_limiter_param.lua`

- **Trigger:** `_RS_REAPERIEM_SET_LIMITER`
- **Input:** EXTSTATE `reaperiem/limiter_set` = `track=N|param=threshold|value=0.4`
- **Action:** Find "LIMITER" FX, set the specified parameter
- **Output:** EXTSTATE `reaperiem/limiter_set_result` = `OK` or error

---

## WebSocket Messages

### Client → Server

```rust
ClientMsg::GetLimiterParams { track_index: usize }
ClientMsg::SetLimiterParam {
    track_index: usize,
    param: String,      // "threshold", "ceiling", "release"
    value: f32,         // normalized 0-1
}
ClientMsg::SetLimiterEnabled {
    track_index: usize,
    enabled: bool,
}
```

### Server → Client

```rust
ServerMsg::LimiterParams {
    track_index: usize,
    track_name: String,
    threshold_db: f32,
    ceiling_db: f32,
    release_ms: f32,
    enabled: bool,
}
```

No `LimiterParamsMulti` — unlike EQ (which has multi-band batch reads for presets), limiter is simple enough to read one track at a time.

---

## Server Handlers (proxy.rs)

### `handle_get_limiter_params()`

1. Acquire `eq_read_lock` (shared lock for REAPER EXTSTATE access)
2. Set EXTSTATE: `reaperiem/limiter_read_track` = track_index
3. Trigger action: `_RS_REAPERIEM_READ_LIMITER`
4. Sleep 300 ms
5. Read EXTSTATE: `reaperiem/limiter_params`
6. Parse response into `ServerMsg::LimiterParams`

### `handle_set_limiter_param()`

1. Acquire `eq_write_lock` (serializes all FX parameter writes)
2. Format: `track=N|param=P|value=V`
3. Set EXTSTATE: `reaperiem/limiter_set`
4. Trigger action: `_RS_REAPERIEM_SET_LIMITER`
5. Sleep 50 ms

### `handle_set_limiter_enabled()`

Same as above but sets the FX bypass state via `TrackFX_SetEnabled()`.

---

## Frontend UI

### Limiter Status Indicator (mixer header bar)

- **Shield icon** on the mixer page header, next to existing controls
- **Green** = limiter active and healthy
- **Red/warning** = limiter bypassed or missing (should almost never happen)
- Visible to ALL users (members and engineers)
- Tapping the shield opens the limiter panel (engineers only)

### Limiter Panel (engineer-only)

A compact panel (not a full modal — simpler than EQ) with:

- **Ceiling slider:** -20 to 0 dB (the hard maximum)
- **Threshold slider:** -30 to 0 dB (where limiting starts)
- **Release slider:** 1–500 ms
- **Enable/disable toggle** with prominent warning when disabled
- **Gain reduction indicator:** Shows real-time GR when the limiter is actively working (read from REAPER via the existing poller or on-demand)
- **Reset to defaults** button

### Access Control

- **Engineers:** Full control — can adjust all parameters on any member's output
- **Members:** Read-only — see the shield icon (green = protected), cannot modify settings
- The limiter is a safety device. Members shouldn't be able to weaken their own hearing protection.

---

## Setup & Deployment

### Initial Setup

The `setup_output_limiter.lua` script is deployed via CI and registered dynamically via meter_bridge (no REAPER restart needed). It runs once to insert ReaLimit on all 11 output tracks.

CI deploy step (added to existing deploy workflow):
1. Deploy script to `REAPER/Scripts/reaperiem/setup_output_limiter.lua`
2. Register dynamically: `SET/EXTSTATE/reaperiem/register_scripts/setup_output_limiter.lua|read_limiter_params.lua|set_limiter_param.lua`
3. Trigger setup: `_RS_REAPERIEM_SETUP_LIMITER`
4. Verify: Read EXTSTATE result

### Persistence

ReaLimit FX state persists in the REAPER project file (.RPP). Once inserted and configured, it survives REAPER restarts. The setup script is idempotent — safe to run on every deploy.

---

## Testing

### Unit Tests (Rust)

- Parse limiter EXTSTATE response format
- WebSocket message serialization/deserialization for new types
- Parameter validation (threshold within range, etc.)

### E2E Tests (Playwright)

- Limiter shield icon visible on mixer page
- Engineer can open limiter panel and adjust threshold
- Member cannot open limiter panel (read-only shield)
- Limiter parameters persist after page reload (read back from REAPER)
- API-level: GET/SET limiter params via WebSocket

### Integration Tests (against real REAPER)

- Setup script inserts ReaLimit on all output tracks
- Read back parameters match defaults
- Set parameter and read back confirms change
- Bypass toggle works

---

## Files Changed

| File | Change |
|------|--------|
| `scripts/reascripts/setup_output_limiter.lua` | **New** — Insert ReaLimit on output tracks |
| `scripts/reascripts/read_limiter_params.lua` | **New** — Read limiter state via EXTSTATE |
| `scripts/reascripts/set_limiter_param.lua` | **New** — Set limiter param via EXTSTATE |
| `iem-mixer/crates/iem-core/src/ws.rs` | Modify — Add limiter WS message types |
| `iem-mixer/crates/iem-server/src/proxy.rs` | Modify — Add limiter handlers |
| `iem-mixer/iem-ui/src/components/limiter_panel.rs` | **New** — Limiter UI component |
| `iem-mixer/iem-ui/src/pages/mixer.rs` | Modify — Wire limiter panel + shield icon |
| `iem-mixer/iem-ui/style.css` | Modify — Limiter panel styles |
| `iem-mixer/e2e/tests/limiter.spec.ts` | **New** — E2E tests |
| `config/reaper_config.yaml` | Modify — Register action IDs |

---

## Out of Scope

- **Compression on vocal channels** — separate sub-issue of #72
- **Shared reverb bus** — separate sub-issue of #72
- **Per-member limiter presets** — not needed; limiter settings should be consistent across all members
- **Gain reduction metering in the poller** — nice-to-have for future; initially read on-demand when panel is open
