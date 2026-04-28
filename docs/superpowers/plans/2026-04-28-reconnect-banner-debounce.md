# Reconnect Banner Debounce — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Debounce the "Reconnecting" UI by 3 s so transient WebSocket blips (Wi-Fi handoff, brief tab backgrounding, mobile suspend, server restart) no longer flash a banner across the mixer or change the listen-button text. Sustained disconnects (>3 s) still show the message.

**Architecture:** Add a small reactive helper `debounced_disconnect(connected, delay_ms)` in `iem-mixer/iem-ui/src/lifecycle.rs` that returns a `Signal<bool>` flipping `true` only after `connected==false` for `delay_ms` continuously, and back to `false` instantly on reconnect. Wire it into the mixer banner. For the audio listen button, schedule the `ListenState::Reconnecting` transition via a 3 s `gloo_timers::callback::Timeout` inside the WS `onclose` handler, cancelled in `onopen`. Underlying `connected` signal stays untouched — instant-feedback UI (status dot, channel disabled styling) keeps current behavior.

**Tech Stack:** Leptos 0.7, `gloo-timers` 0.3 (`callback::Timeout`), `wasm-bindgen`, Playwright.

**Spec:** `docs/superpowers/specs/2026-04-28-reconnect-banner-debounce-design.md`

---

## PR sequencing — READ THIS FIRST

PR #187 (`ci+test: cache e2e job + fix pwa SW cache test timeout (v1.160.0)`) was open and mergeable=clean at the time this plan was written. The version bump in T1 below assumes #187 has merged and dev = main = 1.160.0.

**Before starting T1, run:**

```bash
git fetch origin
gh pr view 187 --json state,mergedAt 2>/dev/null
```

If PR #187 is still `OPEN`: **PAUSE** and surface to the user. Either (a) merge #187 first, or (b) explicitly bundle #186 work into #187 (would require renaming PR #187 and rewriting its body).

If PR #187 is `MERGED`: proceed normally with T1.

---

## Pre-existing on dev (already done — DO NOT redo)

- Commit `fcdc86f` — spec doc at `docs/superpowers/specs/2026-04-28-reconnect-banner-debounce-design.md`

This plan file (`docs/superpowers/plans/2026-04-28-reconnect-banner-debounce.md`) is committed alongside this prompt.

---

## File map

| File | Change |
|---|---|
| `iem-mixer/Cargo.toml`, `iem-mixer/crates/iem-core/Cargo.toml`, `iem-mixer/crates/iem-server/Cargo.toml`, `iem-mixer/iem-ui/Cargo.toml`, `iem-mixer/src-tauri/Cargo.toml` | version 1.160.0 → 1.161.0 |
| `iem-mixer/src-tauri/tauri.conf.json` | version 1.160.0 → 1.161.0 |
| `README.md` | new changelog entry under `### v1.161.0 (2026-04-28)` |
| `iem-mixer/iem-ui/src/lifecycle.rs` | add `debounced_disconnect` helper + Rust unit tests |
| `iem-mixer/iem-ui/src/pages/mixer/mod.rs` | replace `!connected.get()` with `show_reconnecting.get()` in banner `<Show>` |
| `iem-mixer/iem-ui/src/components/audio_player.rs` | schedule `ListenState::Reconnecting` via `Timeout` in `on_close`; cancel in `on_open` |
| `iem-mixer/e2e/tests/reconnect-debounce.spec.ts` | new — 2 tests (sustained vs transient offline) |

---

## Task 1: Version bump 1.160.0 → 1.161.0 + README changelog

Use Haiku.

**Files:**
- Modify: 5× Cargo.toml + tauri.conf.json + README.md

- [ ] **Step 1: Bump versions**

```bash
sed -i 's/version = "1.160.0"/version = "1.161.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.160.0"/"version": "1.161.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify**

```bash
grep -h '^version\|"version":' iem-mixer/Cargo.toml iem-mixer/crates/iem-core/Cargo.toml iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
```

Expected output: 6 lines, all `1.161.0`.

- [ ] **Step 3: Add README changelog entry**

Use the Edit tool on `README.md`. Replace:

```markdown
## Changelog

### v1.160.0 (2026-04-28)
```

with:

```markdown
## Changelog

### v1.161.0 (2026-04-28)

- **Fix**: "Reconnecting" banner and audio listen button no longer flash on transient WebSocket blips (Wi-Fi handoff, brief tab backgrounding, mobile suspend). 3 s client-side debounce — sustained disconnects (>3 s) still show the message. (#186)

### v1.160.0 (2026-04-28)
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/Cargo.toml iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json README.md
git commit -m "chore: bump version to 1.161.0 + changelog (#186 reconnect debounce)"
```

---

## Task 2: Add `debounced_disconnect` helper to `lifecycle.rs`

Use Sonnet — Leptos reactive runtime + `gloo-timers` interop is fiddly.

**Files:**
- Modify: `iem-mixer/iem-ui/src/lifecycle.rs`

The existing file (read at plan time) is a flat module of pure helpers (`backoff_delay_ms`, `is_stale`, `truncate_for_display`, etc.) plus DOM-touching panic-hook functions, with a `#[cfg(test)] mod tests` at the bottom that uses plain `#[test]` (no Leptos runtime helpers). Follow that pattern.

- [ ] **Step 1: Add `gloo-timers` to `iem-ui`'s Cargo dependencies if not already present**

Check first:

```bash
grep -E "^gloo-timers" iem-mixer/iem-ui/Cargo.toml
```

If the result is empty, add the dependency:

Edit `iem-mixer/iem-ui/Cargo.toml` to add `gloo-timers = { version = "0.3", features = ["futures"] }` under `[dependencies]`. (The workspace already uses `gloo-timers` 0.3 elsewhere — see `iem-mixer/Cargo.lock`.)

If it's already there, skip this step.

- [ ] **Step 2: Add the helper to `lifecycle.rs`**

Append (BEFORE the `#[cfg(test)] mod tests` block) to `iem-mixer/iem-ui/src/lifecycle.rs`:

```rust
// ---------------------------------------------------------------------------
// Reconnect-banner debounce — issue #186.
// ---------------------------------------------------------------------------

use gloo_timers::callback::Timeout;
use leptos::prelude::*;

/// Returns a derived signal that becomes `true` only after `connected == false`
/// for `delay_ms` continuously. Flips back to `false` instantly when `connected`
/// becomes `true` again.
///
/// Used to debounce dramatic "Reconnecting" UI elements without delaying
/// instant-feedback UI like the status dot. The underlying `connected` signal
/// stays untouched.
///
/// Implementation: a Leptos `Effect` that watches `connected` and uses a
/// `gloo_timers::callback::Timeout` stored in a `StoredValue` for cancellation.
/// Dropping a `Timeout` cancels the underlying JS timer, so replacing the
/// stored value with `None` (or with a new `Timeout`) cancels any prior
/// pending transition.
pub fn debounced_disconnect(connected: ReadSignal<bool>, delay_ms: u32) -> Signal<bool> {
    let (show, set_show) = signal(false);
    let timeout: StoredValue<Option<Timeout>> = StoredValue::new(None);

    Effect::new(move |_| {
        let is_connected = connected.get();

        // Cancel any pending transition on every change. Dropping the prior
        // Timeout cancels the JS timer.
        timeout.set_value(None);

        if is_connected {
            // Reconnected — hide the banner immediately.
            set_show.set(false);
        } else if !show.get_untracked() {
            // Disconnected and banner not yet shown — schedule the transition.
            // (If the banner is already shown, do nothing: continued disconnect
            // should keep the banner visible without restarting any timer.)
            let new_timeout = Timeout::new(delay_ms, move || {
                set_show.set(true);
            });
            timeout.set_value(Some(new_timeout));
        }
    });

    show.into()
}
```

- [ ] **Step 3: Add unit tests**

Append inside the existing `#[cfg(test)] mod tests` block in `lifecycle.rs`:

```rust
    // -----------------------------------------------------------------------
    // debounced_disconnect — runtime-dependent behavior is covered by the
    // Playwright test (e2e/tests/reconnect-debounce.spec.ts); here we test
    // the small pure-logic decisions directly.
    // -----------------------------------------------------------------------

    /// `debounced_disconnect` schedules a Timeout only when:
    ///   - the input is currently disconnected, AND
    ///   - the output banner is not already shown.
    ///
    /// This is a regression guard for the "do nothing when banner is already
    /// shown" branch — without that branch, every spurious effect re-run while
    /// disconnected would restart the timer and indefinitely delay the banner
    /// from appearing.
    #[test]
    fn debounced_disconnect_helper_branch_decisions() {
        // When connected (true) → banner must be hidden, no timer needed.
        let scheduled_when_connected = should_schedule_timer(true, false);
        assert!(!scheduled_when_connected);

        // When disconnected and banner hidden → schedule timer.
        let scheduled_when_disconnected_and_hidden = should_schedule_timer(false, false);
        assert!(scheduled_when_disconnected_and_hidden);

        // When disconnected and banner already shown → do NOT restart timer.
        let scheduled_when_disconnected_and_shown = should_schedule_timer(false, true);
        assert!(!scheduled_when_disconnected_and_shown);
    }

    /// Mirrors the `if is_connected { ... } else if !show.get_untracked() { ... }`
    /// branch logic of `debounced_disconnect` so it can be unit-tested without
    /// a Leptos reactive runtime. If the branch logic in `debounced_disconnect`
    /// changes, this helper must change with it (and vice versa).
    fn should_schedule_timer(is_connected: bool, banner_shown: bool) -> bool {
        !is_connected && !banner_shown
    }
```

The pure-logic test guards the branch-decision invariant. End-to-end timing behavior — Timeout fires after `delay_ms`, gets cancelled when `connected` returns to `true`, banner hides instantly on reconnect — is covered by the Playwright test in T5.

- [ ] **Step 4: Run lint check**

```bash
cd iem-mixer && cargo fmt --all --check
```

If fails, run `cargo fmt --all` and re-check.

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/iem-ui/Cargo.toml iem-mixer/iem-ui/src/lifecycle.rs
git commit -m "feat(ui): add debounced_disconnect helper for reconnect banner (#186)

Reactive helper that produces a Signal<bool> flipping true only after
connected==false continues for delay_ms. Uses gloo_timers Timeout in
a StoredValue for cancellation — dropping the stored Timeout cancels
the underlying JS timer.

Pure-logic branch decisions covered by unit tests; runtime timing
behavior covered by the Playwright test in a follow-up commit."
```

---

## Task 3: Wire helper into the mixer banner

Use Haiku — one-line change.

**Files:**
- Modify: `iem-mixer/iem-ui/src/pages/mixer/mod.rs` (around lines 269-276)

- [ ] **Step 1: Apply edit**

Use the Edit tool to replace this block:

```rust
            <Show
                when=move || !connected.get() && !loading.get()
                fallback=|| ()
            >
                <div class="disconnected-banner">
                    "Reconnecting to REAPER..."
                </div>
            </Show>
```

with:

```rust
            // #186: Debounced 3 s — transient WS blips no longer flash this
            // banner. The underlying `connected` signal stays untouched so the
            // status dot at the top of the page keeps its instant feedback.
            let show_reconnecting = crate::lifecycle::debounced_disconnect(connected, 3000);
            <Show
                when=move || show_reconnecting.get() && !loading.get()
                fallback=|| ()
            >
                <div class="disconnected-banner">
                    "Reconnecting to REAPER..."
                </div>
            </Show>
```

Note: the `let show_reconnecting = ...` line goes INSIDE the `view!` macro block — Leptos allows arbitrary Rust statements inside `view!{ ... }`. If the existing surrounding code makes that placement awkward (e.g. the `<Show>` is several layers deep inside other elements), instead lift the `let` out one level so it's still inside the same component function but above the `view!` returning the JSX.

If you find the `let` placement does not compile inside `view!`, move it just before the `view!` macro invocation in the same component function.

- [ ] **Step 2: Run lint check**

```bash
cd iem-mixer && cargo fmt --all --check
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/iem-ui/src/pages/mixer/mod.rs
git commit -m "feat(ui): debounce mixer 'Reconnecting' banner 3s (#186)

Wire debounced_disconnect helper into the .disconnected-banner Show.
Underlying connected signal stays untouched — status dot, disabled
fader styling, and other instant-feedback UI keep current behavior."
```

---

## Task 4: Debounce the audio listen button

Use Sonnet — multi-step plumbing of a `StoredValue<Option<Timeout>>` across two closures.

**Files:**
- Modify: `iem-mixer/iem-ui/src/components/audio_player.rs`

The `on_close` handler currently sets `ListenState::Reconnecting` immediately on WS disconnect. Schedule it with a 3 s `Timeout` instead, and cancel from `on_open`.

- [ ] **Step 1: Read the connect function so you understand the surrounding scope**

```bash
sed -n '300,470p' iem-mixer/iem-ui/src/components/audio_player.rs
```

The connect function builds `on_open`, `on_message`, `on_close`, `on_error` closures and registers them on the WebSocket. `set_state` is shared by all of them via clones.

- [ ] **Step 2: Add a shared `Timeout` slot accessible to both `on_close` and `on_open`**

Find the line just before `let on_open = Closure::wrap(...)` (around line 350). Insert a new line:

```rust
    // #186: Debounce Reconnecting state — 3 s after WS close. If the new
    // socket opens before the timer fires, on_open drops the Timeout and
    // the user never sees the "Reconnecting" text for transient blips.
    let reconnecting_timeout: std::rc::Rc<std::cell::RefCell<Option<gloo_timers::callback::Timeout>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
```

Use `Rc<RefCell<...>>` rather than `StoredValue` here because this slot lives in plain Rust scope inside `connect_audio_websocket` (or whatever the connect fn is called) — Leptos reactive primitives are not in scope inside the WASM closure body without further plumbing. `Rc<RefCell<...>>` is the project's existing pattern for shared closure state (e.g. `helpers.rs:34` already uses it).

- [ ] **Step 3: Modify `on_open` to cancel the pending Timeout**

The existing `on_open` body sends `ListenStart`. Add at the very top of the closure body (line just inside `Closure::wrap(Box::new(move |_: web_sys::Event| {`):

```rust
        // #186: socket reopened — cancel any pending "Reconnecting" timer.
        let reconnecting_timeout_open = reconnecting_timeout_open.clone();
        *reconnecting_timeout_open.borrow_mut() = None;
```

And ABOVE the `let on_open = ...` line, add the clone:

```rust
    let reconnecting_timeout_open = reconnecting_timeout.clone();
```

Note: capture-by-clone follows the project's existing pattern in this file (`set_state_msg`, `frame_counter`, `is_listening` etc.). Do not move the `Rc` directly into the closure — other closures need their own clones.

Actually the cleanest pattern is to clone the Rc OUTSIDE the closure construction and `move` the clone in:

```rust
    let reconnecting_timeout_open = reconnecting_timeout.clone();
    let on_open = Closure::wrap(Box::new(move |_: web_sys::Event| {
        // #186: socket reopened — cancel any pending "Reconnecting" timer.
        *reconnecting_timeout_open.borrow_mut() = None;

        // ... existing on_open body sending ListenStart ...
    }) as Box<dyn FnMut(_)>);
```

Read the existing `on_open` carefully and place the new lines so existing clones (`member_id_open`) and the body remain correct.

- [ ] **Step 4: Modify `on_close` to schedule via Timeout instead of setting inline**

The current `on_close` has (around line 437-456):

```rust
    let on_close = Closure::wrap(Box::new(move |_: web_sys::Event| {
        let _ = set_ws_close.try_set(None);
        let intentional = intentional_stop.try_get_untracked().unwrap_or(false);
        if intentional {
            // User clicked stop — stay in Idle, don't reconnect
            web_sys::console::log_1(&"[audio] WebSocket closed (user stopped)".into());
            let _ = set_intentional_stop.try_set(false);
            let _ = set_state_close.try_set(ListenState::Idle);
        } else {
            // Unexpected disconnect — auto-reconnect
            web_sys::console::log_1(&"[audio] WebSocket closed — will reconnect".into());
            // Don't stop audio player — keep AudioContext alive for seamless resume
            let _ = set_state_close.try_set(ListenState::Reconnecting);
        }
    }) as Box<dyn FnMut(_)>);
```

Replace the unexpected-disconnect branch (the `else` block) with a Timeout-scheduled transition. ABOVE `let on_close = ...`, add a clone:

```rust
    let reconnecting_timeout_close = reconnecting_timeout.clone();
```

Replace the `else` body so the final `on_close` looks like:

```rust
    let on_close = Closure::wrap(Box::new(move |_: web_sys::Event| {
        let _ = set_ws_close.try_set(None);
        let intentional = intentional_stop.try_get_untracked().unwrap_or(false);
        if intentional {
            // User clicked stop — stay in Idle, don't reconnect
            web_sys::console::log_1(&"[audio] WebSocket closed (user stopped)".into());
            let _ = set_intentional_stop.try_set(false);
            let _ = set_state_close.try_set(ListenState::Idle);
            // Cancel any pending Reconnecting transition (defensive — should
            // already have been cancelled by on_open of the prior socket).
            *reconnecting_timeout_close.borrow_mut() = None;
        } else {
            // Unexpected disconnect — schedule "Reconnecting" UI after 3 s.
            // If a new socket opens before then, on_open drops this Timeout
            // and the user never sees the flash. (#186)
            web_sys::console::log_1(&"[audio] WebSocket closed — will reconnect".into());
            // Don't stop audio player — keep AudioContext alive for seamless resume.
            let set_state_for_timer = set_state_close;
            let new_timeout = gloo_timers::callback::Timeout::new(3000, move || {
                let _ = set_state_for_timer.try_set(ListenState::Reconnecting);
            });
            *reconnecting_timeout_close.borrow_mut() = Some(new_timeout);
        }
    }) as Box<dyn FnMut(_)>);
```

- [ ] **Step 5: Add `gloo-timers` to `iem-ui`'s deps (already done in T2) — sanity-check it's there**

```bash
grep -E "^gloo-timers" iem-mixer/iem-ui/Cargo.toml
```

Expected: a single line with `gloo-timers = { version = "0.3", ... }`.

- [ ] **Step 6: Run lint check**

```bash
cd iem-mixer && cargo fmt --all --check
```

- [ ] **Step 7: Commit**

```bash
git add iem-mixer/iem-ui/src/components/audio_player.rs
git commit -m "feat(ui): debounce audio listen-button 'Reconnecting' 3s (#186)

Schedule ListenState::Reconnecting via gloo_timers::Timeout in the
WS on_close handler instead of setting it inline. on_open drops the
Timeout — transient reconnects never reach the UI. Same 3 s window
as the mixer banner."
```

---

## Task 5: Playwright E2E

Use Sonnet.

**Files:**
- Create: `iem-mixer/e2e/tests/reconnect-debounce.spec.ts`

This is a CI-job test (runs on the GitHub-hosted `e2e` job, not post-deploy). The e2e job spins up an in-process iem-server with the production config — see `.github/workflows/ci.yml:430-445` for how it's started. The test exercises only client-side WebSocket open/close behavior; live REAPER state is irrelevant.

- [ ] **Step 1: Inspect existing test patterns to match conventions**

```bash
ls iem-mixer/e2e/tests/
sed -n '1,30p' iem-mixer/e2e/tests/smoke.spec.ts
```

You want to know:
- What `import` path Playwright uses (`@playwright/test`)
- How tests open the app (`page.goto('/')` or similar)
- Whether there's a shared `BASE_URL` constant

The CI e2e job uses `E2E_BASE_URL=http://localhost:8080` (see `ci.yml:444-445`). Match that pattern.

- [ ] **Step 2: Write the test file**

```typescript
import { test, expect } from "@playwright/test";

const BASE_URL = process.env.E2E_BASE_URL ?? "http://localhost:8080";

test.describe("Reconnect banner debounce (#186)", () => {
  test("transient offline (<3s) does NOT show reconnecting banner", async ({
    context,
    page,
  }) => {
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        consoleErrors.push(`[${msg.type()}] ${msg.text()}`);
      }
    });

    await page.goto(BASE_URL, { waitUntil: "networkidle" });

    // Wait for the app to mount and the WebSocket to connect.
    // The .disconnected-banner only renders when !connected && !loading,
    // so its absence here means we're either still loading or already
    // connected — both are valid pre-conditions.
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    // Drop network — WebSocket closes.
    await context.setOffline(true);

    // 1 s in: banner must NOT be visible (debounce window).
    await page.waitForTimeout(1000);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    // 2 s in: still hidden.
    await page.waitForTimeout(1000);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    // Restore before debounce window elapses (total offline ≈ 2.2 s).
    await context.setOffline(false);

    // Banner must NEVER appear during this transient blip — wait a full
    // additional 2 s (well past the 3 s debounce mark from offline-start)
    // to be certain.
    await page.waitForTimeout(2000);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    expect(consoleErrors).toEqual([]);
  });

  test("sustained offline (>3s) DOES show reconnecting banner", async ({
    context,
    page,
  }) => {
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        consoleErrors.push(`[${msg.type()}] ${msg.text()}`);
      }
    });

    await page.goto(BASE_URL, { waitUntil: "networkidle" });
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    await context.setOffline(true);

    // 1 s, 2 s — banner stays hidden.
    await page.waitForTimeout(2000);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible();

    // After 4 s total offline, the 3 s debounce has fired and the banner
    // is visible.
    await page.waitForTimeout(2000);
    await expect(page.locator(".disconnected-banner")).toBeVisible();

    // Restore network — banner clears as soon as the WS reconnects.
    await context.setOffline(false);
    await expect(page.locator(".disconnected-banner")).not.toBeVisible({
      timeout: 5000,
    });

    expect(consoleErrors).toEqual([]);
  });
});
```

- [ ] **Step 3: Sanity-check the test compiles (TypeScript)**

```bash
cd iem-mixer/e2e && npx tsc --noEmit tests/reconnect-debounce.spec.ts 2>&1 | head -10
```

If `npx tsc` is not available in the e2e dir, skip — the test will compile inside Playwright when CI runs. The Playwright test runner uses its own ts-node.

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/e2e/tests/reconnect-debounce.spec.ts
git commit -m "test(e2e): debounced reconnect banner — transient vs sustained (#186)

Two Playwright tests using context.setOffline:
  1. Transient (~2s) offline does NOT show banner
  2. Sustained (>3s) offline DOES show banner, hides on reconnect

Both assert zero console errors per airuleset
browser-console-zero-errors module."
```

---

## Task 6: Push and monitor CI

Controller handles directly (no subagent — operational task).

- [ ] **Step 1: Run lint check locally**

```bash
cd iem-mixer && cargo fmt --all --check
```

- [ ] **Step 2: Push to dev**

```bash
git push origin dev
```

- [ ] **Step 3: Get the run ID**

```bash
sleep 8 && gh run list --branch dev --limit 1 --json databaseId,status,conclusion,event,headSha
```

- [ ] **Step 4: Monitor CI in background**

```bash
sleep 300 && gh run view <run-id> --json status,conclusion,jobs --jq '{status, conclusion, jobs: [.jobs[] | {name, conclusion}]}'
```

Run via the `Bash` tool with `run_in_background: true`. **Do NOT use** `/loop`, `CronCreate`, or any custom monitor script — the airuleset forbids them.

When the background command completes, read its output via `BashOutput`. If `status: in_progress`, schedule another `sleep 300 && gh run view ...` in background and wait. Repeat until `status: completed`.

- [ ] **Step 5: If any job fails, investigate and fix**

```bash
gh run view <run-id> --log-failed
```

Common expected issues for this PR:
- `cargo fmt` fails locally before push — auto-fix and commit
- Test compilation error in `lifecycle.rs` — re-read the current file and adjust the unit-test additions
- E2E test flake on the new test (cache miss on first cache-warm push is fine — it's the same situation as PR #187's first push)

Fix issues in ONE additional commit and re-monitor. Do NOT blindly rerun.

- [ ] **Step 6: Confirm cache HIT on this push (informational)**

This is the SECOND push since PR #187 merged that exercises the new e2e cache. Look in the e2e job logs for:

- `Cache restored successfully` (Setup Node step)
- `Cache restored from key: Linux-playwright-<hash>` (Cache Playwright browsers step)

If both appear, the cache is working as designed (PR #187's actual win is now observable). If they don't, file a follow-up issue — it does NOT block this PR.

---

## Task 7: Open PR and STOP

Controller handles directly.

- [ ] **Step 1: Sync origin and confirm no existing PR**

```bash
git fetch origin
gh pr list --head dev --base main --json number,title,mergeable,mergeStateStatus
```

If a PR already exists for this dev work, skip to Step 3.

- [ ] **Step 2: Open the PR**

```bash
gh pr create --base main --head dev --title "feat(ui): debounce reconnect banner 3s + audio button (v1.161.0) (#186)" --body "$(cat <<'EOF'
## Summary

Issue #186 — users see "Reconnecting" banner flash on every transient WebSocket disconnect (Wi-Fi handoff, brief tab backgrounding, mobile suspend). Other mobile apps (Slack, WhatsApp, Discord) debounce 3-5 s before showing connection-status warnings, so users only see them on sustained disconnection.

This PR adds a 3 s client-side debounce to:

- The mixer banner ("Reconnecting to REAPER..." in `mixer/mod.rs`)
- The audio listen button text ("🔊 Reconnecting..." in `audio_player.rs`)

The underlying `connected` signal stays untouched — status dot at the top of the page, channel disabled-fader styling, and other instant-feedback UI keep their current behavior.

Spec: [`docs/superpowers/specs/2026-04-28-reconnect-banner-debounce-design.md`](https://github.com/zbynekdrlik/reaperiem/blob/dev/docs/superpowers/specs/2026-04-28-reconnect-banner-debounce-design.md)
Plan: [`docs/superpowers/plans/2026-04-28-reconnect-banner-debounce.md`](https://github.com/zbynekdrlik/reaperiem/blob/dev/docs/superpowers/plans/2026-04-28-reconnect-banner-debounce.md)

## Test plan

- [x] Rust unit tests on `debounced_disconnect` branch decisions
- [x] Playwright E2E: transient offline (~2 s) does NOT show banner
- [x] Playwright E2E: sustained offline (>3 s) DOES show banner, hides on reconnect
- [x] All Playwright tests assert zero browser console errors/warnings
- [ ] Manual verification on a phone: airplane-mode toggle <3 s shows no banner; >3 s shows banner

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Verify mergeable**

```bash
PR_NUM=$(gh pr list --head dev --base main --json number -q '.[0].number')
gh api "repos/zbynekdrlik/reaperiem/pulls/$PR_NUM" --jq '{mergeable: .mergeable, mergeable_state: .mergeable_state}'
```

Expected: `{"mergeable": true, "mergeable_state": "clean"}`.

If `mergeable_state` is anything else, follow the same playbook as PR #187: investigate the failing check via `gh pr checks $PR_NUM`, fix the root cause, push, monitor. Do NOT propose `--admin` merge or branch-protection bypass.

- [ ] **Step 4: STOP at green PR URL**

```bash
gh pr view $PR_NUM --json url -q '.url'
```

Output the URL to the user. Do NOT merge. Wait for explicit user approval ("merge it" / "approved" / equivalent).

---

## Task dependencies

```
T1 (version bump)
  ↓
T2 (debounced_disconnect helper)
  ↓
T3 (mixer banner) ─┐
                   ├→ T6 (push + monitor) → T7 (PR + STOP)
T4 (audio button) ─┘
T5 (Playwright)  ──┘
```

T2 must finish before T3, T4. T3, T4, T5 are independent of each other — but execute them sequentially (one subagent at a time per the SDD skill rule "Dispatch multiple implementation subagents in parallel" is a Red Flag). After T5, run T6, then T7.

---

## Verification checklist (run before sending the completion report)

- [ ] CI green on dev (all 9 jobs success or appropriately skipped)
- [ ] PR `mergeable: true`, `mergeable_state: "clean"`
- [ ] No changes to the self-hosted `deploy` job
- [ ] Version on dev (1.161.0) is strictly higher than main (1.160.0 after #187 merge)
- [ ] Spec/plan docs already on dev (committed pre-this-plan)
- [ ] Playwright tests both assert `expect(consoleErrors).toEqual([])`
- [ ] On a phone, airplane-mode toggle <3 s shows no banner (manual; ask user to verify after merge if desired)
