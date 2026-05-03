# EQ UI gain mismatch (#194) — design

## Problem

EQ panel slider position derives from REAPER's normalized 0–1 value through UI-side approximation curve `norm_to_gain_db()` (`iem-mixer/iem-ui/src/components/eq_modal.rs:87`). Gain text label reads REAPER's actual formatted dB (`gain_db`). Two formulas disagree — slider thumb sits at one tick, text says another. After user nudge, `on_change` rewrites `gain_db_sig` from slider position, then server roundtrip overwrites with REAPER truth → visual "snaps" to correct dB. Close + reopen → mismatch returns.

User-observed symptom (MIREC): open EQ → text shows 1 dB while REAPER has 4 dB; slight nudge → jumps to 4 (correct); close + reopen → 1 again; REAPER still 4 throughout.

Root cause: **dual sources of truth** — UI's approximation curve vs REAPER's actual mapping.

## Architecture (after fix)

**Single source of truth: REAPER's formatted dB value.**

### Data model

`EqBand` adds `gain_db_min: f32` and `gain_db_max: f32` populated from REAPER via `TrackFX_FormatParamValueNormalized(track, fx, param, 0.0)` and `(.., 1.0)`. These are the parameter's true dB endpoints, sampled without mutating REAPER state.

`gain_norm` stays in the data model but becomes server-internal — UI rendering does not consume it.

### Render

```
slider_position = (gain_db − gain_db_min) / (gain_db_max − gain_db_min)
text            = format!("{:+.1} dB", gain_db)
```

Both derive from `gain_db_sig`. Always agree on initial render and across re-mounts.

### Send (user moves slider)

```
desired_db = gain_db_min + slider_position × (gain_db_max − gain_db_min)
emit SetEqBand { band, param: "gain_db", value: desired_db }
```

Server `set_eq_param.lua` receives `desired_db`. Builds 21-point lookup using `TrackFX_FormatParamValueNormalized` (samples at norm = 0.00, 0.05, …, 1.00 — no mutation). Linear-interpolates to find norm for `desired_db`. Calls `TrackFX_SetParam(track, fx_idx, param_idx, norm)`. Read-back via existing path returns REAPER's exact post-set state to UI.

### Backwards compatibility

- New `param=gain_db` ReaScript branch is parallel to existing `param=gain` (norm). ReaScript handles both. Old call sites continue to function.
- New `gd_min` / `gd_max` Lua response fields are additive. Server's `parse_eq_band` (`proxy.rs:2532`) defaults to existing approximation when fields missing.

## Files changed

| File | Change |
|---|---|
| `scripts/reascripts/read_eq_params.lua` | Per band: add `gd_min=` and `gd_max=` to response via `FormatParamValueNormalized(track, eq_idx, gain_idx, 0.0/1.0)` — no mutation |
| `scripts/reascripts/set_eq_param.lua` | New `param=gain_db` branch: build 21-point lookup, linear-interpolate norm for desired dB, `TrackFX_SetParam` |
| `iem-mixer/crates/iem-core/src/ws.rs` | `EqBand` adds `gain_db_min: f32, gain_db_max: f32` (Default impl supplies fallback ±12 for legacy snapshots) |
| `iem-mixer/crates/iem-server/src/proxy.rs` | `parse_eq_band` extracts `gd_min` / `gd_max` fields; defaults to ±12 if absent |
| `iem-mixer/iem-ui/src/components/eq_modal.rs` | Slider value derives from `gain_db_sig` + bounds (gain_db_min/max). `on_change` computes `desired_db` from slider position and emits `param=gain_db`. Remove `norm_to_gain_db` from render path. Keep `gain_db_to_norm` only as dead-code-tagged test fixture or delete entirely. |
| `iem-mixer/crates/iem-server/src/poller.rs` (or `connection.rs`) | None expected — only `EqParams` payload changes |
| `iem-mixer/iem-ui/src/pages/mixer/connection.rs` | Plumb `gain_db_min` / `gain_db_max` from WS `EqParams` into `EqBandState` |
| `iem-mixer/iem-ui/src/components/eq_modal.rs` (`EqBandState`) | Add `gain_db_min: f32, gain_db_max: f32` fields. `BandLocalState` adds matching `RwSignal`s populated in init Effect (line 458) and synced in subsequent Effect (line 482). |
| `iem-mixer/iem-ui/src/pages/mixer/handlers.rs` | `SetEqBand` emit path: when `param == "gain_db"`, value is dB (not norm). Existing `param == "gain"` (norm) path stays for backwards compat |
| `iem-mixer/e2e/tests/live/eq.spec.ts` | New test: pre-set engineer-track band gain to +4 dB via curl; open EQ as engineer; assert text "+4.0 dB" AND slider thumb pixel position matches `(4 − db_min) / (db_max − db_min) × track_width` ±2 px; close; reopen; re-assert. Cleanup: restore band to original. |

## Risk: existing user EQ corruption

**Zero — by construction.**

- Render-only changes (slider derive formula) don't write to REAPER. No mutation on open / display / close.
- Send path becomes more accurate (REAPER's own mapping replaces UI's approximation). A user "+4 dB" gesture lands at exactly +4 dB instead of approximately.
- ReaScript's new `param=gain_db` branch samples REAPER state via `FormatParamValueNormalized` — pure read, no `SetParam`.
- Test runs on engineer track only (per `feedback_live_test_safety.md`) and restores original state in `finally`.

## Out of scope

Same dual-source pattern likely affects FREQ and BW sliders. Not user-reported. Same fix template applies but multiplies test work. File as separate issue if reported.

## Bug-fix TDD order (per airuleset)

1. **Write failing Playwright E2E** (live REAPER, engineer-track only). Set engineer band to +4 dB, open EQ, capture slider thumb pixel position, assert match against expected. FAILS today.
2. **Apply fix layer by layer**: ReaScript (read + set) → server (parse) → core types → UI (state, render, send). Each layer one commit.
3. **Run E2E — passes.** Close + reopen → assertions still hold.
4. **Cleanup**: restore engineer track state in test `finally`.

## Testing plan

### Live E2E (deploy runner — production-safe)

- `eq.spec.ts` new test:
  - Pre-condition: REAPER engineer track band 0 (highpass) at gain norm = X (will compute to e.g. +4 dB)
  - Open EQ panel as engineer, locate gain slider for band 0
  - Read text label → assert matches `+4.0 dB` (±0.1)
  - Read slider thumb computed CSS position → compare to expected `(4 − db_min)/(db_max − db_min) × width`
  - Close panel
  - Reopen panel
  - Re-assert text + thumb position
  - `finally`: restore engineer band gain to original norm

### Unit tests

- `parse_eq_band` accepts `gd_min` / `gd_max` fields, defaults to ±12 if absent (covers backwards compat with stored snapshots)
- `EqBand` serde roundtrip with new fields

### Mutation testing

Existing `cargo-mutants --in-diff` gate covers any new helpers in `parse_eq_band`. Specific mutation targets: the `gd_min` / `gd_max` field-extraction `unwrap_or` defaults — write boundary-asserting unit tests that distinguish 12 vs other values.

## Verification

After deploy:

1. Open dashboard at https://iem.newlevel.media/ as engineer
2. Open EQ for engineer track
3. Read text + slider position → must agree
4. Move slider to +6 dB → REAPER band gain reads +6 dB (verify via curl `GET/TRACK/N/FX/M/PARAM/...`)
5. Close + reopen → values persist exactly

Completion-report `✅ Deploy:` line includes confirmation that EQ slider position matches text on initial render after deploy.
