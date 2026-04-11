# PWA Self-Healing + Diagnostic Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Android PWA survive zombie WebSockets, report WASM panics to the server, and apply exponential backoff on reconnect — so the next "random freeze" becomes either a self-healed reconnect or a grep-able server log line.

**Architecture:** One new client module `iem-ui/src/lifecycle.rs` holds pure helpers (`backoff_delay_ms`, `is_stale`) and the `install_panic_hook()` function. The `connect_websocket` function in `iem-ui/src/pages/mixer.rs` is modified to track `last_frame_at`, reset/increment a `reconnect_attempt` counter, apply `backoff_delay_ms`, and run a 5-second watchdog interval that force-closes a socket that hasn't received frames for >30 seconds. A new public route `POST /api/client-error` in `iem-server/src/routes.rs` + `proxy.rs` deserializes a `ClientErrorReport` and logs it via `tracing::warn!`. No new UI components — the existing `.status-dot` and `.disconnected-banner` already handle disconnected state.

**Tech Stack:** Rust (wasm-bindgen, web-sys 0.3, leptos 0.7), Axum, tracing, Playwright.

**Spec:** `docs/superpowers/specs/2026-04-11-pwa-self-healing-design.md`

---

## Context

Issue #153 reports random UI freezes on Android PWA with no clear trigger. The reporter is the engineer (power user, heavy mixer use). The existing WebSocket reconnect loop at `iem-mixer/iem-ui/src/pages/mixer.rs:819-908` only detects CLOSED sockets, not zombie sockets where the socket is OPEN but no frames flow. The app currently installs `console_error_panic_hook::set_once()` at `iem-mixer/iem-ui/src/lib.rs:18` which prints panics to console but leaves the user with a blank or partially broken UI. Reconnect polls every 2 seconds forever without backoff, which on a mobile radio is a battery drain.

This plan ships ONE PR covering five items (panic hook replacement, watchdog, backoff, server endpoint, version bump). We intentionally do NOT add a new UI chip — the existing `.disconnected-banner` at `mixer.rs:1268-1271` is already wired to `connected` and fires when `set_connected(false)` is called by `onclose`.

---

## File Map

### Files to create

- `iem-mixer/iem-ui/src/lifecycle.rs` — pure helpers and `install_panic_hook()`

### Files to modify

- `iem-mixer/iem-ui/src/lib.rs` — add `pub mod lifecycle;`, replace `console_error_panic_hook::set_once()` with `lifecycle::install_panic_hook()`
- `iem-mixer/iem-ui/src/pages/mixer.rs` — add `last_frame_at` + `reconnect_attempt` state to `connect_websocket`, update `onmessage` to reset/refresh them, modify the reconnect interval closure to use `backoff_delay_ms`, add a second 5-second watchdog interval
- `iem-mixer/crates/iem-server/src/proxy.rs` — add `ClientErrorReport` struct and `client_error` handler
- `iem-mixer/crates/iem-server/src/routes.rs` — register `.route("/api/client-error", post(proxy::client_error))` on the public routes section
- Version files (6 files) — bump 1.141.0 → 1.142.0
- `README.md` — append v1.142.0 changelog entry

### Test files

- `iem-mixer/crates/iem-server/src/proxy.rs` — extend existing `#[cfg(test)] mod tests` with 5 new tests for `client_error`
- `iem-mixer/iem-ui/src/lifecycle.rs` — `#[cfg(test)] mod tests` with 3 tests for pure helpers
- `iem-mixer/e2e/tests/live/client-error-reporting.spec.ts` — post-deploy E2E that POSTs to the endpoint and greps the deployed log

**CI-only watchdog E2E is deferred.** Reasoning: a useful watchdog test requires killing the server-side WebSocket half mid-session, which means a test-only endpoint gated by `#[cfg(debug_assertions)]`. The release build on CI uses `--release`, so `debug_assertions` is OFF, meaning the test endpoint won't exist in the binary being tested. Building a second debug-profile binary just for this test doubles CI time. The unit tests for `is_stale` and `backoff_delay_ms` plus the post-deploy E2E for `/api/client-error` cover the risky code paths. We accept that end-to-end watchdog behavior is verified only via manual/field observation, documented in the post-deploy verification section below.

---

## Task 1: Version Bump (1.141.0 → 1.142.0)

**Files:**

- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`

- [ ] **Step 1: Bump all six version files**

Run this exact command:

```bash
sed -i 's/version = "1.141.0"/version = "1.142.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.141.0"/"version": "1.142.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify all files show the new version**

Run:

```bash
grep -l '1.142.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json | wc -l
```

Expected output: `6`

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.142.0"
```

---

## Task 2: Lifecycle module scaffolding + pure helpers

This task creates the new `lifecycle.rs` file with only the pure helpers (`backoff_delay_ms`, `is_stale`) and their unit tests. The panic hook and install function are added in Task 3.

**Files:**

- Create: `iem-mixer/iem-ui/src/lifecycle.rs`
- Modify: `iem-mixer/iem-ui/src/lib.rs` (add `pub mod lifecycle;`)

- [ ] **Step 1: Create the lifecycle.rs skeleton with pure helpers and failing tests**

Create `iem-mixer/iem-ui/src/lifecycle.rs` with exactly this content:

```rust
//! Lifecycle helpers: panic hook, WebSocket watchdog staleness check, reconnect backoff.
//!
//! Pure helpers live at module root; side-effecting install functions (panic hook)
//! are in sub-sections. Unit tests cover the pure helpers only — DOM mutation
//! paths rely on simplicity and end-to-end verification.

/// Return the reconnect delay (in ms) for the given attempt number.
///
/// Schedule: 2s, 4s, 8s, 15s, then 30s forever. Matches the design in
/// docs/superpowers/specs/2026-04-11-pwa-self-healing-design.md.
pub fn backoff_delay_ms(attempt: u32) -> u32 {
    match attempt {
        0 => 2_000,
        1 => 4_000,
        2 => 8_000,
        3 => 15_000,
        _ => 30_000,
    }
}

/// Return true if the time since `last_frame_ms` exceeds `threshold_ms`.
///
/// `now_ms` and `last_frame_ms` are millisecond timestamps (e.g. from
/// `js_sys::Date::now()`). Used by the WS watchdog to detect zombie sockets.
pub fn is_stale(last_frame_ms: f64, now_ms: f64, threshold_ms: f64) -> bool {
    now_ms - last_frame_ms > threshold_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_delay_ms_follows_schedule() {
        assert_eq!(backoff_delay_ms(0), 2_000);
        assert_eq!(backoff_delay_ms(1), 4_000);
        assert_eq!(backoff_delay_ms(2), 8_000);
        assert_eq!(backoff_delay_ms(3), 15_000);
        assert_eq!(backoff_delay_ms(4), 30_000);
        assert_eq!(backoff_delay_ms(5), 30_000);
        assert_eq!(backoff_delay_ms(100), 30_000);
    }

    #[test]
    fn is_stale_below_threshold_returns_false() {
        // 29_999 ms since last frame, threshold 30_000 ms → NOT stale
        assert!(!is_stale(100.0, 30_099.0, 30_000.0));
    }

    #[test]
    fn is_stale_above_threshold_returns_true() {
        // 30_001 ms since last frame, threshold 30_000 ms → stale
        assert!(is_stale(0.0, 30_001.0, 30_000.0));
    }
}
```

- [ ] **Step 2: Register the module in lib.rs**

Edit `iem-mixer/iem-ui/src/lib.rs`. Find this block near the top:

```rust
pub mod api;
pub mod auth;
pub mod components;
pub mod pages;
pub mod router;
```

Change it to:

```rust
pub mod api;
pub mod auth;
pub mod components;
pub mod lifecycle;
pub mod pages;
pub mod router;
```

- [ ] **Step 3: Run the new tests on CI**

Local cargo commands are forbidden by the project. Push to a WIP branch or just continue — the Tests job runs `cargo test --workspace` on CI. We'll verify all tests pass in Task 9.

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/iem-ui/src/lifecycle.rs iem-mixer/iem-ui/src/lib.rs
git commit -m "feat: add lifecycle module with backoff_delay_ms and is_stale helpers (#153)"
```

---

## Task 3: Custom panic hook

Replace the default `console_error_panic_hook::set_once()` with a custom hook that renders a reload overlay and POSTs diagnostics to `/api/client-error`.

**Files:**

- Modify: `iem-mixer/iem-ui/src/lifecycle.rs` (add `install_panic_hook` function and submodule)
- Modify: `iem-mixer/iem-ui/src/lib.rs` (replace panic hook call in `main`)

- [ ] **Step 1: Add the install_panic_hook function to lifecycle.rs**

Edit `iem-mixer/iem-ui/src/lifecycle.rs`. Append the following AFTER the existing `is_stale` function but BEFORE the `#[cfg(test)]` module:

```rust
// ---------------------------------------------------------------------------
// Panic hook — side-effecting, not unit-testable.
// ---------------------------------------------------------------------------

use wasm_bindgen::JsCast;

/// Install a custom panic hook that:
/// 1. Logs the panic to the browser console (via `console_error_panic_hook`).
/// 2. Renders a hardcoded red overlay into `document.body` with a "Reload" button.
/// 3. POSTs a `ClientErrorReport` to `/api/client-error` (fire-and-forget).
///
/// Replaces `console_error_panic_hook::set_once()`. Must be called BEFORE
/// `leptos::mount::mount_to_body` so any panic during mount is captured.
pub fn install_panic_hook() {
    // Keep the nice console-side formatting from console_error_panic_hook.
    console_error_panic_hook::set_once();

    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Delegate to the previous hook first so the console still gets a
        // structured backtrace. If that panics, we swallow it.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            prev(info);
        }));

        // Best-effort: render the overlay and POST diagnostics. Both are
        // wrapped in catch_unwind so a failure in one doesn't block the other
        // and nothing can re-panic inside the hook.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            render_panic_overlay(info);
        }));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            post_panic_report(info);
        }));
    }));
}

fn render_panic_overlay(info: &std::panic::PanicHookInfo<'_>) {
    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };
    let Some(body) = document.body() else { return };

    let message = format_panic_message(info);
    let summary = truncate_for_display(&message, 200);
    let version = iem_core::version_label();

    // Raw HTML bypasses the (now broken) Leptos reactive graph.
    let overlay_html = format!(
        r#"<div id="iem-panic-overlay" style="
            position:fixed;inset:0;z-index:2147483647;
            background:rgba(20,0,0,0.95);color:#fff;
            font-family:system-ui,-apple-system,sans-serif;
            display:flex;flex-direction:column;
            align-items:center;justify-content:center;
            padding:24px;text-align:center;">
            <h1 style="color:#ff4444;margin:0 0 16px 0;font-size:22px;">
                IEM Mixer encountered an error
            </h1>
            <p style="opacity:0.85;max-width:480px;margin:0 0 24px 0;
                font-family:monospace;font-size:13px;word-break:break-word;">
                {}
            </p>
            <button id="iem-panic-reload" style="
                padding:14px 32px;font-size:16px;
                background:#ff4444;color:#fff;border:0;border-radius:8px;
                cursor:pointer;">
                Reload
            </button>
            <p style="opacity:0.5;font-size:11px;margin-top:16px;">{}</p>
        </div>"#,
        html_escape(&summary),
        html_escape(&version),
    );

    body.set_inner_html(&overlay_html);

    // Wire the reload button.
    if let Some(btn) = document.get_element_by_id("iem-panic-reload") {
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            if let Some(w) = web_sys::window() {
                let _ = w.location().reload();
            }
        }) as Box<dyn FnMut()>);
        let _ = btn
            .dyn_ref::<web_sys::HtmlElement>()
            .map(|el| el.set_onclick(Some(closure.as_ref().unchecked_ref())));
        // Leak the closure — the overlay is a terminal state, the page will reload.
        closure.forget();
    }
}

fn post_panic_report(info: &std::panic::PanicHookInfo<'_>) {
    let Some(window) = web_sys::window() else { return };

    let url = window
        .location()
        .pathname()
        .unwrap_or_else(|_| String::from("/"));
    let user_agent = window.navigator().user_agent().unwrap_or_default();
    let message = format_panic_message(info);
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_default();

    let body = serde_json::json!({
        "panic_message": message,
        "version": iem_core::VERSION,
        "git_hash": iem_core::git_hash(),
        "url": url,
        "user_agent": user_agent,
        "location": location,
    });

    let body_string = match serde_json::to_string(&body) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Fire-and-forget fetch. We don't await; the page is about to reload anyway.
    let opts = web_sys::RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&wasm_bindgen::JsValue::from_str(&body_string));

    let headers = web_sys::Headers::new().unwrap_or_else(|_| {
        // If we can't even construct Headers, give up silently.
        web_sys::Headers::new().unwrap()
    });
    let _ = headers.set("content-type", "application/json");
    opts.set_headers(&headers);

    if let Ok(request) = web_sys::Request::new_with_str_and_init("/api/client-error", &opts) {
        let _ = window.fetch_with_request(&request);
        // Intentionally drop the returned Promise without awaiting.
    }
}

fn format_panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        String::from("(unknown panic payload)")
    }
}

fn truncate_for_display(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
```

- [ ] **Step 2: Add tests for the pure helper functions in the panic hook**

Still in `iem-mixer/iem-ui/src/lifecycle.rs`, find the existing `#[cfg(test)] mod tests` block. Add these test functions inside it, after `is_stale_above_threshold_returns_true`:

```rust
    #[test]
    fn truncate_for_display_below_limit_is_unchanged() {
        assert_eq!(truncate_for_display("short", 200), "short");
    }

    #[test]
    fn truncate_for_display_above_limit_is_truncated_with_ellipsis() {
        let long = "a".repeat(250);
        let result = truncate_for_display(&long, 200);
        assert_eq!(result.chars().count(), 201); // 200 + ellipsis
        assert!(result.ends_with('…'));
    }

    #[test]
    fn html_escape_replaces_all_special_chars() {
        assert_eq!(
            html_escape(r#"<script>alert("&")</script>"#),
            "&lt;script&gt;alert(&quot;&amp;&quot;)&lt;/script&gt;"
        );
    }
```

- [ ] **Step 3: Replace the existing panic hook call in lib.rs**

Edit `iem-mixer/iem-ui/src/lib.rs`. Find this block:

```rust
#[wasm_bindgen(start)]
pub fn main() {
    // Better panic messages in console
    console_error_panic_hook::set_once();

    // Mount the app
    leptos::mount::mount_to_body(router::App);
```

Change it to:

```rust
#[wasm_bindgen(start)]
pub fn main() {
    // Install panic hook with overlay + server reporting (#153)
    lifecycle::install_panic_hook();

    // Mount the app
    leptos::mount::mount_to_body(router::App);
```

- [ ] **Step 4: Verify iem-core exposes `VERSION` const and `git_hash()` function**

Already verified at plan-writing time: `iem-mixer/crates/iem-core/src/lib.rs:29` has `pub const VERSION: &str = env!("CARGO_PKG_VERSION");` and line 33 has `pub fn git_hash() -> &'static str { option_env!("GIT_HASH").unwrap_or("unknown") }`. Both are accessible as `iem_core::VERSION` and `iem_core::git_hash()` — the code in Step 1 matches this. No action needed for this step; keep it as a checklist item to re-confirm if a future refactor breaks these symbols.

- [ ] **Step 5: Add serde_json import check**

Run:

```bash
grep -n 'serde_json' iem-mixer/iem-ui/Cargo.toml
```

Expected: serde_json is already in the dependencies (confirmed from the Cargo.toml exploration: `serde_json = "1.0"`). If not, stop and add it.

- [ ] **Step 6: Commit**

```bash
git add iem-mixer/iem-ui/src/lifecycle.rs iem-mixer/iem-ui/src/lib.rs
git commit -m "feat: panic hook renders reload overlay and POSTs diagnostics (#153)"
```

---

## Task 4: WebSocket liveness watchdog + exponential backoff

Add `last_frame_at` tracking, a 5-second watchdog interval, a `reconnect_attempt` counter, and backoff-driven reconnect delays to `connect_websocket` in `mixer.rs`.

**Files:**

- Modify: `iem-mixer/iem-ui/src/pages/mixer.rs` (lines ~60-112, ~160-390, ~819-908)

- [ ] **Step 1: Add Rc<Cell<f64>> for last_frame_at and Rc<Cell<u32>> for reconnect_attempt to connect_websocket**

Edit `iem-mixer/iem-ui/src/pages/mixer.rs`. Find the `connect_websocket` function body near the top (after the `fn connect_websocket(` signature block, before the first `let ws = ...` or similar WebSocket construction). Locate the existing state declarations — look for lines near the beginning of the function that use `Rc<Cell<_>>` or `Rc::new(RefCell::new(...))`.

Add these two declarations at the top of the function body (right after the opening `{`):

```rust
    // Watchdog state — updated on every received frame, checked every 5s.
    // Wrapped in Rc<Cell> so closures can share mutation without RefCell borrows.
    let last_frame_at = std::rc::Rc::new(std::cell::Cell::new(js_sys::Date::now()));
    // Reconnect attempt counter for exponential backoff. Resets to 0 on first
    // frame received on a new socket. Incremented on every onclose.
    let reconnect_attempt = std::rc::Rc::new(std::cell::Cell::new(0u32));
```

- [ ] **Step 2: Update onmessage to refresh last_frame_at and reset reconnect_attempt on the first frame**

Still in `mixer.rs`, find the `onmessage` closure construction inside `connect_websocket` (around line 160-218, identifiable by `let onmessage = Closure::wrap(Box::new(move |e: web_sys::MessageEvent|`).

The closure uses `move` so it captures by value. We need clones of `last_frame_at` and `reconnect_attempt` that are moved INTO the closure. Add these clones BEFORE the `let onmessage = ...` line:

```rust
    let last_frame_at_msg = last_frame_at.clone();
    let reconnect_attempt_msg = reconnect_attempt.clone();
```

Then inside the onmessage closure body, at the very top of the closure (before `if let Some(text) = e.data().as_string()`), add:

```rust
        // Refresh liveness timestamp — any received frame counts as "alive".
        last_frame_at_msg.set(js_sys::Date::now());
        // Reset backoff counter — we've successfully received data.
        reconnect_attempt_msg.set(0);
```

- [ ] **Step 3: Update onclose to increment reconnect_attempt**

Still in `mixer.rs`, find the `onclose` closure construction (around line 380-390, starts with `let onclose = Closure::wrap(Box::new(move |_: web_sys::CloseEvent|`).

Before the `let onclose = ...` line, add:

```rust
    let reconnect_attempt_close = reconnect_attempt.clone();
```

Then inside the onclose closure body, after the existing `fail_count_close.set(fail_count_close.get() + 1);` line, add:

```rust
        reconnect_attempt_close.set(reconnect_attempt_close.get() + 1);
```

- [ ] **Step 4: Modify the reconnect interval to apply exponential backoff**

Still in `mixer.rs`, find the reconnect interval setup (around line 819-908). Look for:

```rust
    let interval_id = web_sys::window()
        .unwrap()
        .set_interval_with_callback_and_timeout_and_arguments_0(
            reconnect_closure.as_ref().unchecked_ref(),
            2000,  // Reconnect check every 2 seconds
        )
        .unwrap();
```

The strategy: keep the 2-second TICK (the interval fires every 2s), but gate the reconnect ACTION on a backoff timer stored in `Rc<Cell<f64>>`. The closure records when the last reconnect attempt was made; on each tick it checks whether `backoff_delay_ms(reconnect_attempt)` has elapsed since that timestamp.

Add this state BEFORE the `let reconnect_closure = Closure::wrap(...)` line:

```rust
    // Timestamp of last reconnect attempt — used with backoff schedule.
    let last_reconnect_attempt_at = std::rc::Rc::new(std::cell::Cell::new(0.0_f64));
    let reconnect_attempt_tick = reconnect_attempt.clone();
    let last_reconnect_attempt_at_tick = last_reconnect_attempt_at.clone();
```

Then INSIDE the `reconnect_closure`, at the top of the closure body (before the existing `let needs_reconnect = ...`), add:

```rust
        // Backoff gate: skip this tick if the scheduled delay hasn't elapsed.
        let now_ms = js_sys::Date::now();
        let attempt = reconnect_attempt_tick.get();
        let delay_ms = crate::lifecycle::backoff_delay_ms(attempt) as f64;
        let last_attempt = last_reconnect_attempt_at_tick.get();
        if last_attempt > 0.0 && (now_ms - last_attempt) < delay_ms {
            return;
        }
```

And just before the existing call to `connect_websocket(...)` inside `if needs_reconnect { ... }`, add:

```rust
            last_reconnect_attempt_at_tick.set(now_ms);
```

- [ ] **Step 5: Add the 5-second watchdog interval**

Still in `mixer.rs`, after the existing reconnect interval setup but BEFORE the existing `on_cleanup(move || { ... })` block, add a second interval:

```rust
    // Watchdog: check every 5s whether the socket has received any frame in
    // the last 30s. If not, force-close it — the existing onclose handler
    // will set connected=false (triggering the .disconnected-banner) and
    // the reconnect loop will open a new socket. Catches zombie sockets
    // where ready_state == OPEN but no data flows. See #153.
    let last_frame_at_watch = last_frame_at.clone();
    let ws_watch = ws;
    let watchdog_closure = Closure::wrap(Box::new(move || {
        let Some(socket) = ws_watch.get_untracked() else { return };
        if socket.ready_state() != web_sys::WebSocket::OPEN {
            return;
        }
        let now = js_sys::Date::now();
        if crate::lifecycle::is_stale(last_frame_at_watch.get(), now, 30_000.0) {
            web_sys::console::warn_1(
                &"WS watchdog: no frames for >30s, force-closing socket".into(),
            );
            let _ = socket.close();
        }
    }) as Box<dyn FnMut()>);

    let watchdog_interval_id = web_sys::window()
        .unwrap()
        .set_interval_with_callback_and_timeout_and_arguments_0(
            watchdog_closure.as_ref().unchecked_ref(),
            5_000,
        )
        .unwrap();
    watchdog_closure.forget();
```

- [ ] **Step 6: Extend on_cleanup to clear the watchdog interval too**

Find the existing `on_cleanup(move || { ... })` block immediately below the reconnect interval setup:

```rust
    on_cleanup(move || {
        if let Some(w) = web_sys::window() {
            w.clear_interval_with_handle(interval_id);
        }
    });
```

Change it to:

```rust
    on_cleanup(move || {
        if let Some(w) = web_sys::window() {
            w.clear_interval_with_handle(interval_id);
            w.clear_interval_with_handle(watchdog_interval_id);
        }
    });
```

- [ ] **Step 7: Sanity-check closure borrowing**

The `ws` signal is captured in multiple places (existing reconnect closure, new watchdog closure). In Leptos 0.7, `ReadSignal<T>` is `Copy`, so capturing it by move in a second closure is fine without clone. If the compiler complains that `ws` has been moved, the fix is to add `let ws_watch = ws;` before the watchdog closure (already done in Step 5). No other closure borrowing changes should be needed.

- [ ] **Step 8: Commit**

```bash
git add iem-mixer/iem-ui/src/pages/mixer.rs
git commit -m "feat: WS liveness watchdog + exponential reconnect backoff (#153)"
```

---

## Task 5: Server `/api/client-error` endpoint

Add the `ClientErrorReport` struct, the `client_error` handler, its unit tests, and route registration.

**Files:**

- Modify: `iem-mixer/crates/iem-server/src/proxy.rs`
- Modify: `iem-mixer/crates/iem-server/src/routes.rs`

- [ ] **Step 1: Write failing unit tests for the client_error handler in proxy.rs**

Open `iem-mixer/crates/iem-server/src/proxy.rs`. Find the existing `#[cfg(test)] mod tests { ... }` block at the bottom (there should already be one from the #150 fix). Add these tests inside it, alongside the existing ones:

```rust
    // ---- /api/client-error tests (#153) ----

    #[tokio::test]
    async fn client_error_accepts_minimal_body() {
        let report = ClientErrorReport {
            panic_message: "boom".to_string(),
            version: None,
            git_hash: None,
            url: None,
            user_agent: None,
            location: None,
            backtrace: None,
        };
        let response = client_error(axum::Json(report)).await;
        assert_eq!(response.into_response().status(), axum::http::StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn client_error_accepts_full_body() {
        let report = ClientErrorReport {
            panic_message: "assertion failed at line 42".to_string(),
            version: Some("1.142.0".to_string()),
            git_hash: Some("abc1234".to_string()),
            url: Some("/engineer".to_string()),
            user_agent: Some("Mozilla/5.0".to_string()),
            location: Some("iem-ui/src/pages/mixer.rs:456:9".to_string()),
            backtrace: Some("  at foo\n  at bar".to_string()),
        };
        let response = client_error(axum::Json(report)).await;
        assert_eq!(response.into_response().status(), axum::http::StatusCode::NO_CONTENT);
    }

    #[test]
    fn client_error_report_deserialize_minimal() {
        // Only panic_message is required.
        let json = r#"{"panic_message":"boom"}"#;
        let report: ClientErrorReport = serde_json::from_str(json).expect("parse");
        assert_eq!(report.panic_message, "boom");
        assert!(report.version.is_none());
        assert!(report.git_hash.is_none());
    }

    #[test]
    fn client_error_report_deserialize_missing_panic_message_fails() {
        // panic_message is required; deserialization must fail without it.
        let json = r#"{"version":"1.142.0"}"#;
        let result: Result<ClientErrorReport, _> = serde_json::from_str(json);
        assert!(result.is_err(), "should reject body with no panic_message");
    }

    #[test]
    fn client_error_report_deserialize_full_body() {
        let json = r#"{
            "panic_message":"boom",
            "version":"1.142.0",
            "git_hash":"abc",
            "url":"/engineer",
            "user_agent":"UA",
            "location":"file:1:1",
            "backtrace":"trace"
        }"#;
        let report: ClientErrorReport = serde_json::from_str(json).expect("parse");
        assert_eq!(report.panic_message, "boom");
        assert_eq!(report.version.as_deref(), Some("1.142.0"));
        assert_eq!(report.git_hash.as_deref(), Some("abc"));
    }
```

These tests will not compile until `ClientErrorReport` and `client_error` exist — that's Step 2.

- [ ] **Step 2: Add the ClientErrorReport struct and client_error handler**

Still in `iem-mixer/crates/iem-server/src/proxy.rs`, near the TOP of the file (after the existing `use` imports and before the first handler function), add:

```rust
/// Error report sent by the WASM client when a panic occurs.
///
/// All fields except `panic_message` are optional so that degraded clients
/// (e.g. broken Leptos graph, missing window globals) can still send a report.
#[derive(Debug, serde::Deserialize)]
pub struct ClientErrorReport {
    pub panic_message: String,
    pub version: Option<String>,
    pub git_hash: Option<String>,
    pub url: Option<String>,
    pub user_agent: Option<String>,
    pub location: Option<String>,
    pub backtrace: Option<String>,
}

/// POST /api/client-error — receive a client-side panic report and log it.
///
/// Public route (no auth) because panics may occur when auth itself is broken.
/// Request body is capped at 10 KB by the route-level `DefaultBodyLimit` layer
/// added in routes.rs. Always returns 204 No Content on success; malformed
/// bodies return 400 via Axum's default JSON rejection.
///
/// Logs via `tracing::warn!` with a structured `client_error` prefix so the
/// line is grep-able: `journalctl -u iem-mixer | grep client_error`.
pub async fn client_error(
    axum::Json(report): axum::Json<ClientErrorReport>,
) -> axum::http::StatusCode {
    tracing::warn!(
        target: "iem_server::client_error",
        version = report.version.as_deref().unwrap_or("?"),
        git_hash = report.git_hash.as_deref().unwrap_or("?"),
        url = report.url.as_deref().unwrap_or("?"),
        user_agent = report.user_agent.as_deref().unwrap_or("?"),
        location = report.location.as_deref().unwrap_or("?"),
        panic = %report.panic_message,
        "client_error",
    );
    if let Some(bt) = report.backtrace.as_deref() {
        // Log backtrace on a separate line to keep the main log line grep-friendly.
        tracing::warn!(target: "iem_server::client_error", backtrace = %bt, "client_error_backtrace");
    }
    axum::http::StatusCode::NO_CONTENT
}
```

- [ ] **Step 3: Verify the tests now pass by reviewing the types**

The tests call `client_error(axum::Json(report))` and expect `.into_response().status() == NO_CONTENT`. The handler returns `axum::http::StatusCode::NO_CONTENT`, which implements `IntoResponse` and produces a 204 response with an empty body. The test pattern matches.

Double-check the `use` imports at the top of `proxy.rs` already include `axum::Json` (from existing handlers). If only a partial `use axum::{ ... }` is present and `Json` is missing, add it. From the exploration: `use axum::{ Json, body::Body, extract::{Path, Query, State}, http::{Method, StatusCode}, response::{IntoResponse, Response}, };` — `Json` is already imported.

- [ ] **Step 4: Register the route in routes.rs**

Open `iem-mixer/crates/iem-server/src/routes.rs`. Find the `api_routes` function (around line 46). Locate the PUBLIC routes section:

```rust
pub fn api_routes(_state: AppState) -> Router<AppState> {
    Router::new()
        // PUBLIC ROUTES (no auth required):
        .route("/api/version", get(get_version))
        .route("/api/auth", post(auth::login))
        .route("/api/members", get(get_members))
        .route("/api/members/{member_id}/photo", get(get_photo))
        .route("/api/push/vapid-key", get(get_vapid_key))
```

Add one line at the end of the public routes section (before the protected routes block). Change to:

```rust
pub fn api_routes(_state: AppState) -> Router<AppState> {
    Router::new()
        // PUBLIC ROUTES (no auth required):
        .route("/api/version", get(get_version))
        .route("/api/auth", post(auth::login))
        .route("/api/members", get(get_members))
        .route("/api/members/{member_id}/photo", get(get_photo))
        .route("/api/push/vapid-key", get(get_vapid_key))
        .route("/api/client-error", post(proxy::client_error))
            .layer(axum::extract::DefaultBodyLimit::max(10 * 1024))
```

Wait — the `.layer()` there would apply to the ENTIRE router above it, not just the one route. That's wrong. Instead, use a per-route body limit via `MethodRouter`:

Replace the single line insertion with this more targeted approach. Change:

```rust
        .route("/api/push/vapid-key", get(get_vapid_key))
```

to:

```rust
        .route("/api/push/vapid-key", get(get_vapid_key))
        .route(
            "/api/client-error",
            post(proxy::client_error)
                .layer(axum::extract::DefaultBodyLimit::max(10 * 1024)),
        )
```

This attaches `DefaultBodyLimit::max(10 * 1024)` only to the POST handler for `/api/client-error`, leaving other routes untouched.

- [ ] **Step 5: Verify imports in routes.rs**

At the top of `routes.rs`, check the `use` block includes `post` and `proxy`. From the exploration:

```rust
use axum::{ routing::{get, post}, Router };
use crate::{ auth, proxy, ... };
```

If `post` or `proxy` is missing, add it. Also add `use axum::extract::DefaultBodyLimit;` at the top if it isn't already there (or keep the fully-qualified `axum::extract::DefaultBodyLimit::max(10 * 1024)` inline as written in Step 4 — both work).

- [ ] **Step 6: Commit**

```bash
git add iem-mixer/crates/iem-server/src/proxy.rs iem-mixer/crates/iem-server/src/routes.rs
git commit -m "feat: POST /api/client-error endpoint for WASM panic reports (#153)"
```

---

## Task 6: Post-deploy Playwright E2E for client error reporting

Verifies the endpoint works end-to-end on the real deployed system and that the log line reaches the server journal.

**Files:**

- Create: `iem-mixer/e2e/tests/live/client-error-reporting.spec.ts`

- [ ] **Step 1: Check how the existing live tests run against the deployed target**

Read `iem-mixer/e2e/playwright.config.ts` to see how `baseURL` is set and which tests are selected for the deploy job. Read one existing short live test (e.g. `iem-mixer/e2e/tests/live/alert.spec.ts`) for the import pattern and any auth helpers.

Run:

```bash
head -60 iem-mixer/e2e/tests/live/alert.spec.ts
head -40 iem-mixer/e2e/playwright.config.ts
```

Expected: `baseURL` resolves from `E2E_BASE_URL` env var (e.g. `http://localhost` during deploy or `http://10.77.9.231` for manual runs). The existing `loginAs` helper uses `page.request.post("/api/auth")` + `page.evaluate(localStorage.setItem(...))`.

- [ ] **Step 2: Write the E2E test**

Create `iem-mixer/e2e/tests/live/client-error-reporting.spec.ts`:

```typescript
import { test, expect } from "@playwright/test";
import { execSync } from "child_process";

/**
 * Post-deploy E2E for POST /api/client-error (#153).
 *
 * Posts a marker panic message from the browser, asserts the server returns
 * 204, then SSHs to iem.lan and greps the systemd journal for the marker
 * within the last minute. Proves the endpoint is wired, the body limit works,
 * and the log line actually reaches systemd.
 *
 * Requires: SSH key for newlevel@iem.lan configured on the runner (already
 * set up by the deploy workflow).
 */
test.describe("Client error reporting (#153)", () => {
  test("marker panic report reaches journalctl within 30s", async ({
    page,
  }) => {
    const marker = `e2e-marker-${Date.now()}`;

    // Navigate to the base URL — no auth needed, the endpoint is public.
    await page.goto("/");

    // POST from the browser context so we exercise the real CORS + path.
    const status = await page.evaluate(async (m) => {
      const res = await fetch("/api/client-error", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          panic_message: `panic-body ${m}`,
          version: "1.142.0",
          git_hash: "e2e",
          url: "/e2e-test",
          user_agent: navigator.userAgent,
          location: "e2e:1:1",
        }),
      });
      return res.status;
    }, marker);

    expect(status).toBe(204);

    // Give the server a moment to flush the tracing subscriber.
    await page.waitForTimeout(1500);

    // SSH to iem.lan and grep the journal. The journalctl command is
    // idempotent; we search the last 2 minutes for our unique marker.
    //
    // Note: the app logs to stdout which is captured by whatever is running
    // the binary (interactive session, Task Scheduler, or systemd). On the
    // current deploy target the app runs from a Scheduled Task, which writes
    // stdout to a rotating log file. We support both targets below.
    const sshCmd = [
      `ssh -o BatchMode=yes -o StrictHostKeyChecking=no newlevel@iem.lan`,
      `"powershell -Command \\"Get-Content -Path '\\$env:LOCALAPPDATA\\IEM Mixer\\logs\\*.log' -Tail 500 -ErrorAction SilentlyContinue | Select-String '${marker}'\\""`,
    ].join(" ");

    let found = "";
    try {
      found = execSync(sshCmd, { timeout: 15_000 }).toString();
    } catch (err) {
      throw new Error(
        `SSH grep for marker '${marker}' failed: ${(err as Error).message}\n` +
          `Command: ${sshCmd}`,
      );
    }

    expect(found).toContain(marker);
    expect(found).toContain("client_error");
  });

  test("oversize body is rejected with 413", async ({ page }) => {
    await page.goto("/");

    const status = await page.evaluate(async () => {
      // 12 KB body — should exceed the 10 KB limit.
      const big = "x".repeat(12 * 1024);
      const res = await fetch("/api/client-error", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ panic_message: big }),
      });
      return res.status;
    });

    expect(status).toBe(413);
  });
});
```

- [ ] **Step 3: Verify the log file path on iem.lan before relying on it**

The SSH grep uses `%LOCALAPPDATA%\IEM Mixer\logs\*.log`. Before pushing, verify this path exists and contains recent log output. Run:

```bash
ssh newlevel@iem.lan 'powershell -Command "Get-ChildItem -Path \"$env:LOCALAPPDATA\\IEM Mixer\\logs\\\" -ErrorAction SilentlyContinue"'
```

Expected: a listing of one or more `.log` files with recent modification times.

If the path does not exist, the app logs to stdout without file redirection and we need a different capture mechanism. In that case, stop this task and check the actual stdout sink:

```bash
ssh newlevel@iem.lan 'powershell -Command "Get-ScheduledTask -TaskName \"*IEM*\" | Select-Object -ExpandProperty Actions | Format-List"'
```

The `-RedirectStandardOutput` argument (or absence of one) determines where logs go. Adjust the `Get-Content` path in the test accordingly. If logging is truly ephemeral (only `Write-Host` output in an interactive session), add a file log appender in a follow-up sub-task before proceeding.

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/e2e/tests/live/client-error-reporting.spec.ts
git commit -m "test: post-deploy E2E for /api/client-error with journal grep (#153)"
```

---

## Task 7: Update README changelog

**Files:**

- Modify: `README.md`

- [ ] **Step 1: Read the existing changelog section**

Find the changelog at the top of `README.md`. The format from CLAUDE.md:

```markdown
### v1.X.0 (YYYY-MM-DD)

- **Feature**: ...
- **Fix**: ...
```

- [ ] **Step 2: Insert the v1.142.0 entry**

Add this block IMMEDIATELY above the existing newest changelog entry (which should be v1.141.0):

```markdown
### v1.142.0 (2026-04-11)

- **Fix**: PWA self-healing — WebSocket watchdog force-reconnects after 30s of silence, catches zombie sockets where the connection is OPEN but no frames flow. Replaces unbounded 2-second reconnect polling with exponential backoff (2s → 4s → 8s → 15s → 30s cap). (#153)
- **Feature**: WASM panic hook renders a reload banner when the UI crashes and POSTs diagnostics to the new `/api/client-error` endpoint. Panic reports are visible via `journalctl -u iem-mixer | grep client_error` (or the equivalent Windows log capture). (#153)
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: changelog for v1.142.0 PWA self-healing (#153)"
```

---

## Task 8: Local lint and format checks

Before pushing, run the ONE allowed local check per airuleset `ci-push-discipline.md`.

- [ ] **Step 1: Run cargo fmt --check on the iem-mixer workspace**

```bash
cd iem-mixer && cargo fmt --all --check
```

Expected: exit 0, no output. If it fails, run `cargo fmt --all` and stage the reformatted files into a new commit (NOT amended):

```bash
cd iem-mixer && cargo fmt --all
cd ..
git add iem-mixer/
git commit -m "chore: cargo fmt (#153)"
```

- [ ] **Step 2: Do NOT run cargo clippy, cargo test, cargo build, or cargo check locally**

These are forbidden by the project's block-cargo hook and by airuleset. They run on CI.

---

## Task 9: Push and monitor CI

- [ ] **Step 1: Check for in-progress runs before pushing**

```bash
gh run list --branch dev --status in_progress --limit 3
```

If any are active for recent commits, that's fine — the push will cancel them via concurrency groups.

- [ ] **Step 2: Push to dev**

```bash
git push origin dev
```

- [ ] **Step 3: Monitor CI to terminal state**

```bash
sleep 120 && gh run list --branch dev --limit 3
```

Then check the latest run:

```bash
gh run view <run-id>
```

Wait for ALL jobs to reach terminal state. Expected green jobs:

- Lint & Format
- Test Integrity Check
- Build VBAN VST3
- Mutation Testing (cargo-mutants --in-diff)
- Build WASM Frontend
- Tests (includes the new lifecycle.rs and proxy.rs unit tests)
- E2E Tests
- Build Tauri (Windows)
- Deploy to iem.lan
- (post-deploy) — the new `client-error-reporting.spec.ts` should run as part of the deploy E2E tier

Verify-Version-Bump should show as SKIPPED on `dev` (only runs on PR to main).

- [ ] **Step 4: If any job fails, investigate with `gh run view <id> --log-failed` before doing anything else**

Do NOT blindly rerun. Common expected issues:

- **lifecycle.rs doesn't compile**: likely a clone-needed-for-move error in `install_panic_hook` or `post_panic_report`. Fix and push as a NEW commit.
- **mixer.rs doesn't compile**: the closure captures for `last_frame_at`, `reconnect_attempt`, or `ws_watch` don't line up. Read the rustc error carefully and add `.clone()` where it suggests, or move a declaration up.
- **Mutation testing finds a surviving mutant in lifecycle.rs**: add a targeted test that kills the mutant. For example, if `backoff_delay_ms` is mutated `attempt => 30_000` and tests still pass, that means the explicit `attempt => 2_000` branches aren't being checked for exactness — but our test already checks `assert_eq!(backoff_delay_ms(0), 2_000)` etc, so this shouldn't happen.
- **E2E ws-watchdog test is missing**: we intentionally didn't create it (see File Map section). If CI expects it, check if the e2e test runner auto-discovers files or if there's an explicit list.
- **client-error-reporting.spec.ts fails on the SSH grep step**: the log file path is wrong. Task 6 Step 3 should have caught this — if it didn't, SSH to iem.lan manually and find where the app logs actually go, then update the test path in a NEW commit.

Fix ALL issues in ONE commit per CI iteration, not many small fixes.

- [ ] **Step 5: After all CI jobs are green, move to Task 10**

---

## Task 10: Create PR and verify deployment

- [ ] **Step 1: Create the PR from dev to main**

```bash
gh pr create --title "fix: PWA self-healing + panic reporting (v1.142.0) (#153)" --body "$(cat <<'EOF'
## Summary

- Replaces the default panic hook with a custom one that renders a reload banner and POSTs diagnostics to a new public `POST /api/client-error` endpoint — silent WASM panics now become visible in the server log.
- Adds a 5-second WebSocket watchdog that force-closes sockets which have received no frames for >30s. The existing `.disconnected-banner` then fires and the reconnect loop opens a new socket. Catches zombie sockets where `ready_state == OPEN` but no data flows.
- Replaces unbounded 2-second reconnect polling with exponential backoff (2s → 4s → 8s → 15s → 30s cap) driven by a new `lifecycle::backoff_delay_ms` pure helper. Gentler on mobile radios.
- Adds 8 new Rust unit tests (3 lifecycle helpers, 5 client_error handler/types) and 2 post-deploy Playwright E2E tests (normal path + oversize body rejection).
- No new UI components — reuses the existing `.status-dot` and `.disconnected-banner` at `mixer.rs:1245-1271`.

Fixes #153 (or at least: gives us the field-data tooling to fix it).

## Test plan

- [ ] CI green, all 9 jobs pass
- [ ] Post-deploy `client-error-reporting.spec.ts` passes on the real iem.lan target
- [ ] Manual: on Android PWA after deploy, verify no regressions in normal operation
- [ ] Manual: disable network briefly (airplane mode on/off), confirm reconnect happens and UI recovers
- [ ] Manual: wait for a real freeze report, check `journalctl | grep client_error` on iem.lan
EOF
)"
```

- [ ] **Step 2: Verify the PR is mergeable**

```bash
gh pr view --json number --jq '.number' | xargs -I{} gh api repos/zbynekdrlik/reaperiem/pulls/{} --jq '{mergeable: .mergeable, mergeable_state: .mergeable_state}'
```

Expected: `{mergeable: true, mergeable_state: "clean"}`. If "behind", sync with main. If "blocked" or "dirty", investigate.

- [ ] **Step 3: Report the green PR URL to the user and WAIT for explicit merge approval**

Per airuleset `pr-merge-policy.md`: never merge without explicit user text approval. Present the PR URL and wait.

- [ ] **Step 4: After merge, monitor main CI run to completion**

Main CI includes the deploy job. Watch it with:

```bash
gh run list --branch main --limit 3
gh run view <main-run-id>
```

Expected: all 10 jobs green (same list as dev, plus Verify Version Bump on PR).

- [ ] **Step 5: Functional verification on the live system**

After deploy completes:

```bash
curl http://10.77.9.231/api/version
```

Expected: `{"version":"1.142.0", ...}` with `branch: main`.

Then verify the new endpoint responds:

```bash
curl -sS -X POST http://10.77.9.231/api/client-error \
  -H "content-type: application/json" \
  -d '{"panic_message":"manual verification"}' \
  -o /dev/null -w "%{http_code}\n"
```

Expected: `204`.

Then verify the log line reached the journal:

```bash
ssh newlevel@iem.lan 'powershell -Command "Get-Content -Path \"$env:LOCALAPPDATA\\IEM Mixer\\logs\\*.log\" -Tail 200 -ErrorAction SilentlyContinue | Select-String client_error"'
```

Expected: at least one line containing `client_error` with the marker from the curl command (or the E2E test).

- [ ] **Step 6: Send the completion report to the user**

Use the format from airuleset `completion-report.md`. The "E2E test coverage" table should have rows for:

| Feature/Fix | E2E Test File | What It Verifies |
| --- | --- | --- |
| Client error endpoint | e2e/tests/live/client-error-reporting.spec.ts | Browser POSTs to /api/client-error → 204 → journal log line contains marker |
| Oversize body rejection | e2e/tests/live/client-error-reporting.spec.ts | 12 KB body → 413 Payload Too Large |
| Watchdog + reconnect behavior | (unit tests only) | Manual/field verification only — see Task 9 File Map note |

Flag the watchdog E2E gap honestly — do NOT list it as ✅ in the table if there's no automated coverage.

---

## Task Dependencies

```
Task 1 (version bump)              ─┐
Task 2 (lifecycle scaffold + helpers) ─┤
Task 3 (panic hook)                ─┼── depends on Task 2
Task 4 (watchdog + backoff in mixer)─┤── depends on Task 2 (imports lifecycle::backoff_delay_ms)
Task 5 (server endpoint + tests)   ─┘
Task 6 (E2E test)                  ── depends on Task 5
Task 7 (changelog)                 ── independent
Task 8 (local format check)        ── after Tasks 1-7
Task 9 (push + monitor CI)         ── after Task 8
Task 10 (PR + deploy verification) ── after Task 9 green
```

Tasks 1, 2, 5, 7 can run in parallel (no shared files). Tasks 3 and 4 both modify lifecycle.rs but only Task 3 adds to lifecycle.rs after Task 2 — if run sequentially, no conflict.

---

## Verification summary

After CI is green and PR is merged:

1. All 10 main-branch CI jobs pass
2. `curl http://10.77.9.231/api/version` returns 1.142.0
3. `curl -X POST .../api/client-error` returns 204
4. `ssh iem.lan '...Get-Content...' | Select-String client_error` shows the marker log line
5. Post-deploy `client-error-reporting.spec.ts` passes
6. Field verification: wait for a real Android freeze report and check the journal

Any freeze report after deploy should now either:
- Self-heal via the watchdog (engineer doesn't notice; only the `.disconnected-banner` flickered), OR
- Trigger the panic hook, render the reload banner, and log a `client_error` line in the journal we can inspect

If neither happens and freezes continue, the next PR should add either a heartbeat/ping protocol or instrumentation around the suspected talkback wedge — guided by whatever the first wave of field data shows.
