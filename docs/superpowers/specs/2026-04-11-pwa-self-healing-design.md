# PWA Self-Healing + Diagnostic Layer Design (#153)

**Issue:** #153 — "PWA app frequently stuck and needs to be killed on phone, frequently also when talk is activated, but also in many other situations"

**Reporter context:** Engineer reports random, no-clear-trigger UI freezes on Android PWA (engineer view, used heavily). Symptom: UI totally frozen, no taps work. Must kill and reopen the app.

**Date:** 2026-04-11

---

## Goal

Convert silent freezes on mobile PWAs into (a) recoverable events with automatic self-healing and (b) server-logged diagnostic records. We cannot reproduce the bug, so this design both fixes the highest-confidence silent-freeze causes AND captures diagnostic data for the rest. After a few days of field data we will know which remaining suspects to chase in follow-up PRs.

## Non-Goals

- Fixing the talkback state-machine wedge (separate PR once field data confirms involvement)
- iOS AudioContext gesture timing (user reported Android; iOS fix is a separate PR)
- WebSocket ping/pong protocol (watchdog at the message layer is enough for now)
- In-app error dashboard (user chose stdout-only logging)
- Offline / service worker improvements

## Architecture

One new client module owns the three client-side concerns (panic hook, WS watchdog, reconnect backoff). One new server route handles error reports. `console_error_panic_hook` is already in `iem-ui/Cargo.toml` — no new dependencies.

**No new chip component needed.** The codebase already has `.status-dot` + `.disconnected-banner` at `iem-mixer/iem-ui/src/pages/mixer.rs:1245-1271` that react to the existing `connected` signal. The watchdog force-closes a zombie socket, which triggers `onclose → set_connected(false)` → the existing banner appears. Reuse, don't duplicate.

**New files:**

- `iem-mixer/iem-ui/src/lifecycle.rs` — panic hook install function, WS liveness helpers, backoff schedule (pure helpers and install functions)

**Modified files:**

- `iem-mixer/iem-ui/src/lib.rs` — declare `pub mod lifecycle;` and call `lifecycle::install_panic_hook()` in `main()` (replacing the existing `console_error_panic_hook::set_once()`)
- `iem-mixer/iem-ui/src/pages/mixer.rs` — integrate watchdog + backoff into existing reconnect loop at `connect_websocket`; track `last_frame_at` timestamp in `onmessage`
- `iem-mixer/crates/iem-server/src/routes.rs` — add `POST /api/client-error` to the public routes section (alongside `/api/version`, `/api/auth`, etc.)
- `iem-mixer/crates/iem-server/src/proxy.rs` — add the `client_error` handler function

## Components

### 1. Panic hook (`lifecycle::install_panic_hook`)

Installed in `main()` BEFORE `leptos::mount_to_body`. Replaces the default panic hook.

**Behavior on panic:**

1. Synchronously capture `PanicInfo`: `message`, `location` (file:line:col), and best-effort backtrace via `JsValue::from(Error::new(""))`'s `.stack` property.
2. Synchronously mutate `document.body.innerHTML` to render a hardcoded HTML overlay — bypasses the (now broken) Leptos reactive graph. The overlay is a full-screen red banner:
   - Title: "IEM Mixer encountered an error"
   - Short error summary (first line of panic message, truncated to 200 chars)
   - Version + git hash (compile-time baked from `iem-core::VERSION` and `iem-core::GIT_HASH`)
   - "Reload" button — onclick = `window.location.reload()`
3. Fire `fetch()` POST to `/api/client-error` (fire-and-forget, `.catch(|_|())`). Body:
   ```json
   {
     "version": "1.142.0",
     "git_hash": "abc1234",
     "url": "/engineer",
     "user_agent": "Mozilla/5.0 ...",
     "panic_message": "assertion failed: ...",
     "location": "iem-ui/src/pages/mixer.rs:456:9",
     "backtrace": "..."
   }
   ```
4. Never re-panic inside the hook. All JS errors in the hook are swallowed.

**CRITICAL INVARIANT:** Step 2 must succeed even if Leptos is broken, so the overlay is rendered via raw `web_sys` DOM API calls, not Leptos components.

### 2. WS liveness watchdog

**State added to `connect_websocket` in mixer.rs:**

- `last_frame_at: Rc<Cell<f64>>` — `js_sys::Date::now()` timestamp of last received frame, set on open and updated on every message. Not a Leptos signal — no UI observes it directly, the existing `data_pulse` signal already serves that purpose.

**On every received WS frame** (in existing `onmessage` handler, alongside the existing `set_data_pulse.update` call): update `last_frame_at.set(js_sys::Date::now())`.

**New watchdog interval running every 5 seconds** (via `set_interval_with_callback_and_timeout_and_arguments_0`, mirroring the existing reconnect interval):

- If socket ready_state is `OPEN` AND `js_sys::Date::now() - last_frame_at.get() > 30_000.0`: call `socket.close()`. This triggers the existing `onclose` handler, which sets `connected` to false (showing the existing `.disconnected-banner`), and the existing reconnect loop opens a new socket.
- Log via `web_sys::console::warn_1`: `"WS watchdog: no frames for >30s, force-closing socket"`.
- The interval handle is stored with the existing reconnect interval handle and cleared in the same `on_cleanup`.

**Threshold justification:** The existing poller broadcasts meter updates every 150 ms. Missing 200 consecutive broadcasts (30 s) is unambiguously dead. A conservative threshold avoids false positives during momentary network blips.

### 3. Reconnect exponential backoff

**Current behavior** (in existing reconnect loop at `mixer.rs:819-899`): poll every 2000 ms for a CLOSED socket, immediately reconnect.

**New behavior:**

- Add `reconnect_attempt: Rc<Cell<u32>>` (lives in the closure state of the reconnect loop; not a Leptos signal — no UI observes it directly).
- Increment on every `onclose`. Reset to 0 on first frame received on a new socket (inside `onmessage`).
- Delay schedule: pure function `pub fn backoff_delay_ms(attempt: u32) -> u32` in `lifecycle.rs`:
  - attempt 0 → 2000
  - attempt 1 → 4000
  - attempt 2 → 8000
  - attempt 3 → 15000
  - attempt ≥ 4 → 30000 (cap)
- The existing auth check after 3 consecutive failures is preserved; both features read the same counter.

**Rationale:** On Android, unbounded 2s polling during a long outage pegs the radio and drains battery. A capped exponential schedule gives the network time to recover and is gentler on mobile.

### 4. `POST /api/client-error` endpoint

**Route:** `POST /api/client-error` — mounted on the PUBLIC router (no `verify_token` middleware). Errors may happen when auth is broken, so the endpoint must not depend on it.

**Request:** JSON body, max 10 KB. Fields (all optional except `panic_message`):

```rust
#[derive(Deserialize)]
struct ClientErrorReport {
    panic_message: String,
    version: Option<String>,
    git_hash: Option<String>,
    url: Option<String>,
    user_agent: Option<String>,
    location: Option<String>,
    backtrace: Option<String>,
}
```

**Handler:**

1. Reject bodies > 10 KB with `413 Payload Too Large`. Implement via `RequestBodyLimitLayer` or manual `ContentLength` check.
2. Log via `tracing::warn!` with structured fields — one log line per report:
   ```
   client_error version=1.142.0 git_hash=abc url=/engineer ua="Mozilla/5.0..." panic="..." location="..."
   ```
3. Return `204 No Content` on success.
4. No rate limiting (if we get flooded, that IS the signal we need).
5. CORS: use existing permissive layer — no changes needed.

**Why `tracing::warn!` and not `error!`:** `error!` is already used for real server errors; we want these grep-able separately. `client_error` as the first token in the message makes `journalctl | grep client_error` trivial.

## Data Flow

```
[Client]
  WASM panic
    → custom panic hook
       ├→ render red overlay via raw DOM
       └→ fetch POST /api/client-error (fire-and-forget)

  WS frame received
    → onmessage handler
       ├→ last_frame_at.set(js_sys::Date::now())
       ├→ reconnect_attempt.set(0) (reset on first frame of a new socket)
       └→ existing handler logic (dispatch to signals)

  Watchdog tick (every 5s)
    → if socket.ready_state() == OPEN && now - last_frame_at > 30_000
       └→ socket.close() → triggers onclose → set_connected(false) → existing .disconnected-banner shows

  Reconnect loop (existing, modified)
    → onclose fires → ws_fail_count += 1 → reconnect_attempt += 1
    → next interval tick checks: elapsed >= backoff_delay_ms(reconnect_attempt)?
    → if yes, open new WebSocket

[Server]
  POST /api/client-error
    → RequestBodyLimit(10 KB)
    → deserialize ClientErrorReport
    → tracing::warn!("client_error version={} git_hash={} url={} ua={} panic={} location={}")
    → 204 No Content
```

## Error Handling

| Scenario | Handling |
| --- | --- |
| Panic inside panic hook | Swallowed with `catch_unwind` equivalent; never re-enters the hook |
| `document.body.innerHTML` fails | Swallowed (overlay missing is bad but non-fatal) |
| `fetch()` to `/api/client-error` fails | Swallowed — overlay still shows, user still sees reload button |
| Watchdog can't close socket (already closed) | Logged, retried next tick (5 s later) |
| Backoff overflow (attempt > u32::MAX) | Impossible in practice; capped at 30 s after attempt 4 |
| Client sends malformed JSON to endpoint | `400 Bad Request` (default Axum behavior) |
| Client sends >10 KB body | `413 Payload Too Large` |
| Server panic in handler | Bubbles to default Axum error handler; client swallows any non-204 response |

## Testing

### Unit tests (Rust)

**`iem-server/src/proxy.rs` (or `client_error.rs` submodule if extracted):**

- `client_error_handler_accepts_minimal_body` — POST `{"panic_message":"foo"}` → 204
- `client_error_handler_accepts_full_body` — POST all fields → 204
- `client_error_handler_rejects_oversize_body` — POST 11 KB body → 413
- `client_error_handler_rejects_missing_required_field` — POST `{}` → 400 (serde deserialization fails on missing `panic_message`)
- `client_error_handler_logs_structured_line` — capture tracing output, verify `client_error version=... panic=...` format

**`iem-ui/src/lifecycle.rs` (pure helpers, no DOM):**

- `backoff_delay_ms_schedule` — attempts 0..10 produce `[2000, 4000, 8000, 15000, 30000, 30000, 30000, 30000, 30000, 30000]`
- `is_stale_below_threshold` — `is_stale(last=100.0, now=29_999.0, threshold=30_000)` returns `false`
- `is_stale_at_threshold` — `is_stale(last=0.0, now=30_001.0, threshold=30_000)` returns `true`

The panic hook's overlay rendering path is not unit-tested (it mutates `document.body` directly and has no return value). Correctness relies on code simplicity and the post-deploy E2E that verifies the server-side receive path.

### Playwright E2E tests

**`e2e/tests/live/ws-watchdog.spec.ts` (CI, synthetic)** — requires a test-only endpoint `POST /api/test/force-ws-close/:member_id` gated behind a build flag:

1. Load mixer as petronela
2. Wait for channel render (proves initial WS connect)
3. Assert header chip is in green/OK state
4. Call `/api/test/force-ws-close/petronela` to kill the server-side socket half
5. Assert header chip transitions to red within 10 s
6. Assert reconnect happens and chip returns to green within 30 s (backoff 2s + connect + handshake)
7. Assert zero console errors

**CRITICAL:** The test-only endpoint must be compiled out of release builds. Two options:

- Option 1: `#[cfg(any(debug_assertions, feature = "test-only-endpoints"))]` — compile-time gate
- Option 2: Config flag `config.test_endpoints_enabled: bool` — runtime gate, verified OFF in production config

Prefer Option 1 to prevent accidental production exposure.

**`e2e/tests/live/client-error-reporting.spec.ts` (post-deploy, real system)** — proves the endpoint works end-to-end:

1. Load mixer as petronela
2. `page.evaluate` a `fetch()` POST to `/api/client-error` with a marker panic message (e.g. `"e2e-test-marker-<timestamp>"`)
3. Assert response is 204
4. SSH to iem.lan, `journalctl -u iem-mixer --since "1 minute ago" | grep "e2e-test-marker-<timestamp>"` — assert the log line is present
5. Assert the log line contains `client_error`, the version string, and the marker

This test REQUIRES SSH access from the runner to iem.lan — the existing deploy jobs already have this, so add it to the deploy-E2E tier, not the CI-only tier.

### Manual / field verification

After deploy:

1. Open PWA on Android phone, confirm no visible regressions
2. Force-close network (airplane mode on / off) — confirm chip goes red then green, no crash
3. Wait for a real freeze report from the user, check `journalctl -u iem-mixer | grep client_error` for new entries within the same time window

## Scope boundaries (explicitly NOT in this PR)

- Talkback state-machine wedge — deferred until field data confirms involvement
- iOS AudioContext gesture timing — defer (user reported Android)
- WebSocket ping/pong protocol — watchdog is sufficient for now
- In-app error dashboard — user chose stdout-only logging
- Rate limiting on `/api/client-error` — add in a follow-up if we see abuse

## Version bump

1.141.0 → 1.142.0. First commit on `dev`, per airuleset `version-bumping.md`.

## Changelog entry (README.md)

```
### v1.142.0 (2026-04-11)

- **Fix**: PWA self-healing — WebSocket watchdog force-reconnects after 30s of silence, exponential backoff replaces unbounded 2s retry loop
- **Feature**: WASM panic hook renders reload banner and POSTs diagnostics to server (visible via `journalctl | grep client_error`)
- **Feature**: Header connection chip shows disconnected state after 5s (#153)
```
