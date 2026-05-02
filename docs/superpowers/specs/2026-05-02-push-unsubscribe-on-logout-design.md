# Push Unsubscribe on Engineer Logout — Design (#188)

## Problem

When the engineer logs out via Settings → Logout, the client clears `localStorage` and navigates to `/`, but never:

1. Calls `pushManager.unsubscribe()` to stop the browser/OS from receiving FCM/APNS pushes.
2. Tells the server to drop the stored endpoint from `push_subscriptions.json`.

Result: every subsequent SOS broadcast iterates the unchanged `push_store.all()` list and still pushes to the logged-out engineer's phone. User reported still receiving SOS messages ~1 week after logout.

**Root cause location:** `iem-mixer/iem-ui/src/components/settings_modal.rs:306-316` (logout `<button>` handler).

**Push store has no member association** — flat `Vec<PushSubscription>` keyed only by endpoint URL. Cleanup must therefore happen on the leaving client, not server-side per-member.

## Goals

- Engineer logout fully revokes push delivery: browser-side unsubscribe + server-side endpoint removal.
- Idempotent: unsubscribing an unknown endpoint, or logging out without an active subscription, must not error.
- Logout itself never blocks on unsubscribe success — network failure must still log the user out.
- One-time cleanup of the existing orphan subscription on iem.lan (user's phone from ~1 week ago) without manual SSH.

## Non-goals

- Per-member push subscription tracking (orthogonal bug — file separately if needed).
- Push delivery retry/dead-letter handling.
- PWA reinstall subscription restoration.

## Design

### 1. Server: `POST /api/push/unsubscribe`

**File:** `iem-mixer/crates/iem-server/src/routes.rs` — new handler next to `push_subscribe`.

**Auth:** engineer JWT in `Authorization: Bearer …` header, identical to `push_subscribe`. Required so a logged-out attacker can't enumerate or wipe endpoints.

**Body:** `{"endpoint": "<full FCM/APNS URL>"}`.

**Action:** call existing `push_store.remove_endpoint(&endpoint)` (already implemented at `push_store.rs:55`).

**Response:** always `200 {"ok": true}` when auth passes — idempotent for unknown endpoints. `400` only if body is missing/empty `endpoint`. `403` on missing/non-engineer token.

**Route registration:** add `.route("/api/push/unsubscribe", post(push_unsubscribe))` in the router builder near line 114 of `routes.rs`.

### 2. Server: one-time orphan migration on startup

**File:** `iem-mixer/crates/iem-server/src/push_store.rs` — extend `PushStore::load()`.

**Mechanism:**

- Marker file path: `<config_dir>/push_subs_v2_migrated` (sibling of `push_subscriptions.json`).
- On `load()`:
  - If marker missing: empty the in-memory `subscriptions = Vec::new()`, immediately `save()` (writes empty `[]` to disk), then create the marker file with empty content.
  - If marker present: behave as today (load existing file).
- Idempotent across restarts: marker exists → no further wipes.

**Why this approach:** self-contained in the server crate — no CI cleanup step, no manual SSH, no second PR to remove cleanup logic. Runs exactly once per host on first server start after the deploy that ships this change.

**Marker creation must be best-effort** — if the marker can't be written (disk error), do not fail server startup. Log a warning. Worst case: wipe runs again next restart (still safe — orphan list will be empty by then anyway).

### 3. Client: `unsubscribe_from_push()` helper

**File:** `iem-mixer/iem-ui/src/pages/mixer/push.rs` — new pub(crate) function.

**Sequence:**

1. Read auth token from `crate::auth::get_token()`. If `None`, return early (already logged out).
2. Get `ServiceWorkerRegistration` via `navigator.serviceWorker.ready` (mirrors `subscribe_to_push`).
3. `push_manager.get_subscription()` → if `null`/`undefined`, log info and return early.
4. Cast to `web_sys::PushSubscription`, read `endpoint()`.
5. Call `subscription.unsubscribe()` on the browser side. Log warning on failure but continue.
6. POST to `/api/push/unsubscribe` with `{"endpoint": <endpoint>}` and `Authorization: Bearer <token>`. Log warning on non-2xx but do not propagate.

**Error policy:** every failure path logs a `console.warn` with the prefix `[push]` (matching `subscribe_to_push` style) and returns. Logout proceeds regardless.

### 4. Client: logout handler wiring

**File:** `iem-mixer/iem-ui/src/components/settings_modal.rs:306-316`.

**Change:** the `on:click` closure becomes:

```rust
move |_| {
    // Fire-and-forget; unsubscribe runs on a spawned future, captures token before clear_auth runs.
    crate::pages::mixer::push::unsubscribe_from_push();
    crate::auth::clear_auth();
    navigate("/", Default::default());
}
```

**Ordering rationale:** `unsubscribe_from_push` reads the token via `get_token()` synchronously inside the spawned future before any `await`. Although `clear_auth()` runs synchronously immediately after the call, the closure has already captured the token — Rust's closure-capture-by-value (the spawn_local moves token into the future) ensures no race. To make this explicit and bullet-proof, `unsubscribe_from_push()` reads the token as its very first synchronous statement before the `wasm_bindgen_futures::spawn_local(async move { ... })` block.

**Visibility:** export `unsubscribe_from_push` from `pages::mixer::push` (sibling of `subscribe_to_push`); add `pub use` in `pages/mixer/mod.rs` if needed for `settings_modal` to reach it.

## Tests

### Unit (Rust, `iem-mixer/crates/iem-server/`)

- `push_store::load` — table-driven test over `(marker_present, file_present)`:
  - `(false, false)`: no-op, marker created, empty list.
  - `(false, true_with_data)`: list wiped, marker created, file becomes `[]`.
  - `(true, true_with_data)`: list loaded, marker untouched, file unchanged.
- `routes::push_unsubscribe` integration test (axum `TestServer` style or hand-rolled handler invocation):
  - Engineer JWT + known endpoint → 200, store no longer contains endpoint.
  - Engineer JWT + unknown endpoint → 200 (idempotent).
  - Missing/empty endpoint → 400.
  - Missing/non-engineer JWT → 403.

### E2E (Playwright, post-deploy live)

**File:** `iem-mixer/e2e/tests/live/push-unsubscribe.spec.ts`.

**Flow:**

1. Login as engineer (existing `loginAs` helper).
2. Navigate to `/engineer` (or whichever member triggers `subscribe_to_push`).
3. Wait for console log `[push] engineer subscribed to Web Push` (subscribe is fire-and-forget).
4. Capture network request to `POST /api/push/subscribe` and the endpoint sent.
5. Open Settings → Logout.
6. Assert:
   - Network request `POST /api/push/unsubscribe` fires with the same endpoint and `Authorization: Bearer …` header.
   - Response status 200.
   - Console log `[push] unsubscribed` appears (or equivalent confirmation log).
   - URL becomes `/`.
   - `localStorage["iem_token"]` is gone.
7. Browser console must have zero errors at end of test (per `browser-console-zero-errors`).

**No API call to enumerate server-side store.** The E2E asserts the contract (client makes the right call); server-side correctness is unit-tested separately.

## Verification (post-deploy)

After CI deploy to iem.lan:

1. SSH or `mcp__win-iem-snv__FileList` `%APPDATA%\iem-mixer\` — confirm `push_subs_v2_migrated` exists and `push_subscriptions.json` is `[]` (or only contains entries from logins after the deploy).
2. Functional: engineer phone confirms no further SOS messages after waiting one SOS-trigger cycle.
3. Re-login on engineer phone, trigger one SOS to confirm new subscription works post-fix.

## Out of scope (track separately if needed)

- Multi-engineer member association in `PushStore` (one engineer can in principle receive another's pushes — orthogonal correctness gap not surfaced by #188).
- Push subscription expiry/renewal handling beyond the existing `404/410 → remove_endpoint` path.

## File map

- `iem-mixer/crates/iem-server/src/routes.rs` — new `push_unsubscribe` handler + route registration.
- `iem-mixer/crates/iem-server/src/push_store.rs` — extend `load()` with marker-based migration.
- `iem-mixer/iem-ui/src/pages/mixer/push.rs` — new `unsubscribe_from_push()`.
- `iem-mixer/iem-ui/src/components/settings_modal.rs` — wire logout handler.
- `iem-mixer/iem-ui/src/pages/mixer/mod.rs` — re-export if needed.
- `iem-mixer/e2e/tests/live/push-unsubscribe.spec.ts` — new post-deploy E2E.
- 5× `Cargo.toml` + `tauri.conf.json` — version bump 1.163.0 → 1.164.0.
- `README.md` — v1.164.0 changelog entry.
