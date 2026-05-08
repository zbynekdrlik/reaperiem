# EQ Freq + BW Drift Fix — Design

**Date:** 2026-05-07
**Issue:** Mirec reports setting EQ freq 321 Hz, leaving EQ modal, returning shows 320 Hz. Same architectural pattern as #194 (gain), this time on freq and bw dimensions.

---

## Problem

UI shows a freq value (e.g. 321 Hz) that differs from what REAPER actually stored (320 Hz). On close + reopen the modal, REAPER's truth wins and the displayed value shifts. User loses trust in the EQ — values they set are not the values they get back.

Same drift pattern bit gain in #194: dual-formula architecture (UI's own norm↔value table vs REAPER's actual mapping) → divergence → user-visible mismatch.

## Root Cause

UI maintains independent approximations of REAPER's ReaEQ mapping:

- `norm_to_freq_hz` — 11-point log-interp table at `iem-mixer/iem-ui/src/components/eq_modal.rs:59`
- `norm_to_bw` — affine `0.01 + norm * 3.99` at `iem-mixer/iem-ui/src/components/eq_modal.rs:85`

Slider drag flow:

1. Slider raw value `v ∈ [0, 1]` (norm).
2. UI sends `param=freq` with norm `v` to ReaScript via `SET/EXTSTATE/reaperiem/eq_set` + `_RS_REAPERIEM_SET_EQ`.
3. UI text shows `norm_to_freq_hz(v)` — UI's own table.
4. REAPER snaps norm to its internal grid; its actual Hz = REAPER's mapping of `v`.
5. UI display shows ~321 (UI table) while REAPER has stored ~320.
6. Modal closes. Reopens, reads `fh=` from `read_eq_params.lua` → 320. UI text now shows 320. Drift surfaces.

Same mechanism applies to bw via `norm_to_bw` vs REAPER's actual `bo=` octaves.

## Architecture

Single source of truth: REAPER. UI sends Hz / octaves directly; ReaScript samples REAPER's actual mapping and writes the norm that produces the requested value.

```
Slider drag
  → on_change(desired_hz | desired_oct)
  → server proxy
  → SET/EXTSTATE eq_set "track=N|band=B|param=freq_hz|value=321.0"
  → _RS_REAPERIEM_SET_EQ
       sample 21× FormatParamValueNormalized(norm=i/20)
       parse Hz from formatted string
       interpolate in log-Hz space → norm
       SetParam(norm)
  → close + reopen
  → read_eq_params returns fh=320 (REAPER truth)
  → UI display = REAPER truth, no drift
```

## Component Changes

### `scripts/reascripts/set_eq_param.lua`

Add two branches mirroring the existing `param=gain_db` branch (added in #194):

#### `param=freq_hz`

Caller sends desired Hz. ReaScript:

1. Compute `freq_param_idx = band * 3` (existing formula).
2. Sample 21 points at `norm_i = i / 20` for `i ∈ [0, 20]`.
3. Read `_, fmt = TrackFX_FormatParamValueNormalized(track, eq_idx, freq_param_idx, norm_i, "")`.
4. Parse Hz: `tonumber(fmt:match("(-?[%d%.]+)"))`. ReaEQ format examples: `"250 Hz"`, `"1.2 kHz"`. Need explicit kHz handling — if `fmt:match("kHz")` then multiply parsed value by 1000.
5. Hard error on parse failure: write `ERROR:sample_parse_failed:band=%d,norm=%.3f,fmt=%s` to EXTSTATE and return.
6. Linear interpolation in log-Hz space: for bracket pair `lo, hi` where `lo.hz <= desired <= hi.hz` and `lo.hz < hi.hz`:
   ```
   t = (ln(desired) - ln(lo.hz)) / (ln(hi.hz) - ln(lo.hz))
   n = lo.norm + t * (hi.norm - lo.norm)
   ```
7. Verify by reading back the formatted Hz at `n`; track best-error norm. Closest-sample fallback if non-monotonic or out of bracket.
8. `TrackFX_SetParam(track, eq_idx, freq_param_idx, best_norm)`.
9. Write `OK:track=N,band=B,param=freq_hz,desired_hz=%.3f,norm=%.6f,formatted=%s` to EXTSTATE.

#### `param=bw_oct`

Caller sends octaves. Same structure:

1. `bw_param_idx = band * 3 + 2`.
2. Sample 21 norm→bw points; parse octaves from formatted string (e.g. `"1.18 oct"`).
3. **Linear** interpolation on octaves (bw is linear in display).
4. SetParam, write OK with `formatted=%s`.

#### Lua regex

Existing fix `param=([%w_]+)` already accepts underscore — `freq_hz` and `bw_oct` parse correctly without further regex change.

### `iem-mixer/iem-ui/src/components/eq_modal.rs`

Mirror the post-#194 gain layout:

#### Freq slider

Replace lines around 716-728:

```rust
<EqSlider
    value=Signal::derive(move || {
        // Single source of truth — REAPER's freq_hz, mapped to UI's log scale.
        let hz = freq_hz_sig.get().clamp(20.0, 24000.0);
        let log_min = 20.0_f32.ln();
        let log_max = 24000.0_f32.ln();
        (hz.ln() - log_min) / (log_max - log_min)
    })
    on_change=Callback::new(move |v: f32| {
        let now = js_sys::Date::now();
        if now - last_send_freq.get_untracked() > 50.0 {
            let _ = last_send_freq.try_set(now);
            let log_min = 20.0_f32.ln();
            let log_max = 24000.0_f32.ln();
            let hz = (log_min + v * (log_max - log_min)).exp();
            on_param_change.run((band_idx_sv.get_value(), "freq_hz".to_string(), hz));
            let _ = freq_hz_sig.try_set(hz);
        }
        let _ = curve_trigger.try_update(|n| *n += 1);
    })
    ...
/>
<span class="eq-param-value">
    {move || { curve_trigger.get(); format_freq(freq_hz_sig.get_untracked().clamp(20.0, 24000.0)) }}
</span>
```

Reset (around line 697-699): change to send Hz default per band type:

```rust
let default_freq_hz: f32 = match band_type_reset.as_str() {
    "lowshelf" => 100.0,
    "highshelf" => 8000.0,
    "highpass" => 80.0,
    "lowpass" => 12000.0,
    _ => 1000.0,
};
let _ = freq_hz_sig.try_set(default_freq_hz);
on_param_change.run((idx, "freq_hz".to_string(), default_freq_hz));
```

(Existing default norm constants → equivalent Hz via REAPER mapping; values above are typical ReaEQ defaults.)

#### BW slider

Same pattern. Slider position derives from `bw_sig` via `(bw - 0.01) / 3.99`. `on_change` computes `oct = 0.01 + v * 3.99`, sends `("bw_oct", oct)`. Reset sends default octaves per band type (typical 1.0 oct).

#### Deletions

- `norm_to_freq_hz` (line 59) — DELETE.
- `norm_to_bw` (line 85) — DELETE.
- Their unit tests (`test_norm_to_freq_hz_matches_reaper` and any bw equivalent) — DELETE.

#### Keep

- `freq_norm` and `bw_norm` fields in `EqBandState` and `EqBandLocal`. Preset save in `iem-mixer/iem-ui/src/pages/mixer/handlers.rs` (around line 160-163) reads them. Same lesson as #194 `gain_norm`.
- Read `fn=` and `bn=` from REAPER (already done in `parse_eq_band`).

### `iem-mixer/crates/iem-server/src/proxy.rs`

`parse_eq_band` is already strict on `fh=` and `bo=` post-#194 (lines 2558-2560). No change.

If a `freq_hz` or `bw_oct` request arrives, the proxy passes it unchanged to the ReaScript via the existing `eq_set` EXTSTATE path. No new endpoint.

### `iem-mixer/crates/iem-core/src/ws.rs`

`EqBand` struct already has `freq_hz` and `bw` fields (post-#194). No change.

## Dual-Protocol Compatibility

Preset and snapshot replay paths use the legacy `param=freq` (norm) and `param=bw` (norm) protocol via `iem-mixer/crates/iem-server/src/preset_routes.rs` and `snapshot_routes.rs`. They write norm directly. They keep working because the ReaScript still has the legacy norm branches (existing `param_offset` lookup).

Add inline comments at both sites:

```rust
// Dual-protocol: live UI uses param=freq_hz (Hz) via REAPER-truth interpolation
// (see set_eq_param.lua). Preset replay uses legacy param=freq (norm) for
// bit-exact restoration of saved presets. Same for bw.
```

## Tests

### ReaScript (no test framework, asserted via integration)

Live E2E exercises the path; mutation killers in Rust unit tests cover the deserialization side.

### Server unit tests (`proxy.rs`)

Already covered post-#194:

- `parse_eq_band` returns `None` when `fh=` missing (existing `test_parse_eq_band_lowshelf_missing_gd_returns_none` style).
- `parse_eq_band` returns `None` when `bo=` missing (add if not yet present).

### Live E2E (`iem-mixer/e2e/tests/live/eq.spec.ts`)

Add inside `EQ value sync - ENGINEER track` describe block:

```typescript
test("freq value persists across close+reopen on engineer band (#mirec-321)", async ({ page }) => {
    // Pre-arrange: set engineer band 1 freq to a specific norm via ReaScript
    // (norm 0.30 ≈ 322 Hz on REAPER's mapping).
    await fetch(`http://10.77.9.231/api/reaper/SET/EXTSTATE/reaperiem/eq_set/track=32%7Cband=1%7Cparam=freq%7Cvalue=0.300`);
    await fetch(`http://10.77.9.231/api/reaper/_RS_REAPERIEM_SET_EQ`);

    // Open EQ for ENGINEER inear, band 1.
    await openEqForChannel(page, "ENGINEER");
    // ... navigate to band 1, capture displayed freq text and slider thumb position.
    const freqText1 = await page.locator(...).textContent();
    const thumbStyle1 = await page.locator(...).getAttribute("style");
    const thumbPct1 = parsePctFromStyle(thumbStyle1);

    // Assert thumb position ↔ text agree (intrinsic, no REAPER cross-check).
    expect(thumbAndTextAgree(thumbPct1, freqText1)).toBe(true);

    // Close + reopen modal.
    await page.locator(...).click(); // close
    await openEqForChannel(page, "ENGINEER");

    const freqText2 = await page.locator(...).textContent();
    const thumbStyle2 = await page.locator(...).getAttribute("style");
    const thumbPct2 = parsePctFromStyle(thumbStyle2);

    // Assert displayed freq stable across reopen.
    expect(freqText2).toBe(freqText1);
    expect(Math.abs(thumbPct2 - thumbPct1)).toBeLessThan(0.5);
    // ... finally: restore original engineer EQ via stored snapshot.
});
```

Same test structure for bw if budget allows; otherwise scoped to freq-only and a separate bw test follows in a later PR.

### Mutation tests

Add unit-level mutation killers on default helpers if any are introduced (none planned — UI computes log/linear inline, no new constants).

## Safety / Production Constraint

Existing user EQ settings must not be corrupted. Engineer-only writes in live E2E (per `feedback_live_test_safety` memory). Pre-arrange + finally-block restoration. No band-member tracks touched.

The fix changes the protocol the UI uses to set freq/bw. It does NOT migrate existing stored norm values — REAPER's existing state is read via `fn=`/`bn=` (norm) and `fh=`/`bo=` (formatted) and presented unchanged. Users who never touch a slider after deploy see exactly what they had. Users who DO touch a slider get REAPER-truth round-trip (no drift). Preset replay is unchanged (uses norm protocol).

## Out of Scope

- Migrating preset/snapshot replay to Hz/oct protocol. Norm-based replay is bit-exact for saved data; no benefit.
- Slider companion mockup — no visual options to compare; same UI as #194 visually.
- gd_min/gd_max equivalent for freq/bw. UI uses fixed visible ranges (20-24000 Hz log, 0.01-4 oct linear). REAPER's full norm range maps inside these bounds; no plumbing of REAPER bounds needed.

## Verification

1. CI green (lint, test, build-wasm, e2e CI, build-tauri, deploy, post-deploy E2E).
2. Live E2E test passes against deployed iem.lan REAPER.
3. Manual smoke: open Mirec's EQ on ENGINEER track, drag freq slider to 321 Hz region, observe text settles to REAPER's truth (e.g. 321 or 320), close + reopen → same value displayed.
4. Production deploy verified at v1.166.0 (or next available) on `https://iem.newlevel.media/`.
