# Vibration Reliability Fix — Implementation Plan (#162)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make phone vibration during SOS alerts reliable by replacing interval-based single pulses with a long native pattern + visibilitychange recovery.

**Architecture:** Replace `setInterval(1500ms)` + `vibrate(500)` calls with `navigator.vibrate([500, 1000, ...] × 30)` pattern that the browser manages natively (immune to JS timer throttling). Add a 30s safety-net interval to re-fire the pattern for very long alerts. Add a `visibilitychange` listener that re-fires the pattern when the engineer returns to the app. Clean up all three (pattern, interval, listener) on alert clear.

**Tech Stack:** Rust/Leptos WASM, web-sys, Playwright E2E

**Spec:** `docs/superpowers/specs/2026-04-12-vibration-reliability-design.md`

---

## Context

The current vibration in `alert_toast.rs` uses `setInterval(1500ms)` with individual `navigator.vibrate(500)` calls on each tick. Mobile browsers throttle `setInterval` in background/minimized PWAs and cancel `navigator.vibrate()` when the page becomes hidden. The pattern-based API (`navigator.vibrate([500, 1000, ...])`) hands timing to the browser engine, which is far more resilient.

**Key facts:**
- `web_sys::Navigator::vibrate_with_pattern(&js_sys::Array)` is the pattern API — requires `Navigator` feature (already enabled in `iem-ui/Cargo.toml:44`)
- `visibilitychange` event pattern exists in `mixer.rs:790` — use same `Closure::wrap` + `add_event_listener_with_callback` pattern
- The visibility listener must be added/removed dynamically (only while alert is active), not leaked with `.forget()`
- Sound loop (chime every 10s) and red page pulse overlay are NOT changed
- Service worker notification is NOT changed
- The `__iem_alert_vib` window property stores the interval ID for cleanup — keep this pattern but for the 30s refresh interval

**Production member PINs for E2E:** petronela/7711, stevo/7711, engineer/1177

---

## File Map

| File | Change |
|------|--------|
| `iem-mixer/iem-ui/src/components/alert_toast.rs:28-48` | Replace interval vibration with pattern + 30s refresh + visibilitychange listener |
| `iem-mixer/iem-ui/src/components/alert_toast.rs:131-158` | Update `stop_loops()` to also remove visibilitychange listener |
| `iem-mixer/e2e/tests/live/alert.spec.ts` | Add E2E test verifying pattern-based vibration is fired |
| 6 version files | 1.145.0 → 1.146.0 |

---

## Task 1: Version Bump (1.145.0 → 1.146.0)

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`

- [ ] **Step 1: Bump all version files**

```bash
sed -i 's/version = "1.145.0"/version = "1.146.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.145.0"/"version": "1.146.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify**

```bash
grep -c '1.146.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
# Both should return 1
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.146.0"
```

---

## Task 2: Replace interval vibration with pattern-based vibration + visibilitychange listener

**Files:**
- Modify: `iem-mixer/iem-ui/src/components/alert_toast.rs:14-48` (vibration start)
- Modify: `iem-mixer/iem-ui/src/components/alert_toast.rs:78-91` (alert clear)
- Modify: `iem-mixer/iem-ui/src/components/alert_toast.rs:131-158` (`stop_loops`)

### Background

The current code (lines 28-48) does:
```rust
// setInterval(1500ms) with vibrate(500) on each tick
let vib_cb = Closure::wrap(Box::new(move || {
    if let Some(window) = web_sys::window() {
        let _ = window.navigator().vibrate_with_duration(500);
    }
}) as Box<dyn FnMut()>);
// ... set_interval_with_callback_and_timeout_and_arguments_0(vib_cb, 1500) ...
// ... immediate vibrate(500) ...
```

This must become:
1. Build a `js_sys::Array` pattern `[500, 1000]` repeated 30 times (30 × 1.5s = 45s of vibration)
2. Call `navigator.vibrate_with_pattern(&pattern)` immediately
3. Set a 30s refresh interval that re-fires the same pattern (safety net for pattern expiry)
4. Add a `visibilitychange` listener on `document` that re-fires the pattern when page becomes visible
5. Store both the interval ID and a reference to the visibility closure for cleanup

- [ ] **Step 1: Replace the vibration start block (lines 14-48)**

Replace the entire vibration section in the `Effect::new` closure. The new code builds a vibration pattern, fires it immediately, sets a 30s refresh interval, and adds a `visibilitychange` listener.

In `alert_toast.rs`, replace lines 14-18 (the `Rc<RefCell>` declarations and beginning of Effect) through line 48 (end of vib_effect borrow_mut) with:

```rust
    // Rc<RefCell> to prevent closures from being dropped (prevents GC of JS callbacks)
    let vib_effect: std::rc::Rc<std::cell::RefCell<Option<Closure<dyn FnMut()>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let snd_effect: std::rc::Rc<std::cell::RefCell<Option<Closure<dyn FnMut()>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let vis_effect: std::rc::Rc<std::cell::RefCell<Option<Closure<dyn FnMut()>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    Effect::new(move || {
        let current = alert.get();
        if let Some((_, ref name)) = current {
            // System notification (ask permission if needed)
            let name_clone = name.clone();
            wasm_bindgen_futures::spawn_local(async move {
                request_and_notify(&name_clone).await;
            });

            // Build vibration pattern: [500, 1000] × 30 = 45s of pulsing
            // Browser handles timing natively — immune to JS timer throttling
            let pattern = js_sys::Array::new();
            for _ in 0..30 {
                pattern.push(&JsValue::from(500));  // vibrate 500ms
                pattern.push(&JsValue::from(1000)); // pause 1000ms
            }

            // Fire pattern immediately
            if let Some(window) = web_sys::window() {
                let _ = window.navigator().vibrate_with_pattern(&pattern);
            }

            // 30s safety-net interval: re-fire pattern for very long alerts
            let pattern_clone = pattern.clone();
            let vib_cb = Closure::wrap(Box::new(move || {
                if let Some(window) = web_sys::window() {
                    let _ = window.navigator().vibrate_with_pattern(&pattern_clone);
                }
            }) as Box<dyn FnMut()>);
            if let Some(window) = web_sys::window() {
                let id = window
                    .set_interval_with_callback_and_timeout_and_arguments_0(
                        vib_cb.as_ref().unchecked_ref(),
                        30_000,
                    )
                    .unwrap_or(0);
                let _ = js_sys::Reflect::set(
                    &window,
                    &JsValue::from_str("__iem_alert_vib"),
                    &JsValue::from(id),
                );
            }
            *vib_effect.borrow_mut() = Some(vib_cb);

            // visibilitychange listener: re-fire pattern when engineer returns to app
            let pattern_vis = pattern.clone();
            let vis_cb = Closure::wrap(Box::new(move || {
                if let Some(window) = web_sys::window() {
                    if let Some(doc) = window.document() {
                        if !doc.hidden() {
                            let _ = window.navigator().vibrate_with_pattern(&pattern_vis);
                        }
                    }
                }
            }) as Box<dyn FnMut()>);
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                let _ = doc.add_event_listener_with_callback(
                    "visibilitychange",
                    vis_cb.as_ref().unchecked_ref(),
                );
            }
            *vis_effect.borrow_mut() = Some(vis_cb);
```

**Note:** The `vis_effect` `Rc<RefCell>` is declared alongside `vib_effect` and `snd_effect` before the `Effect::new` closure. The closure captures all three.

- [ ] **Step 2: Update the alert-clear branch (line 78-91)**

In the `else` branch of the Effect (when alert is `None`), add cleanup of the visibility listener. Replace:

```rust
        } else {
            // Drop closures
            vib_effect.borrow_mut().take();
            snd_effect.borrow_mut().take();
            stop_loops();
```

With:

```rust
        } else {
            // Remove visibilitychange listener before dropping the closure
            if let Some(ref cb) = *vis_effect.borrow() {
                if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                    let _ = doc.remove_event_listener_with_callback(
                        "visibilitychange",
                        cb.as_ref().unchecked_ref(),
                    );
                }
            }
            // Drop closures
            vib_effect.borrow_mut().take();
            snd_effect.borrow_mut().take();
            vis_effect.borrow_mut().take();
            stop_loops();
```

- [ ] **Step 3: Update `stop_loops()` — no changes needed**

The existing `stop_loops()` function (lines 131-159) already:
1. Clears `__iem_alert_vib` interval (now the 30s refresh instead of 1.5s)
2. Clears `__iem_alert_snd` interval (unchanged)
3. Calls `vibrate_with_duration(0)` to cancel any in-progress pattern

This is correct for the new implementation. The `vibrate(0)` call cancels the native pattern — no changes needed.

- [ ] **Step 4: Run `cargo fmt`**

```bash
cd iem-mixer && cargo fmt --all
```

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/iem-ui/src/components/alert_toast.rs
git commit -m "fix: replace interval vibration with pattern-based + visibilitychange recovery (#162)"
```

---

## Task 3: E2E test — verify pattern-based vibration fires on alert

**Files:**
- Modify: `iem-mixer/e2e/tests/live/alert.spec.ts`

The vibration API (`navigator.vibrate()`) is not available in headless Chromium, but we can monkey-patch it to capture calls and verify the app fires a pattern (array) instead of single durations.

- [ ] **Step 1: Add E2E test for pattern-based vibration**

Add this test inside the existing `test.describe("Band Member Alert Button (#125)")` block in `alert.spec.ts`, after the "alert persists until engineer dismisses" test (after line 191):

```typescript
  test("engineer vibration uses pattern array, not single pulses (#162)", async ({ browser }) => {
    const consoleMessages: string[] = [];
    const ctx1 = await browser.newContext();
    const ctx2 = await browser.newContext();
    const memberPage = await ctx1.newPage();
    const engineerPage = await ctx2.newPage();

    engineerPage.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        if (!text.includes("navigator.vibrate")) {
          consoleMessages.push(`[${msg.type()}] ${text}`);
        }
      }
    });

    // Monkey-patch navigator.vibrate on engineer page to capture calls
    await engineerPage.goto("/");
    await engineerPage.evaluate(() => {
      (window as any).__vibrateCalls = [];
      navigator.vibrate = (pattern: any) => {
        (window as any).__vibrateCalls.push(
          Array.isArray(pattern) ? { type: "pattern", length: pattern.length }
                                 : { type: "single", value: pattern }
        );
        return true;
      };
    });

    await loginAs(engineerPage, "engineer", "1177");
    await engineerPage.goto("/engineer");
    await waitForMixer(engineerPage);

    // Member triggers SOS
    await memberPage.goto("/");
    const membersResp = await memberPage.request.get("/api/members");
    const members = await membersResp.json();
    const member = members[0];
    await loginAs(memberPage, member.id);
    await memberPage.goto(`/${member.id}`);
    await waitForMixer(memberPage);

    // Wait for WS to be ready
    await memberPage.waitForFunction(
      () => document.querySelectorAll(".channel").length > 0,
      { timeout: 10000 },
    );

    // Clear any residual alert
    const alertBtn = memberPage.locator(".alert-btn");
    await expect(alertBtn).toBeVisible({ timeout: 5000 });
    let hadResidual = false;
    try {
      await expect(alertBtn).toHaveClass(/active/, { timeout: 1000 });
      hadResidual = true;
    } catch { /* no residual */ }
    if (hadResidual) {
      await alertBtn.click({ force: true });
      await expect(alertBtn).not.toHaveClass(/active/, { timeout: 5000 });
    }

    // Re-apply monkey-patch (navigation may have cleared it)
    await engineerPage.evaluate(() => {
      (window as any).__vibrateCalls = [];
      navigator.vibrate = (pattern: any) => {
        (window as any).__vibrateCalls.push(
          Array.isArray(pattern) ? { type: "pattern", length: pattern.length }
                                 : { type: "single", value: pattern }
        );
        return true;
      };
    });

    // Member clicks SOS
    await alertBtn.click({ force: true });

    // Engineer should see the toast
    const toast = engineerPage.locator(".alert-toast");
    await expect(toast).toBeVisible({ timeout: 10000 });

    // Wait a moment for vibration to fire
    await engineerPage.waitForTimeout(500);

    // Check vibration calls — should have at least one pattern call, no single 500ms pulses
    const calls = await engineerPage.evaluate(() => (window as any).__vibrateCalls);
    const patternCalls = calls.filter((c: any) => c.type === "pattern");
    const singlePulses = calls.filter((c: any) => c.type === "single" && c.value === 500);

    expect(patternCalls.length).toBeGreaterThanOrEqual(1);
    // Pattern should be [500, 1000] × 30 = 60 elements
    expect(patternCalls[0].length).toBe(60);
    // No old-style single 500ms pulses (vibrate(0) for cancel is OK)
    expect(singlePulses.length).toBe(0);

    // Clean up — dismiss the alert
    const dismissBtn = engineerPage.locator(".alert-toast-dismiss");
    await dismissBtn.click({ force: true });
    await expect(toast).not.toBeVisible({ timeout: 3000 });

    // Zero console errors
    expect(consoleMessages).toEqual([]);

    await ctx1.close();
    await ctx2.close();
  });
```

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/e2e/tests/live/alert.spec.ts
git commit -m "test: E2E verifies pattern-based vibration on SOS alert (#162)"
```

---

## Task 4: Changelog + Push + Monitor CI

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add changelog entry**

Add under the changelog section in `README.md`, before the v1.145.0 entry:

```markdown
### v1.146.0 (2026-04-12)
- **Fix**: Vibration reliability in SOS alerts — replaced interval-based single pulses with browser-native pattern vibration + foreground recovery (#162)
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: changelog for v1.146.0 vibration reliability fix (#162)"
```

- [ ] **Step 3: Run local lint check**

```bash
cd iem-mixer && cargo fmt --all --check
```

- [ ] **Step 4: Push and monitor CI**

```bash
git push origin dev
gh run list --limit 3
# Monitor the run until ALL jobs reach terminal state
```

- [ ] **Step 5: If CI fails, investigate with `gh run view <id> --log-failed` and fix all issues in ONE commit**

---

## Task 5: PR + Post-Deploy Verification

- [ ] **Step 1: Create PR**

```bash
gh pr create --title "fix: vibration reliability in SOS alerts (#162)" --body "$(cat <<'EOF'
## Summary
- Replace `setInterval(1500ms)` + `vibrate(500)` with `navigator.vibrate([500, 1000] × 30)` pattern
- Add `visibilitychange` listener to re-fire pattern when engineer returns to app
- 30s safety-net refresh interval for very long alerts (vs 1.5s polling)

Fixes #162

## Test plan
- [ ] E2E: pattern-based vibration fires on SOS alert (monkey-patched `navigator.vibrate` captures calls)
- [ ] E2E: existing alert tests still pass (button visibility, active state, dismiss)
- [ ] Manual: trigger SOS on real phone, verify vibration pulses continuously
- [ ] Manual: switch away from app and back — vibration resumes

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Wait for all CI jobs to pass on PR**

- [ ] **Step 3: After merge + deploy, verify on live system**

Open `http://10.77.9.231/` in Playwright, trigger an SOS alert, verify the toast appears and the app version is 1.146.0.

---

## Task Dependencies

```
Task 1 (version bump)          ─┐
                                 ├── sequential
Task 2 (vibration pattern fix)  ─┤
                                 ├── sequential
Task 3 (E2E test)               ─┤
                                 ├── sequential
Task 4 (changelog + push + CI)  ─┤
                                 ├── sequential
Task 5 (PR + verify)            ─┘
```

All tasks are sequential — each builds on the previous.
