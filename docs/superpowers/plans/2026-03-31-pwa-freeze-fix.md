# PWA App Freeze Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix PWA app becoming unresponsive on phones (requires force-stop to recover) by eliminating memory leaks, fixing resource cleanup, and adding throttling for reactive signal updates.

**Architecture:** Five targeted fixes: (1) replace `Closure::once().forget()` anti-patterns with `Closure::once_into_js()`, (2) fix AudioContext `onstatechange` leak, (3) store Effect closures in `Rc<RefCell<>>` for proper cleanup instead of `.forget()`, (4) throttle ChannelUpdate signals same as Meters, (5) add page visibility handling to pause updates when backgrounded.

**Tech Stack:** Leptos 0.8, wasm-bindgen, web-sys, Web Audio API

---

## File Map

| File | Change | Responsibility |
|------|--------|---------------|
| `iem-mixer/iem-ui/src/api.rs` | Modify:68-80 | Fix `Closure::once().forget()` → `Closure::once_into_js()` |
| `iem-mixer/iem-ui/src/pages/mixer.rs` | Modify:1118-1126, 197-227 | Fix `Closure::once().forget()`, add ChannelUpdate throttle |
| `iem-mixer/iem-ui/audio_player.js` | Modify:84-95, 291-317 | Fix `onstatechange` cleanup |
| `iem-mixer/iem-ui/src/components/audio_player.rs` | Modify:103-128, 151-185 | Store closures in Rc<RefCell<>> |
| `iem-mixer/iem-ui/src/components/talk_button.rs` | Modify:56-109 | Store vibration closure in Rc<RefCell<>> |
| `iem-mixer/iem-ui/src/components/alert_toast.rs` | Modify:25-64 | Store closures in Rc<RefCell<>> |
| `iem-mixer/crates/iem-core/Cargo.toml` | Modify | Version bump 1.123.0 → 1.124.0 |

---

### Task 1: Version Bump

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`

- [ ] **Step 1: Bump version from 1.123.0 to 1.124.0**

```bash
sed -i 's/version = "1.123.0"/version = "1.124.0"/' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.123.0"/"version": "1.124.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.124.0"
```

---

### Task 2: Fix `Closure::once().forget()` Anti-Patterns (2 instances)

These leak the Closure wrapper because `.forget()` prevents deallocation. `Closure::once_into_js()` auto-deallocates after the callback fires.

**Files:**
- Modify: `iem-mixer/iem-ui/src/api.rs:68-80`
- Modify: `iem-mixer/iem-ui/src/pages/mixer.rs:1118-1126`

- [ ] **Step 1: Fix api.rs timeout closure**

In `iem-mixer/iem-ui/src/api.rs`, replace lines 68-80:

**Before:**
```rust
    let controller_clone = controller.clone();
    let timeout_closure = Closure::once(Box::new(move || {
        controller_clone.abort();
    }) as Box<dyn FnOnce()>);

    window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            timeout_closure.as_ref().unchecked_ref(),
            NETWORK_TIMEOUT_MS,
        )
        .map_err(|_| "Failed to set timeout")?;

    // Keep closure alive until timeout fires or fetch completes
    timeout_closure.forget();
```

**After:**
```rust
    let controller_clone = controller.clone();
    let timeout_closure = Closure::once_into_js(move || {
        controller_clone.abort();
    });

    window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            timeout_closure.as_ref().unchecked_ref(),
            NETWORK_TIMEOUT_MS,
        )
        .map_err(|_| "Failed to set timeout")?;
```

- [ ] **Step 2: Fix mixer.rs EQ modal close closure**

In `iem-mixer/iem-ui/src/pages/mixer.rs`, replace lines 1118-1126:

**Before:**
```rust
                            on_close=Callback::new(move |_: ()| {
                                let cb = Closure::once(move || {
                                    set_eq_open.set(None);
                                    set_eq_bands.set(Vec::new());
                                });
                                web_sys::window()
                                    .unwrap()
                                    .set_timeout_with_callback(cb.as_ref().unchecked_ref())
                                    .unwrap();
                                cb.forget(); // prevent drop before timer fires
                            })
```

**After:**
```rust
                            on_close=Callback::new(move |_: ()| {
                                // Defer to next macrotask via setTimeout(0) — setting eq_open=None
                                // destroys the EQ modal DOM. This must happen AFTER the current
                                // event handler stack fully unwinds (including microtasks), or
                                // Leptos reactive graph teardown hits dropped closures.
                                let cb = Closure::once_into_js(move || {
                                    set_eq_open.set(None);
                                    set_eq_bands.set(Vec::new());
                                });
                                web_sys::window()
                                    .unwrap()
                                    .set_timeout_with_callback(cb.as_ref().unchecked_ref())
                                    .unwrap();
                            })
```

- [ ] **Step 3: Verify no other `Closure::once` + `.forget()` patterns exist**

```bash
cd iem-mixer && grep -rn "Closure::once" iem-ui/src/ | grep -v "once_into_js"
```

Expected: zero results (all should now use `once_into_js`).

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/iem-ui/src/api.rs iem-mixer/iem-ui/src/pages/mixer.rs
git commit -m "fix: replace Closure::once().forget() with Closure::once_into_js()"
```

---

### Task 3: Fix AudioContext `onstatechange` Leak

When `stopAudioPlayer()` closes the AudioContext, the `onstatechange` handler retains a reference to `pendingFrames`. After multiple listen/stop cycles, stale contexts with dangling handlers accumulate (~1-2MB each).

**Files:**
- Modify: `iem-mixer/iem-ui/audio_player.js:291-317`

- [ ] **Step 1: Add onstatechange cleanup in stopAudioPlayer()**

In `iem-mixer/iem-ui/audio_player.js`, in the `stopAudioPlayer()` function, add `audioContext.onstatechange = null;` **before** `audioContext.close()`:

**Before (lines 300-304):**
```javascript
  gainNode = null;
  if (audioContext) {
    audioContext.close();
    audioContext = null;
  }
```

**After:**
```javascript
  gainNode = null;
  if (audioContext) {
    audioContext.onstatechange = null;
    audioContext.close();
    audioContext = null;
  }
```

- [ ] **Step 2: Also clear pendingFrames before closing**

This prevents the `onstatechange` handler (if it fires during close) from processing stale frames:

**Before (lines 308-309):**
```javascript
  pendingFrames = [];
  bufferMs = MIN_BUFFER_MS;
```

Move `pendingFrames = [];` to BEFORE the audioContext close block. The final stopAudioPlayer should look like:

```javascript
export function stopAudioPlayer() {
  if (decoder) {
    try {
      decoder.close();
    } catch (_) {
      // ignore
    }
    decoder = null;
  }
  pendingFrames = [];
  gainNode = null;
  if (audioContext) {
    audioContext.onstatechange = null;
    audioContext.close();
    audioContext = null;
  }
  nextStartTime = 0;
  lastAudioLevel = -150;
  lastError = null;
  bufferMs = MIN_BUFFER_MS;
  lastDropoutTime = 0;
  lastFrameArrivalTime = 0;
  dropoutCount = 0;
  playbackDropouts = 0;
  scheduledFrameCount = 0;
  totalFrames = 0;
  lastSourceEndTime = 0;
  console.log("[audio] Player stopped");
}
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/iem-ui/audio_player.js
git commit -m "fix: clear AudioContext onstatechange handler before close to prevent memory leak"
```

---

### Task 4: Store Effect Closures in Rc<RefCell<>> Instead of `.forget()`

Inside `Effect::new` blocks, `Closure::wrap().forget()` leaks the Rust closure wrapper on every Effect re-run. While the interval IDs are properly tracked and cleared, the Closure objects accumulate. Fix by storing them in `Rc<RefCell<Option<Closure>>>` and dropping old ones when the Effect re-runs.

**Files:**
- Modify: `iem-mixer/iem-ui/src/components/audio_player.rs:103-128, 151-185`
- Modify: `iem-mixer/iem-ui/src/components/talk_button.rs:56-109`
- Modify: `iem-mixer/iem-ui/src/components/alert_toast.rs:25-64`

#### 4a: Fix audio_player.rs stats polling closure

- [ ] **Step 1: Add stored closure for stats polling**

In `iem-mixer/iem-ui/src/components/audio_player.rs`, before the `Effect::new` at line 103, add a stored closure ref:

```rust
    let stats_closure_ref: std::rc::Rc<std::cell::RefCell<Option<Closure<dyn FnMut()>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let stats_closure_effect = stats_closure_ref.clone();
```

- [ ] **Step 2: Replace the `.forget()` pattern in the stats Effect**

Replace lines 103-128 with:

```rust
    Effect::new(move || {
        let current_state = state.get();

        // Clear previous interval if any
        if let Some(id) = stats_interval.get_untracked() {
            if let Some(w) = web_sys::window() {
                w.clear_interval_with_handle(id);
            }
            set_stats_interval.set(None);
        }
        // Drop old closure (prevents leak)
        stats_closure_effect.borrow_mut().take();

        if current_state == ListenState::Listening {
            let closure = Closure::wrap(Box::new(move || {
                set_stream_stats.set(poll_stream_stats());
            }) as Box<dyn FnMut()>);
            let id = web_sys::window()
                .unwrap()
                .set_interval_with_callback_and_timeout_and_arguments_0(
                    closure.as_ref().unchecked_ref(),
                    500,
                )
                .unwrap();
            // Store closure to keep it alive (and allow cleanup on re-run)
            *stats_closure_effect.borrow_mut() = Some(closure);
            set_stats_interval.set(Some(id));
        }
    });
```

#### 4b: Fix audio_player.rs reconnect backoff closure

- [ ] **Step 3: Add stored closure for reconnect backoff**

Before the reconnect Effect at line 131, add:

```rust
    let reconnect_closure_ref: std::rc::Rc<std::cell::RefCell<Option<Closure<dyn FnMut()>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let reconnect_closure_effect = reconnect_closure_ref.clone();
```

- [ ] **Step 4: Replace the `.forget()` pattern in the reconnect Effect**

Replace lines 132-185, specifically the closure creation part:

**Before (lines 154-184):**
```rust
        let closure = Closure::wrap(Box::new(move || {
            // ... reconnect logic
        }) as Box<dyn FnMut()>);
        let id = web_sys::window()
            .unwrap()
            .set_interval_with_callback_and_timeout_and_arguments_0(
                closure.as_ref().unchecked_ref(),
                2000,
            )
            .unwrap();
        closure.forget();
        set_reconnect_interval.set(Some(id));
```

**After:**
```rust
        let closure = Closure::wrap(Box::new(move || {
            let current_backoff = backoff.get();
            web_sys::console::log_1(
                &format!("[audio] Reconnect attempt (backoff {}ms)", current_backoff).into(),
            );
            start_listening(
                set_state,
                set_ws,
                set_listen_target,
                member_id_inner.clone(),
                intentional_stop,
                set_intentional_stop,
            );
            let next = (current_backoff * 2).min(8000);
            backoff.set(next);
        }) as Box<dyn FnMut()>);
        let id = web_sys::window()
            .unwrap()
            .set_interval_with_callback_and_timeout_and_arguments_0(
                closure.as_ref().unchecked_ref(),
                2000,
            )
            .unwrap();
        // Store closure to keep it alive (and allow cleanup on re-run)
        *reconnect_closure_effect.borrow_mut() = Some(closure);
        set_reconnect_interval.set(Some(id));
```

- [ ] **Step 5: Update on_cleanup to drop stored closures**

In the `on_cleanup` at lines 188-204, add closure drops:

```rust
    on_cleanup(move || {
        if let Some(id) = stats_interval.get_untracked() {
            if let Some(w) = web_sys::window() {
                w.clear_interval_with_handle(id);
            }
        }
        if let Some(id) = reconnect_interval.get_untracked() {
            if let Some(w) = web_sys::window() {
                w.clear_interval_with_handle(id);
            }
        }
        // Drop stored closures
        stats_closure_ref.borrow_mut().take();
        reconnect_closure_ref.borrow_mut().take();
        set_intentional_stop.set(true);
        if let Some(ws) = ws.get_untracked() {
            let _ = ws.close();
        }
        stop_audio_player();
    });
```

#### 4c: Fix talk_button.rs vibration closure

- [ ] **Step 6: Store vibration closure in talk_button.rs**

In `iem-mixer/iem-ui/src/components/talk_button.rs`, before the vibration Effect at line 56, add:

```rust
    let vib_closure_ref: std::rc::Rc<std::cell::RefCell<Option<Closure<dyn FnMut()>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let vib_closure_effect = vib_closure_ref.clone();
```

- [ ] **Step 7: Replace `.forget()` in the vibration Effect**

In the Effect at lines 56-109, replace the vibration section:

**Before (lines 61-78):**
```rust
                let vib_cb = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                    if let Some(w) = web_sys::window() {
                        let _ = w.navigator().vibrate_with_duration(200);
                    }
                })
                    as Box<dyn FnMut()>);
                let id = window
                    .set_interval_with_callback_and_timeout_and_arguments_0(
                        vib_cb.as_ref().unchecked_ref(),
                        1000,
                    )
                    .unwrap_or(0);
                let _ = js_sys::Reflect::set(
                    &window,
                    &wasm_bindgen::JsValue::from_str("__iem_talk_vib"),
                    &wasm_bindgen::JsValue::from(id),
                );
                vib_cb.forget();
```

**After:**
```rust
                let vib_cb = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                    if let Some(w) = web_sys::window() {
                        let _ = w.navigator().vibrate_with_duration(200);
                    }
                })
                    as Box<dyn FnMut()>);
                let id = window
                    .set_interval_with_callback_and_timeout_and_arguments_0(
                        vib_cb.as_ref().unchecked_ref(),
                        1000,
                    )
                    .unwrap_or(0);
                let _ = js_sys::Reflect::set(
                    &window,
                    &wasm_bindgen::JsValue::from_str("__iem_talk_vib"),
                    &wasm_bindgen::JsValue::from(id),
                );
                // Store closure (dropped when state changes or component unmounts)
                *vib_closure_effect.borrow_mut() = Some(vib_cb);
```

And in the `else` branch (lines 87-100), add drop of old closure:

```rust
            } else {
                // Drop old closure
                vib_closure_effect.borrow_mut().take();
                // Stop vibration loop
                if let Ok(val) = js_sys::Reflect::get(
```

- [ ] **Step 8: Add closure drop in on_cleanup**

Update the on_cleanup at line 231:

```rust
    on_cleanup(move || {
        vib_closure_ref.borrow_mut().take();
        stop_talkback();
    });
```

#### 4d: Fix alert_toast.rs vibration + sound closures

- [ ] **Step 9: Store both closures in alert_toast.rs**

In `iem-mixer/iem-ui/src/components/alert_toast.rs`, before the Effect, add:

```rust
    let alert_vib_ref: std::rc::Rc<std::cell::RefCell<Option<Closure<dyn FnMut()>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let alert_snd_ref: std::rc::Rc<std::cell::RefCell<Option<Closure<dyn FnMut()>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let vib_effect = alert_vib_ref.clone();
    let snd_effect = alert_snd_ref.clone();
```

- [ ] **Step 10: Replace both `.forget()` calls**

For the vibration closure (line 44), replace `vib_cb.forget();` with:
```rust
                *vib_effect.borrow_mut() = Some(vib_cb);
```

For the sound closure (line 64), replace `sound_cb.forget();` with:
```rust
                *snd_effect.borrow_mut() = Some(sound_cb);
```

In the `else` branch (line 74), add drops before `stop_loops()`:
```rust
        } else {
            // Drop closures
            vib_effect.borrow_mut().take();
            snd_effect.borrow_mut().take();
            stop_loops();
```

- [ ] **Step 11: Commit**

```bash
git add iem-mixer/iem-ui/src/components/audio_player.rs iem-mixer/iem-ui/src/components/talk_button.rs iem-mixer/iem-ui/src/components/alert_toast.rs
git commit -m "fix: store Effect closures in Rc<RefCell<>> to prevent memory leaks on re-run"
```

---

### Task 5: Throttle ChannelUpdate Signal Updates

Meters are throttled at 50ms, but `ChannelUpdate`, `GlobalVolumeUpdate`, and `StemsVolumeUpdate` fire `set_channels.update()` immediately on every WebSocket message. With 50 channels, this causes ~333 signal updates/sec → reactive graph saturation → unresponsive UI on phones.

**Files:**
- Modify: `iem-mixer/iem-ui/src/pages/mixer.rs:211-240`

- [ ] **Step 1: Add throttle timestamp for channel updates**

In `iem-mixer/iem-ui/src/pages/mixer.rs`, next to the existing `last_meter_time` (line 146), add:

```rust
    let last_channel_time = std::cell::Cell::new(0.0_f64);
```

- [ ] **Step 2: Wrap ChannelUpdate in throttle**

Replace lines 211-228:

**Before:**
```rust
                    iem_core::ServerMsg::ChannelUpdate {
                        track_index,
                        level_db,
                        muted,
                        pan,
                    } => {
                        if !touched.get(&track_index).copied().unwrap_or(false) {
                            set_channels.update(|chs| {
                                if let Some(ch) =
                                    chs.iter_mut().find(|c| c.track_index == track_index)
                                {
                                    ch.level_db = level_db;
                                    ch.muted = muted;
                                    ch.pan = pan;
                                }
                            });
                        }
                    }
```

**After:**
```rust
                    iem_core::ServerMsg::ChannelUpdate {
                        track_index,
                        level_db,
                        muted,
                        pan,
                    } => {
                        let now = js_sys::Date::now();
                        if now - last_channel_time.get() >= 50.0 {
                            last_channel_time.set(now);
                            if !touched.get(&track_index).copied().unwrap_or(false) {
                                set_channels.update(|chs| {
                                    if let Some(ch) =
                                        chs.iter_mut().find(|c| c.track_index == track_index)
                                    {
                                        ch.level_db = level_db;
                                        ch.muted = muted;
                                        ch.pan = pan;
                                    }
                                });
                            }
                        }
                    }
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/iem-ui/src/pages/mixer.rs
git commit -m "fix: throttle ChannelUpdate signals at 50ms to prevent reactive graph saturation"
```

---

### Task 6: Add Page Visibility Handling

When phone browser tabs are backgrounded, WebSocket messages accumulate. On resume, a flood of messages processes synchronously → main thread blocked for 500ms+ → app appears frozen. Fix by pausing meter processing when hidden and draining stale messages on resume.

**Files:**
- Modify: `iem-mixer/iem-ui/src/pages/mixer.rs` (onmessage handler + component mount)

- [ ] **Step 1: Add visibility tracking flag**

In `iem-mixer/iem-ui/src/pages/mixer.rs`, in the `connect_websocket` function, add a page-visible flag next to the throttle timestamps (after `last_channel_time`):

```rust
    // Track page visibility — skip meter updates when backgrounded
    let page_visible = std::rc::Rc::new(std::cell::Cell::new(true));
    let page_visible_closure = page_visible.clone();
```

- [ ] **Step 2: Register visibilitychange listener**

After creating the WebSocket and before registering onmessage, add:

```rust
    // Listen for page visibility changes
    let vis_closure = Closure::wrap(Box::new(move || {
        if let Some(w) = web_sys::window() {
            if let Some(doc) = w.document() {
                let hidden = doc.hidden();
                page_visible_closure.set(!hidden);
                if !hidden {
                    // Page resumed — reset throttle timestamps to process next message immediately
                    last_meter_time.set(0.0);
                    last_channel_time.set(0.0);
                }
            }
        }
    }) as Box<dyn FnMut()>);
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        let _ = doc.add_event_listener_with_callback(
            "visibilitychange",
            vis_closure.as_ref().unchecked_ref(),
        );
    }
    vis_closure.forget(); // Lives for page lifetime — acceptable since it's one-time
```

- [ ] **Step 3: Skip meter processing when page is hidden**

In the onmessage handler, update the Meters arm to skip entirely when hidden:

**Before:**
```rust
                    iem_core::ServerMsg::Meters { meters: m } => {
                        let now = js_sys::Date::now();
                        if now - last_meter_time.get() >= 50.0 {
```

**After:**
```rust
                    iem_core::ServerMsg::Meters { meters: m } => {
                        if !page_visible.get() {
                            return; // Skip meter updates when backgrounded
                        }
                        let now = js_sys::Date::now();
                        if now - last_meter_time.get() >= 50.0 {
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/iem-ui/src/pages/mixer.rs
git commit -m "fix: skip meter processing when page is backgrounded to prevent freeze on resume"
```

---

### Task 7: Local Lint + Push

- [ ] **Step 1: Run cargo fmt check**

```bash
cd iem-mixer && cargo fmt --all --check
```

If it fails, run `cargo fmt --all` and fix.

- [ ] **Step 2: Verify no dead code introduced**

```bash
cd iem-mixer && grep -rn "allow(dead_code)" iem-ui/src/
```

Expected: zero results.

- [ ] **Step 3: Push to dev**

```bash
git push origin dev
```

- [ ] **Step 4: Monitor CI until ALL jobs pass**

```bash
gh run list --branch dev --limit 3
gh run view <run-id>  # poll until terminal state
```

If any job fails: `gh run view <run-id> --log-failed`, fix ALL issues in ONE commit, push again.

---

### Task 8: Post-Deploy Verification

- [ ] **Step 1: Verify app loads on phone**

Open `https://iem.newlevel.media/` on phone browser, confirm app loads without blank page or console errors.

- [ ] **Step 2: Verify no console errors**

Open Chrome DevTools remote debugging connected to phone. Check console for zero errors and zero warnings.

- [ ] **Step 3: Simulate reconnection stress test**

In Chrome DevTools:
1. Navigate to mixer page
2. Toggle WiFi off/on 5 times in quick succession
3. Verify app recovers each time without freezing
4. Check Memory tab → heap snapshot should not grow unboundedly

- [ ] **Step 4: Verify backgrounding doesn't freeze**

1. Open mixer on phone
2. Switch to another app for 30 seconds
3. Switch back — app should respond to touch immediately
4. Meters should resume updating within 1-2 seconds

---

## Verification Summary

| Fix | How to verify |
|-----|--------------|
| `Closure::once_into_js` | `grep -rn "Closure::once" iem-ui/src/ \| grep -v once_into_js` → zero |
| AudioContext cleanup | Listen → stop → listen 20 times → heap stable |
| Effect closure storage | 10 reconnects → Memory tab shows flat heap |
| ChannelUpdate throttle | Chrome Performance tab → signal updates ≤20/sec |
| Visibility handling | Background 30s → resume → immediate touch response |
