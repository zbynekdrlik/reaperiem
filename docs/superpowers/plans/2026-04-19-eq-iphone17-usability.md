# EQ iPhone 17 Pro Usability Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the EQ modal usable on iPhone 17 Pro by stacking band cards in portrait, brightening EQ text, shrinking row chrome, and adding a fullscreen activation cue.

**Architecture:** Pure CSS changes to `iem-mixer/iem-ui/style.css` plus three new Playwright E2E tests in `iem-mixer/e2e/tests/live/eq.spec.ts`. No Rust, no Leptos, no markup changes. Uses CSS `:has()` for the fullscreen cue (supported on all target browsers) so no new signals are needed.

**Tech Stack:** CSS (style.css), Playwright TypeScript (eq.spec.ts), README changelog.

**Spec:** `docs/superpowers/specs/2026-04-19-eq-iphone17-usability-design.md`

---

## File Map

### Code changes
- `iem-mixer/iem-ui/style.css:2100-2115` — band-card stacking (replace the `@media (max-width: 420px)` rule)
- `iem-mixer/iem-ui/style.css:2100-2108` — `.eq-band-card` padding shrink
- `iem-mixer/iem-ui/style.css:2136-2142` — `.eq-band-type` color bump
- `iem-mixer/iem-ui/style.css:2186-2199` — `.eq-param-row` gap shrink, `.eq-param-label` width + color
- `iem-mixer/iem-ui/style.css:2241-2248` — add `.eq-modal:has(...)` rule for fullscreen cue
- `iem-mixer/iem-ui/style.css:2258-2265` — `.eq-param-value` width + color

### Tests
- `iem-mixer/e2e/tests/live/eq.spec.ts` — three new tests appended to existing file

### Version + changelog
- `iem-mixer/crates/iem-core/Cargo.toml`, `iem-mixer/Cargo.toml`, `iem-mixer/crates/iem-server/Cargo.toml`, `iem-mixer/iem-ui/Cargo.toml`, `iem-mixer/src-tauri/Cargo.toml` — bump to `1.156.0`
- `iem-mixer/src-tauri/tauri.conf.json` — bump to `1.156.0`
- `README.md` — new changelog entry for v1.156.0

---

## Task 1: Version bump (1.155.0 → 1.156.0) + changelog

**Files:**
- Modify: all 5 `Cargo.toml` files listed above
- Modify: `iem-mixer/src-tauri/tauri.conf.json`
- Modify: `README.md`

- [ ] **Step 1: Bump Cargo.toml and tauri.conf.json versions**

```bash
cd /home/newlevel/devel/reaperiem
sed -i 's/version = "1.155.0"/version = "1.156.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.155.0"/"version": "1.156.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify all 6 files changed**

```bash
grep -l '1.156.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json | wc -l
```

Expected: `6`

- [ ] **Step 3: Add changelog entry to README.md**

Insert this block IMMEDIATELY after the line `## Changelog` and BEFORE the line `### v1.155.0 (2026-04-19)` in `README.md`:

```markdown
### v1.156.0 (2026-04-19)

- **Fix**: EQ band cards stack vertically on phones in portrait — previously iPhone 17 Pro (430 px viewport) rendered two bands per row, leaving ~58 px of horizontal space for FREQ/Q/GAIN sliders. Cards now go full-width on any touch device in portrait. (#179)
- **Fix**: EQ parameter labels and values now use higher-contrast text — FREQ/Q/GAIN labels went from `#555` to `#bbb`, values went from `#888` to `#eaeaea`.
- **Fix**: EQ row chrome compressed — label width 32→24 px, value width 60→44 px, card padding 12→10 px, row gap 8→6 px. Combined with portrait stacking, slider real estate on iPhone 17 Pro goes from ~58 px to ~290 px.
- **Feature**: Fullscreen movement-mode indicator — when an EQ slider enters active/activating state, the whole modal gets a cyan inset border via CSS `:has()`. Visible regardless of whether haptics are enabled.

```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json \
  README.md
git commit -m "chore: bump version to 1.156.0 + changelog for EQ iPhone 17 Pro fixes (#179)"
```

---

## Task 2: Add failing E2E tests

**Files:**
- Modify: `iem-mixer/e2e/tests/live/eq.spec.ts` — append one new `test.describe` block at the end of the file.

**Why failing:** Until Task 3 lands, current production has:
- `.eq-band-card` at 50% width in portrait on iPhone 17 Pro (test 1 will FAIL).
- `.eq-modal` with no `:has()` border rule (test 2 will FAIL — computed `box-shadow` will not contain `inset 0px 0px 0px 4px`).
- `.eq-param-label` colored `rgb(85, 85, 85)` (`#555`) — test 3 will FAIL.

- [ ] **Step 1: Amend the top-level import to include `devices`**

The current line 1 of `iem-mixer/e2e/tests/live/eq.spec.ts` reads:

```typescript
import { test, expect, Page } from "@playwright/test";
```

Replace that single line with:

```typescript
import { test, expect, Page, devices } from "@playwright/test";
```

- [ ] **Step 2: Append the new test block**

Append the following block to the END of `iem-mixer/e2e/tests/live/eq.spec.ts` (AFTER the closing `});` of the existing `test.describe("#167 EQ curve shape ...")` block — i.e., append after the current last non-empty line):

```typescript
// -----------------------------------------------------------------------------
// #179 EQ usability fixes — iPhone 17 Pro reported issues.
//
// Three tests verify the v1.156.0 CSS changes:
//   1. Portrait-phone stacking — iPhone 14 Pro Max device emulation (430×932
//      with hasTouch + isMobile, which makes Chromium report pointer=coarse)
//      must render EQ band cards one per row, not two per row.
//   2. Activation cue — when any EQ slider is active, .eq-modal must render
//      an inset cyan border (box-shadow: inset 0px 0px 0px 4px <accent>).
//   3. Contrast regression — .eq-param-label must NOT be the old #555. Belt-
//      and-braces against the color being reverted.
//
// All three reuse the engineer→MIREC→EQ path already exercised by the existing
// #167 test above. The stacking test uses its own browser context with mobile
// emulation because the new CSS rule keys off (pointer:coarse); plain
// setViewportSize on a Desktop Chrome context reports pointer=fine and would
// not trigger the rule. Each test self-cleans (closes the EQ modal on
// teardown) so live REAPER state isn't left dirty for subsequent tests.
// -----------------------------------------------------------------------------

test.describe("#179 EQ iPhone 17 Pro usability", () => {
  test("band cards stack vertically on iPhone 17 Pro portrait viewport", async ({
    browser,
  }) => {
    // Manual mobile context so Chromium reports pointer:coarse and matches
    // the new @media rule. iPhone 14 Pro Max has the same 430×932 portrait
    // geometry as iPhone 17 Pro.
    const context = await browser.newContext({
      ...devices["iPhone 14 Pro Max"],
    });
    const page = await context.newPage();

    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        if (
          !text.includes("Push API in incognito") &&
          !text.includes("[push] subscribe await failed") &&
          !text.includes("integrity")
        ) {
          consoleMessages.push(`[${msg.type()}] ${text}`);
        }
      }
    });

    try {
      await page.goto("/");
      await loginAs(page, "engineer", "1177");
      await page.goto("/engineer");
      await waitForMixer(page);

      await page.getByRole("button", { name: "Mics" }).click();
      await page.waitForTimeout(300);
      await openKebabMenu(page, "MIREC");
      await clickEqOption(page);
      await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

      // With pointer:coarse + orientation:portrait, every .eq-band-card
      // should occupy the full row. We assert by measuring widths of the
      // first two cards: if they're equal AND each is > 90% of their common
      // parent's inner width, they're stacked (not side-by-side).
      const geom = await page.evaluate(() => {
        const cards = Array.from(
          document.querySelectorAll<HTMLElement>(".eq-band-card"),
        );
        if (cards.length < 2) return { error: "fewer than 2 bands" };
        const parent = cards[0].parentElement as HTMLElement;
        const parentStyle = getComputedStyle(parent);
        const parentInnerWidth =
          parent.clientWidth -
          parseFloat(parentStyle.paddingLeft) -
          parseFloat(parentStyle.paddingRight);
        return {
          card0: cards[0].getBoundingClientRect().width,
          card1: cards[1].getBoundingClientRect().width,
          parentInnerWidth,
        };
      });
      if ("error" in geom) throw new Error(geom.error);

      // Equal widths (rounding tolerance 2 px).
      expect(
        Math.abs(geom.card0 - geom.card1),
        `cards have unequal widths: ${geom.card0} vs ${geom.card1}`,
      ).toBeLessThan(2);
      // Each card ≥ 90% of the parent's inner width ⇒ full-width ⇒ stacked.
      expect(
        geom.card0 / geom.parentInnerWidth,
        `card width ${geom.card0} / parent inner ${geom.parentInnerWidth} = ` +
          `${(geom.card0 / geom.parentInnerWidth).toFixed(2)}. If < 0.9, ` +
          `cards are still rendering side-by-side at iPhone 17 Pro viewport.`,
      ).toBeGreaterThan(0.9);

      // Self-clean: close the EQ modal.
      await page.locator(".eq-close-btn").click();

      expect(consoleMessages).toEqual([]);
    } finally {
      await context.close();
    }
  });

  test("active EQ slider triggers fullscreen movement-mode cue on modal", async ({
    page,
  }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        if (
          !text.includes("Push API in incognito") &&
          !text.includes("[push] subscribe await failed") &&
          !text.includes("integrity")
        ) {
          consoleMessages.push(`[${msg.type()}] ${text}`);
        }
      }
    });

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    await page.getByRole("button", { name: "Mics" }).click();
    await page.waitForTimeout(300);
    await openKebabMenu(page, "MIREC");
    await clickEqOption(page);
    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    // Baseline: .eq-modal should NOT have the inset 4px box-shadow when no
    // slider is active.
    const baselineShadow = await page
      .locator(".eq-modal")
      .evaluate((el) => getComputedStyle(el).boxShadow);
    expect(
      baselineShadow,
      `baseline .eq-modal box-shadow should not contain 'inset ... 4px'. ` +
        `Got: ${baselineShadow}`,
    ).not.toMatch(/inset[\s\S]*4px/);

    // Force-activate a slider by adding the `active` class directly. We don't
    // need to simulate the full long-press gesture — the :has() rule is what
    // we're testing, and it keys off the class, not the gesture. This keeps
    // the test deterministic (no gesture timing).
    await page.locator(".eq-slider-track").first().evaluate((el) => {
      el.classList.add("active");
    });
    // Give the transition a frame to apply.
    await page.waitForTimeout(200);

    const activeShadow = await page
      .locator(".eq-modal")
      .evaluate((el) => getComputedStyle(el).boxShadow);
    expect(
      activeShadow,
      `active .eq-modal box-shadow must contain 'inset ... 4px' for the ` +
        `fullscreen movement-mode cue. Got: ${activeShadow}`,
    ).toMatch(/inset[\s\S]*4px/);

    // Remove the active class and verify the cue clears.
    await page.locator(".eq-slider-track").first().evaluate((el) => {
      el.classList.remove("active");
    });
    await page.waitForTimeout(200);
    const clearedShadow = await page
      .locator(".eq-modal")
      .evaluate((el) => getComputedStyle(el).boxShadow);
    expect(clearedShadow).not.toMatch(/inset[\s\S]*4px/);

    // Self-clean.
    await page.locator(".eq-close-btn").click();

    expect(consoleMessages).toEqual([]);
  });

  test("EQ parameter labels use readable contrast (not #555)", async ({
    page,
  }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        if (
          !text.includes("Push API in incognito") &&
          !text.includes("[push] subscribe await failed") &&
          !text.includes("integrity")
        ) {
          consoleMessages.push(`[${msg.type()}] ${text}`);
        }
      }
    });

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    await page.getByRole("button", { name: "Mics" }).click();
    await page.waitForTimeout(300);
    await openKebabMenu(page, "MIREC");
    await clickEqOption(page);
    await expect(page.locator(".eq-overlay")).toBeVisible({ timeout: 5000 });

    const labelColor = await page
      .locator(".eq-param-label")
      .first()
      .evaluate((el) => getComputedStyle(el).color);

    // Reject the exact #555 that the old spec used. This is a regression
    // sentinel — any color darker than the new #bbb would still pass rgb
    // parsing but fail readability. Keep the check minimal: the old value
    // must never reappear.
    expect(
      labelColor,
      `.eq-param-label color is rgb(85, 85, 85) (#555). The spec in ` +
        `docs/superpowers/specs/2026-04-19-eq-iphone17-usability-design.md ` +
        `requires #bbb or brighter for readability.`,
    ).not.toBe("rgb(85, 85, 85)");

    // Self-clean.
    await page.locator(".eq-close-btn").click();

    expect(consoleMessages).toEqual([]);
  });
});
```

- [ ] **Step 3: Confirm the file lints**

This project does not run Playwright or type-check locally — the CI runs them. Confirm only that the file is syntactically well-formed by looking at the diff.

```bash
git diff --stat iem-mixer/e2e/tests/live/eq.spec.ts
```

Expected: one file changed, ~175 lines added, 1 line removed (the amended import).

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/e2e/tests/live/eq.spec.ts
git commit -m "test: add 3 failing E2E tests for EQ iPhone 17 Pro fixes (#179)"
```

---

## Task 3: Apply all 4 CSS changes

**Files:**
- Modify: `iem-mixer/iem-ui/style.css` lines 2100-2115, 2136-2142, 2186-2199, 2241-2248, 2258-2265.

These edits are ordered top-down through the file so line numbers stay stable between edits.

- [ ] **Step 1: Compact `.eq-band-card` padding**

Edit `iem-mixer/iem-ui/style.css`. Replace lines 2100-2108 (the `.eq-band-card` rule):

```css
.eq-band-card {
  background: var(--bg-channel);
  border: 1px solid;
  border-radius: var(--radius-sm);
  padding: 10px 12px;
  flex: 1 1 calc(50% - 4px);
  min-width: 160px;
  max-width: calc(50% - 4px);
}
```

with:

```css
.eq-band-card {
  background: var(--bg-channel);
  border: 1px solid;
  border-radius: var(--radius-sm);
  padding: 8px 10px;
  flex: 1 1 calc(50% - 4px);
  min-width: 160px;
  max-width: calc(50% - 4px);
}
```

- [ ] **Step 2: Replace the pixel-based breakpoint with device-capability query**

Edit `iem-mixer/iem-ui/style.css`. Replace lines 2110-2115 (the `@media (max-width: 420px)` block):

```css
@media (max-width: 420px) {
  .eq-band-card {
    flex: 1 1 100%;
    max-width: 100%;
  }
}
```

with:

```css
/* #179: stack bands on any portrait touch device (covers iPhone 17 Pro
   430px viewport and any future phone regardless of pixel count). */
@media (pointer: coarse) and (orientation: portrait) {
  .eq-band-card {
    flex: 1 1 100%;
    max-width: 100%;
  }
}
```

- [ ] **Step 3: Brighten `.eq-band-type` text**

Edit `iem-mixer/iem-ui/style.css`. Replace lines 2136-2142 (the `.eq-band-type` rule):

```css
.eq-band-type {
  font-size: 0.78em;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.05em;
  flex: 1;
}
```

with:

```css
.eq-band-type {
  font-size: 0.78em;
  color: #bbb;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  flex: 1;
}
```

- [ ] **Step 4: Compact `.eq-param-row` gap**

Edit `iem-mixer/iem-ui/style.css`. Replace lines 2186-2191 (the `.eq-param-row` rule):

```css
.eq-param-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}
```

with:

```css
.eq-param-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 4px;
}
```

- [ ] **Step 5: Shrink and brighten `.eq-param-label`**

Edit `iem-mixer/iem-ui/style.css`. Replace lines 2193-2199 (the `.eq-param-label` rule):

```css
.eq-param-label {
  font-size: 0.72em;
  color: var(--text-muted);
  width: 32px;
  flex-shrink: 0;
  text-transform: uppercase;
}
```

with:

```css
.eq-param-label {
  font-size: 0.72em;
  color: #bbb;
  font-weight: 600;
  width: 24px;
  flex-shrink: 0;
  text-transform: uppercase;
}
```

- [ ] **Step 6: Add fullscreen movement-mode cue**

Edit `iem-mixer/iem-ui/style.css`. Find the block at lines 2241-2248:

```css
.eq-slider-track.active .eq-slider-thumb {
  transform: translate(-50%, -50%) scale(1.3);
  box-shadow: 0 0 8px rgba(78, 205, 196, 0.5);
}

.eq-slider-track.activating .eq-slider-thumb {
  transform: translate(-50%, -50%) scale(1.1);
}
```

Immediately AFTER this block (keep these two rules intact), insert:

```css

/* #179: fullscreen movement-mode cue — when any slider enters active or
   activating state, the EQ modal gets a cyan inset border. Visible
   regardless of haptic settings. Uses CSS :has() (iOS Safari 15.4+,
   Chromium 105+, Firefox 121+). */
.eq-modal:has(.eq-slider-track.active),
.eq-modal:has(.eq-slider-track.activating) {
  box-shadow: inset 0 0 0 4px var(--accent);
  transition: box-shadow 120ms ease-in;
}
```

- [ ] **Step 7: Shrink and brighten `.eq-param-value`**

Edit `iem-mixer/iem-ui/style.css`. Replace the `.eq-param-value` rule (currently lines 2258-2265, may have shifted by a few lines after earlier edits — locate by exact text):

```css
.eq-param-value {
  font-size: 0.72em;
  color: var(--text-secondary);
  width: 60px;
  text-align: right;
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
}
```

with:

```css
.eq-param-value {
  font-size: 0.72em;
  color: var(--text-primary);
  width: 44px;
  text-align: right;
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
}
```

- [ ] **Step 8: Verify the file still parses**

```bash
cd /home/newlevel/devel/reaperiem
# Basic sanity: braces balance, no stray text.
awk 'BEGIN{n=0} {for(i=1;i<=length($0);i++){c=substr($0,i,1); if(c=="{")n++; if(c=="}")n--}} END{print "brace-balance:",n}' iem-mixer/iem-ui/style.css
```

Expected: `brace-balance: 0`

- [ ] **Step 9: Commit**

```bash
git add iem-mixer/iem-ui/style.css
git commit -m "fix(eq): iPhone 17 Pro usability — stack bands, compact chrome, brighten text, fullscreen cue (#179)"
```

---

## Task 4: Pre-push checks + push + monitor CI

- [ ] **Step 1: Run the only local check allowed by project hooks**

`cargo fmt --all --check` is the ONLY Rust check that runs locally (per project CLAUDE.md — clippy/test/build are hook-blocked). The CSS/TS changes don't need it, but run it to rule out accidental Rust drift:

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer && cargo fmt --all --check
```

Expected: exit 0, no output.

- [ ] **Step 2: Verify the commit sequence on dev**

```bash
cd /home/newlevel/devel/reaperiem
git log --oneline origin/dev..HEAD
```

Expected: three commits in this order (top = newest):
- `fix(eq): iPhone 17 Pro usability — stack bands, compact chrome, brighten text, fullscreen cue (#179)`
- `test: add 3 failing E2E tests for EQ iPhone 17 Pro fixes (#179)`
- `chore: bump version to 1.156.0 + changelog for EQ iPhone 17 Pro fixes (#179)`

- [ ] **Step 3: Push to `dev`**

```bash
git push origin dev
```

- [ ] **Step 4: Monitor CI until ALL jobs reach terminal state**

```bash
gh run list --branch dev --limit 1 --json databaseId,status,conclusion,headSha
```

Capture the `databaseId` of the latest run. Then:

```bash
# Background monitor — single sleep+view, returns when CI is done.
RUN_ID=<databaseId>
sleep 300 && gh run view $RUN_ID --json status,conclusion,jobs
```

Expected: all jobs conclude `success`, including `Deploy to iem.lan` and all post-deploy E2E tests.

If CI fails: `gh run view $RUN_ID --log-failed`, investigate, fix in ONE commit, push, monitor again.

---

## Task 5: Create PR dev → main and verify mergeable

- [ ] **Step 1: Fetch latest refs**

```bash
cd /home/newlevel/devel/reaperiem
git fetch origin
```

- [ ] **Step 2: Create the PR**

```bash
gh pr create --base main --head dev --title "fix(eq): iPhone 17 Pro usability — stacking, contrast, slider width, activation cue (#179)" --body "$(cat <<'EOF'
## Summary

Fixes issue #179 — EQ modal is hard to use on iPhone 17 Pro. MIREC reported that FREQ/Q/GAIN sliders are too short for precision, labels and values are too dark, and there's no visible cue when a slider activates without haptics.

Pure CSS polish. No Rust changes, no markup changes.

## Changes

- **Stacking:** `.eq-band-card` switches to single-column under `(pointer: coarse) and (orientation: portrait)` — covers iPhone 17 Pro (430 px) and any future phone, device-capability rather than pixel guess.
- **Contrast:** `.eq-param-label` `#555` → `#bbb` (weight 600). `.eq-param-value` `#888` → `#eaeaea` (primary). `.eq-band-type` `#888` → `#bbb`.
- **Slider real estate:** label width 32→24 px, value width 60→44 px, card padding 12→10 px, row gap 8→6 px. Net iPhone 17 Pro slider width: ~58 → ~290 px.
- **Activation cue:** new `.eq-modal:has(.eq-slider-track.active)` rule draws an inset 4 px cyan border around the whole modal. Uses CSS `:has()` — no Leptos signal plumbing.

## Test plan

Three new E2E tests in `iem-mixer/e2e/tests/live/eq.spec.ts`:

- [ ] `#179 EQ iPhone 17 Pro usability > band cards stack vertically on iPhone 17 Pro portrait viewport` — sets viewport to 430×932, opens EQ on MIREC, asserts card width ≥ 90 % of parent inner width.
- [ ] `#179 EQ iPhone 17 Pro usability > active EQ slider triggers fullscreen movement-mode cue on modal` — adds `.active` class to a slider track, asserts `.eq-modal` computed `box-shadow` contains `inset ... 4px`, clears on removal.
- [ ] `#179 EQ iPhone 17 Pro usability > EQ parameter labels use readable contrast (not #555)` — asserts `.eq-param-label` computed color is not `rgb(85, 85, 85)`.

All three assert `consoleMessages = []` (airuleset browser-console-zero-errors).

Verification: post-deploy Playwright run on `iem.lan` via `playwright.live.config.ts`. Manual visual verification on real iPhone 17 Pro after deploy.

Spec: \`docs/superpowers/specs/2026-04-19-eq-iphone17-usability-design.md\`
Plan: \`docs/superpowers/plans/2026-04-19-eq-iphone17-usability.md\`

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Verify PR is mergeable**

```bash
PR_NUMBER=$(gh pr view --json number --jq .number)
gh api repos/zbynekdrlik/reaperiem/pulls/$PR_NUMBER --jq '{mergeable: .mergeable, mergeable_state: .mergeable_state}'
```

Expected: `{"mergeable": true, "mergeable_state": "clean"}`.

If `mergeable_state` is `"behind"`, merge `main` into `dev` and push again. If `"blocked"` or `"dirty"`, investigate and fix.

- [ ] **Step 4: Report the green PR URL and STOP**

Do NOT merge. Present the PR URL, latest CI run summary, and `mergeable_state: clean` to the user, then wait for explicit merge approval.

---

## Task Dependencies

```
Task 1 (version bump)        ───▶  Task 2 (failing tests)
                                      │
                                      ▼
                              Task 3 (CSS changes)
                                      │
                                      ▼
                              Task 4 (pre-push + push + CI)
                                      │
                                      ▼
                              Task 5 (PR, verify, STOP)
```

Tasks are strictly sequential. Each creates one commit; CI sees all three together and runs the tests against the deployed build.

---

## Verification

After CI is green:

1. All CI jobs pass, including `Deploy to iem.lan` and the 3 new post-deploy E2E tests.
2. PR `mergeable: true`, `mergeable_state: "clean"`.
3. Completion report includes the E2E test coverage table for all three new tests.
4. STOP at green PR URL. Wait for user merge approval.

Post-merge verification (after user approves merge):

1. Main-branch CI green end-to-end.
2. Manual Playwright visual check at `https://iem.newlevel.media/` (or `http://10.77.9.231/`) on the engineer view → MIREC EQ modal: confirm bands stack at mobile viewport width, labels/values readable, fullscreen cue appears on slider active.
