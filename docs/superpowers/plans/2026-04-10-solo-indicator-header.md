# Solo Indicator in Header Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a prominent "Clear Solo" button in the mixer header that appears when solo is active on any channel, visible across all tabs, allowing one-click unsolo from any category tab.

**Architecture:** UI-only change (server logic unchanged). Header layout reclaims space by shrinking back button and making LAN/WAN indicator vertical. When `soloed` signal is non-empty, the header-version display is replaced by a prominent yellow "SOLO ✕" button that sends `SetSolo { soloed: [] }` on click. The existing server-side exit-solo logic (v1.137.0) handles the restore.

**Tech Stack:** Rust (Leptos WASM frontend), CSS, Playwright E2E tests

**Spec:** `docs/superpowers/specs/2026-04-10-solo-indicator-header-design.md`

---

## File Map

### UI code
- `iem-mixer/iem-ui/src/pages/mixer.rs:1182-1225` — Header markup: shrink back button container, add conditional render for SOLO button vs version display
- `iem-mixer/iem-ui/style.css` — CSS: compact back button, vertical network indicator, new `.header-solo-btn` with pulse animation

### E2E tests
- `iem-mixer/e2e/tests/live/mixer.spec.ts` — Add 2 new tests for header solo button (single-tab clear, cross-tab visibility)

### Version bump
- 5 Cargo.toml + 1 tauri.conf.json: 1.137.0 → 1.138.0

### Changelog
- `README.md` — Add v1.138.0 entry

---

## Task 1: Version Bump (1.137.0 → 1.138.0)

- [ ] **Step 1: Bump all version files**

```bash
sed -i 's/version = "1.137.0"/version = "1.138.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.137.0"/"version": "1.138.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify**

```bash
grep -c '1.138.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
# Both should return 1
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.138.0"
```

---

## Task 2: CSS — Compact back button and vertical network indicator

**Files:**
- Modify: `iem-mixer/iem-ui/style.css`

- [ ] **Step 1: Shrink `.mixer-header .back-btn`**

Find the existing rule at line ~91:
```css
.mixer-header .back-btn {
  width: 40px;
  height: 40px;
  ...
}
```

Replace the `width` and `height` to save horizontal space:
```css
.mixer-header .back-btn {
  width: 32px;
  height: 32px;
  border-radius: var(--radius-sm);
  border: none;
  background: transparent;
  color: var(--text-primary);
  font-size: 1.1rem;
  cursor: pointer;
  padding: 0;
}
```

Keep all other existing properties (hover, etc.) unchanged.

- [ ] **Step 2: Make `.network-indicator` vertical**

Find the existing rule at line ~1847:
```css
.network-indicator {
  font-size: 0.6em;
  font-weight: 700;
  padding: 2px 6px;
  ...
}
```

Replace with:
```css
.network-indicator {
  font-size: 0.5em;
  font-weight: 700;
  padding: 2px 3px;
  border-radius: 4px;
  letter-spacing: 0;
  flex-shrink: 0;
  margin-right: 4px;
  writing-mode: vertical-rl;
  transform: rotate(180deg);
  line-height: 1.1;
  max-width: 14px;
}
```

Keep `.network-indicator.local` and `.network-indicator.remote` unchanged.

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/iem-ui/style.css
git commit -m "style: shrink back button and verticalize network indicator for header space (#155)"
```

---

## Task 3: CSS — Add `.header-solo-btn` styles

**Files:**
- Modify: `iem-mixer/iem-ui/style.css`

- [ ] **Step 1: Add solo button styles after `.header-version-date`**

Find the existing block at line ~120:
```css
.header-version-date {
  font-size: 0.55em;
  color: #fff;
  white-space: nowrap;
}
```

Add AFTER it:
```css
/* Solo active indicator in header (replaces version when solo is active) */
.header-solo-btn {
  background: #f59e0b;
  color: #fff;
  border: none;
  padding: 6px 14px;
  border-radius: var(--radius-sm);
  font-weight: 700;
  font-size: 0.85rem;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  margin-right: 8px;
  animation: pulse-solo 1.5s ease-in-out infinite;
  letter-spacing: 0.05em;
  text-transform: uppercase;
}

.header-solo-btn:hover {
  background: #d97706;
}

.header-solo-btn:active {
  background: #b45309;
}

.header-solo-btn .solo-close {
  font-size: 1rem;
  font-weight: 900;
  line-height: 1;
}

@keyframes pulse-solo {
  0%, 100% {
    box-shadow: 0 0 0 0 rgba(245, 158, 11, 0.6);
  }
  50% {
    box-shadow: 0 0 0 6px rgba(245, 158, 11, 0);
  }
}

@media (prefers-reduced-motion: reduce) {
  .header-solo-btn {
    animation: none;
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/iem-ui/style.css
git commit -m "style: add header solo button styles with pulse animation (#155)"
```

---

## Task 4: UI — Conditional render of solo button in header

**Files:**
- Modify: `iem-mixer/iem-ui/src/pages/mixer.rs`

- [ ] **Step 1: Read current header code to confirm line numbers**

Read lines 1182-1225 of `iem-mixer/iem-ui/src/pages/mixer.rs` to confirm the header structure before editing.

- [ ] **Step 2: Replace `.header-version` block with conditional render**

Find this block (around lines 1194-1197):
```rust
                <div class="header-version">
                    <span class="header-version-number">{iem_core::version_label()}</span>
                    <span class="header-version-date">{iem_core::build_datetime()}</span>
                </div>
```

Replace with:
```rust
                <Show
                    when=move || !soloed.get().is_empty()
                    fallback=|| view! {
                        <div class="header-version">
                            <span class="header-version-number">{iem_core::version_label()}</span>
                            <span class="header-version-date">{iem_core::build_datetime()}</span>
                        </div>
                    }
                >
                    <button
                        class="header-solo-btn"
                        aria-label="Clear solo"
                        on:click=move |_| {
                            if !connected.get() {
                                return;
                            }
                            // Optimistic UI: clear soloed signal locally and restore pre-solo mutes
                            let saved = pre_solo_mutes.get();
                            set_channels.update(|chs| {
                                for c in chs.iter_mut() {
                                    let should_be_muted = saved.get(&c.track_index).copied().unwrap_or(false);
                                    c.muted = should_be_muted;
                                }
                            });
                            set_pre_solo_mutes.set(HashMap::new());
                            set_soloed.set(std::collections::HashSet::new());
                            // Send empty SetSolo to server — triggers server-side unsolo + restore
                            ws_send(ws, &iem_core::ClientMsg::SetSolo { soloed: vec![] });
                        }
                    >
                        "SOLO"
                        <span class="solo-close">"\u{2715}"</span>
                    </button>
                </Show>
```

**Notes for implementer:**
- The `view!` macro is already imported (used elsewhere in mixer.rs)
- `Show` component from leptos is already used elsewhere in this file (see line 1201)
- `soloed`, `set_soloed`, `connected`, `pre_solo_mutes`, `set_pre_solo_mutes`, `set_channels`, `ws` are all in scope inside the `view!` macro (captured from the component params)
- `HashMap` is already imported (used elsewhere)
- `ws_send` helper already exists and is used (see `on_solo_click` handler)
- The `"\u{2715}"` is the multiplication X symbol (✕)

- [ ] **Step 3: Format**

```bash
cd iem-mixer/iem-ui && cargo fmt --all
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/iem-ui/src/pages/mixer.rs
git commit -m "feat: add solo clear button in header when solo is active (#155)"
```

---

## Task 5: E2E test — Header solo button shows and clears solo (single tab)

**Files:**
- Modify: `iem-mixer/e2e/tests/live/mixer.spec.ts`

- [ ] **Step 1: Add test inside `test.describe("Solo sync", ...)` block**

Find the "unsolo restores original mute states" test (around line 2317). After its closing `});`, and BEFORE the `});` that closes `test.describe("Solo sync", ...)`, add:

```typescript
  test("header solo button clears solo from any tab", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();

    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    await waitForMixer(page);

    await expect(page.locator(".channel").first()).toBeVisible({ timeout: 15000 });

    // Switch to Mics tab to find a solo button
    const micsTab = page.locator(".category-tab.mics");
    if ((await micsTab.count()) > 0) await micsTab.click();
    await page.waitForTimeout(500);

    // Verify header initially shows version (no solo active)
    await expect(page.locator(".header-version")).toBeVisible();
    await expect(page.locator(".header-solo-btn")).toHaveCount(0);

    // Activate solo
    const soloBtn = page.locator(".solo-btn").first();
    await soloBtn.click({ force: true });
    await page.waitForTimeout(500);
    await expect(soloBtn).toHaveClass(/on/, { timeout: 5000 });

    // Header should now show SOLO button
    await expect(page.locator(".header-solo-btn")).toBeVisible({ timeout: 3000 });
    await expect(page.locator(".header-version")).toHaveCount(0);

    // Click header SOLO button to clear
    await page.locator(".header-solo-btn").click({ force: true });
    await page.waitForTimeout(500);

    // Header should revert to version display
    await expect(page.locator(".header-version")).toBeVisible({ timeout: 3000 });
    await expect(page.locator(".header-solo-btn")).toHaveCount(0);

    // Channel solo button should be off
    await expect(soloBtn).toHaveClass(/off/, { timeout: 3000 });

    await ctx.close();
  });
```

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/e2e/tests/live/mixer.spec.ts
git commit -m "test: header solo button shows and clears solo (#155)"
```

---

## Task 6: E2E test — Cross-tab header solo button visibility

**Files:**
- Modify: `iem-mixer/e2e/tests/live/mixer.spec.ts`

- [ ] **Step 1: Add second test after Task 5's test, before `});` of describe block**

```typescript
  test("header solo button appears on second tab when first tab solos", async ({
    browser,
  }) => {
    const ctx1 = await browser.newContext();
    const ctx2 = await browser.newContext();
    const page1 = await ctx1.newPage();
    const page2 = await ctx2.newPage();

    await page1.goto("/");
    await page2.goto("/");
    await loginAs(page1, "petronela");
    await page1.goto("/petronela");
    await loginAs(page2, "petronela");
    await page2.goto("/petronela");

    await waitForMixer(page1);
    await waitForMixer(page2);

    await expect(page1.locator(".channel").first()).toBeVisible({ timeout: 15000 });
    await expect(page2.locator(".channel").first()).toBeVisible({ timeout: 15000 });

    // Switch both to Mics tab
    const micsTab1 = page1.locator(".category-tab.mics");
    if ((await micsTab1.count()) > 0) await micsTab1.click();
    const micsTab2 = page2.locator(".category-tab.mics");
    if ((await micsTab2.count()) > 0) await micsTab2.click();
    await page1.waitForTimeout(500);
    await page2.waitForTimeout(500);

    // Both headers show version initially
    await expect(page1.locator(".header-version")).toBeVisible();
    await expect(page2.locator(".header-version")).toBeVisible();

    // Activate solo on tab1
    const soloBtn1 = page1.locator(".solo-btn").first();
    await soloBtn1.click({ force: true });
    await page1.waitForTimeout(500);
    await expect(soloBtn1).toHaveClass(/on/, { timeout: 5000 });

    // Tab1 header shows SOLO button
    await expect(page1.locator(".header-solo-btn")).toBeVisible({ timeout: 3000 });

    // Tab2 should ALSO show SOLO button (sync via SoloUpdate broadcast)
    await expect(page2.locator(".header-solo-btn")).toBeVisible({ timeout: 5000 });

    // Clear solo from tab2
    await page2.locator(".header-solo-btn").click({ force: true });
    await page2.waitForTimeout(500);

    // Both tabs revert to version display
    await expect(page2.locator(".header-version")).toBeVisible({ timeout: 3000 });
    await expect(page1.locator(".header-version")).toBeVisible({ timeout: 5000 });

    // Both tabs' solo buttons are off
    await expect(soloBtn1).toHaveClass(/off/, { timeout: 3000 });

    await ctx1.close();
    await ctx2.close();
  });
```

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/e2e/tests/live/mixer.spec.ts
git commit -m "test: cross-tab header solo button visibility and clear (#155)"
```

---

## Task 7: Format check + Push + Monitor CI

- [ ] **Step 1: Run format check**

```bash
cd iem-mixer && cargo fmt --all --check
cd iem-mixer/iem-ui && cargo fmt --all --check
```

Fix any issues with `cargo fmt --all` if needed.

- [ ] **Step 2: Push**

```bash
git push origin dev
```

- [ ] **Step 3: Monitor CI**

```bash
gh run list --branch dev --limit 3
```

Poll `gh run view <run-id> --json status,conclusion,jobs` until all jobs reach terminal state. All 9 jobs must be green (Test Integrity, Lint, VBAN, WASM, Tests, E2E, Tauri, Deploy). Post-deploy E2E must pass all tests (including the 2 new header solo tests).

- [ ] **Step 4: If CI fails, investigate with `gh run view <id> --log-failed` and fix all issues in ONE commit**

Common expected issues:
- Clippy warnings on new Rust code (collapsible_if, unused imports)
- Format issues (cargo fmt)
- E2E test timing issues on production system — may need waitForTimeout tweaks
- The `Show` component usage — verify the view! macro syntax is correct in Leptos

---

## Task 8: Update changelog + PR update

- [ ] **Step 1: Add changelog entry to README.md**

Read `README.md` to find the `## Changelog` section. Add after `### v1.137.0` (at the top of the changelog):

```markdown
### v1.138.0 (2026-04-10)

- **Feature**: Solo indicator in header — when solo is active, a prominent "SOLO ✕" button replaces the version display in the header, visible on every tab. Click it to clear solo from anywhere.
- **UI**: Header compacted — smaller back button and vertical LAN/WAN indicator to save horizontal space
```

- [ ] **Step 2: Commit and push**

```bash
git add README.md
git commit -m "docs: add changelog for v1.138.0 solo indicator in header (#155)"
git push origin dev
```

- [ ] **Step 3: Update PR #158 title/description**

Since PR #158 is still open and includes these new commits, update its description to mention the additional v1.138.0 feature:

```bash
gh pr edit 158 --title "fix: solo crash recovery + header indicator (v1.137.0 + v1.138.0) (#155)" --body "$(cat <<'EOF'
## Summary
**v1.137.0 — Solo crash recovery:**
- Solo muting moved from UI to server — prevents orphaned mutes on PWA crash
- Pre-solo mute states saved server-side, restored on unsolo
- Solo persists across WebSocket disconnects until explicitly turned off

**v1.138.0 — Solo indicator in header:**
- Yellow "SOLO ✕" button appears in header when solo is active (replaces version display)
- Visible on every category tab — solves visibility problem from v1.137.0 persistence
- One click clears solo from any tab
- Header compacted: smaller back button, vertical LAN/WAN indicator

Fixes #155

## Test plan
- [x] Solo on → unsolo → mute states restored correctly
- [x] Solo on → kill PWA → reopen → solo still active → unsolo → mutes restored
- [x] Solo exclusive mode still works (solo A, then solo B replaces A)
- [x] Multi-tab: solo syncs across tabs
- [x] Header SOLO button clears solo from any tab
- [x] Cross-tab: solo on tab1 → tab2 header shows SOLO button
- [x] Post-deploy E2E: all tests pass on live iem.lan

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 4: Verify PR is mergeable after CI completes**

```bash
gh api repos/zbynekdrlik/reaperiem/pulls/158 --jq '{mergeable: .mergeable, mergeable_state: .mergeable_state}'
```

Should return `mergeable: true, mergeable_state: "clean"`.

---

## Task Dependencies

```
Task 1 (version bump)           ──┐
Task 2 (CSS compact header)     ──┤
Task 3 (CSS solo button)        ──┤── Sequential
Task 4 (UI conditional render)  ──┤
Task 5 (E2E single-tab)         ──┤
Task 6 (E2E cross-tab)          ──┘
         │
         ▼
Task 7 (format + push + CI)
         │
         ▼
Task 8 (changelog + PR update)
```

Tasks 2-6 depend on Task 1 (version bump must be first). Task 4 depends on Tasks 2-3 (CSS must exist before UI can reference it, although order doesn't strictly matter for CSS-only builds). Tasks 5-6 depend on Task 4 (tests require the UI to work). Tasks 7-8 run after all implementation is done.

---

## Verification checklist

After CI is green:

1. All 9 CI jobs pass on dev branch
2. Post-deploy E2E passes including the 2 new header solo tests
3. `.header-solo-btn` visible in DOM when solo is active (verify manually via http://10.77.9.231/)
4. Header reverts to version display when solo is cleared
5. PR #158 includes both v1.137.0 and v1.138.0 work
6. PR mergeable: clean
