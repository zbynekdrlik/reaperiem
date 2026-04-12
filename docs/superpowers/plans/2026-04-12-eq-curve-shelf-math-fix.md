# EQ Curve Shelf Math Fix — Implementation Plan (#167)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix frontend EQ curve math so shelving biquads no longer ring, eliminating the +1.4 dB overshoot that makes MIREC's mic look "oversaturated" vs REAPER's native ReaEQ.

**Architecture:** Pure frontend fix in `iem-mixer/iem-ui/src/components/eq_modal.rs` — rewrite `biquad_low_shelf` and `biquad_high_shelf` to use the Audio EQ Cookbook's S-parameterized alpha (`S = 1/bw_oct` clamped to `[0.01, 2.0]`) instead of the peaking-EQ Q formula. Unit tests enforce shelf invariants; a live Playwright E2E test on MIREC guarantees no regression. No server, no ReaScript, no protocol changes.

**Tech Stack:** Rust 2021 (Leptos WASM), Playwright TypeScript, `cargo-mutants` (already in CI), GitHub Actions self-hosted runner (`iem-lan`).

**Spec:** `docs/superpowers/specs/2026-04-12-eq-curve-shelf-math-design.md`

---

## Context

**What went wrong.** For MIREC mic (track 7 in production REAPER), band 2 is a +4.3 dB peaking filter at 640 Hz. Iem-mixer draws the curve peaking at y=78.4 in SVG coordinates (= +5.73 dB computed) while the band 2 dot sits at y=96.25 (= +4.3 dB stated) — an excess of **17.85 px / +1.43 dB** at the band's *own* center frequency. The Audio EQ Cookbook (Robert Bristow-Johnson) uses a different alpha for shelves than for peaking filters:

```
peakingEQ:  alpha = sin(w0) / (2 * Q)
shelving:   alpha = sin(w0) / 2 * sqrt((A + 1/A)(1/S - 1) + 2)
```

`eq_modal.rs:172-202` currently uses the peaking formula for both shelves, with `Q = bw_to_q(bw_oct, w0)`. For the MIREC b1 lowshelf (bw=0.56 oct) this computes Q≈2.54, which is >3× the Butterworth Q=0.707 a sane shelf should use. High-Q shelves ring — and that ringing adds ~+1.4 dB at the neighbouring peaking band's center frequency.

**What we captured.** Visual evidence is in the spec folder:
- `docs/superpowers/specs/2026-04-12-eq-curve-shelf-math/mirec-reaeq-v1.146.0.png` — REAPER ReaEQ for MIREC, ground truth
- `docs/superpowers/specs/2026-04-12-eq-curve-shelf-math/mirec-iem-mixer-v1.146.0.png` — iem-mixer v1.146.0 buggy state

**Constants that matter.** `SAMPLE_RATE` in `eq_modal.rs:28` is `96000.0` (Dante network rate). All test math must use 96 kHz.

**Git state at start.** Branch `dev` matches `origin/main` at commit `cd98430` (post PR #169 merge, v1.146.0). `dev` is 1 commit ahead of `origin/dev` — the merge commit. Version MUST be bumped to 1.147.0 as the very first commit before any other change (airuleset `version-bumping.md`).

**Local vs CI rule.** This repo does NOT allow running `cargo build` / `cargo test` / `cargo clippy` locally — all compilation happens on CI (`block-cargo.sh` hook enforces it). Only `cargo fmt --all --check` runs locally. Unit tests will be written in the same commit as the code fix so CI can verify them together. Playwright can and must be run locally against production v1.146.0 to get Phase-1 RED verification before we ship the fix.

**Tool already staged on iem.lan.** During brainstorming I deployed a helper `C:\Users\newlevel\AppData\Roaming\REAPER\Scripts\reaperiem\show_eq.lua` and registered it via `meter_bridge` — its numeric action command_id is `53108`, and after trigger it writes `EXTSTATE reaperiem/show_eq_result = OK:track=N,fx=M`. It's live for the session but not committed to the repo. Used for post-deploy visual verification only. Re-register with `curl "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/register_scripts/show_eq.lua"` if REAPER was restarted; then read the new cmd_id from `reaperiem/action_show_eq`.

---

## File Map

### Code fix
- `iem-mixer/iem-ui/src/components/eq_modal.rs` — rewrite `biquad_low_shelf` (lines 172–186) and `biquad_high_shelf` (lines 188–202). Add new unit tests inside the existing `#[cfg(test)] mod tests` block starting at line 1166.

### New test
- `iem-mixer/e2e/tests/live/eq.spec.ts` — append a new `test.describe("#167 EQ curve shape", ...)` block after the existing `});` closing at line 1311. New test logs in as engineer (PIN 1177), not petronela.

### Version bump (6 files, same pattern as every previous version bump)
- `iem-mixer/crates/iem-core/Cargo.toml`
- `iem-mixer/Cargo.toml`
- `iem-mixer/crates/iem-server/Cargo.toml`
- `iem-mixer/iem-ui/Cargo.toml`
- `iem-mixer/src-tauri/Cargo.toml`
- `iem-mixer/src-tauri/tauri.conf.json`

### Changelog
- `README.md` — prepend new entry to Changelog section

---

## Task 1: Version bump 1.146.0 → 1.147.0

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`

**Why this is Task 1, not Task 5:** airuleset `version-bumping.md` mandates the version bump MUST be the first commit on `dev` when dev matches main. After PR #169 merged, dev == main == 1.146.0. If we start writing feature code before bumping, the version check CI job will fail and we'll waste a CI cycle.

- [ ] **Step 1: Fetch latest from origin**

```bash
cd /home/newlevel/devel/reaperiem
git fetch origin
git status
```
Expected: `On branch dev`, dev ahead of origin/dev by 1 commit (the merge), local dev at `cd98430` which matches `origin/main`.

- [ ] **Step 2: Apply the version bump with one sed invocation per file**

```bash
sed -i 's/version = "1.146.0"/version = "1.147.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.146.0"/"version": "1.147.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 3: Verify the bump took effect everywhere**

```bash
grep -H '1.147.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
```
Expected: 6 lines of output, one per file. If any file is missing, stop and fix before committing.

```bash
grep -rn '1.146.0' iem-mixer/Cargo.toml iem-mixer/crates iem-mixer/iem-ui iem-mixer/src-tauri || echo "clean"
```
Expected: `clean` — no stragglers.

- [ ] **Step 4: Commit the bump**

```bash
git add \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml \
  iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.147.0 (#167)

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Write Phase-1 RED Playwright E2E test (append to `eq.spec.ts`)

**File:** `iem-mixer/e2e/tests/live/eq.spec.ts`

**Why Phase-1 RED matters.** We already measured the bug on v1.146.0 (`path minY = 78.4`, `band 2 dot cy = 96.25`, excess = 17.85 px). Before writing the fix, we must prove the new test ACTUALLY catches this. Running it against production right now must produce a failure whose error message includes the excess value. If the test passes against v1.146.0, the test is broken — fix the test, don't proceed to the code fix.

- [ ] **Step 1: Append the new `test.describe` block at the very end of `eq.spec.ts`**

The existing file ends at line 1311 with `});` closing the `test.describe("EQ Feature", ...)` block. Append this new block AFTER that closing brace as a sibling top-level block. It uses its own login flow because engineer PIN (1177) differs from petronela (7711).

```typescript
// Regression test for #167 — EQ curve shape
//
// Bug: biquad_low_shelf/high_shelf used the peaking-EQ Q formula instead of
// the Audio EQ Cookbook's shelf-slope S formula, so shelves rang near their
// corner frequencies. For MIREC (track 7), band 2 is a +4.3 dB peaking at
// 640 Hz, but the drawn curve peaked at +5.73 dB because the adjacent
// lowshelf at 510 Hz rang upward just above its corner.
//
// Invariant being tested: at each enabled band's center frequency, the
// summed response curve must equal that band's stated gain within a small
// tolerance. In SVG coords (±12 dB mapped to 0..300 px, i.e. 12.5 px/dB),
// 3 px ≈ 0.24 dB is well below REAPER's typical 0.1 dB display resolution
// but far above numerical noise.
//
// On v1.146.0 this test fails with a measured excess of ~17.85 px at band 2.

test.describe("#167 EQ curve shape (live MIREC)", () => {
  test("engineer EQ curve does not overshoot band dots", async ({ page }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        // Known-benign browser/runtime noise — same filters used by alert.spec.ts
        if (
          !text.includes("Push API in incognito") &&
          !text.includes("[push] subscribe await failed") &&
          !text.includes("integrity")
        ) {
          consoleMessages.push(`[${msg.type()}] ${text}`);
        }
      }
    });

    // Engineer login (PIN 1177). The /api/auth endpoint returns the token.
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    // Switch to Mics tab — MIREC is an input mic, lives there, not on Main.
    await page.getByRole("button", { name: "Mics" }).click();
    await page.waitForTimeout(300);

    // Open MIREC's kebab menu and pick EQ.
    await openKebabMenu(page, "MIREC");
    await clickEqOption(page);

    // Wait for the SVG curve to render (201 path points).
    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });
    await page.waitForFunction(
      () => {
        const path = document.querySelector(".eq-overlay svg path[d*='M']");
        const d = path?.getAttribute("d") || "";
        return (d.match(/[ML]/g) || []).length > 50;
      },
      { timeout: 5000 },
    );

    // Extract path + band dots from the DOM.
    const geometry = await page.evaluate(() => {
      const svg = document.querySelector<SVGSVGElement>(".eq-overlay svg");
      if (!svg) return { error: "no svg" };
      const pathEl = svg.querySelector<SVGPathElement>("path[d*='M']");
      const d = pathEl?.getAttribute("d") || "";
      const points: [number, number][] = [];
      const regex = /[ML]([\d.]+),([\d.]+)/g;
      let m: RegExpExecArray | null;
      while ((m = regex.exec(d)) !== null) {
        points.push([parseFloat(m[1]), parseFloat(m[2])]);
      }
      const dots = Array.from(svg.querySelectorAll<SVGCircleElement>("circle"))
        .map((c) => ({
          cx: parseFloat(c.getAttribute("cx") || "0"),
          cy: parseFloat(c.getAttribute("cy") || "0"),
          r: parseFloat(c.getAttribute("r") || "0"),
        }))
        // Drop tiny tick marks / grid helpers — band dots have r >= 6
        .filter((c) => c.r >= 6);
      return { points, dots };
    });

    if ("error" in geometry) {
      throw new Error(`EQ modal did not render: ${geometry.error}`);
    }
    const { points, dots } = geometry;
    expect(points.length).toBeGreaterThan(100);
    expect(dots.length).toBeGreaterThan(0);

    // Helper: find the path y value at x (nearest neighbour in the 201-point
    // path, which is enough because dots sit on the same x grid as path samples).
    const pathYAt = (targetX: number): number => {
      let best = points[0];
      let bestDx = Math.abs(points[0][0] - targetX);
      for (const p of points) {
        const dx = Math.abs(p[0] - targetX);
        if (dx < bestDx) {
          bestDx = dx;
          best = p;
        }
      }
      return best[1];
    };

    // Pixel tolerance: 3 px ≈ 0.24 dB at 12.5 px/dB. Below user-visible.
    const TOLERANCE_PX = 3;

    // Invariant 1: at every enabled band dot's x, the curve y must be within
    // TOLERANCE_PX of that dot's y. Skip dots at the viewport edges (x=0 or
    // x=800) because highpass/lowpass filters at the extremes are not required
    // to sit on the curve in the same way peaking/shelf bands do.
    for (const dot of dots) {
      if (dot.cx <= 2 || dot.cx >= 798) continue;
      const pathY = pathYAt(dot.cx);
      const deviation = Math.abs(pathY - dot.cy);
      expect(
        deviation,
        `Band dot at (${dot.cx.toFixed(1)}, ${dot.cy.toFixed(1)}) — ` +
          `path y at same x is ${pathY.toFixed(1)}, deviation ${deviation.toFixed(1)} px ` +
          `(≈ ${(deviation / 12.5).toFixed(2)} dB). On v1.146.0 the MIREC b2 ` +
          `deviation is ~17.85 px / +1.43 dB — that is the #167 bug.`,
      ).toBeLessThanOrEqual(TOLERANCE_PX);
    }

    // Invariant 2: the tallest point of the curve must not exceed the tallest
    // enabled band dot by more than TOLERANCE_PX. "Tallest" = smallest y.
    const curveMinY = Math.min(...points.map((p) => p[1]));
    const dotMinY = Math.min(...dots.map((d) => d.cy));
    expect(
      curveMinY,
      `Curve peak y=${curveMinY.toFixed(1)} exceeds tallest dot y=${dotMinY.toFixed(1)} ` +
        `by ${(dotMinY - curveMinY).toFixed(1)} px (≈ ${((dotMinY - curveMinY) / 12.5).toFixed(2)} dB). ` +
        `On v1.146.0 curve peaks at y=78.4 while b2 dot is at y=96.25 — #167.`,
    ).toBeGreaterThanOrEqual(dotMinY - TOLERANCE_PX);

    // Invariant 3: no console errors or warnings (airuleset
    // browser-console-zero-errors.md).
    expect(consoleMessages).toEqual([]);
  });
});
```

- [ ] **Step 2: Phase-1 RED verification — run the new test against production v1.146.0**

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer/e2e
npm ci  # skip if node_modules already up to date
npx playwright install chromium  # skip if already installed
E2E_BASE_URL=https://iem.newlevel.media \
  npx playwright test \
  --config playwright.live.config.ts \
  tests/live/eq.spec.ts \
  -g "#167 EQ curve shape" \
  --reporter=list
```

Expected outcome: **FAIL**. The error must mention one of:
- `Band dot at (401.x, 96.25) — path y at same x is 78.4, deviation 17.85 px`
- `Curve peak y=78.4 exceeds tallest dot y=96.25 by 17.85 px`

Capture the output of this run (save to `/tmp/eq167-phase1-red.txt`) for the PR description — this is the irrefutable evidence that the test catches the bug.

```bash
E2E_BASE_URL=https://iem.newlevel.media \
  npx playwright test \
  --config playwright.live.config.ts \
  tests/live/eq.spec.ts \
  -g "#167 EQ curve shape" \
  --reporter=list 2>&1 | tee /tmp/eq167-phase1-red.txt
```

If the test PASSES against v1.146.0, do NOT proceed. The test is wrong — debug the `page.evaluate` extraction, check that the EQ modal actually opened, check the dot filter (r ≥ 6), and iterate until the test fails for the right reason.

- [ ] **Step 3: Commit the failing E2E test**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/e2e/tests/live/eq.spec.ts
git commit -m "test(eq): live E2E test for curve overshoot on MIREC (#167)

Red test — currently fails against v1.146.0 production with a ~17.85 px
(+1.43 dB) excess at MIREC's band 2 (+4.3 dB peaking at 640 Hz). The
failure proves the adjacent lowshelf at 510 Hz is ringing and inflating
the neighbouring peaking band.

Asserts two invariants per enabled band:
  1. |path.y_at(dot.cx) - dot.cy| <= 3 px  (0.24 dB)
  2. min(path.y) >= min(dot.cy) - 3 px

Engineer login (PIN 1177) because MIREC mic is on the Mics tab which
petronela cannot access.

Phase-1 RED output captured at /tmp/eq167-phase1-red.txt for PR evidence.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Rewrite shelf biquads + add inline unit tests (atomic commit — tests pass only with fix)

**File:** `iem-mixer/iem-ui/src/components/eq_modal.rs`

**Why atomic.** We can't run `cargo test` locally — it's blocked by `block-cargo.sh`. So the unit tests will run for the first time on CI. If we commit tests first and fix later, CI goes red on the intermediate commit and that's a waste. Combine them.

- [ ] **Step 1: Rewrite `biquad_low_shelf` (lines 172–186)**

Replace the existing function body entirely:

```rust
fn biquad_low_shelf(w0: f32, gain_db: f32, bw_oct: f32) -> BiquadCoeffs {
    let a = 10.0_f32.powf(gain_db / 40.0);
    // Audio EQ Cookbook shelf-slope formula (NOT the peaking-Q formula).
    // S is the shelf slope parameter; for a given "bandwidth in octaves"
    // display value, S = 1 / bw_oct is the standard DSP mapping —
    // narrow bandwidth → steeper shelf. Clamped to [0.01, 2.0]:
    //   - lower: prevents sqrt(negative) when (1/S - 1) goes too negative
    //   - upper: above S≈2 the formula re-introduces the same resonance
    //     we are trying to remove
    let s = (1.0 / bw_oct.max(0.01)).clamp(0.01, 2.0);
    let alpha = w0.sin() / 2.0
        * ((a + 1.0 / a) * (1.0 / s - 1.0) + 2.0).max(0.0).sqrt();
    let cos_w0 = w0.cos();
    let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

    let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
    let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
    let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
    let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
    let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
    let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha;
    (b0, b1, b2, a0, a1, a2)
}
```

The coefficient expressions (b0, b1, b2, a0, a1, a2) are UNCHANGED — only the `s` / `alpha` derivation is new. The coefficient formulas themselves are standard cookbook lowshelf. The bug was feeding them a wrong alpha.

- [ ] **Step 2: Rewrite `biquad_high_shelf` (lines 188–202)**

Same pattern — replace function body:

```rust
fn biquad_high_shelf(w0: f32, gain_db: f32, bw_oct: f32) -> BiquadCoeffs {
    let a = 10.0_f32.powf(gain_db / 40.0);
    // Audio EQ Cookbook shelf-slope formula. See biquad_low_shelf comment.
    let s = (1.0 / bw_oct.max(0.01)).clamp(0.01, 2.0);
    let alpha = w0.sin() / 2.0
        * ((a + 1.0 / a) * (1.0 / s - 1.0) + 2.0).max(0.0).sqrt();
    let cos_w0 = w0.cos();
    let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

    let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
    let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
    let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
    let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
    let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
    let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha;
    (b0, b1, b2, a0, a1, a2)
}
```

- [ ] **Step 3: Add 6 new unit tests inside `mod tests` at `eq_modal.rs:1167`**

Paste these tests after the existing `test_display_order` and related tests but inside the same `mod tests { ... }` block. The existing tests use `SAMPLE_RATE = 96000.0` implicitly; so do these.

```rust
    /// Helper: build an EqBandState with sensible defaults.
    fn band(ty: &str, freq_hz: f32, gain_db: f32, bw: f32) -> EqBandState {
        EqBandState {
            band_type: ty.to_string(),
            freq_hz,
            gain_db,
            bw,
            freq_norm: 0.0,
            gain_norm: 0.0,
            bw_norm: 0.0,
            enabled: true,
        }
    }

    /// Regression guard: peaking filter's magnitude at its center frequency
    /// must equal the stated gain. This already passes on v1.146.0 — we
    /// commit it so future changes can't break it.
    #[test]
    fn test_peaking_exact_at_center_frequency() {
        for &gain in &[-12.0_f32, -6.0, -3.0, 0.0, 3.0, 6.0, 12.0] {
            for &bw in &[0.5_f32, 1.0, 2.0] {
                let b = band("band", 1000.0, gain, bw);
                let g = compute_band_gain(1000.0, &b);
                assert!(
                    (g - gain).abs() < 0.05,
                    "peaking {gain} dB bw={bw}: got {g} at center freq"
                );
            }
        }
    }

    /// Lowshelf passband (well below corner) must equal stated gain.
    #[test]
    fn test_lowshelf_passband_equals_gain() {
        for &gain in &[-6.0_f32, -3.0, 3.0, 6.0] {
            for &bw in &[0.5_f32, 1.0] {
                let b = band("lowshelf", 500.0, gain, bw);
                // Evaluate far below corner — should be full shelf gain.
                let g = compute_band_gain(20.0, &b);
                assert!(
                    (g - gain).abs() < 0.3,
                    "lowshelf 500 Hz {gain} dB bw={bw}: passband at 20 Hz = {g}"
                );
            }
        }
    }

    /// Highshelf passband (well above corner) must equal stated gain.
    #[test]
    fn test_highshelf_passband_equals_gain() {
        for &gain in &[-6.0_f32, -3.0, 3.0, 6.0] {
            for &bw in &[0.5_f32, 1.0] {
                let b = band("highshelf", 5000.0, gain, bw);
                // Evaluate far above corner — should be full shelf gain.
                let g = compute_band_gain(20000.0, &b);
                assert!(
                    (g - gain).abs() < 0.3,
                    "highshelf 5 kHz {gain} dB bw={bw}: passband at 20 kHz = {g}"
                );
            }
        }
    }

    /// Lowshelf must not overshoot its passband — no ringing above the
    /// stated gain across the entire 20 Hz .. 20 kHz range.
    #[test]
    fn test_lowshelf_no_overshoot() {
        let b = band("lowshelf", 500.0, 6.0, 0.5);
        let mut max_gain = f32::NEG_INFINITY;
        let mut min_gain = f32::INFINITY;
        // Log-sweep 20 Hz .. 20 kHz in 400 steps.
        for i in 0..=400 {
            let t = i as f32 / 400.0;
            let freq = 20.0 * (1000.0_f32).powf(t);
            let g = compute_band_gain(freq, &b);
            if g > max_gain {
                max_gain = g;
            }
            if g < min_gain {
                min_gain = g;
            }
        }
        // +6 dB shelf must stay within [−0.3, 6.3] dB across the whole
        // spectrum. 0.3 dB of slop covers the smooth transition region.
        assert!(
            max_gain <= 6.3,
            "lowshelf +6 dB overshoots: max={max_gain}"
        );
        assert!(
            min_gain >= -0.3,
            "lowshelf +6 dB dips below 0 dB: min={min_gain}"
        );
    }

    /// Highshelf must not overshoot either.
    #[test]
    fn test_highshelf_no_overshoot() {
        let b = band("highshelf", 5000.0, 6.0, 0.5);
        let mut max_gain = f32::NEG_INFINITY;
        let mut min_gain = f32::INFINITY;
        for i in 0..=400 {
            let t = i as f32 / 400.0;
            let freq = 20.0 * (1000.0_f32).powf(t);
            let g = compute_band_gain(freq, &b);
            if g > max_gain {
                max_gain = g;
            }
            if g < min_gain {
                min_gain = g;
            }
        }
        assert!(
            max_gain <= 6.3,
            "highshelf +6 dB overshoots: max={max_gain}"
        );
        assert!(
            min_gain >= -0.3,
            "highshelf +6 dB dips below 0 dB: min={min_gain}"
        );
    }

    /// #167 regression: captured MIREC fixture from production v1.146.0.
    /// With the buggy math, summing these bands gives a peak of +5.73 dB
    /// at 640 Hz; with the fix, the peak must sit at b2's stated +4.3 dB
    /// within 0.3 dB.
    #[test]
    fn test_mirec_fixture_no_shelf_ringing_167() {
        let bands = vec![
            // b0 highpass disabled — skip
            band("lowshelf", 510.8, -2.1, 0.56),
            band("band", 640.6, 4.3, 1.14),
            band("band", 1473.3, -1.5, 0.92),
            band("highshelf", 4448.1, 3.6, 0.80),
        ];
        // Sum responses at 640 Hz — should equal b2's +4.3 dB ± 0.3 dB.
        let mut total = 0.0_f32;
        for b in &bands {
            total += compute_band_gain(640.6, b);
        }
        assert!(
            (total - 4.3).abs() < 0.3,
            "MIREC sum at 640 Hz = {total} dB, expected +4.3 ± 0.3"
        );
        // And the whole curve max (scanned log-sweep) must not exceed +4.6 dB.
        let mut curve_max = f32::NEG_INFINITY;
        for i in 0..=400 {
            let t = i as f32 / 400.0;
            let freq = 20.0 * (1000.0_f32).powf(t);
            let mut sum = 0.0;
            for b in &bands {
                sum += compute_band_gain(freq, b);
            }
            if sum > curve_max {
                curve_max = sum;
            }
        }
        assert!(
            curve_max <= 4.6,
            "MIREC curve max = {curve_max} dB, expected ≤ 4.6 (no shelf ringing)"
        );
    }
```

- [ ] **Step 4: Local format check**

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer
cargo fmt --all --check
```
Expected: no output, exit 0. If it fails, run `cargo fmt --all` and re-check before committing.

- [ ] **Step 5: Commit the atomic fix + tests**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/iem-ui/src/components/eq_modal.rs
git commit -m "fix(eq): shelf biquad math uses cookbook S-slope, not peaking Q (#167)

biquad_low_shelf and biquad_high_shelf used the peaking-EQ Q formula
(alpha = sin(w0)/(2*Q) with Q = bw_to_q(...)). That gave shelves a Q of
~2.5 for REAPER's typical bw=0.56 oct, well above Butterworth 0.707, and
the shelves rang upward near their corner frequencies.

The Audio EQ Cookbook (RBJ) specifies a different alpha for shelving
filters: alpha = sin(w0)/2 * sqrt((A + 1/A)(1/S - 1) + 2) where S is
the shelf slope. This commit uses that formula with S = 1/bw_oct clamped
to [0.01, 2.0] — narrow bandwidth yields a steeper slope, wide bandwidth
yields a gentler one, and the edges are bounded to prevent numerical
blowup and re-introduced resonance.

Coefficient expressions (b0..a2) are the same cookbook lowshelf/highshelf
polynomials — only the alpha derivation changed.

Six new unit tests inside the existing #[cfg(test)] mod:
  - test_peaking_exact_at_center_frequency (regression guard, was already
    passing)
  - test_lowshelf_passband_equals_gain
  - test_highshelf_passband_equals_gain
  - test_lowshelf_no_overshoot
  - test_highshelf_no_overshoot
  - test_mirec_fixture_no_shelf_ringing_167 (exact MIREC band params
    captured from production v1.146.0 — asserts curve max ≤ +4.6 dB
    where old buggy math gave +5.73)

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Changelog entry

**File:** `README.md`

- [ ] **Step 1: Open README.md and locate the Changelog section**

```bash
grep -n '^## Changelog\|^### v1\.146' README.md | head -5
```
Expected: shows the `## Changelog` heading line and `### v1.146.0` as the current top entry.

- [ ] **Step 2: Insert the new v1.147.0 entry ABOVE the current v1.146.0 entry**

Use `sed` to insert a block right before `### v1.146.0`:

```bash
python3 << 'PY'
import re
p = 'README.md'
with open(p) as f:
    txt = f.read()
block = """### v1.147.0 (2026-04-12)

- **Fix**: EQ visualization — shelving filters (lowshelf/highshelf) no longer ring near their corner frequencies. Rewrote shelf biquad math to use the Audio EQ Cookbook's S-parameterized formula instead of the peaking-EQ Q formula, eliminating the ~1.4 dB overshoot that made neighbouring peaking bands look "oversaturated" versus REAPER's native ReaEQ display (#167).

"""
new = re.sub(r'(### v1\.146\.0)', block + r'\1', txt, count=1)
assert new != txt, 'insertion failed — v1.146.0 heading not found'
with open(p, 'w') as f:
    f.write(new)
print('changelog updated')
PY
```

- [ ] **Step 3: Verify the insertion**

```bash
grep -A 2 'v1.147.0' README.md | head -10
```
Expected: shows the new heading, blank line, and the fix bullet.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: changelog entry for v1.147.0 EQ curve fix (#167)

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Push and monitor CI until all 10 jobs are green

- [ ] **Step 1: Confirm the commit log before push**

```bash
git log --oneline origin/dev..HEAD
```
Expected: 4 commits ahead of origin/dev — the post-#169 merge commit, plus:
1. `chore: bump version to 1.147.0 (#167)`
2. `test(eq): live E2E test for curve overshoot on MIREC (#167)`
3. `fix(eq): shelf biquad math uses cookbook S-slope, not peaking Q (#167)`
4. `docs: changelog entry for v1.147.0 EQ curve fix (#167)`

Plus the pre-existing merge commit. Total: 5. Acceptable.

- [ ] **Step 2: Check for in-progress CI runs on dev before pushing**

```bash
gh run list --branch dev --status in_progress --limit 3
```
Expected: none. If there are any, wait for them to finish or understand why they exist before pushing (we don't want to stomp on someone else's work).

- [ ] **Step 3: Push**

```bash
git push origin dev
```

- [ ] **Step 4: Identify the run ID for the push**

```bash
sleep 5 && gh run list --branch dev --limit 3
```
Note the newest run ID (top of the list — should show your commit message).

- [ ] **Step 5: Monitor all 10 jobs until terminal state**

Per `ci-monitoring.md`: poll with `gh run view <run-id>`, NOT `gh run watch` (which hammers the GitHub API). Use background sleeps between polls.

```bash
RUN_ID=<id from step 4>
gh run view $RUN_ID
# If jobs are still running, wait ~4 minutes and re-check:
sleep 240 && gh run view $RUN_ID
# Keep polling every 4–6 minutes until every job reaches success or failure.
```

Required terminal state: ALL 10 jobs green —
- Lint & Format
- Test Integrity Check
- Build VBAN VST3
- Verify Version Bump (this WILL run because of the PR path; on direct dev push it's skipped — fine)
- Build WASM Frontend
- Mutation Testing
- Tests (Rust unit — our new 6 tests must pass here)
- Build Tauri (Windows)
- E2E Tests (CI synthetic — not live)
- Deploy to iem.lan (THEN post-deploy E2E runs the new #167 live test — must also pass)

If any job fails: `gh run view $RUN_ID --log-failed`, investigate root cause, fix in ONE new commit, push, re-monitor. Do NOT rerun blindly (airuleset `ci-monitoring.md`).

Common failure modes to anticipate:
- Rust test floating-point tolerance too tight → loosen the 0.3 dB band in the unit tests, push again
- Playwright test selector mismatch → the EQ modal's DOM changed slightly; adjust the `dots.filter(r >= 6)` threshold
- cargo-mutants finds a surviving mutant in the new shelf code → tighten a unit test assertion so the mutant is killed
- Console noise filter misses a new warning → add it to the filter list, same pattern as existing tests

---

## Task 6: Manual visual regression (post-deploy) for PR evidence

**Goal:** Prove the fix actually produces a curve visually indistinguishable from REAPER's ReaEQ for MIREC. Not a test gate — just a screenshot pair for the PR description.

- [ ] **Step 1: Confirm production is on v1.147.0**

```bash
curl -s http://10.77.9.231/api/version | python3 -m json.tool
```
Expected: `"version": "1.147.0"`, `"branch": "dev"`, git_hash matching the latest dev commit.

- [ ] **Step 2: Re-register show_eq.lua on iem.lan (in case REAPER restarted during deploy)**

```bash
curl -sf "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/register_scripts/show_eq.lua"
sleep 3
curl -sf "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/register_result"
```
Expected: `EXTSTATE reaperiem register_result OK:1:show_eq=<command_id>`. Capture the new command_id — REAPER assigns a fresh one every run.

- [ ] **Step 3: Open ReaEQ for MIREC track 7**

```bash
SHOW_EQ_CMD=<command_id from step 2>
curl -sf "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/show_eq_track/7"
curl -sf "http://iem.lan:8080/_/${SHOW_EQ_CMD}"
sleep 1
curl -sf "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/show_eq_result"
```
Expected: `OK:track=7,fx=1` or similar.

- [ ] **Step 4: Capture the ReaEQ window screenshot**

Use `mcp__win-iem-snv__Shell` to run a PowerShell snippet that finds the `EQ - Track 7 "MIREC mic"` window, resizes it to 700×500, uses `PrintWindow(..., 2)` to capture into a bitmap, and saves it as `C:\temp\mirec-reaeq-v1.147.0.png`. Exact PowerShell (paste as-is):

```powershell
Add-Type -AssemblyName System.Drawing
$code = @'
using System;
using System.Runtime.InteropServices;
public class CW3 {
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint f);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lp);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int c);
  [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint flg);
  public delegate bool EnumWindowsProc(IntPtr h, IntPtr lp);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
'@
Add-Type -TypeDefinition $code
$target = [IntPtr]::Zero
$cb = { param($h, $lp)
  if ([CW3]::IsWindowVisible($h)) {
    $len = [CW3]::GetWindowTextLength($h)
    if ($len -gt 0) {
      $sb = New-Object System.Text.StringBuilder ($len + 1)
      [CW3]::GetWindowText($h, $sb, $sb.Capacity) | Out-Null
      if ($sb.ToString() -match 'EQ - Track 7.*MIREC') { $script:target = $h }
    }
  }
  return $true
}
[CW3]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
if ($target -eq [IntPtr]::Zero) { Write-Host "no window"; exit 1 }
[CW3]::SetWindowPos($target, [IntPtr]::Zero, 100, 100, 700, 500, 0x0040) | Out-Null
Start-Sleep -Milliseconds 400
$r = New-Object CW3+RECT
[CW3]::GetWindowRect($target, [ref]$r) | Out-Null
$w = $r.Right - $r.Left; $hh = $r.Bottom - $r.Top
$bmp = New-Object System.Drawing.Bitmap $w, $hh
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
[CW3]::PrintWindow($target, $hdc, 2) | Out-Null
$g.ReleaseHdc($hdc); $g.Dispose()
New-Item -Type Directory -Force "C:\temp" | Out-Null
$bmp.Save("C:\temp\mirec-reaeq-v1.147.0.png", [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
(Get-Item "C:\temp\mirec-reaeq-v1.147.0.png").Length
```

Copy it to the dev machine:
```bash
mkdir -p docs/superpowers/specs/2026-04-12-eq-curve-shelf-math
scp newlevel@iem.lan:C:/temp/mirec-reaeq-v1.147.0.png \
  docs/superpowers/specs/2026-04-12-eq-curve-shelf-math/mirec-reaeq-v1.147.0.png
```

- [ ] **Step 5: Capture the new iem-mixer EQ screenshot via Playwright**

Use `mcp__plugin_playwright_playwright__browser_navigate` to go to `https://iem.newlevel.media/login`, login as engineer (PIN 1177), click Mics tab, click MIREC ⋮, click ≡ EQ, wait for the SVG, then `browser_take_screenshot` with `filename: "mirec-iem-mixer-v1.147.0.png"`. Move the resulting screenshot into the spec folder:

```bash
# After the screenshot is saved by the Playwright MCP:
mv ./mirec-iem-mixer-v1.147.0.png \
  docs/superpowers/specs/2026-04-12-eq-curve-shelf-math/mirec-iem-mixer-v1.147.0.png
```

- [ ] **Step 6: Commit the v1.147.0 screenshot pair**

```bash
git add docs/superpowers/specs/2026-04-12-eq-curve-shelf-math/mirec-reaeq-v1.147.0.png \
        docs/superpowers/specs/2026-04-12-eq-curve-shelf-math/mirec-iem-mixer-v1.147.0.png
git commit -m "docs: post-fix visual evidence for #167 — v1.147.0 vs REAPER ReaEQ

Before/after screenshots for MIREC mic with the same band settings
used in the v1.146.0 evidence. The v1.147.0 iem-mixer curve now peaks
at MIREC b2's stated +4.3 dB dot (no overshoot), matching REAPER's
native ReaEQ display.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
git push origin dev
```

- [ ] **Step 7: Wait for CI to process the docs-only commit (skipped jobs are OK)**

Docs-only commits trigger CI but all code jobs should short-circuit via path filters (or run fast). Poll until terminal state:

```bash
sleep 120 && gh run list --branch dev --limit 3
```
All jobs green or skipped is acceptable — the point is that the new docs commit doesn't break CI.

---

## Task 7: Open PR to main and wait for explicit merge approval

- [ ] **Step 1: Confirm the dev branch state**

```bash
git fetch origin
git log --oneline origin/main..dev
```
Should show ALL 5 commits from this PR: version bump, E2E test, code fix + unit tests, changelog, docs/screenshots.

- [ ] **Step 2: Open the PR**

```bash
gh pr create --base main --head dev \
  --title "fix: EQ curve shelf math — no more shelf ringing (#167)" \
  --body "$(cat <<'EOF'
## Summary

- Rewrites `biquad_low_shelf` / `biquad_high_shelf` in `iem-mixer/iem-ui/src/components/eq_modal.rs` to use the Audio EQ Cookbook's S-parameterized alpha instead of the peaking-EQ Q formula. Shelves no longer ring near their corner frequencies.
- Adds 6 new unit tests inside the existing `#[cfg(test)] mod tests` block, covering peaking-at-w0 invariant, shelf passband = stated gain, shelf monotonicity (no overshoot), and a MIREC-fixture regression for #167.
- Adds a live Playwright E2E test in `iem-mixer/e2e/tests/live/eq.spec.ts` that logs in as engineer, opens MIREC's EQ, and asserts `|path.y_at(dot.cx) − dot.cy| ≤ 3 px` for every band dot AND `min(path.y) ≥ min(dot.cy) − 3 px`. On v1.146.0 this test captured a 17.85 px / +1.43 dB excess at MIREC's band 2 — Phase-1 RED evidence at `/tmp/eq167-phase1-red.txt`.
- Version bump 1.146.0 → 1.147.0.

## Root cause

Shelves used `alpha = sin(w0) / (2 * Q)` with `Q = bw_to_q(bw_oct, w0)` — that's the peaking-EQ formula. For MIREC's `bw = 0.56 oct` lowshelf, Q came out to ~2.54 (vs Butterworth 0.707), giving the shelf sharp resonance around the corner. That resonance added ~+1.4 dB at the neighbouring +4.3 dB peaking band's center frequency, making the whole curve look "oversaturated" compared to REAPER's ReaEQ display.

The Audio EQ Cookbook specifies a different alpha for shelving filters:
```
alpha = sin(w0) / 2 * sqrt((A + 1/A)(1/S - 1) + 2)
```
where `S` is the shelf slope. With `S = 1/bw_oct` clamped to `[0.01, 2.0]` the shelves become smooth rolloffs with no overshoot.

## Visual evidence

**Before (v1.146.0 production):**
![v1.146.0 buggy curve](docs/superpowers/specs/2026-04-12-eq-curve-shelf-math/mirec-iem-mixer-v1.146.0.png)

**After (v1.147.0 deploy):**
![v1.147.0 fixed curve](docs/superpowers/specs/2026-04-12-eq-curve-shelf-math/mirec-iem-mixer-v1.147.0.png)

**REAPER ReaEQ (ground truth, same MIREC band settings):**
![REAPER ReaEQ](docs/superpowers/specs/2026-04-12-eq-curve-shelf-math/mirec-reaeq-v1.147.0.png)

## E2E test coverage

| Feature/Fix | E2E Test File | What It Verifies |
|---|---|---|
| #167 EQ shelf math | `iem-mixer/e2e/tests/live/eq.spec.ts` (`#167 EQ curve shape`) | Engineer login → Mics → MIREC ⋮ → ≡ EQ; extracts SVG path + band dots; asserts per-band `|path.y_at(cx) − cy| ≤ 3 px` AND `min(path.y) ≥ min(dot.cy) − 3 px`; asserts zero console errors/warnings |

## Test plan

- [ ] All 10 CI jobs green on dev
- [ ] Rust unit tests — 6 new passing, zero `cargo-mutants` survivors on the shelf biquads
- [ ] Post-deploy live E2E: #167 test passes on v1.147.0 (same test that FAILED on v1.146.0 — see `/tmp/eq167-phase1-red.txt`)
- [ ] Manual visual regression: v1.147.0 screenshots above visibly match REAPER's ReaEQ for the same MIREC settings
- [ ] Production `/api/version` reports `1.147.0` after merge

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Capture the PR URL from the output.

- [ ] **Step 3: Verify the PR is mergeable**

```bash
PR_URL=<url from step 2>
PR_NUM=$(echo "$PR_URL" | grep -o '[0-9]*$')
gh api repos/zbynekdrlik/reaperiem/pulls/$PR_NUM --jq '{mergeable: .mergeable, mergeable_state: .mergeable_state}'
```
Expected: `mergeable: true`, `mergeable_state: "clean"`. If `behind`, sync dev first. If `blocked`, CI hasn't reached green yet — wait. If `dirty`, resolve the conflict.

- [ ] **Step 4: Present the green PR URL to the user and STOP — do NOT merge**

Per airuleset `pr-merge-policy.md`: green CI is NOT permission to merge. Wait for explicit user text ("merge it", "approved", "go ahead"). Silence is NOT approval. Do NOT auto-merge.

Output to the user:
```
PR ready: <PR_URL>
  mergeable: true
  state: clean
  CI: 10/10 green
Awaiting your explicit merge instruction.
```

---

## Task 8: Post-merge — main CI + production deploy verification (only after user says "merge")

- [ ] **Step 1: Merge the PR (only after explicit user approval)**

```bash
gh pr merge $PR_NUM --merge
```

- [ ] **Step 2: Sync local dev with main so the next session starts clean**

```bash
git fetch origin
git checkout dev
git merge origin/main
```
Expected: fast-forward from the previous dev HEAD to the new merge commit.

- [ ] **Step 3: Monitor main CI until terminal state**

```bash
gh run list --branch main --limit 3
MAIN_RUN_ID=<id>
gh run view $MAIN_RUN_ID
# Poll every 4–6 minutes with: sleep 240 && gh run view $MAIN_RUN_ID
```
All 10 jobs must be green, including Deploy to iem.lan and post-deploy E2E (the #167 test runs here too and must still pass).

- [ ] **Step 4: Verify production version after deploy**

```bash
curl -s http://10.77.9.231/api/version | python3 -m json.tool
```
Expected:
- `version`: `1.147.0`
- `branch`: `main`
- `git_hash`: matches the main merge commit

- [ ] **Step 5: Final completion report**

Produce the airuleset completion-report format (see `completion-report.md`). Must end with the `## ✅ Work Complete` block including the E2E test coverage table, PR URL, CI status, and production verification.

---

## Task Dependencies

```
Task 1 (version bump)     — must be first on dev
    ↓
Task 2 (E2E RED test)     — runs against v1.146.0 live, must FAIL
    ↓
Task 3 (fix + unit tests) — atomic, reason: no local cargo test
    ↓
Task 4 (changelog)        — parallel-safe with Task 3 in principle
    ↓
Task 5 (push + monitor)   — blocks until all 10 CI jobs are terminal green
    ↓
Task 6 (visual evidence)  — runs after deploy completes
    ↓
Task 7 (open PR, wait)    — explicit user approval gate
    ↓
Task 8 (merge + verify)   — only fires on user approval
```

Tasks are **strictly sequential**. Parallelisation gains are near-zero for a 1-file fix and would make debugging CI failures harder.

---

## Verification Checklist (post-Task 8)

1. [ ] PR `zbynekdrlik/reaperiem#<N>` merged to main
2. [ ] All 10 CI jobs green on the main push
3. [ ] Production `/api/version` reports `1.147.0`, branch `main`
4. [ ] Post-deploy live E2E includes a passing `#167 EQ curve shape` test (captured in `gh run view --log` for the main deploy)
5. [ ] `mirec-iem-mixer-v1.147.0.png` and `mirec-reaeq-v1.147.0.png` committed under `docs/superpowers/specs/2026-04-12-eq-curve-shelf-math/` and visually match
6. [ ] Changelog line for v1.147.0 present in `README.md`
7. [ ] No leaked or pending changes in `git status`
