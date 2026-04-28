# Reconnect Banner Debounce — Design

**Date:** 2026-04-28
**Issue:** [#186](https://github.com/zbynekdrlik/reaperiem/issues/186) — "users report very frequent connecting message, abnormal over other mobile apps. In critical live event situations it is extremely unpleasant"
**Status:** Approved
**Scope:** Single PR `dev` → `main`, UI-only client-side change. No backend changes, no protocol changes.

## Problem

The mixer UI shows a "Reconnecting to REAPER..." banner the instant the WebSocket disconnects, with no debounce:

- `iem-mixer/iem-ui/src/pages/mixer/mod.rs:269-276` — `<Show when=move || !connected.get() && !loading.get()>` shows the banner whenever `connected==false`
- `iem-mixer/iem-ui/src/components/audio_player.rs:38,449` — the listen toolbar button flips to `ListenState::Reconnecting` (text "🔊 Reconnecting...") instantly on `onclose`
- `iem-mixer/iem-ui/src/pages/mixer/connection.rs:619` — `onclose` handler sets `connected = false` immediately, no delay

Mobile networks produce many sub-second disconnects (Wi-Fi handoff, 4G→5G transition, brief tab backgrounding, OS suspend/resume, server restart, etc.). The current UI flashes "Reconnecting" on every blip, which:

- Looks broken to users used to other mobile apps that hide brief blips
- Is "extremely unpleasant" during live events when users glance at the mixer to make a quick adjustment

Other mobile apps (Slack ≈ 5s, WhatsApp ≈ 3s, Discord ≈ 3s) only show connection-status warnings after sustained disconnection.

## Non-goals

- **WebSocket disconnect frequency** is not addressed here. If the user observes that reconnects fire too often even on stable networks, a follow-up issue can investigate server-side keep-alive, client visibility detection, mobile suspend handling, etc. The debounce is a UX fix, not a reliability fix.
- **Adaptive debounce thresholds** (e.g., shorter on first disconnect, longer on repeated). Fixed 3s is enough.
- **Minimum-show duration** once banner appears. The banner hides as soon as `connected` returns to `true`, even if it was visible for only 200 ms.
- **Status dot at `mixer/mod.rs:251`** keeps its instant-feedback behavior — like a cellular signal-strength bar, instant flicker there is desirable.
- **Channel-level "disconnected" CSS class** (`components.rs:182,415,1091`) likewise stays instant.
- **Switch from `npm install` to `npm ci`** etc. — not in scope.

## Architecture

A small reactive helper in `iem-mixer/iem-ui/src/lifecycle.rs`:

```rust
/// Returns a derived signal that becomes `true` only after `connected == false`
/// for `delay_ms` continuously. Flips back to `false` instantly when `connected`
/// becomes `true` again. Used to debounce dramatic "Reconnecting" UI elements
/// without delaying other instant-feedback UI (status dot, disabled-fader styling).
pub fn debounced_disconnect(connected: ReadSignal<bool>, delay_ms: u32) -> Signal<bool>;
```

Implementation sketch (~20 lines):

- `let (show, set_show) = signal(false);`
- `let timeout: StoredValue<Option<gloo_timers::callback::Timeout>> = StoredValue::new(None);`
- `Effect::new(move |_| { ... })` reacts to `connected.get()`:
  - On every change, drop the previous timeout (cancels it)
  - If `is_connected == true`, `set_show(false)`
  - Else, if `show` is currently `false`, schedule a `Timeout` of `delay_ms` that sets `show=true`; store the handle in `timeout`. (If `show` is already true, leave as-is — continued disconnect should keep the banner visible.)
- Return `show.into()`.

Cancellation works because dropping a `gloo_timers::callback::Timeout` cancels the underlying JS timer.

The underlying `connected` signal is **untouched** — only the dramatic banner/button consume the debounced version.

## Component changes

### A. Mixer banner — `iem-mixer/iem-ui/src/pages/mixer/mod.rs`

```rust
let show_reconnecting = debounced_disconnect(connected, 3000);
<Show
    when=move || show_reconnecting.get() && !loading.get()
    fallback=|| ()
>
    <div class="disconnected-banner">"Reconnecting to REAPER..."</div>
</Show>
```

### B. Audio listen button — `iem-mixer/iem-ui/src/components/audio_player.rs`

Apply the **minimal** variant: schedule the `ListenState::Reconnecting` transition with a 3-second `gloo_timers::callback::Timeout` inside the `onclose` handler instead of setting it inline. Cancel the timeout in `onopen` (drop the stored handle). Net effect identical to the helper above but inlined into the audio_player state machine, since the audio state is an explicit enum rather than a boolean.

The text "🔊 Reconnecting..." and the `toolbar-btn-listen reconnecting` CSS class flip 3 seconds after disconnect, not instantly.

### C. New helper — `iem-mixer/iem-ui/src/lifecycle.rs`

Add `debounced_disconnect` plus a Rust unit test exercising the four behaviors:
1. Stays `false` while `connected==true`
2. Flips to `true` after `delay_ms` of `connected==false`
3. Stays `false` if `connected` returns to `true` before `delay_ms` elapses (timer cancelled)
4. Flips back to `false` immediately when `connected` returns to `true` after the banner is showing

## Testing

**Unit test (Rust)** — in `lifecycle.rs` test module under `#[cfg(test)]`:
- Uses Leptos test runtime helpers + `gloo-timers` test mode
- Covers all four behaviors above

**Playwright E2E** — new file `iem-mixer/e2e/tests/reconnect-debounce.spec.ts`:

```typescript
test('reconnect banner is debounced (does not flash on transient disconnect)', async ({ context, page }) => {
  const consoleErrors: string[] = [];
  page.on('console', (msg) => {
    if (msg.type() === 'error' || msg.type() === 'warning') {
      consoleErrors.push(`[${msg.type()}] ${msg.text()}`);
    }
  });

  await loginAsKnownMember(page);  // mirrors existing live-test login pattern
  await expect(page.locator('.disconnected-banner')).not.toBeVisible();

  // Drop network — WebSocket closes
  await context.setOffline(true);

  // 1s in — banner must NOT be visible (debounce window)
  await page.waitForTimeout(1000);
  await expect(page.locator('.disconnected-banner')).not.toBeVisible();

  // 2s in — still hidden
  await page.waitForTimeout(1000);
  await expect(page.locator('.disconnected-banner')).not.toBeVisible();

  // 4s total — debounce window elapsed, banner appears
  await page.waitForTimeout(2000);
  await expect(page.locator('.disconnected-banner')).toBeVisible();

  // Restore network — banner clears immediately
  await context.setOffline(false);
  await expect(page.locator('.disconnected-banner')).not.toBeVisible({ timeout: 5000 });

  expect(consoleErrors).toEqual([]);
});
```

Runs on the GitHub-hosted `e2e` job (in-process iem-server, no live REAPER needed). Live REAPER state is irrelevant — only WebSocket open/close behavior is exercised.

A second test asserts a **transient** disconnect (offline → online within 2s) never shows the banner at all.

## Risk and rollback

- **Low risk.** The helper is additive. A broken implementation either flashes the banner immediately (current behavior, no regression) or never shows it (clear regression caught by Playwright test).
- The underlying `connected` signal is unchanged — status dot and other instant-feedback UI keep behaving as today.
- No backend, protocol, or message-schema changes.
- **Rollback:** single revert undoes everything (helper file, three call sites, two tests).

## Files changed

- `iem-mixer/iem-ui/src/lifecycle.rs` — new `debounced_disconnect` helper + unit tests
- `iem-mixer/iem-ui/src/pages/mixer/mod.rs` — use helper for banner
- `iem-mixer/iem-ui/src/components/audio_player.rs` — schedule `ListenState::Reconnecting` with 3 s `Timeout` instead of setting inline
- `iem-mixer/e2e/tests/reconnect-debounce.spec.ts` — new (2 tests, asserts zero console errors per airuleset)
- 5× `Cargo.toml` + `tauri.conf.json`: 1.160.0 → 1.161.0 (airuleset version-bumping hygiene; fires on any new dev PR after the previous merge to main)
- `README.md` — changelog entry under `### v1.161.0`

## Verification

After CI green and post-deploy:

1. Open https://iem.newlevel.media/ on a phone in a known-flaky Wi-Fi area
2. Toggle airplane mode briefly (< 3 s) — banner should NOT appear
3. Toggle airplane mode for > 3 s — banner appears at ~3 s mark
4. Restore network — banner disappears within 2 s
5. Same checks on the audio listen button text
