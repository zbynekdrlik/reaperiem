# MixerPage Decomposition — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the 2865-line `mixer.rs` into a module directory with dedicated files for state, connection management, handlers, sub-components, and helpers — with deterministic disposal via `ConnectionManager::Drop`.

**Architecture:** Phase 1 moves code verbatim into new files (no logic changes). Phase 2 introduces `MixerState` struct and `ConnectionManager` with `Drop`. Phase 3 verifies inventory counts and CI.

**Tech Stack:** Rust, Leptos (reactive WASM framework), web-sys, wasm-bindgen

**Spec:** `docs/superpowers/specs/2026-04-16-mixer-page-decomposition-design.md`

---

## Context

The source file `iem-mixer/iem-ui/src/pages/mixer.rs` is 2865 lines. It contains:

- `MixerPage` component body (lines 649–1637, ~988 lines) with ~50 signal declarations
- `connect_websocket` function (lines 74–426, 44 parameters)
- 3 sub-components: `GlobalVolumeFader` (1641–1872), `StemsVolumeFader` (1876–2092), `ChannelList` (2096–2820)
- Helper functions and types (lines 25–71, 2822–2865)
- `subscribe_to_push` + `base64url_decode` (lines 430–645)

### Pre-Refactor Inventory (Baseline)

These counts are checked after EVERY task across all `pages/mixer/*.rs` files:

| Metric | Baseline |
|--------|----------|
| `signal(` | 56 |
| `try_set` or `try_update` | 158 |
| `set_interval` | 3 |
| `spawn_local` | 4 |
| `on_cleanup` | 4 (→ 1 after Phase 2) |
| `Callback::new` | 27 |
| `Closure::wrap` or `Closure::once` | 17 |
| Total lines | 2865 |

### Inventory verification command

Run this after every task to verify no code was lost or duplicated:

```bash
cd /home/newlevel/devel/reaperiem && \
echo "signal: $(grep -rc 'signal(' iem-mixer/iem-ui/src/pages/mixer/ | awk -F: '{s+=$2}END{print s}')" && \
echo "try_set/try_update: $(grep -rc 'try_set\|try_update' iem-mixer/iem-ui/src/pages/mixer/ | awk -F: '{s+=$2}END{print s}')" && \
echo "set_interval: $(grep -rc 'set_interval' iem-mixer/iem-ui/src/pages/mixer/ | awk -F: '{s+=$2}END{print s}')" && \
echo "spawn_local: $(grep -rc 'spawn_local' iem-mixer/iem-ui/src/pages/mixer/ | awk -F: '{s+=$2}END{print s}')" && \
echo "on_cleanup: $(grep -rc 'on_cleanup' iem-mixer/iem-ui/src/pages/mixer/ | awk -F: '{s+=$2}END{print s}')" && \
echo "Callback::new: $(grep -rc 'Callback::new' iem-mixer/iem-ui/src/pages/mixer/ | awk -F: '{s+=$2}END{print s}')" && \
echo "Closure: $(grep -rc 'Closure::wrap\|Closure::once' iem-mixer/iem-ui/src/pages/mixer/ | awk -F: '{s+=$2}END{print s}')" && \
echo "Total lines: $(cat iem-mixer/iem-ui/src/pages/mixer/*.rs | wc -l)"
```

Any discrepancy = code was lost or duplicated. Fix before proceeding.

---

## File Map

### Version bump
- `iem-mixer/crates/iem-core/Cargo.toml` — 1.151.0 → 1.152.0
- `iem-mixer/Cargo.toml` — 1.151.0 → 1.152.0
- `iem-mixer/crates/iem-server/Cargo.toml` — 1.151.0 → 1.152.0
- `iem-mixer/iem-ui/Cargo.toml` — 1.151.0 → 1.152.0
- `iem-mixer/src-tauri/Cargo.toml` — 1.151.0 → 1.152.0
- `iem-mixer/src-tauri/tauri.conf.json` — 1.151.0 → 1.152.0

### Phase 1: Move-only (new files)
- Create: `iem-mixer/iem-ui/src/pages/mixer/mod.rs` — MixerPage component
- Create: `iem-mixer/iem-ui/src/pages/mixer/helpers.rs` — ws_send, types, constants, utils, tests
- Create: `iem-mixer/iem-ui/src/pages/mixer/push.rs` — subscribe_to_push, base64url_decode
- Create: `iem-mixer/iem-ui/src/pages/mixer/components.rs` — GlobalVolumeFader, StemsVolumeFader, ChannelList
- Delete: `iem-mixer/iem-ui/src/pages/mixer.rs` — replaced by `mixer/mod.rs`

### Phase 2: Interface refactoring (new files)
- Create: `iem-mixer/iem-ui/src/pages/mixer/state.rs` — MixerState struct
- Create: `iem-mixer/iem-ui/src/pages/mixer/connection.rs` — ConnectionManager with Drop

### Existing (no changes)
- `iem-mixer/iem-ui/src/pages/mod.rs` — already says `pub mod mixer;`, no change needed

---

## Task Dependencies

```
T1 (version bump) → T2 (create module dir + helpers.rs + push.rs) → T3 (move sub-components) → T4 (convert mixer.rs to mod.rs) → T5 (verify Phase 1) → T6 (extract MixerState) → T7 (extract ConnectionManager) → T8 (verify Phase 2) → T9 (changelog) → T10 (push + CI) → T11 (PR)
```

All tasks are sequential — each depends on the previous.

---

## Task 1: Version Bump (1.151.0 → 1.152.0)

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`

- [ ] **Step 1: Bump all version files**

```bash
cd /home/newlevel/devel/reaperiem
sed -i 's/version = "1.151.0"/version = "1.152.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.151.0"/"version": "1.152.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify**

```bash
grep -c '1.152.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
# Both should return 1
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.152.0"
```

---

## Task 2: Create Module Directory + helpers.rs + push.rs

This task creates the `pages/mixer/` directory and moves self-contained helper code out of `mixer.rs`.

**Files:**
- Create: `iem-mixer/iem-ui/src/pages/mixer/helpers.rs`
- Create: `iem-mixer/iem-ui/src/pages/mixer/push.rs`
- Modify: `iem-mixer/iem-ui/src/pages/mixer.rs` — remove moved code

**CRITICAL: Do NOT create `mixer/mod.rs` yet.** The file `mixer.rs` must remain as the module root until Task 4, because Rust cannot have both `mixer.rs` and `mixer/mod.rs`. In this task, `mixer.rs` stays at its current location and gains `mod helpers;` and `mod push;` declarations.

### helpers.rs content

- [ ] **Step 1: Create `helpers.rs`**

Create `iem-mixer/iem-ui/src/pages/mixer/helpers.rs` containing the following code copied verbatim from `mixer.rs`:

- Lines 25–31: constants `POST_RELEASE_GUARD_MS` and `THROTTLE_INTERVAL_MS`
- Lines 33–42: `DisplayChannel` struct
- Lines 44–53: `ws_send` function
- Lines 55–70: `WsClosures` type, `WsClosureStore` type, `WsFailCounter` type, `MAX_WS_FAILURES` constant
- Lines 2822–2865: `parse_track_name`, `format_db`, and `mod tests`

The file needs its own imports. Write the complete file:

```rust
//! Shared helpers, types, and constants for the mixer module.

use leptos::prelude::*;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

/// Post-release guard duration in milliseconds.
/// With server-side echo suppression, this only needs to cover WebSocket round-trip (~10-20ms).
pub(super) const POST_RELEASE_GUARD_MS: i32 = 100;

/// Minimum interval between WebSocket sends per track (ms).
/// Limits to ~20 commands/sec to avoid overwhelming the server.
pub(super) const THROTTLE_INTERVAL_MS: f64 = 50.0;

/// Processed channel for display (handles stereo pairs)
/// Note: level_db, pan, muted are read via derived signals from channels
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DisplayChannel {
    pub track_index: usize,
    pub display_name: String,
    pub is_stereo: bool,
    pub partner_index: Option<usize>,
    pub is_my_input: bool,
}

/// Send a command via WebSocket (synchronous, non-blocking)
pub(super) fn ws_send(ws: ReadSignal<Option<web_sys::WebSocket>>, cmd: &iem_core::ClientMsg) {
    if let Some(ws) = ws.get_untracked() {
        if ws.ready_state() == web_sys::WebSocket::OPEN {
            if let Ok(json) = serde_json::to_string(cmd) {
                let _ = ws.send_with_str(&json);
            }
        }
    }
}

/// Storage for WebSocket closures to prevent memory leaks on reconnect.
/// Dropping a Closure that was passed to JS via `as_ref().unchecked_ref()` properly
/// releases the WASM-side allocation. Without this, `Closure::forget()` leaks on every reconnect.
/// Uses Rc<RefCell<>> because wasm_bindgen::Closure is !Send (WASM is single-threaded).
pub(super) type WsClosures = (
    Closure<dyn FnMut(web_sys::MessageEvent)>,
    Closure<dyn FnMut(web_sys::CloseEvent)>,
);
pub(super) type WsClosureStore = std::rc::Rc<std::cell::RefCell<Option<WsClosures>>>;

/// Counter for consecutive WebSocket failures without receiving data.
/// Shared across connect_websocket calls via Rc<Cell<>>.
pub(super) type WsFailCounter = std::rc::Rc<std::cell::Cell<u32>>;

/// Max consecutive WS failures before redirecting to login
pub(super) const MAX_WS_FAILURES: u32 = 3;

/// Parse track name into main and type parts
pub(super) fn parse_track_name(name: &str) -> (String, String) {
    let parts: Vec<&str> = name.split_whitespace().collect();
    if parts.len() >= 2 {
        (parts[0].to_string(), parts[1..].join(" "))
    } else {
        (name.to_string(), String::new())
    }
}

/// Format dB value for display with unit suffix
pub(super) fn format_db(db: f32) -> String {
    if db <= -60.0 {
        "-\u{221E}dB".to_string()
    } else if db >= 0.0 {
        format!("+{:.1}dB", db)
    } else {
        format!("{:.1}dB", db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_db_uses_proper_notation() {
        assert!(format_db(0.0).ends_with("dB"), "Must use 'dB' not 'db'");
        assert!(format_db(-6.0).ends_with("dB"));
        assert!(format_db(-60.0).ends_with("dB")); // -inf case
    }

    #[test]
    fn test_format_db_max_length() {
        let cases = [0.0, 6.0, 12.0, -6.0, -12.5, -59.9, -60.0, -100.0];
        for db in cases {
            let s = format_db(db);
            assert!(
                s.chars().count() <= 7,
                "format_db({db}) = \"{s}\" exceeds 7 chars"
            );
        }
    }
}
```

- [ ] **Step 2: Create `push.rs`**

Create `iem-mixer/iem-ui/src/pages/mixer/push.rs` containing the following code copied verbatim from `mixer.rs`:

- Lines 428–645: `subscribe_to_push` function and `base64url_decode` helper

The file needs its own imports:

```rust
//! Web Push subscription for engineer SOS alerts.

use wasm_bindgen::prelude::*;

/// Subscribe to Web Push for engineer SOS alerts (#133).
/// Fetches VAPID key, subscribes via Push API, sends subscription to server.
pub(super) fn subscribe_to_push() {
    // ... (copy lines 431–631 verbatim from mixer.rs)
}

/// Decode base64url (no padding) to bytes.
/// Note: atob() returns a Latin-1 string (each char = one byte 0-255).
/// Rust's `.bytes()` gives UTF-8 which mangles values > 127. Use `.chars() as u8` instead.
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    // ... (copy lines 637–644 verbatim from mixer.rs)
}
```

**Important:** Copy the COMPLETE function bodies from mixer.rs lines 431–644 verbatim. Do not summarize or abbreviate.

- [ ] **Step 3: Remove moved code from mixer.rs**

In `mixer.rs`, delete:
- Lines 25–70 (constants, DisplayChannel, ws_send, type aliases)
- Lines 428–645 (subscribe_to_push, base64url_decode)
- Lines 2822–2865 (parse_track_name, format_db, tests)

Replace with `mod` declarations and `use` re-exports. Add at the top of `mixer.rs` (after the existing `use` imports):

```rust
mod helpers;
mod push;

use helpers::*;
use push::subscribe_to_push;
```

**Note:** The `#[allow(clippy::too_many_arguments)]` on `connect_websocket` (line 73) must remain — it's not part of the moved code.

- [ ] **Step 4: Run `cargo fmt`**

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer && cargo fmt --all
cd /home/newlevel/devel/reaperiem/iem-mixer/iem-ui && cargo fmt --all
```

- [ ] **Step 5: Run inventory verification**

Run the inventory verification command from the Context section above. All counts must match baseline (56/158/3/4/4/27/17). Total lines will differ slightly due to added `mod`/`use` lines and removed code, but the sum of grep counts must be exact.

- [ ] **Step 6: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/iem-ui/src/pages/mixer.rs \
  iem-mixer/iem-ui/src/pages/mixer/helpers.rs \
  iem-mixer/iem-ui/src/pages/mixer/push.rs
git commit -m "refactor: extract helpers.rs and push.rs from mixer (#165)"
```

**WAIT — Rust module resolution problem.** Having both `mixer.rs` and `mixer/helpers.rs` is impossible — Rust requires either `mixer.rs` OR `mixer/mod.rs`, not both. The correct approach is:

1. **Rename** `mixer.rs` → `mixer/mod.rs`
2. Create `mixer/helpers.rs` and `mixer/push.rs` alongside it

So Step 3 must first **move** `mixer.rs` to `mixer/mod.rs`:

```bash
mkdir -p iem-mixer/iem-ui/src/pages/mixer
git mv iem-mixer/iem-ui/src/pages/mixer.rs iem-mixer/iem-ui/src/pages/mixer/mod.rs
```

Then proceed with the deletions and additions to `mod.rs`.

---

## Task 3: Move Sub-Components to components.rs

**Files:**
- Create: `iem-mixer/iem-ui/src/pages/mixer/components.rs`
- Modify: `iem-mixer/iem-ui/src/pages/mixer/mod.rs` — remove moved code, add `mod components;`

- [ ] **Step 1: Create `components.rs`**

Create `iem-mixer/iem-ui/src/pages/mixer/components.rs` containing the following code copied verbatim from `mod.rs`:

- Lines 1639–2820 (after Task 2 renumbering): `GlobalVolumeFader`, `StemsVolumeFader`, `ChannelList`

The file needs its own imports:

```rust
//! Sub-components for the mixer page: GlobalVolumeFader, StemsVolumeFader, ChannelList.

use leptos::prelude::*;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

use crate::api::Channel;
use crate::components::category_tabs::Category;
use crate::components::eq_modal::EqBandState;
use crate::components::fader::Fader;
use crate::components::meter::Meter;
use crate::components::pan::PanKnob;

use super::helpers::{
    format_db, parse_track_name, ws_send, DisplayChannel, POST_RELEASE_GUARD_MS,
    THROTTLE_INTERVAL_MS,
};

// ... (paste GlobalVolumeFader, StemsVolumeFader, ChannelList verbatim)
```

**Important:** The three components reference `ws_send`, `format_db`, `parse_track_name`, `DisplayChannel`, `POST_RELEASE_GUARD_MS`, and `THROTTLE_INTERVAL_MS` — all now in `helpers.rs`. The `use super::helpers::*` import covers these.

- [ ] **Step 2: Remove moved code from mod.rs**

Delete the three component functions from `mod.rs` (everything from `/// Global IEM volume fader` through the end of `ChannelList`'s closing brace, excluding `parse_track_name`, `format_db`, and `mod tests` which were already moved in Task 2).

Add to the `mod` declarations in `mod.rs`:

```rust
mod components;

use components::{ChannelList, GlobalVolumeFader, StemsVolumeFader};
```

- [ ] **Step 3: Run `cargo fmt`**

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer && cargo fmt --all
cd /home/newlevel/devel/reaperiem/iem-mixer/iem-ui && cargo fmt --all
```

- [ ] **Step 4: Run inventory verification**

All counts must match baseline.

- [ ] **Step 5: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/iem-ui/src/pages/mixer/mod.rs \
  iem-mixer/iem-ui/src/pages/mixer/components.rs
git commit -m "refactor: move sub-components to components.rs (#165)"
```

---

## Task 4: Verify Phase 1 — Format Check + Inventory

**Files:** None modified — verification only.

- [ ] **Step 1: Run cargo fmt check**

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer && cargo fmt --all --check
cd /home/newlevel/devel/reaperiem/iem-mixer/iem-ui && cargo fmt --all --check
```

Both must exit 0.

- [ ] **Step 2: Run full inventory verification**

Run the inventory command. Compare each count against baseline:

| Metric | Expected |
|--------|----------|
| `signal(` | 56 |
| `try_set` or `try_update` | 158 |
| `set_interval` | 3 |
| `spawn_local` | 4 |
| `on_cleanup` | 4 |
| `Callback::new` | 27 |
| `Closure::wrap` or `Closure::once` | 17 |

If ANY count is off, investigate and fix before proceeding.

- [ ] **Step 3: Verify file structure**

```bash
ls -la iem-mixer/iem-ui/src/pages/mixer/
# Expected: mod.rs, helpers.rs, push.rs, components.rs
```

- [ ] **Step 4: Verify mod.rs line count**

```bash
wc -l iem-mixer/iem-ui/src/pages/mixer/mod.rs
# Should be ~700-800 lines (MixerPage body + connect_websocket + imports + mod declarations)
# Down from 2865 — the sub-components, helpers, and push code are now in separate files
```

---

## Task 5: Extract MixerState Struct (state.rs)

This is the first Phase 2 task — actual interface refactoring.

**Files:**
- Create: `iem-mixer/iem-ui/src/pages/mixer/state.rs`
- Modify: `iem-mixer/iem-ui/src/pages/mixer/mod.rs`

- [ ] **Step 1: Create `state.rs`**

Create `iem-mixer/iem-ui/src/pages/mixer/state.rs` with the `MixerState` struct that bundles all signal pairs. The struct definition is in the spec at `docs/superpowers/specs/2026-04-16-mixer-page-decomposition-design.md` under "MixerState (state.rs)".

The file must include:
1. The struct definition with all fields as `pub(super)` tuple pairs `(ReadSignal<T>, WriteSignal<T>)`
2. `MixerState::new(member_id: &str) -> Self` that creates all signals with their default values
3. Necessary imports

```rust
//! Reactive state for the MixerPage component.
//!
//! Bundles all ~50 signal pairs into a single struct so that
//! `connect_websocket` can take one reference instead of 44 parameters.

use leptos::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::api::Channel;
use crate::components::category_tabs::Category;
use crate::components::eq_modal::EqBandState;
use crate::components::settings_modal::UserSettings;
use crate::components::talk_button::TalkState;

/// All reactive state owned by MixerPage.
///
/// Each field is a `(ReadSignal<T>, WriteSignal<T>)` tuple.
/// Access: `state.channels.0` for read, `state.channels.1` for write.
pub(super) struct MixerState {
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
            soloed: signal(HashSet::new()),
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

- [ ] **Step 2: Replace signal declarations in mod.rs with MixerState**

In `mod.rs`, add the module declaration:

```rust
mod state;
use state::MixerState;
```

Replace all ~50 `let (foo, set_foo) = signal(...)` declarations in `MixerPage()` body with a single:

```rust
let state = MixerState::new(&member_id());
```

Then update ALL references throughout `mod.rs`:
- `channels` → `state.channels.0`
- `set_channels` → `state.channels.1`
- `meters` → `state.meters.0`
- `set_meters` → `state.meters.1`
- etc. for every signal pair

This is a large mechanical replacement. Be thorough — every signal name in MixerPage body, in `connect_websocket` call sites, in the view template, and in callback closures.

- [ ] **Step 3: Update `connect_websocket` signature**

Replace the 44-parameter signature with:

```rust
fn connect_websocket(
    member: &str,
    last_frame_at: std::rc::Rc<std::cell::Cell<f64>>,
    reconnect_attempt: std::rc::Rc<std::cell::Cell<u32>>,
    state: &MixerState,
    ws_closures: WsClosureStore,
    ws_fail_count: WsFailCounter,
    page_visible: std::rc::Rc<std::cell::Cell<bool>>,
) {
```

Update the function body to read signals from `state.foo.0` and write to `state.foo.1`.

Update both call sites (initial connect in Effect and reconnect closure) to pass `&state`.

**Ownership note:** The `connect_websocket` function's `onmessage` closure captures `WriteSignal` values. Since `WriteSignal` is `Copy`, the closure captures copies — it does NOT borrow from `state`. This means `state` does not need to outlive the closure; the signals are `Copy` types that the closure owns independently.

- [ ] **Step 4: Update sub-component call sites**

The view template passes individual signals to sub-components. These change from:
```rust
// Before:
level=global_level
set_level=set_global_level

// After:
level=state.global_level.0
set_level=state.global_level.1
```

Update ALL component instantiations in the view template.

- [ ] **Step 5: Run cargo fmt**

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer && cargo fmt --all
cd /home/newlevel/devel/reaperiem/iem-mixer/iem-ui && cargo fmt --all
```

- [ ] **Step 6: Run inventory verification**

`signal(` count will DECREASE in `mod.rs` (the 38 signals that moved into `state.rs::new()`), but the TOTAL across all files must still be 56.

- [ ] **Step 7: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/iem-ui/src/pages/mixer/state.rs \
  iem-mixer/iem-ui/src/pages/mixer/mod.rs
git commit -m "refactor: extract MixerState struct, reduce connect_websocket to 7 params (#165)"
```

---

## Task 6: Extract ConnectionManager (connection.rs)

**Files:**
- Create: `iem-mixer/iem-ui/src/pages/mixer/connection.rs`
- Modify: `iem-mixer/iem-ui/src/pages/mixer/mod.rs`

This is the most complex task — it introduces the `disposal_guard` and moves all background tasks (WS connection, reconnect, watchdog, token expiry, visibility listener) into a struct with deterministic `Drop`.

- [ ] **Step 1: Create `connection.rs`**

Create `iem-mixer/iem-ui/src/pages/mixer/connection.rs` containing:

```rust
//! WebSocket connection manager with deterministic disposal.
//!
//! Owns all background tasks: WebSocket, reconnect interval, watchdog interval,
//! token-expiry interval, visibility listener. Drop clears everything.

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

use crate::api::Channel;
use crate::components::eq_modal::EqBandState;
use crate::components::talk_button::TalkState;

use super::helpers::{WsClosureStore, WsClosures, WsFailCounter, MAX_WS_FAILURES};
use super::state::MixerState;

/// Manages WebSocket connection lifecycle and all background intervals.
///
/// When dropped, sets the disposal guard and clears all JS intervals.
/// All background closures check the guard before writing signals.
pub(super) struct ConnectionManager {
    /// Flips to true on Drop. All background closures check this before running.
    disposal_guard: std::rc::Rc<std::cell::Cell<bool>>,

    /// JS interval handles for cleanup
    reconnect_interval_id: i32,
    watchdog_interval_id: i32,
    expiry_interval_id: i32,

    /// WebSocket closure storage (prevents leak on reconnect)
    _ws_closures: WsClosureStore,
}

impl Drop for ConnectionManager {
    fn drop(&mut self) {
        self.disposal_guard.set(true);
        if let Some(w) = web_sys::window() {
            w.clear_interval_with_handle(self.reconnect_interval_id);
            w.clear_interval_with_handle(self.watchdog_interval_id);
            w.clear_interval_with_handle(self.expiry_interval_id);
        }
    }
}

impl ConnectionManager {
    /// Create a new ConnectionManager that immediately connects and starts all intervals.
    pub fn new(state: &MixerState, member_id: impl Fn() -> String + Clone + 'static) -> Self {
        let disposal_guard = std::rc::Rc::new(std::cell::Cell::new(false));

        // ... (move all background task setup code here from mod.rs)
        // This includes:
        // 1. The initial connect_websocket Effect
        // 2. The reconnect closure + setInterval (2s)
        // 3. The watchdog closure + setInterval (5s)
        // 4. The token-expiry closure + setInterval (60s)
        // 5. The visibility listener

        // The connect_websocket function body moves here as a method

        // ... (implementation details below)

        todo!("Full implementation in step 2")
    }
}
```

- [ ] **Step 2: Move connect_websocket into ConnectionManager**

The `connect_websocket` free function becomes `ConnectionManager::connect()`. It takes `&self` plus `&MixerState` and `member: &str`.

At the top of the `onmessage` closure, add the disposal guard check:

```rust
// Before (current proxy check via fader_touched):
let Some(touched) = fader_touched.try_get_untracked() else {
    return;
};

// After (explicit guard + keep try_get as defense-in-depth):
if disposal_guard.get() { return; }
let Some(touched) = fader_touched.try_get_untracked() else {
    return;
};
```

The guard `Rc<Cell<bool>>` is cloned into each closure.

- [ ] **Step 3: Move reconnect/watchdog/expiry intervals into `ConnectionManager::new()`**

Move the three interval setup blocks from `MixerPage()` body into `ConnectionManager::new()`. Each closure captures a clone of `disposal_guard` and checks it at the top:

```rust
// Example for reconnect closure:
let guard = disposal_guard.clone();
let reconnect_closure = Closure::wrap(Box::new(move || {
    if guard.get() { return; }
    // ... rest of reconnect logic
}) as Box<dyn FnMut()>);
```

- [ ] **Step 4: Move visibility listener into `ConnectionManager::new()`**

Move the `visibilitychange` event listener setup from `MixerPage()` body into `ConnectionManager::new()`. The `page_visible` `Rc<Cell<bool>>` becomes a field of `ConnectionManager` (or stays as a local in `new()` since it's only used by `connect`).

- [ ] **Step 5: Remove moved code from mod.rs**

Delete from `MixerPage()` body:
- The `connect_websocket` free function (entire function)
- The initial connect Effect
- The reconnect closure + setInterval
- The watchdog closure + setInterval
- The token-expiry closure + setInterval
- The visibility listener
- All 4 `on_cleanup` calls
- All `Rc<Cell<>>` declarations for watchdog/reconnect state

Replace with:

```rust
mod connection;
use connection::ConnectionManager;

// In MixerPage():
let _connection = ConnectionManager::new(&state, member_id.clone());
```

The `_connection` binding keeps the manager alive for the component's lifetime. When Leptos drops the component scope, `_connection` is dropped, which triggers `ConnectionManager::Drop`.

- [ ] **Step 6: Run cargo fmt**

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer && cargo fmt --all
cd /home/newlevel/devel/reaperiem/iem-mixer/iem-ui && cargo fmt --all
```

- [ ] **Step 7: Run inventory verification**

Expected changes from baseline:
- `on_cleanup`: 4 → 0 in mod.rs (all replaced by `ConnectionManager::Drop`). The total across all files may be 0 or 1 depending on whether `on_cleanup` is used inside `ConnectionManager::new()` — it should be 0 since Drop handles cleanup.
- `Closure::wrap` count in mod.rs drops (moved to connection.rs), but total across files stays at 17.
- `set_interval` count in mod.rs drops to 0 (moved to connection.rs), but total stays at 3.

- [ ] **Step 8: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/iem-ui/src/pages/mixer/connection.rs \
  iem-mixer/iem-ui/src/pages/mixer/mod.rs
git commit -m "refactor: extract ConnectionManager with disposal_guard and Drop (#165)"
```

---

## Task 7: Verify Phase 2 — Line Counts + Format Check

**Files:** None modified — verification only.

- [ ] **Step 1: Check mod.rs line count**

```bash
wc -l iem-mixer/iem-ui/src/pages/mixer/mod.rs
# Target: under 500 lines (issue success criterion)
```

If over 500, identify what else can be moved. The display_channels Memo and preset handlers are candidates for a `handlers.rs` extraction — but only if needed to hit the target.

- [ ] **Step 2: Run cargo fmt check**

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer && cargo fmt --all --check
cd /home/newlevel/devel/reaperiem/iem-mixer/iem-ui && cargo fmt --all --check
```

- [ ] **Step 3: Run full inventory verification**

| Metric | Expected |
|--------|----------|
| `signal(` | 56 (38 in state.rs, rest in components.rs + mod.rs) |
| `try_set` or `try_update` | 158 |
| `set_interval` | 3 (all in connection.rs) |
| `spawn_local` | 4 (1 in push.rs, rest in mod.rs or connection.rs) |
| `on_cleanup` | 0 (replaced by Drop) |
| `Callback::new` | 27 |
| `Closure::wrap` or `Closure::once` | 17 |

- [ ] **Step 4: Verify no `try_set`/`try_update` in MixerPage body**

```bash
# Count try_set/try_update in mod.rs ONLY (should be near 0 in component body)
grep -c 'try_set\|try_update' iem-mixer/iem-ui/src/pages/mixer/mod.rs
# The remaining ones should only be in:
# - The view template's inline event handlers (these are fine — they're UI callbacks)
# - Callback::new closures for preset/mute/solo (these are fine — they're event handlers)
# The goal is zero try_set/try_update in the "setup" part of MixerPage body
```

- [ ] **Step 5: If mod.rs > 500 lines, extract handlers**

If mod.rs is still over 500 lines after Task 6, create `handlers.rs` and move:
- `display_channels` Memo
- `get_current_state` Callback
- `on_load_preset` Callback
- `on_mute_all` Callback

This is contingent — only do it if needed to hit the 500-line target.

---

## Task 8: Changelog Entry

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Add v1.152.0 changelog entry**

Add a new changelog section in README.md under the existing changelog:

```markdown
### v1.152.0 (2026-04-16)

- **Refactor**: MixerPage decomposed from single 2865-line file into module directory — separate files for state, connection management, sub-components, and helpers (#165)
- **Arch**: Introduced `ConnectionManager` with deterministic `Drop` — all background tasks (WebSocket, reconnect, watchdog, token expiry) torn down in one place
- **Arch**: Introduced `MixerState` struct — `connect_websocket` reduced from 44 parameters to 7
- **Internal**: `disposal_guard` replaces per-signal scope-alive checks — new signals no longer need `try_set` convention
```

- [ ] **Step 2: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add README.md
git commit -m "docs: changelog entry for v1.152.0 MixerPage decomposition (#165)"
```

---

## Task 9: Push + Monitor CI

- [ ] **Step 1: Run local format check**

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer && cargo fmt --all --check
cd /home/newlevel/devel/reaperiem/iem-mixer/iem-ui && cargo fmt --all --check
```

Both must pass before pushing.

- [ ] **Step 2: Push**

```bash
cd /home/newlevel/devel/reaperiem
git push origin dev
```

- [ ] **Step 3: Monitor CI**

```bash
gh run list --limit 3
```

Get the run ID and monitor with background sleep:

```bash
sleep 300 && gh run view <run-id> --json status,conclusion,jobs
```

Wait for ALL jobs to reach terminal state (success/failure/skipped). If any job fails:

```bash
gh run view <run-id> --log-failed
```

Investigate the failure, fix ALL issues in ONE commit, push, and monitor again.

- [ ] **Step 4: Verify all 10 jobs pass**

Expected jobs: test-integrity, lint, test, build-wasm, e2e, mutation-test, build-tauri, build-vban, deploy, post-deploy E2E (if applicable).

ALL must be green (or skipped for expected reasons like version-bump-check on dev push).

---

## Task 10: Open PR + Verify Mergeable

- [ ] **Step 1: Create PR**

```bash
gh pr create --title "refactor: MixerPage decomposition (#165)" --body "$(cat <<'EOF'
## Summary
- Decomposed 2865-line `mixer.rs` into `pages/mixer/` module directory with 5 files
- Introduced `ConnectionManager` with deterministic `Drop` for all background tasks
- Introduced `MixerState` struct — `connect_websocket` reduced from 44 parameters to 7
- Added `disposal_guard` for single-point disposal checking

## Changes
- `mod.rs` — MixerPage component (~500 lines, down from ~988)
- `state.rs` — MixerState struct bundling all 38+ signal pairs
- `connection.rs` — ConnectionManager with WS, reconnect, watchdog, token expiry, Drop
- `components.rs` — GlobalVolumeFader, StemsVolumeFader, ChannelList (moved as-is)
- `helpers.rs` — ws_send, types, constants, format_db, parse_track_name
- `push.rs` — subscribe_to_push, base64url_decode

## Verification
- Pre/post inventory counts verified (signal, try_set, set_interval, spawn_local, on_cleanup, Callback, Closure)
- All E2E tests pass without modification
- No UI changes, no REAPER changes, no protocol changes

## Test plan
- [x] All existing CI E2E tests pass (no modifications needed)
- [x] Inventory counts match baseline across all files
- [x] `cargo fmt --all --check` passes
- [x] MixerPage body under 500 lines

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Verify mergeable**

```bash
# Get PR number from the create output
gh api repos/zbynekdrlik/reaperiem/pulls/<NUMBER> --jq '{mergeable: .mergeable, mergeable_state: .mergeable_state}'
```

Wait for `mergeable: true` AND `mergeable_state: "clean"`.

- [ ] **Step 3: Present PR URL and STOP**

Report the PR URL with CI status and STOP. Do NOT merge.

---

## Verification Checklist (Issue #165 Success Criteria)

After all tasks are complete, verify against the issue's success criteria:

- [ ] `MixerPage` body under ~500 lines (run `wc -l iem-mixer/iem-ui/src/pages/mixer/mod.rs`)
- [ ] Zero `.set()` / `.update()` / `.try_set()` / `.try_update()` calls in the MixerPage setup code (event handler callbacks in the view template are OK)
- [ ] Every background task has a single owner (`ConnectionManager`) whose `Drop` is the teardown
- [ ] All existing E2E tests pass without modification
- [ ] `disposal_guard` short-circuits all background closures on disposal
