# MixerPage Decomposition — Design Spec

**Issue:** #165 — Introduce disposal_guard and connection-manager struct for MixerPage  
**Date:** 2026-04-16  
**Status:** Draft

---

## Problem

`iem-mixer/iem-ui/src/pages/mixer.rs` is a 2865-line single file. The `MixerPage` component body alone is ~988 lines (649–1637), declaring ~50 reactive signals, 3 `setInterval` timers, a visibility listener, and a `connect_websocket` function with **44 parameters**. Each new feature (EQ, limiter, alerts, talkback) appends more signals and more parameters to `connect_websocket`.

The disposal-race fix (#153) converted 158 signal writes to `try_set`/`try_update` — correct but fragile. Every new signal must use the `try_` variant or risk a panic on navigation-back. The structural problem is that background tasks (WS callbacks, reconnect intervals, watchdog) outlive the reactive scope and write to disposed signals individually instead of being owned by a single guard.

## Goals

1. `MixerPage` component body under **~500 lines** (from ~988).
2. **Zero loose signal writes** in the component body — all signal writes flow through controller structs or the WS message handler.
3. Every background task (interval, timeout, WS callback) has a **single owner** whose `Drop` is the teardown — replacing 4 separate `on_cleanup` / `Closure::forget` calls.
4. `connect_websocket` takes **1 struct reference** instead of 44 parameters.
5. No UI changes, no REAPER changes, no protocol changes. All existing E2E tests pass without modification.

## Non-Goals

- Refactoring sub-components (`GlobalVolumeFader`, `StemsVolumeFader`, `ChannelList`) internally — they move to a new file but their code is unchanged.
- Changing the WebSocket protocol or server-side code.
- Adding features or fixing bugs — this is a pure refactor.

---

## Pre-Refactor Inventory (Baseline)

These counts are verified before work begins and re-verified after each task:

| Metric | Count |
|--------|-------|
| `signal(` calls | 56 |
| `try_set` / `try_update` calls | 158 |
| `set_interval` calls | 3 |
| `spawn_local` calls | 4 |
| `on_cleanup` calls | 4 |
| `Callback::new` calls | 27 |
| `Closure::wrap` / `Closure::once` calls | 17 |
| Total lines | 2865 |

After refactoring, the sum across all files in `pages/mixer/` must match these counts exactly, except:
- `on_cleanup` drops from 4 → 1 (replaced by `Drop` impl)
- `try_set`/`try_update` in the **component body** drops to 0 (they move into controller methods or the WS handler)
- Total `try_set`/`try_update` across all files stays at 158

---

## Architecture

### File Structure

Convert `pages/mixer.rs` into a `pages/mixer/` module directory:

```
pages/mixer/
├── mod.rs            # MixerPage component: signal setup, wiring, view template (~500 lines)
├── state.rs          # MixerState struct bundling all signals (~120 lines)
├── connection.rs     # ConnectionManager: WS, reconnect, watchdog, expiry, disposal guard (~500 lines)
├── handlers.rs       # Preset callbacks, display_channels Memo, mute-all, solo-clear (~250 lines)
├── components.rs     # GlobalVolumeFader, StemsVolumeFader, ChannelList (moved as-is, ~1200 lines)
├── push.rs           # subscribe_to_push + base64url_decode (moved as-is, ~220 lines)
└── helpers.rs        # ws_send, parse_track_name, format_db, DisplayChannel, constants, tests (~80 lines)
```

`pages/mod.rs` changes from `pub mod mixer;` to `pub mod mixer;` (no change — Rust resolves `mixer/mod.rs` automatically).

### MixerState (state.rs)

Bundles all 50+ signal pairs into a single struct. This eliminates `connect_websocket`'s 44-parameter signature.

```rust
pub struct MixerState {
    // Core
    pub channels: (ReadSignal<Vec<Channel>>, WriteSignal<Vec<Channel>>),
    pub meters: (ReadSignal<HashMap<usize, [f32; 2]>>, WriteSignal<HashMap<usize, [f32; 2]>>),
    pub connected: (ReadSignal<bool>, WriteSignal<bool>),
    pub loading: (ReadSignal<bool>, WriteSignal<bool>),

    // Fader touch guards
    pub fader_touched: (ReadSignal<HashMap<usize, bool>>, WriteSignal<HashMap<usize, bool>>),
    pub global_touched: (ReadSignal<bool>, WriteSignal<bool>),
    pub stems_touched: (ReadSignal<bool>, WriteSignal<bool>),

    // Global volume
    pub global_level: (ReadSignal<f32>, WriteSignal<f32>),
    pub global_muted: (ReadSignal<bool>, WriteSignal<bool>),

    // Stems
    pub stems_level: (ReadSignal<f32>, WriteSignal<f32>),
    pub stems_muted: (ReadSignal<bool>, WriteSignal<bool>),
    pub stems_bus_idx: (ReadSignal<Option<usize>>, WriteSignal<Option<usize>>),

    // EQ
    pub eq_open: (ReadSignal<Option<(usize, String)>>, WriteSignal<Option<(usize, String)>>),
    pub eq_bands: (ReadSignal<Vec<EqBandState>>, WriteSignal<Vec<EqBandState>>),
    pub eq_loading: (ReadSignal<bool>, WriteSignal<bool>),

    // Limiter
    pub limiter_open: (ReadSignal<Option<(usize, String)>>, WriteSignal<Option<(usize, String)>>),
    pub limiter_limit_db: (ReadSignal<f32>, WriteSignal<f32>),
    pub limiter_limit_norm: (ReadSignal<f32>, WriteSignal<f32>),
    pub limiter_enabled: (ReadSignal<bool>, WriteSignal<bool>),
    pub limiter_loading: (ReadSignal<bool>, WriteSignal<bool>),
    pub limiter_active_seconds: (ReadSignal<f64>, WriteSignal<f64>),

    // UI state
    pub active_category: (ReadSignal<Category>, WriteSignal<Category>),
    pub data_pulse: (ReadSignal<bool>, WriteSignal<bool>),
    pub pinned_channels: (ReadSignal<Vec<usize>>, WriteSignal<Vec<usize>>),
    pub hidden_channels: (ReadSignal<Vec<usize>>, WriteSignal<Vec<usize>>),
    pub network_mode: (ReadSignal<String>, WriteSignal<String>),
    pub output_track_idx: (ReadSignal<Option<usize>>, WriteSignal<Option<usize>>),
    pub soloed: (ReadSignal<HashSet<usize>>, WriteSignal<HashSet<usize>>),
    pub pre_solo_mutes: (ReadSignal<HashMap<usize, bool>>, WriteSignal<HashMap<usize, bool>>),
    pub double_tap_fader: (ReadSignal<bool>, WriteSignal<bool>),
    pub has_photo: (ReadSignal<bool>, WriteSignal<bool>),

    // Modals
    pub preset_modal_visible: (ReadSignal<bool>, WriteSignal<bool>),
    pub pin_modal_visible: (ReadSignal<bool>, WriteSignal<bool>),
    pub settings_modal_visible: (ReadSignal<bool>, WriteSignal<bool>),
    pub snapshot_modal_visible: (ReadSignal<bool>, WriteSignal<bool>),

    // Alerts & talkback
    pub alert_data: (ReadSignal<Option<(String, String)>>, WriteSignal<Option<(String, String)>>),
    pub alert_active: (ReadSignal<bool>, WriteSignal<bool>),
    pub talk_state: (ReadSignal<TalkState>, WriteSignal<TalkState>),
    pub engineer_talking: (ReadSignal<bool>, WriteSignal<bool>),

    // WebSocket
    pub ws: (ReadSignal<Option<web_sys::WebSocket>>, WriteSignal<Option<web_sys::WebSocket>>),
}

impl MixerState {
    pub fn new(member_id: &str) -> Self {
        let user_settings = UserSettings::load(member_id);
        Self {
            channels: signal(Vec::new()),
            meters: signal(HashMap::new()),
            connected: signal(false),
            loading: signal(true),
            fader_touched: signal(HashMap::new()),
            global_touched: signal(false),
            stems_touched: signal(false),
            global_level: signal(0.0),
            global_muted: signal(false),
            stems_level: signal(0.0),
            stems_muted: signal(false),
            stems_bus_idx: signal(None),
            eq_open: signal(None),
            eq_bands: signal(Vec::new()),
            eq_loading: signal(false),
            limiter_open: signal(None),
            limiter_limit_db: signal(-6.0),
            limiter_limit_norm: signal(0.0),
            limiter_enabled: signal(true),
            limiter_loading: signal(false),
            limiter_active_seconds: signal(0.0),
            active_category: signal(Category::Main),
            data_pulse: signal(false),
            pinned_channels: signal(Vec::new()),
            hidden_channels: signal(Vec::new()),
            network_mode: signal(String::new()),
            output_track_idx: signal(None),
            soloed: signal(std::collections::HashSet::new()),
            pre_solo_mutes: signal(HashMap::new()),
            double_tap_fader: signal(user_settings.double_tap_fader),
            has_photo: signal(false),
            preset_modal_visible: signal(false),
            pin_modal_visible: signal(false),
            settings_modal_visible: signal(false),
            snapshot_modal_visible: signal(false),
            alert_data: signal(None),
            alert_active: signal(false),
            talk_state: signal(TalkState::Idle),
            engineer_talking: signal(false),
            ws: signal(None),
        }
    }
}
```

### ConnectionManager (connection.rs)

Owns all background tasks: WebSocket connection, reconnect interval, watchdog interval, token-expiry interval, visibility listener, and the disposal guard.

```rust
pub struct ConnectionManager {
    /// Flips to true on Drop. All background closures check this before writing signals.
    disposal_guard: Rc<Cell<bool>>,

    /// JS interval handles for cleanup
    reconnect_interval_id: i32,
    watchdog_interval_id: i32,
    expiry_interval_id: i32,

    /// WebSocket closure storage (prevents leak on reconnect)
    ws_closures: WsClosureStore,

    /// Consecutive WS failures without data
    ws_fail_count: WsFailCounter,

    /// Watchdog state
    last_frame_at: Rc<Cell<f64>>,
    reconnect_attempt: Rc<Cell<u32>>,
    last_reconnect_attempt_at: Rc<Cell<f64>>,

    /// Page visibility (skip meter updates when backgrounded)
    page_visible: Rc<Cell<bool>>,
}
```

**Key design decision:** The `disposal_guard` replaces all `try_get_untracked()` scope-alive checks in the WS `onmessage` handler. Currently `connect_websocket` uses `fader_touched.try_get_untracked()` as a proxy disposal check (line 185). With the guard, the check becomes:

```rust
if disposal_guard.get() { return; }
```

This is checked once at the top of `onmessage`, and the rest of the handler can use plain `.try_set()` / `.try_update()` as before. The `try_` variants remain as a defense-in-depth layer — we do NOT convert them back to `.set()` / `.update()`. The guard prevents the handler from running at all after disposal; the `try_` variants prevent panics if a race slips through.

**Lifecycle:**

1. `ConnectionManager::new(state: &MixerState, member_id: &str)` — creates the WS, registers all intervals, wires up callbacks.
2. `ConnectionManager::connect(&self, state: &MixerState, member_id: &str)` — called by the reconnect interval when WS is closed. Replaces the current `connect_websocket` free function.
3. `Drop for ConnectionManager` — sets `disposal_guard = true`, clears all 3 intervals. The WS callbacks hold an `Rc` to the guard and self-deactivate.

**What moves into ConnectionManager:**
- `connect_websocket` function (lines 74–426) — becomes `ConnectionManager::connect`
- Reconnect closure (lines 884–968) — created inside `ConnectionManager::new`
- Watchdog closure (lines 985–1013) — created inside `ConnectionManager::new`
- Token-expiry closure (lines 1029–1048) — created inside `ConnectionManager::new`
- Visibility listener (lines 786–802) — created inside `ConnectionManager::new`
- All 4 `on_cleanup` calls — replaced by `Drop`

### No FaderController

The issue mentions extracting fader optimistic-update state into a `FaderController`. After code review, the "touched" flags are just 3 simple booleans (`fader_touched`, `global_touched`, `stems_touched`) used by the WS handler to skip server-echo during drags. The actual fader throttling (timestamps, pending timeouts) lives in `components/fader.rs`, not in MixerPage. Extracting 3 booleans into a struct adds complexity for no gain. They stay as fields in `MixerState`.

### helpers.rs

Moved verbatim from `mixer.rs`:
- `ws_send` function (lines 45–53)
- `DisplayChannel` struct (lines 36–42)
- `parse_track_name` function (lines 2823–2831)
- `format_db` function (lines 2833–2845)
- `POST_RELEASE_GUARD_MS` and `THROTTLE_INTERVAL_MS` constants (lines 27–31)
- `MAX_WS_FAILURES` constant (line 68)
- `WsClosureStore` and `WsFailCounter` type aliases (lines 56–71)
- Unit tests (lines 2844–2865)

### push.rs

Moved verbatim from `mixer.rs`:
- `subscribe_to_push` function (lines 430–631)
- `base64url_decode` helper (lines 636–645)

### handlers.rs

Moved from MixerPage component body:
- `display_channels` Memo (lines 1062–1159)
- `get_current_state` Callback for preset save (lines 1162–1217)
- `on_load_preset` Callback for preset load (lines 1219–1260)
- `on_mute_all` Callback (lines 1282–1289)

### components.rs

Moved verbatim from `mixer.rs`:
- `GlobalVolumeFader` component (lines 1641–1874)
- `StemsVolumeFader` component (lines 1876–2094)
- `ChannelList` component (lines 2096–2821)

---

## Verification Strategy

### Phase 1: Move-only (no logic changes)

Each task moves code blocks verbatim into new files. The diff for each move must show identical lines deleted from `mixer.rs` and added to the new file. No renaming, no restructuring.

After each move:
1. Run inventory counts across all `pages/mixer/*.rs` files — totals must match baseline.
2. `cargo fmt --all --check` passes.
3. CI compilation succeeds (WASM target catches any missing imports or type mismatches).

### Phase 2: Interface refactoring

After all code is moved and verified:
1. Replace 50 loose `let (x, set_x) = signal(...)` with `MixerState::new()`.
2. Replace `connect_websocket`'s 44-param signature with `ConnectionManager::connect(&self, state: &MixerState, ...)`.
3. Replace 4 `on_cleanup` + `Closure::forget` with `ConnectionManager::Drop`.
4. Add `disposal_guard` check to WS `onmessage` handler.

### Phase 3: Final verification

1. All 10 CI jobs pass (lint, test, build-wasm, e2e, build-tauri, mutation, test-integrity, build-vban, deploy, post-deploy E2E).
2. Post-move inventory counts match baseline (with documented exceptions for `on_cleanup` reduction).
3. `MixerPage` component body is under 500 lines.
4. `grep -c 'try_set\|try_update' pages/mixer/mod.rs` shows 0 in the component body (all moved into connection.rs or handlers.rs).

---

## Risk Assessment

**Low risk:**
- Moving sub-components (`GlobalVolumeFader`, etc.) — they are already self-contained with explicit props. Move is pure copy-paste.
- Moving `subscribe_to_push` and helpers — zero coupling to component state.

**Medium risk:**
- Creating `MixerState` struct — every field access changes from `foo` to `state.foo.0` (read) or `state.foo.1` (write). Many call sites to update, but each is mechanical.
- `ConnectionManager` ownership — the WS `onmessage` closure captures many `WriteSignal`s. These need to be captured as clones from `&MixerState` at `ConnectionManager::new` time.

**Mitigated by:**
- Rust compiler catches every missing field, wrong type, and borrow error.
- WASM build verifies all closures compile.
- E2E tests verify all user workflows still work end-to-end.

---

## Success Criteria (from issue #165)

- [x] `MixerPage` body under ~500 lines (currently ~988)
- [x] Zero `.set()` / `.update()` / `.try_set()` / `.try_update()` calls inside the component body
- [x] Every background task has a single owner whose `Drop` impl is the teardown
- [x] All existing E2E tests pass without modification
- [x] If the CI disposal-safety gate is removed, the controllers still short-circuit on disposal via `disposal_guard`

## Out of Scope

- Touching the REAPER side of the app
- UI / visual changes
- Feature work
- Refactoring sub-component internals (they move files but keep their code)
