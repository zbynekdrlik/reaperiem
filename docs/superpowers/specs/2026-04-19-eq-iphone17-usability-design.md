# EQ iPhone 17 Pro Usability Fix — Design Spec

**Issue:** [#179](https://github.com/zbynekdrlik/reaperiem/issues/179)
**Date:** 2026-04-19
**Scope:** Pure CSS visual polish in `iem-mixer/iem-ui/style.css`. No changes to `eq_modal.rs` markup or logic.

---

## Problem

MIREC reports four EQ modal usability issues on iPhone 17 Pro (portrait, ~430 px logical viewport):

1. EQ band cards render two-per-row — faders are extremely short, precision impossible.
2. Parameter labels (FREQ / Q / GAIN) and numeric values are too dark to read.
3. Per-row chrome (label + value widget + padding) eats most of the horizontal space, leaving little for the slider.
4. With haptics disabled, there is no obvious indicator that a slider has entered "movement mode" — only the thumb scales slightly.

## Root causes

- `.eq-band-card` uses `@media (max-width: 420px)` to switch to single-column. iPhone 17 Pro portrait is ~430 px, so the breakpoint misses and cards stay two-per-row.
- `.eq-param-label` uses `var(--text-muted)` (`#555`) — near-invisible on the dark background.
- `.eq-param-value` uses `var(--text-secondary)` (`#888`) — low contrast.
- Inside a card: 32 px label + 60 px value + 8 px gaps + 24 px card padding = 124 px of chrome. At the two-per-row iPhone card width (~174 px useful area), the slider itself gets only ~58 px.
- Activation cue is `.eq-slider-track.active .eq-slider-thumb { transform: scale(1.3) }`. A thumb enlarging from 18 px to 23 px is too subtle without haptic confirmation.

## Design

### 1. Portrait-phone stacking

Replace the pixel-based breakpoint with a device-capability query:

```css
@media (pointer: coarse) and (orientation: portrait) {
  .eq-band-card { flex: 1 1 100%; max-width: 100%; }
}
```

- Covers all phones regardless of viewport width (430 px today, any width tomorrow).
- Desktop (`pointer: fine`) keeps the two-per-row layout.
- Landscape tablets and landscape phones keep two-per-row, which is correct — horizontal space is abundant.

### 2. Text contrast

| Selector | Current | New |
|---|---|---|
| `.eq-param-label` | `var(--text-muted)` (`#555`) | `#bbb`, `font-weight: 600` |
| `.eq-param-value` | `var(--text-secondary)` (`#888`) | `var(--text-primary)` (`#eaeaea`) |
| `.eq-band-type` | `var(--text-secondary)` (`#888`) | `#bbb` |

Values are brightest (primary text) because they are what the user reads to hit a target frequency or gain. Labels and band-type are mid-contrast (`#bbb`) — clearly readable without competing with values.

### 3. Compact row chrome

| Property | Current | New |
|---|---|---|
| `.eq-param-label` width | `32px` | `24px` |
| `.eq-param-value` width | `60px` | `44px` |
| `.eq-band-card` padding | `10px 12px` | `8px 10px` |
| `.eq-param-row` gap | `8px` | `6px` |

Savings per row: ~32 px of extra slider real estate.

Slider width budget on iPhone 17 Pro portrait:

| Scenario | Card useful width | Slider width |
|---|---|---|
| Current (two-per-row) | ~174 px | ~58 px |
| After stacking fix only | ~382 px | ~258 px |
| After stacking + compact chrome | ~390 px | ~290 px |

The composite gain is 5× slider real estate on iPhone 17 Pro.

### 4. Fullscreen movement-mode cue

Add a CSS-only rule that lights the entire EQ modal's inner border while any slider is in `active` or `activating` state:

```css
.eq-modal:has(.eq-slider-track.active),
.eq-modal:has(.eq-slider-track.activating) {
  box-shadow: inset 0 0 0 4px var(--accent);
  transition: box-shadow 120ms ease-in;
}
```

- Uses CSS `:has()` — supported on iOS Safari 15.4+, Chromium 105+, Firefox 121+. iPhone 17 Pro runs iOS ≥ 19, full support.
- No state wiring, no new Leptos signals. Existing `EqSlider` already toggles `active` / `activating` classes (line 1165-1166 of `eq_modal.rs`).
- The existing `.eq-slider-track.active .eq-slider-thumb { scale(1.3) }` cue stays — both indicators reinforce each other.
- Border clears the frame after pointer release. Not persistent.

### 5. Version bump

`1.155.0` → `1.156.0` as the first commit on `dev`.

### 6. Changelog

Add to `README.md` changelog under v1.156.0:

- **Fix:** EQ bands stack vertically on phones in portrait (was two-per-row on iPhone 17 Pro, fader too short for precision).
- **Fix:** EQ label and value text contrast improved.
- **Fix:** EQ slider track widened — ~290 px on iPhone vs ~58 px previously.
- **Feature:** Visible fullscreen cue when an EQ slider activates into movement mode (helps when phone haptics are disabled).

---

## Testing

### Playwright E2E

Add to `iem-mixer/e2e/tests/live/eq.spec.ts`:

1. **Portrait-phone stacking test** — use Playwright device emulation (`devices['iPhone 14 Pro Max']` = 430×932 viewport, matches iPhone 17 Pro geometry). Open EQ modal, assert `.eq-band-card:nth-child(1)` computed width equals `.eq-band-card:nth-child(2)` computed width AND both equal the container inner width (within 4 px tolerance) — proving they stack.

2. **Activation cue test** — open EQ modal on default desktop viewport. Simulate the existing tap + hold activation on a FREQ slider. Assert `.eq-modal` `box-shadow` computed value contains `inset` and `4px` while held, and does not contain them 500 ms after release.

3. **Contrast regression** — assert `.eq-param-label` computed `color` is not `rgb(85, 85, 85)` (the old `#555`). Cheap belt-and-braces against the color being reverted.

All three tests must pass with zero browser console errors per `browser-console-zero-errors` module.

### No unit tests

This is pure CSS and pure declarative markup. There is no Rust logic being changed.

### Mutation testing

Nothing to mutate — no Rust changes.

## Out of scope

- Landscape-phone layout tuning (user did not report issues there; landscape has plenty of width already).
- Desktop layout (user did not report issues, two-per-row is correct on wide screens).
- Haptic feedback settings panel (separate feature; this spec only addresses the visual fallback when haptics are off).
- Reworking `EqSlider` component internals (1624-line file; out-of-scope refactor would be a patchwork-first violation).

## Success criteria

- Playwright E2E passes on iPhone 14 Pro Max device emulation.
- Manual verification on iPhone 17 Pro (production, https://iem.newlevel.media) confirms:
  - Bands stack vertically in portrait.
  - Labels and values are clearly readable.
  - When holding a slider, a visible cyan border appears around the modal.
- CI green (all jobs) on dev push.
- Post-deploy E2E green on iem.lan.
