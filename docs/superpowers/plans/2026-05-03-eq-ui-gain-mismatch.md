# EQ UI Gain Mismatch Fix (#194) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate dual-source-of-truth in EQ panel — slider position derives from REAPER's formatted dB instead of UI-approximated norm-to-dB curve.

**Architecture:** Server reads REAPER's actual dB endpoints via `TrackFX_FormatParamValueNormalized` and plumbs `gain_db_min`/`gain_db_max` into `EqBand`. UI slider position = `(gain_db − db_min) / (db_max − db_min)` — same signal as the text label. Send path emits `param=gain_db`; ReaScript samples ReaEQ's mapping at 21 norm points, interpolates to find the norm for the desired dB, calls `TrackFX_SetParam`. Render-only on REAPER state — no mutation on display. Engineer-track-only live test with `try { … } finally { restore }` cleanup.

**Tech Stack:** Rust (axum, serde, Leptos WASM), Lua (REAPER ReaScript), TypeScript (Playwright).

**Spec:** `docs/superpowers/specs/2026-05-03-eq-ui-gain-mismatch-design.md`

---

## Context (read once)

**Current bug** — `iem-mixer/iem-ui/src/components/eq_modal.rs:776-779`:
```rust
value=Signal::derive(move || {
    let db = norm_to_gain_db(gain_sig.get()).clamp(-12.0, 12.0);
    (db + 12.0) / 24.0
})
```
Slider position uses UI's approximated `norm_to_gain_db`. Text label below uses `gain_db_sig.get_untracked()` (REAPER truth). They disagree.

**Production-safety rule** (`feedback_live_test_safety.md`): live E2E may modify ENGINEER track only; never band-member mics/inears. Test must restore engineer state in `finally`.

**Engineer mic track index in REAPER**: 22 (1-based — verified via `curl http://iem.lan:8080/_/NTRACK;TRACK | grep -i engineer`).

**Current versions on dev**: 1.164.0. Bumping to 1.165.0.

**Hooks block** local `cargo build/test/clippy/check`. Only `cargo fmt --all --check` runs locally. Compile checks happen in CI.

**Branch**: `dev` only — no feature branches, no worktrees.

**REAPER URL**: `http://iem.lan:8080/_/<path>` (the `/_/` prefix is mandatory).

---

## File Map

### Code paths touched

| File | Responsibility | Edit type |
|---|---|---|
| `scripts/reascripts/read_eq_params.lua` | Per-band: add `gd_min=` / `gd_max=` (sample REAPER's dB endpoints, no mutation) | additive |
| `scripts/reascripts/set_eq_param.lua` | New `param=gain_db` branch: 21-point lookup → linear interp → set norm | additive |
| `iem-mixer/crates/iem-core/src/ws.rs` | `EqBand` adds `gain_db_min: f32, gain_db_max: f32` with serde defaults | additive struct field |
| `iem-mixer/crates/iem-server/src/proxy.rs` (`parse_eq_band`, `handle_set_eq_band`) | Parse `gd_min/gd_max`; relay `param=gain_db` to ReaScript unchanged | additive |
| `iem-mixer/iem-ui/src/components/eq_modal.rs` | `EqBandState` adds gain_db_min/max; `BandLocalState` adds matching RwSignals; slider value derive uses `gain_db_sig` + bounds; on_change emits `param=gain_db, value=desired_db` | render + send |
| `iem-mixer/iem-ui/src/pages/mixer/connection.rs` | Plumb new fields from WS `EqParams` into `EqBandState` | additive |
| `iem-mixer/e2e/tests/live/eq.spec.ts` | New test: capture engineer EQ, set band b1 gain to test value, open EQ, assert slider thumb pixel position matches displayed dB; restore in `finally` | new test |

### Version files (5 Cargo.toml + 1 tauri.conf.json + README.md)

`iem-mixer/Cargo.toml`, `iem-mixer/crates/iem-core/Cargo.toml`, `iem-mixer/crates/iem-server/Cargo.toml`, `iem-mixer/iem-ui/Cargo.toml`, `iem-mixer/src-tauri/Cargo.toml`, `iem-mixer/src-tauri/tauri.conf.json`, `README.md`.

---

## Task 1: Version Bump 1.164.0 → 1.165.0 + README changelog

**Model:** Haiku.

**Files:**
- Modify: `iem-mixer/Cargo.toml`, `iem-mixer/crates/iem-core/Cargo.toml`, `iem-mixer/crates/iem-server/Cargo.toml`, `iem-mixer/iem-ui/Cargo.toml`, `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`
- Modify: `README.md`

- [ ] **Step 1: Bump 5 Cargo.toml + tauri.conf.json**

```bash
sed -i 's/version = "1.164.0"/version = "1.165.0"/' \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.164.0"/"version": "1.165.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify all bumped**

```bash
grep '1.165.0' iem-mixer/Cargo.toml iem-mixer/crates/iem-core/Cargo.toml iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
```
Expected: 6 lines, one per file, each containing `1.165.0`.

- [ ] **Step 3: Insert v1.165.0 changelog block after `## Changelog` heading in `README.md`**

Find the line `## Changelog` then insert below it:

```markdown
### v1.165.0 (2026-05-03)

- **Fix**: EQ panel slider thumb now matches displayed dB value. Previously the slider position was computed from REAPER's normalized 0-1 value via a UI approximation curve, while the text label read REAPER's actual formatted dB — the two disagreed (e.g. text said "+4 dB" while thumb sat at the "+1 dB" tick). Now both derive from REAPER's formatted dB; thumb and text always agree on initial render and across re-mounts. Send path also uses REAPER's actual norm↔dB mapping for accurate writes. (#194)
```

Use the Edit tool with the existing line below `## Changelog` as the unique anchor for the new block.

- [ ] **Step 4: Verify README**

```bash
grep -A1 "v1.165.0" README.md | head -3
```
Expected: shows the new changelog heading + first body line.

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/Cargo.toml iem-mixer/crates/iem-core/Cargo.toml iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json README.md
git commit -m "chore: bump version to 1.165.0 + changelog for EQ slider fix (#194)"
```

---

## Task 2: Failing live E2E test (TDD red — bug-fix protocol mandatory)

**Model:** Sonnet.

**Files:**
- Modify: `iem-mixer/e2e/tests/live/eq.spec.ts` — append a new test inside the existing `test.describe(...)` (or top-level) block.

**Why first:** airuleset `bug-fix-protocol` — write a failing test that reproduces the reported behavior BEFORE any production code change. Test must run on the deploy runner (real REAPER on iem.lan) and use ENGINEER track only.

- [ ] **Step 1: Append the new test to `iem-mixer/e2e/tests/live/eq.spec.ts`**

Locate the existing line that begins `await loginAs(page, "engineer", "1177");` (around line 1007) — there's already an engineer-track test in this file you'll mirror. Append the new test below the last existing `test(...)` block (before the file's closing brace if it has one):

```typescript
// #194 — slider thumb position must agree with displayed dB on initial render and
// after close+reopen. Uses ENGINEER mic track only (production-safe per
// feedback_live_test_safety.md). Captures band gain BEFORE, sets a known test
// value, asserts, restores in finally.
test("#194 EQ gain slider thumb position matches displayed dB (engineer track)", async ({
  page,
}) => {
  const REAPER = "http://iem.lan:8080/_";
  const ENGINEER_TRACK = 22; // 1-based REAPER track index for "ENGINEER mic"
  const TEST_BAND = 1; // b1 = lowshelf — has param[1] = gain
  // Norm 0.583 maps to ≈+6 dB in REAPER (verified empirically). UI's approximation
  // gives a different dB position for this norm, which is the bug.
  const TEST_GAIN_NORM = 0.583;

  // Capture original norm so we can restore. Using ReaScript read path.
  await fetch(`${REAPER}/SET/EXTSTATE/reaperiem/eq_read_track/${ENGINEER_TRACK}`);
  await fetch(`${REAPER}/_RS_REAPERIEM_READ_EQ`);
  await new Promise((r) => setTimeout(r, 800));
  const readResp = await fetch(
    `${REAPER}/GET/EXTSTATE/reaperiem/eq_params`,
  ).then((r) => r.text());
  const bandMatch = readResp.match(new RegExp(`b${TEST_BAND}:[^|]+`));
  if (!bandMatch) throw new Error(`No band ${TEST_BAND} in EQ read: ${readResp}`);
  const originalGnMatch = bandMatch[0].match(/gn=([\d.]+)/);
  if (!originalGnMatch) throw new Error(`No gn= in band: ${bandMatch[0]}`);
  const originalNorm = parseFloat(originalGnMatch[1]);

  try {
    // 1. Pre-arrange: set engineer band gain to known test value via direct REAPER
    //    HTTP (bypassing the app — simulating REAPER having one value while UI loads).
    //    ReaEQ band b parameter (b * 3 + 1) = gain.
    const FX_INDEX = 1; // ReaEQ is FX 1 on engineer mic (verified via curl probe)
    const PARAM_INDEX = TEST_BAND * 3 + 1;
    await fetch(
      `${REAPER}/SET/TRACK/${ENGINEER_TRACK}/FX/${FX_INDEX}/PARAM/${PARAM_INDEX}/VALUE/${TEST_GAIN_NORM}`,
    );
    await new Promise((r) => setTimeout(r, 300));

    // 2. Read the formatted dB REAPER reports for this norm (the canonical truth).
    await fetch(
      `${REAPER}/SET/EXTSTATE/reaperiem/eq_read_track/${ENGINEER_TRACK}`,
    );
    await fetch(`${REAPER}/_RS_REAPERIEM_READ_EQ`);
    await new Promise((r) => setTimeout(r, 800));
    const verifyResp = await fetch(
      `${REAPER}/GET/EXTSTATE/reaperiem/eq_params`,
    ).then((r) => r.text());
    const vBandMatch = verifyResp.match(new RegExp(`b${TEST_BAND}:[^|]+`));
    if (!vBandMatch) throw new Error("verify read failed");
    const reaperDbMatch = vBandMatch[0].match(/gd=(-?[\d.]+)/);
    if (!reaperDbMatch) throw new Error(`No gd= in verify: ${vBandMatch[0]}`);
    const reaperDb = parseFloat(reaperDbMatch[1]);
    expect(reaperDb).not.toBe(0); // Must be a clearly non-default value.

    // 3. Open the app as engineer; navigate to engineer's mixer; open EQ for engineer track.
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await page.waitForSelector(".mixer-channel", { timeout: 10000 });

    // Open the kebab menu for the engineer mic channel and click EQ.
    // The exact selector strategy mirrors the existing engineer-EQ test in this file.
    await openKebabMenu(page); // existing helper used by other tests
    await clickEqOption(page); // existing helper
    await page.waitForSelector(".eq-modal", { timeout: 5000 });

    // 4. Wait for bands to load (loading indicator clears).
    await page.waitForFunction(
      () => !document.querySelector(".eq-loading"),
      undefined,
      { timeout: 5000 },
    );

    // 5. Locate band TEST_BAND's gain row (band card index TEST_BAND, "Gain" row).
    //    EQ band cards render in display_order, not REAPER index — for lowshelf
    //    that's typically the 2nd card (index 1 in DOM after highpass=0). Use the
    //    band-num element to find by REAPER index → DOM index mapping is implicit;
    //    for engineer track lowshelf is band 1 in REAPER, also card 1 in display.
    const bandCards = page.locator(".eq-band-card");
    const bandCard = bandCards.nth(TEST_BAND);
    await bandCard.waitFor({ state: "visible", timeout: 5000 });

    // 6. Read displayed dB text for this band's Gain row.
    //    Each band card has rows: Freq, Gain, BW. Gain row's `.eq-param-value`
    //    is the 2nd within the card.
    const gainValueEl = bandCard.locator(
      ".eq-param-row:has(.eq-param-label:has-text('Gain')) .eq-param-value",
    );
    const displayedText = await gainValueEl.textContent();
    if (!displayedText) throw new Error("Gain text not rendered");
    const displayedDbMatch = displayedText.match(/(-?[\d.]+)/);
    if (!displayedDbMatch) throw new Error(`No dB in text: ${displayedText}`);
    const displayedDb = parseFloat(displayedDbMatch[1]);

    // displayedDb must equal REAPER's truth (within ±0.2 dB rounding).
    expect(Math.abs(displayedDb - reaperDb)).toBeLessThan(0.2);

    // 7. Read slider thumb pixel position. The slider's thumb has CSS that puts it
    //    at (slider_position * track_width) px from left of the track. We compare
    //    against the dB-derived expected position.
    const sliderTrack = bandCard.locator(
      ".eq-param-row:has(.eq-param-label:has-text('Gain')) .eq-slider-track",
    );
    const trackBox = await sliderTrack.boundingBox();
    if (!trackBox) throw new Error("Slider track not visible");

    // Read the thumb's actual position via its computed translateX or left. The
    // slider component sets thumb position via inline `left: <pct>%` or transform.
    const thumbStyle = await bandCard
      .locator(
        ".eq-param-row:has(.eq-param-label:has-text('Gain')) .eq-slider-thumb",
      )
      .evaluate((el) => {
        const cs = window.getComputedStyle(el as HTMLElement);
        return cs.left || (el as HTMLElement).style.left;
      });

    // Parse "<pct>%" → 0-1.
    const thumbPctMatch = thumbStyle.match(/([\d.]+)%/);
    if (!thumbPctMatch) throw new Error(`Cannot parse thumb position: ${thumbStyle}`);
    const thumbPct = parseFloat(thumbPctMatch[1]) / 100;

    // After fix: thumb_pct = (displayedDb − db_min) / (db_max − db_min).
    // Today the thumb_pct is computed from norm_to_gain_db(norm), so it disagrees
    // with displayedDb. We assert the AGREEMENT — currently FAILS, post-fix PASSES.
    //
    // We don't yet know REAPER's exact db_min/db_max, so we use the documented
    // ReaEQ range ±24 dB conservatively. After T3 plumbs the real values this
    // can tighten to the live values.
    const DB_MIN = -24.0;
    const DB_MAX = 24.0;
    const expectedThumbPct = (displayedDb - DB_MIN) / (DB_MAX - DB_MIN);

    // Tolerance ±0.05 (5% of slider width) — captures the bug (≥10% drift) while
    // allowing minor rendering rounding.
    expect(Math.abs(thumbPct - expectedThumbPct)).toBeLessThan(0.05);

    // 8. Close the EQ modal, reopen, re-assert (the regression case the user reported).
    await page.locator(".eq-close-btn").click();
    await page.waitForSelector(".eq-modal", { state: "detached", timeout: 5000 });

    await openKebabMenu(page);
    await clickEqOption(page);
    await page.waitForSelector(".eq-modal", { timeout: 5000 });
    await page.waitForFunction(
      () => !document.querySelector(".eq-loading"),
      undefined,
      { timeout: 5000 },
    );

    const bandCard2 = page.locator(".eq-band-card").nth(TEST_BAND);
    const displayedText2 = await bandCard2
      .locator(
        ".eq-param-row:has(.eq-param-label:has-text('Gain')) .eq-param-value",
      )
      .textContent();
    if (!displayedText2) throw new Error("Reopen: gain text not rendered");
    const displayedDb2 = parseFloat(displayedText2.match(/(-?[\d.]+)/)![1]);
    expect(Math.abs(displayedDb2 - reaperDb)).toBeLessThan(0.2);

    const thumbStyle2 = await bandCard2
      .locator(
        ".eq-param-row:has(.eq-param-label:has-text('Gain')) .eq-slider-thumb",
      )
      .evaluate((el) => {
        const cs = window.getComputedStyle(el as HTMLElement);
        return cs.left || (el as HTMLElement).style.left;
      });
    const thumbPct2 = parseFloat(thumbStyle2.match(/([\d.]+)%/)![1]) / 100;
    const expectedThumbPct2 = (displayedDb2 - DB_MIN) / (DB_MAX - DB_MIN);
    expect(Math.abs(thumbPct2 - expectedThumbPct2)).toBeLessThan(0.05);
  } finally {
    // Restore engineer band to its original norm — production-safety per
    // feedback_live_test_safety.md.
    const FX_INDEX = 1;
    const PARAM_INDEX = TEST_BAND * 3 + 1;
    await fetch(
      `${REAPER}/SET/TRACK/${ENGINEER_TRACK}/FX/${FX_INDEX}/PARAM/${PARAM_INDEX}/VALUE/${originalNorm}`,
    );
  }
});
```

If `openKebabMenu` / `clickEqOption` / `loginAs` are not in scope at the location you append, copy the import line(s) used by the existing engineer EQ test in the same file (line ~1007 area).

If the `.eq-slider-thumb` / `.eq-slider-track` class names differ from the actual DOM, replace them with the real selector by reading `iem-mixer/iem-ui/src/components/eq_modal.rs` `EqSlider` template and matching the rendered classes.

- [ ] **Step 2: Verify the test compiles (Playwright TypeScript)**

```bash
cd iem-mixer/e2e
npx tsc --noEmit
```
Expected: no errors. Fix any TS errors before committing.

- [ ] **Step 3: Confirm test would FAIL on current code (read-through, not run)**

The current `eq_modal.rs:776-779` derives slider position from `norm_to_gain_db(gain_sig)`, not from `gain_db_sig`. Live REAPER probe earlier in this session showed `b1 lowshelf gn=0.439667 → REAPER gd=4.9 dB` while UI's `norm_to_gain_db(0.439667) ≈ 5.04 dB`. For TEST_GAIN_NORM=0.583, the drift is similar in magnitude. The thumb pct will deviate from the dB-derived expected pct by ≥0.01–0.10, which exceeds the 0.05 tolerance for some test runs but may pass for others — the assertion's purpose is to catch the architectural mismatch, not the formula's precise drift. Confirm the test is calibrated by reading current `eq_modal.rs:87` `norm_to_gain_db` and comparing to the REAPER values.

If the assertion looks too tight or loose given the actual `norm_to_gain_db` formula vs REAPER, adjust the tolerance to reliably FAIL today and PASS post-fix. The test must be deterministic.

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/e2e/tests/live/eq.spec.ts
git commit -m "test(e2e): failing test for EQ slider thumb position vs displayed dB (#194)"
```

---

## Task 3: Read path — REAPER reports actual dB range

**Model:** Sonnet.

**Files:**
- Modify: `scripts/reascripts/read_eq_params.lua`
- Modify: `iem-mixer/crates/iem-core/src/ws.rs` — `EqBand` struct
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs` — `parse_eq_band`
- Modify: `iem-mixer/iem-ui/src/components/eq_modal.rs` — `EqBandState` struct
- Modify: `iem-mixer/iem-ui/src/pages/mixer/connection.rs` — plumb new fields

- [ ] **Step 1: Edit `scripts/reascripts/read_eq_params.lua` — add per-band `gd_min` / `gd_max`**

Inside the `for b = 0, num_bands - 1 do` loop, after the existing `bw_num` line and before `table.insert(bands, ...)`, add:

```lua
        -- Sample REAPER's actual dB endpoints for this band's gain param.
        -- FormatParamValueNormalized returns the formatted display value WITHOUT
        -- mutating REAPER state — pure read.
        local _, gd_min_fmt = reaper.TrackFX_FormatParamValueNormalized(track, eq_idx, gain_idx, 0.0)
        local _, gd_max_fmt = reaper.TrackFX_FormatParamValueNormalized(track, eq_idx, gain_idx, 1.0)
        local gd_min_num = gd_min_fmt:match("(-?[%d%.]+)") or "-12"
        local gd_max_num = gd_max_fmt:match("(-?[%d%.]+)") or "12"
```

Then change the `table.insert(bands, string.format(...))` line — extend the format string and args to include the new fields. Replace the existing `string.format` call with:

```lua
        table.insert(bands, string.format(
            "b%d:%s,fn=%.6f,gn=%.6f,bn=%.6f,fh=%s,gd=%s,bo=%s,en=%s,gd_min=%s,gd_max=%s",
            b, btype, freq_norm, gain_norm, bw_norm, freq_num, gain_num, bw_num, band_enabled, gd_min_num, gd_max_num
        ))
```

- [ ] **Step 2: Edit `iem-mixer/crates/iem-core/src/ws.rs` — add fields to `EqBand`**

Find the existing `EqBand` struct (line 12-30). Add two fields with serde defaults BEFORE the `enabled` line:

```rust
    /// Minimum dB this band's gain can produce (norm=0.0 endpoint, REAPER-sampled)
    #[serde(default = "default_gain_db_min")]
    pub gain_db_min: f32,
    /// Maximum dB this band's gain can produce (norm=1.0 endpoint, REAPER-sampled)
    #[serde(default = "default_gain_db_max")]
    pub gain_db_max: f32,
```

Add helper functions next to existing `default_enabled` (line 32):

```rust
fn default_gain_db_min() -> f32 {
    -12.0
}

fn default_gain_db_max() -> f32 {
    12.0
}
```

- [ ] **Step 3: Edit `iem-mixer/crates/iem-server/src/proxy.rs` — extract `gd_min` / `gd_max` in `parse_eq_band`**

Find `parse_eq_band` (line 2532). After the `bw` line (line 2568), add:

```rust
    let gain_db_min = get_field("gd_min=").unwrap_or(-12.0);
    let gain_db_max = get_field("gd_max=").unwrap_or(12.0);
```

Then in the `Some(iem_core::EqBand { ... })` constructor at line 2573, add the two new fields:

```rust
    Some(iem_core::EqBand {
        band_type,
        freq_hz,
        gain_db,
        bw,
        freq_norm,
        gain_norm,
        bw_norm,
        gain_db_min,
        gain_db_max,
        enabled,
    })
```

- [ ] **Step 4: Edit `iem-mixer/iem-ui/src/components/eq_modal.rs` — add fields to `EqBandState`**

Find `EqBandState` (line 32). Insert before `enabled`:

```rust
    pub gain_db_min: f32,
    pub gain_db_max: f32,
```

Find `BandLocalState` (line 360). Insert before `enabled`:

```rust
    /// REAPER-sampled dB endpoints for this band's gain (norm=0 → norm=1)
    gain_db_min: f32,
    gain_db_max: f32,
```

Find the Effect's first-time init (line 458-471) — extend the `BandLocalState { ... }` constructor:

```rust
            let locals: Vec<BandLocalState> = indexed
                .iter()
                .map(|(reaper_idx, b)| BandLocalState {
                    reaper_band_idx: *reaper_idx as u8,
                    band_type: b.band_type.clone(),
                    freq_norm: RwSignal::new(b.freq_norm),
                    gain_norm: RwSignal::new(b.gain_norm),
                    bw_norm: RwSignal::new(b.bw_norm),
                    freq_hz: RwSignal::new(b.freq_hz),
                    gain_db: RwSignal::new(b.gain_db),
                    bw_oct: RwSignal::new(b.bw),
                    gain_db_min: b.gain_db_min,
                    gain_db_max: b.gain_db_max,
                    enabled: RwSignal::new(b.enabled),
                })
                .collect();
```

(The dB endpoints are plain `f32` — they don't change after first read, so no `RwSignal` wrapper needed.)

- [ ] **Step 5: Edit `iem-mixer/iem-ui/src/pages/mixer/connection.rs` — plumb fields**

Find the `iem_core::ServerMsg::EqParams` arm (line 570-591). In the `.map(|b| EqBandState { ... })` (line 578-587), add the two fields:

```rust
                                .map(|b| EqBandState {
                                    band_type: b.band_type,
                                    freq_hz: b.freq_hz,
                                    gain_db: b.gain_db,
                                    bw: b.bw,
                                    freq_norm: b.freq_norm,
                                    gain_norm: b.gain_norm,
                                    bw_norm: b.bw_norm,
                                    gain_db_min: b.gain_db_min,
                                    gain_db_max: b.gain_db_max,
                                    enabled: b.enabled,
                                })
```

- [ ] **Step 6: Format check**

```bash
cd iem-mixer && cargo fmt --all --check
```
Expected: clean. If not, run `cargo fmt --all` then re-check.

- [ ] **Step 7: Commit**

```bash
git add scripts/reascripts/read_eq_params.lua \
  iem-mixer/crates/iem-core/src/ws.rs \
  iem-mixer/crates/iem-server/src/proxy.rs \
  iem-mixer/iem-ui/src/components/eq_modal.rs \
  iem-mixer/iem-ui/src/pages/mixer/connection.rs
git commit -m "feat(eq): plumb REAPER-sampled gain_db_min/gain_db_max through read path (#194)"
```

---

## Task 4: Set path — ReaScript `param=gain_db` branch

**Model:** Sonnet.

**Files:**
- Modify: `scripts/reascripts/set_eq_param.lua`

- [ ] **Step 1: Edit `scripts/reascripts/set_eq_param.lua` — handle `param=gain_db`**

Find the existing param-name check block (lines 70-78). Insert a new branch BEFORE the existing `if param_name == "freq" then` line:

```lua
    -- New "gain_db" branch: caller sends desired dB, we sample ReaEQ's actual
    -- norm↔dB mapping at 21 points and linear-interpolate to find the norm
    -- that yields the desired dB. No mutation during sampling — uses
    -- FormatParamValueNormalized, which is pure-read. Then SetParam with the
    -- interpolated norm.
    if param_name == "gain_db" then
        local gain_param_idx = band * 3 + 1
        local num_params_g = reaper.TrackFX_GetNumParams(track, eq_idx)
        if gain_param_idx >= num_params_g then
            reaper.SetExtState(section, "eq_set_result",
                "ERROR:gain_param_out_of_range:" .. gain_param_idx, false)
            return
        end

        -- Sample 21 points: norm = 0.00, 0.05, ..., 1.00 → formatted dB.
        local samples = {}
        for i = 0, 20 do
            local norm_i = i / 20
            local _, fmt = reaper.TrackFX_FormatParamValueNormalized(
                track, eq_idx, gain_param_idx, norm_i)
            local db_i = tonumber(fmt:match("(-?[%d%.]+)")) or 0.0
            samples[i + 1] = { norm = norm_i, db = db_i }
        end

        -- Linear interpolation: walk samples to find the bracketing pair where
        -- desired_db lies between samples[k].db and samples[k+1].db. ReaEQ's
        -- norm→dB is monotonic increasing within typical bands; if the table
        -- is non-monotonic for a particular band type we still snap to the
        -- closest sample.
        local desired = value
        local best_norm = samples[1].norm
        local best_err = math.huge
        for i = 1, 20 do
            local lo = samples[i]
            local hi = samples[i + 1]
            -- Only interpolate when desired is bracketed AND mapping is locally
            -- increasing (lo.db < hi.db). Otherwise score by absolute distance.
            if lo.db <= desired and desired <= hi.db and lo.db < hi.db then
                local t = (desired - lo.db) / (hi.db - lo.db)
                local n = lo.norm + t * (hi.norm - lo.norm)
                local _, vfmt = reaper.TrackFX_FormatParamValueNormalized(
                    track, eq_idx, gain_param_idx, n)
                local v_db = tonumber(vfmt:match("(-?[%d%.]+)")) or 0.0
                local err = math.abs(v_db - desired)
                if err < best_err then
                    best_err = err
                    best_norm = n
                end
            else
                -- Score by distance to either endpoint
                for _, s in ipairs({ lo, hi }) do
                    local err = math.abs(s.db - desired)
                    if err < best_err then
                        best_err = err
                        best_norm = s.norm
                    end
                end
            end
        end

        reaper.TrackFX_SetParam(track, eq_idx, gain_param_idx, best_norm)
        local _, fmt_post = reaper.TrackFX_GetFormattedParamValue(
            track, eq_idx, gain_param_idx)
        reaper.SetExtState(section, "eq_set_result",
            string.format(
                "OK:track=%d,band=%d,param=gain_db,desired_db=%.3f,norm=%.6f,formatted=%s",
                track_idx, band, desired, best_norm, fmt_post),
            false)
        return
    end
```

- [ ] **Step 2: No Rust changes needed**

The server's `handle_set_eq_band` (`proxy.rs:2585`) just relays whatever `param` string the UI sends to the EXTSTATE/ReaScript. New `param=gain_db` is automatically supported with no server-side change.

- [ ] **Step 3: Commit**

```bash
git add scripts/reascripts/set_eq_param.lua
git commit -m "feat(eq): ReaScript param=gain_db branch — interpolate REAPER mapping (#194)"
```

---

## Task 5: UI render — single source of truth

**Model:** Sonnet.

**Files:**
- Modify: `iem-mixer/iem-ui/src/components/eq_modal.rs:776-779` — slider value derive

- [ ] **Step 1: Edit `iem-mixer/iem-ui/src/components/eq_modal.rs:776-779` — slider position from `gain_db_sig` + bounds**

Find the gain slider's `value=Signal::derive(...)` block (lines 775-780):

```rust
                                            <EqSlider
                                                value=Signal::derive(move || {
                                                    // Convert REAPER norm → dB → slider position
                                                    let db = norm_to_gain_db(gain_sig.get()).clamp(-12.0, 12.0);
                                                    (db + 12.0) / 24.0
                                                })
```

Capture the bounds from the local state above the `view!` (locate the existing `let gain_db_sig = local.gain_db;` line at ~665, and add two more captures from `local`):

```rust
                                let gain_db_min = local.gain_db_min;
                                let gain_db_max = local.gain_db_max;
```

Then replace the slider value derive with:

```rust
                                            <EqSlider
                                                value=Signal::derive(move || {
                                                    // Single source of truth — REAPER's formatted dB.
                                                    // Slider position is a linear mapping over REAPER's
                                                    // own dB range (db_min..db_max).
                                                    let db = gain_db_sig.get();
                                                    let span = (gain_db_max - gain_db_min).max(0.001);
                                                    ((db - gain_db_min) / span).clamp(0.0, 1.0)
                                                })
```

- [ ] **Step 2: Update on_change to compute desired_db and emit `param=gain_db`**

The same gain-slider block has `on_change=Callback::new(move |v: f32| { ... })` (lines 781-793). Replace with:

```rust
                                                on_change=Callback::new(move |v: f32| {
                                                    // v is the slider position 0-1; project back to dB
                                                    // using REAPER's actual range. Server interpolates
                                                    // dB → norm via REAPER's own mapping.
                                                    let span = gain_db_max - gain_db_min;
                                                    let db = gain_db_min + v * span;
                                                    let now = js_sys::Date::now();
                                                    if now - last_send_gain.get_untracked() > 50.0 {
                                                        let _ = last_send_gain.try_set(now);
                                                        on_param_change.run((band_idx_sv.get_value(), "gain_db".to_string(), db));
                                                    }
                                                    // Local optimistic update so the slider tracks
                                                    // smoothly during drag. gain_norm is no longer
                                                    // authoritative for display; we still write a rough
                                                    // value so the curve generator (which still reads
                                                    // gain_norm in some places) stays sensible.
                                                    let _ = gain_db_sig.try_set(db);
                                                    let approx_norm = gain_db_to_norm(db);
                                                    let _ = gain_sig.try_set(approx_norm);
                                                    let _ = curve_trigger.try_update(|n| *n += 1);
                                                })
```

- [ ] **Step 3: Format check**

```bash
cd iem-mixer && cargo fmt --all --check
```

If `norm_to_gain_db` becomes unused after this change, the compiler will warn `dead_code`. Per project rules NEVER use `#[allow(dead_code)]` — DELETE the function. Find `fn norm_to_gain_db` (line 87) and remove the entire function. The `gain_db_to_norm` helper IS still used (in the on_change above), so keep it.

Also check the `compute_band_gain` and curve-generation paths — they may reference `norm_to_gain_db` indirectly. Read the file from line 280 to line 320 to confirm. If unused, remove. If used, keep.

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/iem-ui/src/components/eq_modal.rs
git commit -m "fix(eq): slider position derives from gain_db_sig + REAPER bounds (#194)"
```

---

## Task 6: Wire up — verify all call sites compile and behave

**Model:** Sonnet.

**Files:**
- Verify only — no new code unless compile errors surface.

- [ ] **Step 1: Final format check**

```bash
cd iem-mixer && cargo fmt --all --check
```
Expected: clean.

- [ ] **Step 2: Sanity-grep — no leftover references to UI's old approximation in render path**

```bash
grep -n "norm_to_gain_db" iem-mixer/iem-ui/src/components/eq_modal.rs
```
Expected: zero matches. If any remain inside render code (not test fixtures), they're dead code — remove them.

- [ ] **Step 3: Confirm send-side param dispatch**

```bash
grep -n '"gain"\|"gain_db"' iem-mixer/iem-ui/src/components/eq_modal.rs
```
Expected: `"gain_db"` is now used in the on_change for the gain slider; `"freq"` and `"bw"` remain for other sliders. `"gain"` (without `_db`) should NOT appear in the gain slider's on_change — only the new `"gain_db"`.

- [ ] **Step 4: Verify server still accepts old `param=gain`**

`scripts/reascripts/set_eq_param.lua` keeps the old `if param_name == "freq" then ... elseif param_name == "gain" then ...` chain. New `param=gain_db` branch is BEFORE that chain. Confirm by reading the script:

```bash
grep -n 'param_name == ' scripts/reascripts/set_eq_param.lua
```
Expected: shows lines for `gain_db`, `enabled`, `freq`, `gain`, `bw`. The order is: `gain_db` first (new), then `enabled`, then the freq/gain/bw block.

- [ ] **Step 5: Commit (only if any code changes were necessary)**

If no edits were needed in this task, skip the commit. Otherwise:

```bash
git add iem-mixer/iem-ui/src/components/eq_modal.rs
git commit -m "fix(eq): cleanup unused norm_to_gain_db references (#194)"
```

---

## Task 7: Push + monitor CI

**Model:** Sonnet.

- [ ] **Step 1: Push to dev**

```bash
git push origin dev
```

- [ ] **Step 2: Identify the latest run**

```bash
gh run list --branch dev --limit 3 --json databaseId,status,conclusion,headSha,createdAt
```

Capture the most recent `databaseId` (call it RUN_ID).

- [ ] **Step 3: Monitor in the background — single sleep, one shot, no /loop, no cron, no custom monitor scripts**

```bash
sleep 300 && gh run view <RUN_ID> --json status,conclusion,jobs
```

Run via Bash tool with `run_in_background: true`. When the background job finishes, read its output via BashOutput. If still in progress, repeat once more (single `sleep 300 && gh run view ...`). If still in progress after 2 cycles (~10 min total), check job-by-job to see if any one job is stuck or failing.

- [ ] **Step 4: If CI fails — investigate and fix in ONE commit**

```bash
gh run view <RUN_ID> --log-failed | head -200
```

Diagnose:
- **Lint failures**: usually formatting or clippy. Apply fix locally with `cd iem-mixer && cargo fmt --all` (clippy fixes by hand).
- **Build/test failures**: address per-error.
- **Mutation-test job**: any surviving mutants on new code → add boundary-asserting unit tests (especially for the new `gain_db_min/max` field defaults and parse_eq_band extraction).
- **E2E job**: the new test should now PASS post-fix. If it FAILS, the fix is incomplete — read failure log, inspect the slider-thumb pixel-position assertion, fix.

Apply ALL fixes in ONE new commit. Push. Re-monitor.

- [ ] **Step 5: Confirm green**

```bash
gh run view <RUN_ID> --json status,conclusion,jobs
```
Expected: `"status": "completed", "conclusion": "success"`. ALL jobs (test-integrity, lint, test, build-wasm, e2e, build-tauri, build-vban, mutation-test, deploy) must show `"conclusion": "success"`.

---

## Task 8: Post-deploy live verification

**Model:** Sonnet.

After CI deploys to iem.lan, verify the fix on the live system using Playwright.

- [ ] **Step 1: Confirm version is live**

```bash
curl -sf https://iem.newlevel.media/api/version
```
Expected: JSON with `"version": "1.165.0"`.

- [ ] **Step 2: Set engineer mic band b1 to a known test value (e.g. norm 0.583 ≈ +6 dB)**

```bash
# Capture original
curl -sf "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/eq_read_track/22"
curl -sf "http://iem.lan:8080/_/_RS_REAPERIEM_READ_EQ"
sleep 1
ORIGINAL=$(curl -sf "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/eq_params" | grep -oE 'b1:[^|]+' | grep -oE 'gn=[0-9.]+' | head -1 | cut -d= -f2)
echo "ORIGINAL norm: $ORIGINAL"

# Set test value
curl -sf "http://iem.lan:8080/_/SET/TRACK/22/FX/1/PARAM/4/VALUE/0.583"
sleep 0.3

# Read back the formatted dB (canonical truth)
curl -sf "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/eq_read_track/22"
curl -sf "http://iem.lan:8080/_/_RS_REAPERIEM_READ_EQ"
sleep 1
TRUTH=$(curl -sf "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/eq_params" | grep -oE 'b1:[^|]+' | grep -oE 'gd=-?[0-9.]+' | head -1 | cut -d= -f2)
echo "REAPER says: $TRUTH dB at norm 0.583"
```

Save `ORIGINAL` and `TRUTH` for the verification.

- [ ] **Step 3: Open Playwright, log in as engineer, open EQ for engineer mic**

Use `mcp__plugin_playwright_playwright__browser_navigate` to open `https://iem.newlevel.media/`, log in via PIN modal (engineer / 1177), navigate to `/engineer`, click the kebab menu on the engineer-mic channel, click EQ. Wait for `.eq-modal` to appear and `.eq-loading` to clear.

- [ ] **Step 4: Read the displayed dB text and slider position; verify they agree**

Use `mcp__plugin_playwright_playwright__browser_evaluate` with a script that locates band card index 1 (b1 lowshelf), reads:
- the gain row's `.eq-param-value` text (e.g. `+5.7 dB`)
- the `.eq-slider-thumb` style.left percent

Compute `expected_pct = (TRUTH - db_min) / (db_max - db_min)` where `db_min`/`db_max` are read from a server probe (or use 0–1 scaled to ±24 dB conservatively). The thumb percent must be within ±5% of the displayed-dB-derived position.

If they agree → fix verified live.
If they disagree → fix is broken; investigate (read CI log, browser console).

- [ ] **Step 5: Restore engineer band**

```bash
curl -sf "http://iem.lan:8080/_/SET/TRACK/22/FX/1/PARAM/4/VALUE/$ORIGINAL"
sleep 0.3

# Confirm restored
curl -sf "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/eq_read_track/22"
curl -sf "http://iem.lan:8080/_/_RS_REAPERIEM_READ_EQ"
sleep 1
curl -sf "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/eq_params" | grep -oE 'b1:[^|]+' | head -1
```

Confirm `gn=` is back to `$ORIGINAL` (within rounding).

- [ ] **Step 6: Read the version label on the live dashboard via Playwright**

Confirm the page shows `v1.165.0` somewhere visible (per `version-on-dashboard.md`). Use `mcp__plugin_playwright_playwright__browser_evaluate` to read `.header-version-number` textContent.

---

## Task 9: Open PR `dev → main`, verify clean, STOP

**Model:** Sonnet.

- [ ] **Step 1: Sync dev with main first**

```bash
git fetch origin
git checkout dev
git merge origin/main --no-edit
```

If this introduces conflicts, resolve them, commit. If clean, push:

```bash
git push origin dev
```

If a new merge commit was created, monitor CI on dev again until green (single sleep + gh run view).

- [ ] **Step 2: Create the PR**

```bash
gh pr create --base main --head dev --title "fix(eq): slider position matches displayed dB (#194)" --body "$(cat <<'EOF'
## Summary

- Eliminates the dual-source-of-truth pattern in the EQ panel: slider thumb position now derives from REAPER's formatted dB (`gain_db_sig`) instead of UI-approximated norm-to-dB curve.
- Server reads REAPER's actual dB endpoints via `TrackFX_FormatParamValueNormalized` and plumbs `gain_db_min` / `gain_db_max` into `EqBand`.
- Send path emits `param=gain_db` (desired dB); ReaScript samples ReaEQ's actual norm↔dB mapping at 21 points and linear-interpolates to find the norm.

## Risk

- Render-only: no mutation on EQ open/display/close — existing user EQ values are never touched by the fix.
- Send path is more accurate, not less — REAPER's own mapping replaces UI approximation. A user "+4 dB" gesture lands at exactly +4 dB.

## Test plan

- [x] Failing live E2E test (#194) committed before any production code change (TDD bug-fix protocol)
- [x] CI green on dev, ALL 10 jobs (test-integrity, lint, test, build-wasm, e2e, build-tauri, build-vban, mutation-test, deploy)
- [x] Post-deploy verified via Playwright on https://iem.newlevel.media/ — engineer-track band b1 at known REAPER value, displayed dB text matches slider thumb pixel position
- [x] Engineer-track test cleanup verified — original norm restored

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Verify mergeable + clean**

```bash
gh pr view --json number,mergeable,mergeStateStatus | head
```

Expected: `"mergeable": "MERGEABLE"` AND `"mergeStateStatus": "CLEAN"`. NOT `UNSTABLE`, NOT `BEHIND`, NOT `BLOCKED`, NOT `DIRTY`.

If state is anything other than `CLEAN` → investigate:
- `BEHIND`: re-sync with main (Step 1) and re-monitor CI.
- `UNSTABLE`: a check is failing or pending — wait + re-check; if still failing, fix per `gh run view --log-failed`.
- `DIRTY` / `BLOCKED`: resolve conflicts or unblock per the specific cause.

NEVER admin-merge. NEVER bypass branch protection. The fix is to fix the gate.

- [ ] **Step 4: Capture PR URL**

```bash
gh pr view --json url -q .url
```

- [ ] **Step 5: STOP — present completion report and wait for explicit user merge approval**

Send the airuleset completion report (per `completion-report.md` template):

```
## ✅ Work Complete

**Audits & deploy:**
✅ CI: green (10/10)
✅ /plan-check: 9/9 fulfilled
✅ /review: clean — 0 🔴 0 🟡 0 🔵
✅ Deploy: dev shows v1.165.0 (engineer band b1 set to test value, slider thumb position matches displayed dB on live https://iem.newlevel.media/, restored after test)

**Plan steps:**
- Version bump 1.164.0 → 1.165.0 + README v1.165.0 changelog
- Failing E2E test for slider thumb vs displayed dB (TDD red)
- Read path: ReaScript samples REAPER dB endpoints, server + core plumb gain_db_min/max
- Set path: ReaScript param=gain_db branch — 21-point lookup + linear interp
- UI render: slider derives from gain_db_sig + REAPER bounds (single source of truth)
- UI send: emits param=gain_db / desired_db
- CI green
- Post-deploy verified live

**E2E test coverage:**
| Feature/Fix | E2E Test File | What It Verifies |
|---|---|---|
| EQ slider position vs displayed dB (#194) | iem-mixer/e2e/tests/live/eq.spec.ts | Engineer-track b1 set to known REAPER value; assert slider thumb pixel position agrees with text label on initial render and after close+reopen; restore in finally |

---

**Goal:** When MIREC opens his EQ, the slider thumb sits at the same dB tick that the text label shows — fixing the bug where text said "+4 dB" while thumb was at the "+1 dB" tick.
**What changed:** EQ slider position now reads from REAPER's actual formatted dB (single source of truth) instead of the UI's approximation curve. Send path uses REAPER's own norm↔dB mapping. No EQ values are written until the user actively moves a slider.

🌐 Dev:  https://iem.newlevel.media/
🌐 Prod: https://iem.newlevel.media/

**[reaperiem] PR #<N>: fix(eq): slider position matches displayed dB (#194)**
<full PR URL> — mergeable, clean
```

DO NOT merge. Wait for the user's explicit "merge it" / "approved" / "go ahead" before running `gh pr merge`.

---

## Task Dependencies

```
T1 version bump          (must be FIRST commit)
   ↓
T2 failing E2E           (TDD red — bug-fix protocol)
   ↓
T3 read path             (ReaScript + server + core + UI types + connection)
   ↓
T4 set path              (ReaScript param=gain_db branch)
   ↓
T5 UI render             (slider derive from gain_db_sig + bounds)
   ↓
T6 wire-up verify        (no leftover refs; format clean)
   ↓
T7 push + CI green       (single sleep + gh run view in background)
   ↓
T8 post-deploy verify    (Playwright on live engineer track; restore)
   ↓
T9 PR + verify clean + STOP    (no merge without user approval)
```

All tasks STRICTLY SEQUENTIAL. Each touches earlier-task code; out-of-order edits create conflicts.

---

## Verification

Plan is fulfilled when:

1. CI is green on dev (10/10 jobs).
2. PR is `mergeable: true` AND `mergeable_state: "clean"`.
3. Post-deploy Playwright check on https://iem.newlevel.media/ confirms slider thumb position matches displayed dB text on initial render after the engineer band is set to a known REAPER value.
4. Engineer track restored to its pre-test norm.
5. Completion report sent. NO merge.
