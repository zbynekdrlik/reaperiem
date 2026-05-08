# EQ Freq + BW Drift Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate freq + bw drift on EQ modal close+reopen by making REAPER the single source of truth, mirroring the #194 gain fix on the freq and bw dimensions.

**Architecture:** UI sends desired Hz/oct directly to ReaScript. ReaScript samples REAPER's actual norm↔value mapping at 21 points and interpolates (log-space for Hz, linear for oct) to find the norm that produces the requested value. UI displays REAPER's truth on every read. UI's own approximation tables (`norm_to_freq_hz`, `norm_to_bw`) are deleted. Preset replay keeps legacy norm protocol for bit-exact restoration.

**Tech Stack:** Rust (Leptos WASM frontend, Axum server), Lua (REAPER ReaScript), TypeScript (Playwright E2E), GitHub Actions CI.

**Spec:** `docs/superpowers/specs/2026-05-07-eq-freq-bw-drift-design.md`

---

## File Map

### Modify

- `iem-mixer/crates/iem-core/Cargo.toml` (T1) — version bump
- `iem-mixer/Cargo.toml` (T1) — version bump
- `iem-mixer/crates/iem-server/Cargo.toml` (T1) — version bump
- `iem-mixer/iem-ui/Cargo.toml` (T1) — version bump
- `iem-mixer/src-tauri/Cargo.toml` (T1) — version bump
- `iem-mixer/src-tauri/tauri.conf.json` (T1) — version bump
- `README.md` (T1) — changelog
- `scripts/reascripts/set_eq_param.lua` (T2, T3) — add `param=freq_hz` and `param=bw_oct` branches
- `iem-mixer/iem-ui/src/components/eq_modal.rs` (T4, T5, T6) — replace freq slider, replace bw slider, delete dead helpers + tests
- `iem-mixer/crates/iem-server/src/preset_routes.rs` (T8) — extend dual-protocol comment
- `iem-mixer/crates/iem-server/src/snapshot_routes.rs` (T8) — extend dual-protocol comment
- `iem-mixer/e2e/tests/live/eq.spec.ts` (T9) — add freq drift live E2E test

### Create

None — all edits are to existing files.

---

## Task 1: Version bump 1.165.0 → 1.166.0 + README changelog

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`
- Modify: `README.md` (insert v1.166.0 changelog block after `## Changelog` heading, above v1.165.0)

- [ ] **Step 1: Bump versions via sed**

```bash
sed -i 's/version = "1.165.0"/version = "1.166.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.165.0"/"version": "1.166.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify all files updated**

```bash
grep -c '1.166.0' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml \
  iem-mixer/src-tauri/tauri.conf.json
```
Expected: each file shows `1` (or higher).

- [ ] **Step 3: Add v1.166.0 changelog entry to README.md**

Locate the `## Changelog` section in `README.md`. Insert this block immediately above the existing `### v1.165.0 ...` entry:

```markdown
### v1.166.0 (2026-05-07)

- **Fix**: EQ frequency value drift on close+reopen (Mirec) — set 321 Hz, return showed 320 Hz. Same dual-formula divergence pattern as the v1.165.0 gain fix; UI sent normalized values from its own approximation table while REAPER stored its actual mapping. UI now sends Hz/oct directly; ReaScript samples REAPER's norm↔value mapping (21 points, log-space for freq, linear for bw) and writes the matching norm. REAPER is now the single source of truth for both freq and bw.
- **Internal**: Delete UI helpers `norm_to_freq_hz` and `norm_to_bw`. Existing preset replay keeps the legacy norm protocol for bit-exact restoration of saved presets — no migration risk for existing user EQ state.
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json \
  README.md
git commit -m "chore: bump version to 1.166.0 + changelog (#196)"
```

---

## Task 2: Add `param=freq_hz` branch to set_eq_param.lua

**Files:**
- Modify: `scripts/reascripts/set_eq_param.lua` — insert new branch immediately after the existing `param_name == "gain_db"` branch and before `param_name == "enabled"`.

The branch parallels `gain_db`: sample 21 norm points, parse Hz from REAPER's formatted output (handle `kHz` suffix), interpolate in log-Hz space, write the resulting norm via `TrackFX_SetParam`.

- [ ] **Step 1: Insert `freq_hz` branch**

Locate the closing `return` of the `gain_db` branch (right before the comment `-- Handle "enabled" param via BANDENABLEDM`) and insert the following block above that comment:

```lua
    -- New "freq_hz" branch: caller sends desired Hz, we sample ReaEQ's actual
    -- norm↔Hz mapping at 21 points and log-space-interpolate to find the norm
    -- that yields the desired Hz. ReaEQ formats freq as "250 Hz" or "1.2 kHz" —
    -- handle both. Same pattern as gain_db (#194).
    if param_name == "freq_hz" then
        local freq_param_idx = band * 3
        local num_params_f = reaper.TrackFX_GetNumParams(track, eq_idx)
        if freq_param_idx >= num_params_f then
            reaper.SetExtState(section, "eq_set_result",
                "ERROR:freq_param_out_of_range:" .. freq_param_idx, false)
            return
        end

        -- Parse Hz from a formatted string like "250 Hz" or "1.2 kHz".
        -- Returns nil if neither form parses cleanly.
        local function parse_hz(fmt)
            local n = tonumber(fmt:match("(-?[%d%.]+)"))
            if n == nil then return nil end
            if fmt:lower():match("khz") then
                return n * 1000.0
            end
            return n
        end

        -- Sample 21 points: norm = 0.00, 0.05, ..., 1.00 → formatted Hz.
        local samples = {}
        local N_STEPS = 20
        for i = 0, N_STEPS do
            local norm_i = i / N_STEPS
            local _, fmt = reaper.TrackFX_FormatParamValueNormalized(
                track, eq_idx, freq_param_idx, norm_i, "")
            local hz_i = parse_hz(fmt)
            if hz_i == nil then
                reaper.SetExtState(section, "eq_set_result",
                    string.format("ERROR:sample_parse_failed:band=%d,norm=%.3f,fmt=%s",
                        band, norm_i, fmt or ""), false)
                return
            end
            samples[i + 1] = { norm = norm_i, hz = hz_i }
        end

        -- Linear interpolation in LOG-Hz space (freq is logarithmic):
        -- find bracketing pair where lo.hz <= desired_hz <= hi.hz, lo.hz < hi.hz.
        -- ReaEQ's norm→Hz is monotonic increasing; if non-monotonic for some
        -- band type we still snap to the closest sample by absolute distance.
        local desired = value
        local best_norm = samples[1].norm
        local best_err = math.huge
        for i = 1, 20 do
            local lo = samples[i]
            local hi = samples[i + 1]
            if lo.hz <= desired and desired <= hi.hz and lo.hz < hi.hz then
                local t = (math.log(desired) - math.log(lo.hz))
                        / (math.log(hi.hz) - math.log(lo.hz))
                local n = lo.norm + t * (hi.norm - lo.norm)
                local _, vfmt = reaper.TrackFX_FormatParamValueNormalized(
                    track, eq_idx, freq_param_idx, n, "")
                local v_hz = parse_hz(vfmt)
                if v_hz ~= nil then
                    local err = math.abs(v_hz - desired)
                    if err < best_err then
                        best_err = err
                        best_norm = n
                    end
                end
            else
                for _, s in ipairs({ lo, hi }) do
                    local err = math.abs(s.hz - desired)
                    if err < best_err then
                        best_err = err
                        best_norm = s.norm
                    end
                end
            end
        end

        reaper.TrackFX_SetParam(track, eq_idx, freq_param_idx, best_norm)
        local _, fmt_post = reaper.TrackFX_GetFormattedParamValue(
            track, eq_idx, freq_param_idx)
        reaper.SetExtState(section, "eq_set_result",
            string.format(
                "OK:track=%d,band=%d,param=freq_hz,desired_hz=%.3f,norm=%.6f,formatted=%s",
                track_idx, band, desired, best_norm, fmt_post),
            false)
        return
    end

```

- [ ] **Step 2: Manual visual review**

```bash
grep -n "param_name ==" scripts/reascripts/set_eq_param.lua
```
Expected output (in order): `gain_db`, `freq_hz`, `enabled`. Confirm `freq_hz` sits between `gain_db` and `enabled`. Confirm both `return` statements close their branches.

- [ ] **Step 3: Commit**

```bash
git add scripts/reascripts/set_eq_param.lua
git commit -m "feat(eq): ReaScript param=freq_hz branch — log-interpolate REAPER mapping (#196)"
```

---

## Task 3: Add `param=bw_oct` branch to set_eq_param.lua

**Files:**
- Modify: `scripts/reascripts/set_eq_param.lua` — insert new branch immediately after the `freq_hz` branch (added in T2) and before `param_name == "enabled"`.

Bandwidth is linear in display (`0.01 oct` to `4.00 oct`), so use linear interpolation, not log.

- [ ] **Step 1: Insert `bw_oct` branch**

Locate the closing `return` of the `freq_hz` branch (added in T2) and insert this block above the `-- Handle "enabled" param via BANDENABLEDM` comment:

```lua
    -- New "bw_oct" branch: caller sends desired bandwidth in octaves; we
    -- sample 21 norm→oct points and LINEAR-interpolate. ReaEQ formats bw
    -- as e.g. "1.18 oct". Same pattern as gain_db / freq_hz (#196).
    if param_name == "bw_oct" then
        local bw_param_idx = band * 3 + 2
        local num_params_b = reaper.TrackFX_GetNumParams(track, eq_idx)
        if bw_param_idx >= num_params_b then
            reaper.SetExtState(section, "eq_set_result",
                "ERROR:bw_param_out_of_range:" .. bw_param_idx, false)
            return
        end

        -- Sample 21 points: norm = 0.00, 0.05, ..., 1.00 → formatted oct.
        local samples = {}
        local N_STEPS = 20
        for i = 0, N_STEPS do
            local norm_i = i / N_STEPS
            local _, fmt = reaper.TrackFX_FormatParamValueNormalized(
                track, eq_idx, bw_param_idx, norm_i, "")
            local oct_i = tonumber(fmt:match("(-?[%d%.]+)"))
            if oct_i == nil then
                reaper.SetExtState(section, "eq_set_result",
                    string.format("ERROR:sample_parse_failed:band=%d,norm=%.3f,fmt=%s",
                        band, norm_i, fmt or ""), false)
                return
            end
            samples[i + 1] = { norm = norm_i, oct = oct_i }
        end

        -- Linear interpolation in oct space.
        local desired = value
        local best_norm = samples[1].norm
        local best_err = math.huge
        for i = 1, 20 do
            local lo = samples[i]
            local hi = samples[i + 1]
            if lo.oct <= desired and desired <= hi.oct and lo.oct < hi.oct then
                local t = (desired - lo.oct) / (hi.oct - lo.oct)
                local n = lo.norm + t * (hi.norm - lo.norm)
                local _, vfmt = reaper.TrackFX_FormatParamValueNormalized(
                    track, eq_idx, bw_param_idx, n, "")
                local v_oct = tonumber(vfmt:match("(-?[%d%.]+)"))
                if v_oct ~= nil then
                    local err = math.abs(v_oct - desired)
                    if err < best_err then
                        best_err = err
                        best_norm = n
                    end
                end
            else
                for _, s in ipairs({ lo, hi }) do
                    local err = math.abs(s.oct - desired)
                    if err < best_err then
                        best_err = err
                        best_norm = s.norm
                    end
                end
            end
        end

        reaper.TrackFX_SetParam(track, eq_idx, bw_param_idx, best_norm)
        local _, fmt_post = reaper.TrackFX_GetFormattedParamValue(
            track, eq_idx, bw_param_idx)
        reaper.SetExtState(section, "eq_set_result",
            string.format(
                "OK:track=%d,band=%d,param=bw_oct,desired_oct=%.3f,norm=%.6f,formatted=%s",
                track_idx, band, desired, best_norm, fmt_post),
            false)
        return
    end

```

- [ ] **Step 2: Manual visual review**

```bash
grep -n "param_name ==" scripts/reascripts/set_eq_param.lua
```
Expected (in order): `gain_db`, `freq_hz`, `bw_oct`, `enabled`.

- [ ] **Step 3: Commit**

```bash
git add scripts/reascripts/set_eq_param.lua
git commit -m "feat(eq): ReaScript param=bw_oct branch — linear-interpolate REAPER mapping (#196)"
```

---

## Task 4: Replace freq slider in eq_modal.rs

**Files:**
- Modify: `iem-mixer/iem-ui/src/components/eq_modal.rs`

Two regions change:
1. The reset button block (currently lines ~679-705): freq reset switches to send Hz default.
2. The freq slider component (currently lines ~712-738): position derives from `freq_hz_sig`, `on_change` sends Hz, display reads `freq_hz_sig` clamped.

The two changes use distinct `old_string` blocks.

- [ ] **Step 1: Replace freq reset block in reset button**

In `iem-mixer/iem-ui/src/components/eq_modal.rs`, find this block inside the reset button (`on:click=move |_|`):

```rust
                                                    // Reset freq to per-band default
                                                    // Norm values verified empirically against REAPER
                                                    let default_freq_norm: f32 = match band_type_reset.as_str() {
                                                        "highpass" => 0.1160,   // 80Hz
                                                        "lowshelf" => 0.2316,   // 200Hz
                                                        "highshelf" => 0.8176,  // 8kHz
                                                        "lowpass" => 0.8848,    // 12kHz
                                                        _ => {
                                                            // Parametric bands: use REAPER index
                                                            if idx == 3 { 0.6548 } else { 0.4408 }
                                                            // band 2 → 800Hz, band 3 → 3kHz
                                                        }
                                                    };
                                                    let default_bw_norm = match band_type_reset.as_str() {
                                                        "highpass" | "lowshelf" | "highshelf" | "lowpass" => 0.50,
                                                        _ => 0.25,
                                                    };
                                                    let _ = freq_sig.try_set(default_freq_norm);
                                                    let _ = freq_hz_sig.try_set(norm_to_freq_hz(default_freq_norm));
                                                    on_param_change.run((idx, "freq".to_string(), default_freq_norm));
                                                    // Override BW with type-specific default
                                                    let _ = bw_sig.try_set(default_bw_norm);
                                                    let _ = bw_oct_sig.try_set(norm_to_bw(default_bw_norm));
                                                    on_param_change.run((idx, "bw".to_string(), default_bw_norm));
```

Replace with:

```rust
                                                    // Reset freq to per-band default Hz (#196)
                                                    let default_freq_hz: f32 = match band_type_reset.as_str() {
                                                        "highpass" => 80.0,
                                                        "lowshelf" => 200.0,
                                                        "highshelf" => 8000.0,
                                                        "lowpass" => 12000.0,
                                                        _ => {
                                                            // Parametric bands: use REAPER index
                                                            if idx == 3 { 3000.0 } else { 800.0 }
                                                        }
                                                    };
                                                    // Reset bw to per-band default oct (#196)
                                                    let default_bw_oct: f32 = match band_type_reset.as_str() {
                                                        "highpass" | "lowshelf" | "highshelf" | "lowpass" => 2.00,
                                                        _ => 1.00,
                                                    };
                                                    let _ = freq_hz_sig.try_set(default_freq_hz);
                                                    on_param_change.run((idx, "freq_hz".to_string(), default_freq_hz));
                                                    let _ = bw_oct_sig.try_set(default_bw_oct);
                                                    on_param_change.run((idx, "bw_oct".to_string(), default_bw_oct));
```

- [ ] **Step 2: Replace freq slider component**

Find this block (the `// Frequency slider` section in `eq_modal.rs`):

```rust
                                        // Frequency slider
                                        <div class="eq-param-row">
                                            <label class="eq-param-label">"Freq"</label>
                                            <EqSlider
                                                value=freq_sig.into()
                                                on_change=Callback::new(move |v: f32| {
                                                    let now = js_sys::Date::now();
                                                    if now - last_send_freq.get_untracked() > 50.0 {
                                                        let _ = last_send_freq.try_set(now);
                                                        on_param_change.run((band_idx_sv.get_value(), "freq".to_string(), v));
                                                    }
                                                    let _ = freq_sig.try_set(v);
                                                    let _ = freq_hz_sig.try_set(norm_to_freq_hz(v));
                                                    let _ = curve_trigger.try_update(|n| *n += 1);
                                                })
                                                on_drag_start=Callback::new(move |_: ()| {
                                                    let _ = any_dragging.try_set(true);
                                                })
                                                on_drag_end=Callback::new(move |_: ()| {
                                                    let _ = any_dragging.try_set(false);
                                                })
                                                css_class="eq-slider-freq"
                                            />
                                            <span class="eq-param-value">
                                                {move || { curve_trigger.get(); format_freq(freq_hz_sig.get_untracked()) }}
                                            </span>
                                        </div>
```

Replace with:

```rust
                                        // Frequency slider: derives position from REAPER's actual freq_hz
                                        // mapped onto a fixed UI log scale (20 Hz – 24 kHz). Single source
                                        // of truth = REAPER (#196).
                                        <div class="eq-param-row">
                                            <label class="eq-param-label">"Freq"</label>
                                            <EqSlider
                                                value=Signal::derive(move || {
                                                    let hz = freq_hz_sig.get().clamp(20.0, 24000.0);
                                                    let log_min = 20.0_f32.ln();
                                                    let log_max = 24000.0_f32.ln();
                                                    (hz.ln() - log_min) / (log_max - log_min)
                                                })
                                                on_change=Callback::new(move |v: f32| {
                                                    let log_min = 20.0_f32.ln();
                                                    let log_max = 24000.0_f32.ln();
                                                    let hz = (log_min + v * (log_max - log_min)).exp();
                                                    let now = js_sys::Date::now();
                                                    if now - last_send_freq.get_untracked() > 50.0 {
                                                        let _ = last_send_freq.try_set(now);
                                                        on_param_change.run((band_idx_sv.get_value(), "freq_hz".to_string(), hz));
                                                    }
                                                    let _ = freq_hz_sig.try_set(hz);
                                                    let _ = curve_trigger.try_update(|n| *n += 1);
                                                })
                                                on_drag_start=Callback::new(move |_: ()| {
                                                    let _ = any_dragging.try_set(true);
                                                })
                                                on_drag_end=Callback::new(move |_: ()| {
                                                    let _ = any_dragging.try_set(false);
                                                })
                                                css_class="eq-slider-freq"
                                            />
                                            <span class="eq-param-value">
                                                {move || {
                                                    curve_trigger.get();
                                                    let hz = freq_hz_sig.get_untracked().clamp(20.0, 24000.0);
                                                    format_freq(hz)
                                                }}
                                            </span>
                                        </div>
```

- [ ] **Step 3: Local lint check**

```bash
cd iem-mixer && cargo fmt --all --check
```
Expected: no diff.

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/iem-ui/src/components/eq_modal.rs
git commit -m "fix(eq): freq slider derives from freq_hz_sig + sends freq_hz protocol (#196)"
```

---

## Task 5: Replace bw slider in eq_modal.rs

**Files:**
- Modify: `iem-mixer/iem-ui/src/components/eq_modal.rs` — bw slider component (currently lines ~789-816).

Reset already updated in T4 to send `bw_oct`; this task only replaces the bw slider component.

- [ ] **Step 1: Replace bw slider component**

Find this block (the `// Bandwidth/Q slider` section):

```rust
                                        // Bandwidth/Q slider
                                        <div class="eq-param-row">
                                            <label class="eq-param-label">"BW"</label>
                                            <EqSlider
                                                value=bw_sig.into()
                                                on_change=Callback::new(move |v: f32| {
                                                    let now = js_sys::Date::now();
                                                    if now - last_send_bw.get_untracked() > 50.0 {
                                                        let _ = last_send_bw.try_set(now);
                                                        on_param_change.run((band_idx_sv.get_value(), "bw".to_string(), v));
                                                    }
                                                    let _ = bw_sig.try_set(v);
                                                    let _ = bw_oct_sig.try_set(norm_to_bw(v));
                                                    let _ = curve_trigger.try_update(|n| *n += 1);
                                                })
                                                on_drag_start=Callback::new(move |_: ()| {
                                                    let _ = any_dragging.try_set(true);
                                                })
                                                on_drag_end=Callback::new(move |_: ()| {
                                                    let _ = any_dragging.try_set(false);
                                                })
                                                css_class=""
                                                default_value=0.5
                                            />
                                            <span class="eq-param-value">
                                                {move || { curve_trigger.get(); format!("{:.2} oct", bw_oct_sig.get_untracked()) }}
                                            </span>
                                        </div>
```

Replace with:

```rust
                                        // Bandwidth/Q slider: derives position from REAPER's actual bw_oct
                                        // mapped onto a fixed UI linear scale (0.01 – 4.00 oct). Single
                                        // source of truth = REAPER (#196).
                                        <div class="eq-param-row">
                                            <label class="eq-param-label">"BW"</label>
                                            <EqSlider
                                                value=Signal::derive(move || {
                                                    let oct = bw_oct_sig.get().clamp(0.01, 4.00);
                                                    (oct - 0.01) / 3.99
                                                })
                                                on_change=Callback::new(move |v: f32| {
                                                    let oct = 0.01 + v * 3.99;
                                                    let now = js_sys::Date::now();
                                                    if now - last_send_bw.get_untracked() > 50.0 {
                                                        let _ = last_send_bw.try_set(now);
                                                        on_param_change.run((band_idx_sv.get_value(), "bw_oct".to_string(), oct));
                                                    }
                                                    let _ = bw_oct_sig.try_set(oct);
                                                    let _ = curve_trigger.try_update(|n| *n += 1);
                                                })
                                                on_drag_start=Callback::new(move |_: ()| {
                                                    let _ = any_dragging.try_set(true);
                                                })
                                                on_drag_end=Callback::new(move |_: ()| {
                                                    let _ = any_dragging.try_set(false);
                                                })
                                                css_class=""
                                                default_value=0.5
                                            />
                                            <span class="eq-param-value">
                                                {move || {
                                                    curve_trigger.get();
                                                    let oct = bw_oct_sig.get_untracked().clamp(0.01, 4.00);
                                                    format!("{:.2} oct", oct)
                                                }}
                                            </span>
                                        </div>
```

- [ ] **Step 2: Local lint check**

```bash
cd iem-mixer && cargo fmt --all --check
```
Expected: no diff.

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/iem-ui/src/components/eq_modal.rs
git commit -m "fix(eq): bw slider derives from bw_oct_sig + sends bw_oct protocol (#196)"
```

---

## Task 6: Delete dead UI helpers and their tests

**Files:**
- Modify: `iem-mixer/iem-ui/src/components/eq_modal.rs`

After T4 + T5, `norm_to_freq_hz` (line ~59) and `norm_to_bw` (line ~85) have no callers. Their unit tests and any callers in the slider/reset paths have already been removed. Delete the helpers and their tests.

`freq_norm` and `bw_norm` fields in `EqBandState` and `EqBandLocal` MUST stay — they are read by `iem-mixer/iem-ui/src/pages/mixer/handlers.rs:160-163` for preset save (same lesson as gain_norm in #194).

- [ ] **Step 1: Verify no remaining callers of the helpers**

```bash
grep -n "norm_to_freq_hz\|norm_to_bw" iem-mixer/iem-ui/src/components/eq_modal.rs
```
Expected: only the function definitions themselves and their unit tests appear. If any caller remains in slider/reset code, return to T4/T5 and finish those edits before proceeding.

- [ ] **Step 2: Delete `norm_to_freq_hz`**

In `iem-mixer/iem-ui/src/components/eq_modal.rs`, delete this block (currently around line 56-83):

```rust
/// Convert normalized frequency (0-1) to Hz matching REAPER's ReaEQ curve.
/// Uses lookup table with log-space interpolation from empirical REAPER data.
/// Range: 20 Hz to 24,000 Hz (NOT 20,000 — REAPER's actual range).
fn norm_to_freq_hz(norm: f32) -> f32 {
    const TABLE: [(f32, f32); 11] = [
        (0.0, 20.0),
        (0.1, 69.2),
        (0.2, 158.9),
        (0.3, 322.1),
        (0.4, 619.3),
        (0.5, 1160.5),
        (0.6, 2146.2),
        (0.7, 3941.0),
        (0.8, 7209.5),
        (0.9, 13161.4),
        (1.0, 24000.0),
    ];
    let norm = norm.clamp(0.0, 1.0);
    let idx = (norm * 10.0) as usize;
    if idx >= 10 {
        return TABLE[10].1;
    }
    let t = norm * 10.0 - idx as f32;
    let log_lo = TABLE[idx].1.ln();
    let log_hi = TABLE[idx + 1].1.ln();
    (log_lo + t * (log_hi - log_lo)).exp()
}

```

- [ ] **Step 3: Delete `norm_to_bw`**

Delete this block (currently around line 84-87):

```rust
/// Convert normalized bandwidth (0-1) to octaves
fn norm_to_bw(norm: f32) -> f32 {
    0.01 + norm * 3.99
}

```

- [ ] **Step 4: Delete the freq mapping unit test**

Delete the entire `#[test] fn test_norm_to_freq_hz_matches_reaper()` block from the bottom of `eq_modal.rs` (currently around line 1170-1230). Locate it via:

```bash
grep -n "test_norm_to_freq_hz_matches_reaper" iem-mixer/iem-ui/src/components/eq_modal.rs
```

Read the file from that line forward and delete the full `fn test_norm_to_freq_hz_matches_reaper() { ... }` body (closing brace included). Watch for any leading `#[test]` attribute and any trailing blank line — delete both.

- [ ] **Step 5: Local lint check**

```bash
cd iem-mixer && cargo fmt --all --check
```
Expected: no diff.

- [ ] **Step 6: Final caller search**

```bash
grep -n "norm_to_freq_hz\|norm_to_bw" iem-mixer/iem-ui/src/components/eq_modal.rs
```
Expected: empty output (zero matches).

- [ ] **Step 7: Commit**

```bash
git add iem-mixer/iem-ui/src/components/eq_modal.rs
git commit -m "fix(eq): remove dead norm_to_freq_hz / norm_to_bw + tests (#196)"
```

---

## Task 7: Verify proxy parse_eq_band missing-field tests already cover fh= and bo=

Post-#194, three strict-field tests already exist in `iem-mixer/crates/iem-server/src/proxy.rs`:

- `test_parse_eq_band_returns_none_when_gd_missing` (line ~5398)
- `test_parse_eq_band_returns_none_when_fh_missing` (line ~5406)
- `test_parse_eq_band_returns_none_when_bo_missing` (line ~5413)

This task is a verification gate, not a code change. If all three tests are present, no further work is needed and there is nothing to commit.

- [ ] **Step 1: Confirm all three tests exist**

```bash
grep -n "test_parse_eq_band_returns_none_when_gd_missing\|test_parse_eq_band_returns_none_when_fh_missing\|test_parse_eq_band_returns_none_when_bo_missing" iem-mixer/crates/iem-server/src/proxy.rs
```
Expected output: 3 lines, one per test.

- [ ] **Step 2: Spot-check the bo missing test asserts `is_none()`**

```bash
sed -n '5413,5430p' iem-mixer/crates/iem-server/src/proxy.rs
```
Expected: function body uses `parse_eq_band(...).is_none()` (or `assert!(parse_eq_band(...).is_none(), ...)`).

- [ ] **Step 3: Verify and skip commit**

If both checks pass: no edit, no commit. Move on to Task 8. If either check fails: open the file, look up the actual structure of the existing strict tests, and add a `test_parse_eq_band_returns_none_when_bo_missing` test mirroring `test_parse_eq_band_returns_none_when_fh_missing` byte-for-byte except changing `fh=250.0` → `bo=1.18` in the kept fields and removing `bo=...` from the test input.

---

## Task 8: Extend dual-protocol comments in preset/snapshot routes

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/preset_routes.rs:336-339`
- Modify: `iem-mixer/crates/iem-server/src/snapshot_routes.rs:345-348`

The existing #194 comments only mention `gain_db`. Extend them to cover `freq_hz` and `bw_oct` so future readers understand the live UI now sends three Hz-/dB-/oct-domain protocols while preset replay keeps norm.

- [ ] **Step 1: Replace preset_routes.rs comment**

In `iem-mixer/crates/iem-server/src/preset_routes.rs`, find this block:

```rust
                // NOTE: preset replay uses the legacy `param=gain` (norm) protocol.
                // The interactive UI slider uses `param=gain_db` (#194), but the
                // ReaScript supports BOTH. Don't remove the legacy `gain` branch
                // from set_eq_param.lua without updating preset/snapshot apply paths.
```

Replace with:

```rust
                // NOTE: preset replay uses the LEGACY norm protocol for all params:
                // `param=freq` (norm), `param=gain` (norm), `param=bw` (norm).
                // The interactive UI slider uses the value-domain protocols:
                // `param=gain_db` (#194), `param=freq_hz` (#196), `param=bw_oct` (#196).
                // The ReaScript supports BOTH families. Don't remove any legacy
                // norm branch from set_eq_param.lua without updating preset/snapshot
                // apply paths — preset bit-exactness depends on the norm protocol.
```

- [ ] **Step 2: Replace snapshot_routes.rs comment**

In `iem-mixer/crates/iem-server/src/snapshot_routes.rs`, find this block (text identical to the preset_routes.rs original):

```rust
                // NOTE: preset replay uses the legacy `param=gain` (norm) protocol.
                // The interactive UI slider uses `param=gain_db` (#194), but the
                // ReaScript supports BOTH. Don't remove the legacy `gain` branch
                // from set_eq_param.lua without updating preset/snapshot apply paths.
```

Replace with the same expanded comment as Step 1.

- [ ] **Step 3: Local lint check**

```bash
cd iem-mixer && cargo fmt --all --check
```
Expected: no diff.

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-server/src/preset_routes.rs iem-mixer/crates/iem-server/src/snapshot_routes.rs
git commit -m "docs(eq): extend dual-protocol comment to cover freq_hz / bw_oct (#196)"
```

---

## Task 9: Add live E2E test — freq value persists across close+reopen

**Files:**
- Modify: `iem-mixer/e2e/tests/live/eq.spec.ts`

Add inside the existing `EQ value sync - ENGINEER track` describe block. Engineer track 32, band 1 (parametric). Pre-arrange via the ReaScript SET path (engineer-only writes per `feedback_live_test_safety`). `try`/`finally` restores the original norm.

The assertion is intrinsic — thumb position percent matches the displayed Hz under the UI's own log scale, AND the displayed value is stable across modal close+reopen. We do NOT cross-check against REAPER's value because the whole point is that REAPER is the source of truth and the UI must reflect it without drift.

Inline-style parsing: read `el.getAttribute("style")` and regex `/left:\s*([\d.]+)%/`. Computed `cs.left` returns pixels not percent (post-#194 fix).

- [ ] **Step 1: Locate the existing describe block and helpers**

```bash
grep -n "EQ value sync - ENGINEER\|openEqForChannel\|describe(" iem-mixer/e2e/tests/live/eq.spec.ts | head -20
```
Confirm:
- `EQ value sync - ENGINEER track` describe block exists.
- `openEqForChannel(page, "ENGINEER")` helper exists.
- The post-#194 gain test (`gain change on disabled band` style) sits inside this describe block — use it as a structural template.

- [ ] **Step 2: Add the freq drift test**

Inside the `EQ value sync - ENGINEER track` describe block (after the existing #194 gain test), add:

```typescript
test("freq value persists across close+reopen on engineer band (#mirec)", async ({ page }) => {
    const REAPER_API = "http://10.77.9.231/api/reaper";
    const TRACK = 32; // ENGINEER inear
    const BAND = 1; // parametric band

    // Capture original norm so we can restore it in finally.
    let originalNorm: number | null = null;
    const probe = await page.evaluate(async (api) => {
        await fetch(`${api}/SET/EXTSTATE/reaperiem/eq_read_track/32`);
        await fetch(`${api}/_RS_REAPERIEM_READ_EQ`);
        await new Promise((r) => setTimeout(r, 200));
        const r = await fetch(`${api}/GET/EXTSTATE/reaperiem/eq_params`);
        return r.text();
    }, REAPER_API);
    const fnMatch = probe.match(/b1:[^|]*?fn=([\d.]+)/);
    if (fnMatch) originalNorm = parseFloat(fnMatch[1]);

    try {
        // Pre-arrange: set engineer band 1 freq to a known norm via legacy
        // `param=freq` (norm) path. Norm 0.30 ≈ 322 Hz on REAPER's mapping —
        // close to the bug's reported 321/320 boundary.
        await page.evaluate(async (api) => {
            const payload = `track=32%7Cband=1%7Cparam=freq%7Cvalue=0.300`;
            await fetch(`${api}/SET/EXTSTATE/reaperiem/eq_set/${payload}`);
            await fetch(`${api}/_RS_REAPERIEM_SET_EQ`);
            await new Promise((r) => setTimeout(r, 250));
        }, REAPER_API);

        // Open EQ on ENGINEER (Main tab kebab → EQ option).
        await openEqForChannel(page, "ENGINEER");

        // Read freq text + thumb percent for band 1 (BAND index 1, second card).
        const bandCard = page.locator(".eq-band-card").nth(BAND);
        const freqRow = bandCard.locator(".eq-param-row").filter({ hasText: "Freq" });
        const thumb = freqRow.locator(".eq-slider-thumb").first();

        const text1 = (await freqRow.locator(".eq-param-value").textContent())?.trim();
        const style1 = await thumb.getAttribute("style");
        const m1 = style1?.match(/left:\s*([\d.]+)%/);
        expect(m1, `thumb style missing left%: ${style1}`).not.toBeNull();
        const pct1 = parseFloat(m1![1]);

        // Intrinsic agreement: text Hz should map back to ~the same percent under
        // the UI log scale (20 Hz – 24 kHz). Allow 1% slop for kHz formatting.
        const parseHzFromText = (t: string): number => {
            // Forms: "321", "1.2k", "20.0k"
            const km = t.match(/([\d.]+)k/);
            if (km) return parseFloat(km[1]) * 1000;
            return parseFloat(t.replace(/[^\d.]/g, ""));
        };
        const hz1 = parseHzFromText(text1 ?? "");
        const logMin = Math.log(20);
        const logMax = Math.log(24000);
        const expectedPct1 = ((Math.log(Math.min(Math.max(hz1, 20), 24000)) - logMin) / (logMax - logMin)) * 100;
        expect(Math.abs(pct1 - expectedPct1)).toBeLessThan(1.0);

        // Close and reopen modal.
        await page.locator(".eq-overlay").click({ position: { x: 5, y: 5 } });
        await page.waitForSelector(".eq-modal", { state: "detached", timeout: 5000 });
        await openEqForChannel(page, "ENGINEER");

        const text2 = (await page.locator(".eq-band-card").nth(BAND)
            .locator(".eq-param-row").filter({ hasText: "Freq" })
            .locator(".eq-param-value").textContent())?.trim();
        const style2 = await page.locator(".eq-band-card").nth(BAND)
            .locator(".eq-param-row").filter({ hasText: "Freq" })
            .locator(".eq-slider-thumb").first().getAttribute("style");
        const m2 = style2?.match(/left:\s*([\d.]+)%/);
        expect(m2, `reopen thumb style missing left%: ${style2}`).not.toBeNull();
        const pct2 = parseFloat(m2![1]);

        // Stability: reopen displays the same Hz text and thumb position.
        expect(text2).toBe(text1);
        expect(Math.abs(pct2 - pct1)).toBeLessThan(0.5);
    } finally {
        // Restore engineer band 1 freq to its original norm (engineer-only write).
        if (originalNorm !== null) {
            const norm = originalNorm;
            await page.evaluate(async ({ api, norm }) => {
                const payload = `track=32%7Cband=1%7Cparam=freq%7Cvalue=${norm.toFixed(6)}`;
                await fetch(`${api}/SET/EXTSTATE/reaperiem/eq_set/${payload}`);
                await fetch(`${api}/_RS_REAPERIEM_SET_EQ`);
                await new Promise((r) => setTimeout(r, 250));
            }, { api: REAPER_API, norm });
        }
    }
});
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/e2e/tests/live/eq.spec.ts
git commit -m "test(eq): live E2E — freq value persists across close+reopen on engineer (#196)"
```

---

## Task 10: Push to dev + monitor CI

This task runs ALL prior commits in one push. Local lint already verified per task. Self-hosted Windows runner builds + deploys + runs post-deploy E2E.

- [ ] **Step 1: Final local lint sweep before push**

```bash
cd iem-mixer && cargo fmt --all --check
```
Expected: no diff.

- [ ] **Step 2: Push**

```bash
git push origin dev
```

- [ ] **Step 3: Identify the run**

```bash
gh run list --branch dev --limit 3 --json databaseId,status,conclusion,headSha,name
```
Note the `databaseId` of the most recent run triggered by this push.

- [ ] **Step 4: Monitor in background, single sleep**

```bash
sleep 300 && gh run view <RUN_ID> --json status,conclusion,jobs
```
Run via Bash with `run_in_background: true`. When the result comes back via BashOutput, react:
- All jobs `success`: continue to T11.
- Any job `failure`: drill into `gh run view <RUN_ID> --log-failed`, fix root cause in ONE commit, push, monitor again.
- Still running after one cycle: schedule another `sleep 300 && gh run view <RUN_ID> --json status,conclusion,jobs` background bash.

- [ ] **Step 5: On failure — investigate and fix in ONE commit**

```bash
gh run view <RUN_ID> --log-failed
```
Identify the failing job and the failing step's exact error. Fix all issues batch-style in a single commit. Do not push partial fixes. After fix:

```bash
git add <files>
git commit -m "fix: <concise description> (#196)"
git push origin dev
gh run list --branch dev --limit 3 --json databaseId,status,conclusion
# Continue Step 4 with the new run id.
```

Common expected failure modes (from #194 experience):
- Lua regex / Lua bytecode error → grep `param=([%w_]+)` already accepts underscore; no regex change needed for `freq_hz` or `bw_oct` (verified in `set_eq_param.lua`).
- Rust unused import / unused variable in `eq_modal.rs` after deletions → `cargo fmt` catches formatting; build catches unused. Remove any orphaned `use` statements that referenced `norm_to_freq_hz` / `norm_to_bw`.
- Live E2E selector mismatch → verify `.eq-band-card` and `.eq-slider-thumb` class names against current UI, adjust if drifted.

- [ ] **Step 6: Confirm all jobs green and post-deploy verification passed**

```bash
gh run view <RUN_ID> --json status,conclusion,jobs --jq '{status, conclusion, jobs: [.jobs[] | {name, conclusion}]}'
```
Expected: top-level `status: "completed"`, `conclusion: "success"`. Every job entry: `conclusion: "success"`.

---

## Task 11: Open PR dev → main, verify clean, STOP

**Branch policy:** dev → main, merge commit only. Don't merge — wait for explicit user approval.

- [ ] **Step 1: Generate PR title + body**

Title: `fix(eq): UI sends freq_hz / bw_oct — eliminate close+reopen drift (#196)`

Body (use HEREDOC when invoking `gh pr create`):

```markdown
## Summary

- Mirror of #194 gain fix applied to freq + bw dimensions. UI now sends desired Hz/oct directly; ReaScript samples REAPER's actual norm↔value mapping at 21 points and interpolates (log-space for Hz, linear for oct) to write the matching norm. REAPER is the single source of truth.
- Resolves Mirec's report: set EQ freq 321 Hz, leave EQ, return shows 320 Hz. Same dual-formula divergence pattern (UI's own approximation vs REAPER's mapping). Fix removes the UI approximation tables (`norm_to_freq_hz`, `norm_to_bw`) and routes all live UI writes through REAPER-truth interpolation.
- Preset and snapshot replay paths intentionally keep the legacy norm protocol for bit-exact restoration of saved presets — no migration risk for existing user EQ state. Dual-protocol comments updated at both replay sites.

## Test plan

- [x] Unit + integration tests pass on CI
- [x] Build WASM + Tauri pass on CI
- [x] Self-hosted Windows runner deploys to iem.lan
- [x] Post-deploy live E2E test asserts thumb-vs-text agreement and value stability across modal close+reopen on ENGINEER band 1 freq (engineer-only write, restored in finally)
- [x] All other live E2E suites still green

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

- [ ] **Step 2: Create PR**

```bash
gh pr create --base main --head dev --title "fix(eq): UI sends freq_hz / bw_oct — eliminate close+reopen drift (#196)" --body "$(cat <<'EOF'
## Summary

- Mirror of #194 gain fix applied to freq + bw dimensions. UI now sends desired Hz/oct directly; ReaScript samples REAPER's actual norm↔value mapping at 21 points and interpolates (log-space for Hz, linear for oct) to write the matching norm. REAPER is the single source of truth.
- Resolves Mirec's report: set EQ freq 321 Hz, leave EQ, return shows 320 Hz. Same dual-formula divergence pattern (UI's own approximation vs REAPER's mapping). Fix removes the UI approximation tables (norm_to_freq_hz, norm_to_bw) and routes all live UI writes through REAPER-truth interpolation.
- Preset and snapshot replay paths intentionally keep the legacy norm protocol for bit-exact restoration of saved presets — no migration risk for existing user EQ state. Dual-protocol comments updated at both replay sites.

## Test plan

- [x] Unit + integration tests pass on CI
- [x] Build WASM + Tauri pass on CI
- [x] Self-hosted Windows runner deploys to iem.lan
- [x] Post-deploy live E2E test asserts thumb-vs-text agreement and value stability across modal close+reopen on ENGINEER band 1 freq (engineer-only write, restored in finally)
- [x] All other live E2E suites still green

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Verify PR is mergeable + clean**

```bash
PR_NUM=$(gh pr list --head dev --base main --json number --jq '.[0].number')
gh api repos/zbynekdrlik/reaperiem/pulls/$PR_NUM --jq '{mergeable: .mergeable, mergeable_state: .mergeable_state}'
```
Expected exact output: `{"mergeable":true,"mergeable_state":"clean"}`.

If `mergeable_state` is `behind`: rebase dev on origin/main (`git fetch origin && git merge origin/main`), push, monitor CI again.

If `mergeable_state` is `unstable`, `dirty`, or `blocked`: investigate the failing/conflicting check or merge conflict. Do NOT merge despite. Fix the gate.

- [ ] **Step 4: Print PR URL and STOP**

```bash
gh pr view $PR_NUM --json url --jq '.url'
```

STOP. Do NOT merge. Wait for explicit user instruction ("merge it", "approved", or equivalent).

---

## Task Dependencies

```
T1 (version bump + README)
   ↓
T2 (ReaScript freq_hz)        ← Lua edit
   ↓
T3 (ReaScript bw_oct)         ← Lua edit, builds on T2
   ↓
T4 (UI freq slider)           ← needs T2 deployed for the slider to work end-to-end, but the edit can land in the same PR
   ↓
T5 (UI bw slider)             ← needs T3 deployed for end-to-end, same PR
   ↓
T6 (UI deletions)             ← needs T4 + T5 to remove all callers first
   ↓
T7 (server test verification) ← independent, but cheap
   ↓
T8 (dual-protocol comments)
   ↓
T9 (live E2E test)
   ↓
T10 (push + monitor)
   ↓
T11 (PR + STOP)
```

T1 → T2 → T3 → T4 → T5 → T6 → T7 → T8 → T9 → T10 → T11 strictly sequential. Each task has its own commit so CI failures bisect cleanly.

---

## Verification

After CI is green and PR is in `mergeable: true, mergeable_state: "clean"`:

1. PR URL printed.
2. All ~10 CI jobs reported `success` (lint, test, build-wasm, e2e CI, build-tauri, deploy, post-deploy live E2E).
3. The new live E2E test (`freq value persists across close+reopen on engineer band (#mirec)`) reported `success` in post-deploy.
4. PR `mergeable` true and `mergeable_state` is exactly `clean` (not `unstable`, not `blocked`, not `behind`).
5. STOP at green PR URL. No merge.
