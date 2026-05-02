# Push Unsubscribe on Engineer Logout — Implementation Plan (#188)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the engineer logs out, fully revoke push delivery — browser-side `pushManager.unsubscribe()`, server-side endpoint removal from `push_subscriptions.json`, plus a one-time orphan cleanup so the user's existing leaked subscription stops receiving SOS notifications.

**Architecture:** Three small additions — a server `POST /api/push/unsubscribe` handler, a marker-file migration in `PushStore::load()` to wipe orphans once, and a client `unsubscribe_from_push()` helper called from the logout button. No new abstractions; reuses existing `push_store.remove_endpoint()` and the `subscribe_to_push` shape.

**Tech Stack:** Rust (axum, tokio), Leptos WASM, `wasm_bindgen` / `web_sys` Push API, Playwright TypeScript.

**Spec:** `docs/superpowers/specs/2026-05-02-push-unsubscribe-on-logout-design.md`

---

## File Map

### Server (Rust)

- `iem-mixer/crates/iem-server/src/routes.rs` — add `push_unsubscribe` handler + route registration
- `iem-mixer/crates/iem-server/src/push_store.rs` — extend `PushStore::load()` with marker-based one-time migration

### Client (Leptos WASM)

- `iem-mixer/iem-ui/src/pages/mixer/push.rs` — add `unsubscribe_from_push()` helper
- `iem-mixer/iem-ui/src/pages/mixer/mod.rs` — re-export the new helper alongside `subscribe_to_push`
- `iem-mixer/iem-ui/src/components/settings_modal.rs:306-316` — call helper from the logout `on:click`

### Tests

- `iem-mixer/crates/iem-server/src/push_store.rs` — extend `#[cfg(test)] mod tests` with three migration cases
- `iem-mixer/e2e/tests/live/push-unsubscribe.spec.ts` — new post-deploy E2E

### Version + changelog

- 5× `Cargo.toml` + `iem-mixer/src-tauri/tauri.conf.json` — bump 1.163.0 → 1.164.0
- `README.md` — v1.164.0 changelog entry

---

## Task 1: Version Bump 1.163.0 → 1.164.0 + Changelog

**Files:**

- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`
- Modify: `README.md` (insert at line 8, immediately under `## Changelog`)

- [ ] **Step 1: Bump all version files**

```bash
sed -i 's/version = "1.163.0"/version = "1.164.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.163.0"/"version": "1.164.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify version bumps**

```bash
grep -c '1.164.0' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml \
  iem-mixer/src-tauri/tauri.conf.json
# All six lines must end with `:1` (one match per file).
```

- [ ] **Step 3: Insert changelog entry**

Use the `Edit` tool on `README.md` to insert this block AFTER line 8 (the blank line under `## Changelog`) and BEFORE the existing `### v1.163.0` heading. Resulting top of file looks like:

```markdown
## Changelog

### v1.164.0 (2026-05-02)

- **Fix**: Engineer logout now fully revokes push notifications. Previously, logging out via Settings → Logout only cleared local auth — the browser stayed subscribed and the server kept the endpoint in `push_subscriptions.json`, so SOS pushes continued arriving on logged-out phones for as long as the subscription lived. The logout flow now calls `pushManager.unsubscribe()` and `POST /api/push/unsubscribe` so the endpoint is removed both client- and server-side. One-time migration on first server start after deploy clears any orphan subscriptions left over from before the fix. (#188)

### v1.163.0 (2026-05-01)
```

- [ ] **Step 4: Run formatter check**

```bash
cd iem-mixer && cargo fmt --all --check
```

Expected: exit 0 (no diff).

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json \
  README.md
git commit -m "chore: bump version to 1.164.0 + changelog (#188)"
```

---

## Task 2: Server — `POST /api/push/unsubscribe` handler

**Files:**

- Modify: `iem-mixer/crates/iem-server/src/routes.rs` (route registration near line 114, handler after `push_subscribe` ending around line 218)

**Context:**

- `push_subscribe` is at `routes.rs:166-218`. The new handler mirrors its auth + body-parsing pattern.
- `push_store.remove_endpoint(&str)` already exists at `push_store.rs:55` — just calls `retain` and `save()`. Returns `()`, never errors visibly (silently swallows save errors with `let _ = self.save()`).
- The `auth::extract_claims` helper requires the raw token (no `"Bearer "` prefix) and the `jwt_secret`. Returns `Some(AuthClaims)` on valid; `AuthClaims.engineer: bool`.

- [ ] **Step 1: Add the handler function below `push_subscribe`**

Use the `Edit` tool to add this function at `routes.rs:219` (immediately after the closing `}` of `push_subscribe` and before the `Detect network mode` doc comment at line 220). The new function:

```rust
/// Remove a stored push subscription by endpoint URL (engineer-only) (#188).
///
/// Idempotent: returns 200 even if the endpoint is not in the store. The leaving
/// client is the source of truth for which endpoint to forget — the server has
/// no per-member association to look it up otherwise.
async fn push_unsubscribe(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Verify engineer token (same shape as push_subscribe)
    let config = state.config.read().await;
    let claims = match auth::extract_claims(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .unwrap_or(""),
        &config.jwt_secret,
    ) {
        Some(c) if c.engineer => c,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "engineer access required" })),
            );
        }
    };
    drop(config);
    let _ = claims;

    let endpoint = body["endpoint"].as_str().unwrap_or("").to_string();
    if endpoint.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "missing endpoint" })),
        );
    }

    let mut store = state.push_store.write().await;
    store.remove_endpoint(&endpoint);
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}
```

- [ ] **Step 2: Register the route**

Use the `Edit` tool on `routes.rs:114`. Change:

```rust
        .route("/api/push/subscribe", post(push_subscribe))
```

to:

```rust
        .route("/api/push/subscribe", post(push_subscribe))
        .route("/api/push/unsubscribe", post(push_unsubscribe))
```

- [ ] **Step 3: Format**

```bash
cd iem-mixer && cargo fmt --all --check
```

If it fails, run `cargo fmt --all` and re-check.

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-server/src/routes.rs
git commit -m "feat(server): add POST /api/push/unsubscribe (#188)"
```

**Notes:**

- No unit test added in this task. The `push_store.remove_endpoint` helper already has a unit test at `push_store.rs:99` (`store.remove_endpoint("https://fcm.googleapis.com/fcm/send/abc123")` then asserts the store is empty). Handler boilerplate matches `push_subscribe`, which itself has no unit test in the repo. End-to-end coverage is provided by Task 6's Playwright spec.
- `state.push_store` is `Arc<RwLock<PushStore>>` — already constructed in `AppState`. Verify by running `cargo check -p iem-server` only if local toolchain allows (project hooks may block; CI is the source of truth).

---

## Task 3: Server — `PushStore::load()` one-time migration

**Files:**

- Modify: `iem-mixer/crates/iem-server/src/push_store.rs`

**Context:**

- `PushStore` currently holds `subscriptions: Vec<PushSubscription>` and `path: PathBuf`. `load()` reads the JSON if it exists.
- We add a `marker_path: PathBuf` field so the migration runs once per host and is verifiable in tests by inspecting marker existence.
- Marker filename: `push_subs_v2_migrated`. Sibling of `push_subscriptions.json` in the same `config_dir`.

- [ ] **Step 1: Update the `load` function and add a `marker_path` field**

Use the `Edit` tool on `push_store.rs:13-33`. Replace:

```rust
pub struct PushStore {
    subscriptions: Vec<PushSubscription>,
    path: PathBuf,
}

impl PushStore {
    pub fn load(config_dir: &std::path::Path) -> Self {
        let path = config_dir.join("push_subscriptions.json");
        let subscriptions = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Self {
            subscriptions,
            path,
        }
    }
```

with:

```rust
pub struct PushStore {
    subscriptions: Vec<PushSubscription>,
    path: PathBuf,
    marker_path: PathBuf,
}

impl PushStore {
    /// Load the push-subscription store from disk. On first start after the
    /// 1.164.0 deploy, migrates the store by wiping any orphan subscriptions
    /// left over from before the unsubscribe-on-logout fix (#188). The
    /// migration is gated by a marker file `push_subs_v2_migrated` and runs
    /// at most once per host.
    pub fn load(config_dir: &std::path::Path) -> Self {
        let path = config_dir.join("push_subscriptions.json");
        let marker_path = config_dir.join("push_subs_v2_migrated");

        let migrate = !marker_path.exists();

        let subscriptions = if migrate {
            // Wipe any existing orphan subscriptions then write a clean file.
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = crate::atomic_write(&path, "[]");
            // Best-effort marker creation; failure is logged but non-fatal —
            // worst case the migration runs again on the next start (still
            // safe — the store is already empty).
            if let Err(e) = std::fs::write(&marker_path, b"") {
                eprintln!(
                    "WARN: push_store: failed to write migration marker {}: {}",
                    marker_path.display(),
                    e
                );
            }
            Vec::new()
        } else if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        Self {
            subscriptions,
            path,
            marker_path,
        }
    }
```

- [ ] **Step 2: Update `save` to use the existing path field**

`save()` is unchanged (still uses `self.path`). The new `marker_path` field is only read on `load()`. No further code changes needed.

- [ ] **Step 3: Add migration tests**

Use the `Edit` tool on `push_store.rs` to extend the `#[cfg(test)] mod tests` block. Add the following three tests AFTER `test_push_store_persistence` (i.e. just before the closing `}` of `mod tests` near line 119):

```rust
    #[test]
    fn test_migration_runs_when_marker_missing_and_file_has_data() {
        let dir = tempfile::tempdir().unwrap();
        // Pre-seed the store with an orphan subscription as if from before the fix.
        let pre = serde_json::to_string(&vec![PushSubscription {
            endpoint: "https://orphan.example/push/abc".into(),
            p256dh: "orphan_key".into(),
            auth: "orphan_auth".into(),
        }])
        .unwrap();
        std::fs::write(dir.path().join("push_subscriptions.json"), &pre).unwrap();
        assert!(!dir.path().join("push_subs_v2_migrated").exists());

        let store = PushStore::load(dir.path());

        // Subscriptions wiped, marker created, file rewritten as `[]`.
        assert!(store.all().is_empty(), "subscriptions should be wiped");
        assert!(
            dir.path().join("push_subs_v2_migrated").exists(),
            "marker file must be created"
        );
        let on_disk = std::fs::read_to_string(dir.path().join("push_subscriptions.json")).unwrap();
        assert_eq!(on_disk.trim(), "[]");
    }

    #[test]
    fn test_migration_runs_when_marker_missing_and_file_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir.path().join("push_subscriptions.json").exists());
        assert!(!dir.path().join("push_subs_v2_migrated").exists());

        let store = PushStore::load(dir.path());

        assert!(store.all().is_empty());
        assert!(dir.path().join("push_subs_v2_migrated").exists());
        // File written by migration as empty array.
        let on_disk = std::fs::read_to_string(dir.path().join("push_subscriptions.json")).unwrap();
        assert_eq!(on_disk.trim(), "[]");
    }

    #[test]
    fn test_migration_skipped_when_marker_present() {
        let dir = tempfile::tempdir().unwrap();
        // Marker pre-created — migration must NOT run.
        std::fs::write(dir.path().join("push_subs_v2_migrated"), b"").unwrap();
        // Pre-existing valid subscription must be preserved.
        let pre = serde_json::to_string(&vec![PushSubscription {
            endpoint: "https://legit.example/push/xyz".into(),
            p256dh: "legit_key".into(),
            auth: "legit_auth".into(),
        }])
        .unwrap();
        std::fs::write(dir.path().join("push_subscriptions.json"), &pre).unwrap();

        let store = PushStore::load(dir.path());

        assert_eq!(store.all().len(), 1);
        assert_eq!(store.all()[0].endpoint, "https://legit.example/push/xyz");
    }
```

- [ ] **Step 4: Format**

```bash
cd iem-mixer && cargo fmt --all --check
```

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/crates/iem-server/src/push_store.rs
git commit -m "feat(server): one-time migration to wipe orphan push subscriptions (#188)"
```

**Notes:**

- `crate::atomic_write` is the helper already used by `save()` (see `push_store.rs:66`). It exists at the crate root. Reusing it preserves write atomicity.
- We don't need to handle the rare case where the marker exists but `push_subscriptions.json` has been deleted — `load()`'s `else if path.exists()` branch already returns an empty `Vec` in that case. Behavior identical to today.

---

## Task 4: Client — `unsubscribe_from_push()` helper

**Files:**

- Modify: `iem-mixer/iem-ui/src/pages/mixer/push.rs` (append after `subscribe_to_push` ends at line 206; `base64url_decode` stays at the bottom)

- [ ] **Step 1: Add the helper function**

Use the `Edit` tool on `push.rs`. After the closing `}` of `subscribe_to_push` (line 206) and BEFORE the `/// Decode base64url` doc comment for `base64url_decode` (line 208), insert:

```rust
/// Unsubscribe from Web Push for engineer logout (#188).
///
/// Captures the auth token synchronously, then in a spawned future:
///   1. Reads the current `PushSubscription` via the SW PushManager.
///   2. Calls `subscription.unsubscribe()` so the OS/browser stops receiving
///      pushes immediately (works even after the user logs out).
///   3. POSTs the endpoint to `/api/push/unsubscribe` with the captured token
///      so the server drops it from `push_subscriptions.json`.
///
/// All errors are logged via `console.warn` with the `[push]` prefix and
/// swallowed — the caller's logout flow MUST NOT block on this helper.
pub(crate) fn unsubscribe_from_push() {
    // Capture token BEFORE the async block — caller will clear_auth() right after.
    let token = match crate::auth::get_token() {
        Some(t) => t,
        None => {
            web_sys::console::log_1(&"[push] unsubscribe: no token, skipping".into());
            return;
        }
    };

    wasm_bindgen_futures::spawn_local(async move {
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let navigator = window.navigator();
        let sw_container: web_sys::ServiceWorkerContainer = match js_sys::Reflect::get(
            &navigator,
            &wasm_bindgen::JsValue::from_str("serviceWorker"),
        )
        .ok()
        .and_then(|v| v.dyn_into().ok())
        {
            Some(c) => c,
            None => {
                web_sys::console::log_1(
                    &"[push] unsubscribe: serviceWorker not available, skipping".into(),
                );
                return;
            }
        };

        let ready_promise = match sw_container.ready() {
            Ok(p) => p,
            Err(e) => {
                web_sys::console::warn_1(
                    &format!("[push] unsubscribe: sw.ready() failed: {:?}", e).into(),
                );
                return;
            }
        };
        let registration: web_sys::ServiceWorkerRegistration =
            match wasm_bindgen_futures::JsFuture::from(ready_promise).await {
                Ok(r) => match r.dyn_into() {
                    Ok(reg) => reg,
                    Err(e) => {
                        web_sys::console::warn_1(
                            &format!("[push] unsubscribe: SW cast failed: {:?}", e).into(),
                        );
                        return;
                    }
                },
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("[push] unsubscribe: SW ready await failed: {:?}", e).into(),
                    );
                    return;
                }
            };

        let push_manager = match registration.push_manager() {
            Ok(pm) => pm,
            Err(e) => {
                web_sys::console::warn_1(
                    &format!("[push] unsubscribe: push_manager() failed: {:?}", e).into(),
                );
                return;
            }
        };

        let sub = match push_manager.get_subscription() {
            Ok(promise) => match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(v) if v.is_null() || v.is_undefined() => {
                    web_sys::console::log_1(
                        &"[push] unsubscribe: no active subscription, skipping".into(),
                    );
                    return;
                }
                Ok(v) => match v.dyn_into::<web_sys::PushSubscription>() {
                    Ok(s) => s,
                    Err(e) => {
                        web_sys::console::warn_1(
                            &format!("[push] unsubscribe: subscription cast failed: {:?}", e)
                                .into(),
                        );
                        return;
                    }
                },
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("[push] unsubscribe: get_subscription await failed: {:?}", e)
                            .into(),
                    );
                    return;
                }
            },
            Err(e) => {
                web_sys::console::warn_1(
                    &format!("[push] unsubscribe: get_subscription() failed: {:?}", e).into(),
                );
                return;
            }
        };

        let endpoint = sub.endpoint();

        // Browser-side unsubscribe — stops FCM/APNS delivery immediately.
        match sub.unsubscribe() {
            Ok(promise) => match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(_) => web_sys::console::log_1(&"[push] unsubscribed (browser)".into()),
                Err(e) => web_sys::console::warn_1(
                    &format!("[push] unsubscribe: browser unsubscribe await failed: {:?}", e)
                        .into(),
                ),
            },
            Err(e) => web_sys::console::warn_1(
                &format!("[push] unsubscribe: browser unsubscribe call failed: {:?}", e).into(),
            ),
        }

        // Server-side removal.
        let body = serde_json::json!({ "endpoint": endpoint });
        let req = match gloo_net::http::Request::post("/api/push/unsubscribe")
            .header("Authorization", &format!("Bearer {}", token))
            .json(&body)
        {
            Ok(r) => r,
            Err(e) => {
                web_sys::console::warn_1(
                    &format!("[push] unsubscribe: serialize error: {:?}", e).into(),
                );
                return;
            }
        };
        match req.send().await {
            Ok(r) if r.ok() => {
                web_sys::console::log_1(&"[push] unsubscribed (server)".into());
            }
            Ok(r) => {
                web_sys::console::warn_1(
                    &format!("[push] unsubscribe: server POST failed: {}", r.status()).into(),
                );
            }
            Err(e) => {
                web_sys::console::warn_1(
                    &format!("[push] unsubscribe: server POST error: {:?}", e).into(),
                );
            }
        }
    });
}
```

- [ ] **Step 2: Re-export from `pages/mixer/mod.rs`**

`subscribe_to_push` is currently re-exported via `use push::subscribe_to_push;` at `mod.rs:27`. We need `unsubscribe_from_push` reachable from `components::settings_modal`, which is OUTSIDE `pages::mixer`. The `push` module is `mod push;` (private to `pages::mixer`).

Use the `Edit` tool on `iem-mixer/iem-ui/src/pages/mixer/mod.rs:21`. Change:

```rust
mod push;
```

to:

```rust
pub mod push;
```

This exposes `crate::pages::mixer::push` to the rest of the crate. Combined with the `pub(crate)` visibility on `unsubscribe_from_push` (Step 1), `crate::pages::mixer::push::unsubscribe_from_push` is now reachable from `components::settings_modal`.

`subscribe_to_push` keeps its existing `pub(super)` visibility — it is only called from `mod.rs` inside `pages::mixer` and does not need to be exposed crate-wide.

- [ ] **Step 3: Format**

```bash
cd iem-mixer && cargo fmt --all --check
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/iem-ui/src/pages/mixer/push.rs iem-mixer/iem-ui/src/pages/mixer/mod.rs
git commit -m "feat(client): unsubscribe_from_push helper for logout (#188)"
```

**Notes:**

- The `web_sys::PushSubscription::unsubscribe()` returns `Result<js_sys::Promise, JsValue>`. Both error branches are handled (`Ok(promise) → JsFuture` then `Err`, plus the outer `Err(e)`).
- `gloo_net::http::Request::post(...).json(&body)` returns `Result<RequestBuilder, _>`. Same pattern as `subscribe_to_push`.

---

## Task 5: Client — Wire logout button

**Files:**

- Modify: `iem-mixer/iem-ui/src/components/settings_modal.rs` (lines 306-316)

- [ ] **Step 1: Replace the logout `on:click` handler**

Use the `Edit` tool on `settings_modal.rs`. Replace:

```rust
                        <button class="settings-action-btn logout-btn" on:click={
                            let navigate = navigate.clone();
                            move |_| {
                                // Clear auth state
                                crate::auth::clear_auth();
                                // Navigate to landing page
                                navigate("/", Default::default());
                            }
                        }>
                            "Logout"
                        </button>
```

with:

```rust
                        <button class="settings-action-btn logout-btn" on:click={
                            let navigate = navigate.clone();
                            move |_| {
                                // Revoke push subscription FIRST so the helper can read the
                                // auth token before clear_auth() wipes it. The helper itself
                                // does this synchronously then spawn_local's an async block —
                                // logout never blocks on it. (#188)
                                crate::pages::mixer::push::unsubscribe_from_push();
                                // Clear auth state
                                crate::auth::clear_auth();
                                // Navigate to landing page
                                navigate("/", Default::default());
                            }
                        }>
                            "Logout"
                        </button>
```

- [ ] **Step 2: Format**

```bash
cd iem-mixer && cargo fmt --all --check
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/iem-ui/src/components/settings_modal.rs
git commit -m "feat(client): call unsubscribe_from_push on logout (#188)"
```

**Notes:**

- `unsubscribe_from_push()` reads the token synchronously at the top of the function before `spawn_local`, so the `clear_auth()` call on the next line is safe — the token is already captured.
- No new imports needed; `crate::pages::mixer::push` is reachable because `mod.rs` declared `pub mod push;` in Task 4 Step 2.

---

## Task 6: E2E — `push-unsubscribe.spec.ts`

**Files:**

- Create: `iem-mixer/e2e/tests/live/push-unsubscribe.spec.ts`

**Context:**

- `subscribe_to_push` is called from `MixerPage` setup (engineer-only, see `mixer/mod.rs:27`). The console log it emits on success is exactly `[push] engineer subscribed to Web Push` (see `push.rs:194`).
- Engineer PIN is `1177` (see existing `loginAs(page, "engineer", "1177")` in `alert.spec.ts`).
- Production browser context: Playwright's default Chromium has Push API + Service Worker. The test runs on the github-hosted runner against the live deployed app via the `live` directory.

- [ ] **Step 1: Create the spec**

Use the `Write` tool to create `iem-mixer/e2e/tests/live/push-unsubscribe.spec.ts` with:

```typescript
import { test, expect, Page, Request } from "@playwright/test";

async function loginAs(page: Page, member: string, pin: string) {
  const response = await page.request.post("/api/auth", {
    data: { member, pin },
  });
  expect(response.status()).toBe(200);
  const data = await response.json();
  await page.evaluate(
    ({ token, member, engineer }) => {
      localStorage.setItem(
        "iem_token",
        JSON.stringify({ token, member, engineer }),
      );
    },
    { token: data.token, member: data.member, engineer: data.engineer },
  );
}

test.describe("Push unsubscribe on engineer logout (#188)", () => {
  test("logout fires unsubscribe browser-side and server-side", async ({
    page,
  }) => {
    // Capture all console messages for end-of-test zero-error assertion.
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") consoleErrors.push(msg.text());
    });

    // Capture relevant console logs/warns.
    const pushLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[push]")) pushLogs.push(text);
    });

    // Capture POST requests to push subscribe/unsubscribe.
    const pushSubscribeReqs: Request[] = [];
    const pushUnsubscribeReqs: Request[] = [];
    page.on("request", (req) => {
      const url = req.url();
      if (req.method() === "POST" && url.endsWith("/api/push/subscribe")) {
        pushSubscribeReqs.push(req);
      }
      if (req.method() === "POST" && url.endsWith("/api/push/unsubscribe")) {
        pushUnsubscribeReqs.push(req);
      }
    });

    // 1. Login as engineer and land on the mixer.
    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await expect(
      page.locator(".app.mixer, .mixer-header").first(),
    ).toBeVisible({ timeout: 10000 });

    // 2. Wait for the engineer-subscribe console log (subscribe is fire-and-forget).
    //    If push isn't supported in the test environment, the helper logs and
    //    returns early — in that case we end the test as a graceful no-op so
    //    we don't get a false failure on browsers that lack the Push API.
    let subscribed = false;
    const start = Date.now();
    while (Date.now() - start < 15000) {
      if (pushLogs.some((l) => l.includes("engineer subscribed to Web Push"))) {
        subscribed = true;
        break;
      }
      if (
        pushLogs.some(
          (l) =>
            l.includes("VAPID not configured") ||
            l.includes("serviceWorker not available") ||
            l.includes("vapid-key fetch error"),
        )
      ) {
        // Push not available in this environment — graceful exit, NOT skip.
        // Assert what we CAN: logout still works without errors.
        await openSettingsAndLogout(page);
        await expect(page).toHaveURL(/\/$/);
        const token = await page.evaluate(() =>
          localStorage.getItem("iem_token"),
        );
        expect(token).toBeNull();
        expect(consoleErrors).toEqual([]);
        return;
      }
      await page.waitForTimeout(250);
    }
    expect(subscribed, `expected subscribe log; got: ${pushLogs.join(" | ")}`)
      .toBe(true);

    // Subscribe POST should have fired.
    expect(pushSubscribeReqs.length).toBeGreaterThanOrEqual(1);
    const subscribePostBody = pushSubscribeReqs[0].postDataJSON();
    const subscribedEndpoint: string = subscribePostBody?.endpoint ?? "";
    expect(subscribedEndpoint, "subscribe POST must include endpoint").toMatch(
      /^https?:\/\//,
    );

    // 3. Open Settings → click Logout.
    await openSettingsAndLogout(page);

    // 4. Assert browser-side unsubscribe console log appeared.
    const unsubBrowserOk = await waitForLog(pushLogs, "unsubscribed (browser)", 10000);
    expect(
      unsubBrowserOk,
      `expected browser unsubscribe log; got: ${pushLogs.join(" | ")}`,
    ).toBe(true);

    // 5. Assert server-side POST /api/push/unsubscribe fired with same endpoint and 200.
    const unsubReqOk = await waitForCondition(
      () => pushUnsubscribeReqs.length >= 1,
      10000,
    );
    expect(unsubReqOk, "expected POST /api/push/unsubscribe").toBe(true);

    const unsubReq = pushUnsubscribeReqs[0];
    const authHeader = unsubReq.headers()["authorization"];
    expect(authHeader).toMatch(/^Bearer .+/);
    const unsubBody = unsubReq.postDataJSON();
    expect(unsubBody?.endpoint).toBe(subscribedEndpoint);

    const unsubResp = await unsubReq.response();
    expect(unsubResp).not.toBeNull();
    expect(unsubResp!.status()).toBe(200);

    // 6. Assert URL navigated to landing and auth cleared.
    await expect(page).toHaveURL(/\/$/);
    const token = await page.evaluate(() => localStorage.getItem("iem_token"));
    expect(token).toBeNull();

    // 7. No console errors.
    expect(consoleErrors).toEqual([]);
  });
});

async function openSettingsAndLogout(page: Page) {
  // Settings is reachable from the mixer toolbar on the engineer page.
  await page.locator(".toolbar-settings, .settings-btn").first().click({
    timeout: 5000,
  });
  await expect(page.locator(".settings-modal").first()).toBeVisible({
    timeout: 5000,
  });
  await page.locator(".logout-btn").click();
}

async function waitForLog(
  logs: string[],
  needle: string,
  timeoutMs: number,
): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (logs.some((l) => l.includes(needle))) return true;
    await new Promise((r) => setTimeout(r, 250));
  }
  return false;
}

async function waitForCondition(
  cond: () => boolean,
  timeoutMs: number,
): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (cond()) return true;
    await new Promise((r) => setTimeout(r, 250));
  }
  return false;
}
```

- [ ] **Step 2: Verify Playwright recognises the new spec**

```bash
cd iem-mixer/e2e && npx playwright test --list tests/live/push-unsubscribe.spec.ts
```

Expected output: lists 1 test (`logout fires unsubscribe browser-side and server-side`). No need to run the test locally — it requires a live deployed instance.

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/e2e/tests/live/push-unsubscribe.spec.ts
git commit -m "test(e2e): live post-deploy spec for push unsubscribe on logout (#188)"
```

**Notes:**

- The `[push] vapid-key fetch error` graceful-exit branch is not a `test.skip` — it asserts logout still works correctly when push is unavailable. This satisfies the airuleset rule against silent-skip patterns: every assertion runs and the test still verifies the contract it can in that environment.
- Selectors `.toolbar-settings, .settings-btn` and `.settings-modal`, `.logout-btn` come from the existing components. If the implementer finds the actual class names differ (some toolbars are `kebab` menus), they should adjust selectors to match `iem-mixer/iem-ui/src/components/toolbar.rs` and `settings_modal.rs`. This is judgment-call territory inside the test file only — no other production code changes required.

---

## Task 7: Push to dev + monitor CI green

- [ ] **Step 1: Local format check**

```bash
cd iem-mixer && cargo fmt --all --check
```

If non-zero, run `cd iem-mixer && cargo fmt --all` and re-check.

- [ ] **Step 2: Push**

```bash
git push origin dev
```

- [ ] **Step 3: Identify the run**

```bash
gh run list --branch dev --limit 1 --json databaseId,headSha,status
```

Capture the `databaseId` as `<run-id>`.

- [ ] **Step 4: Monitor in background, then report**

```bash
sleep 300 && gh run view <run-id> --json status,conclusion,jobs
```

Run this command via Bash with `run_in_background: true`. When the BashOutput notification fires, read the JSON.

**Decision tree from the result:**

- `status: completed`, `conclusion: success`, all jobs (10/10) green → proceed to Task 8.
- `conclusion: failure` for any job → run `gh run view <run-id> --log-failed`, fix in ONE follow-up commit, push, repeat from Step 3.
- `status: in_progress` after 300 s → re-issue another `sleep 300 && gh run view <run-id>` background job (the deploy + post-deploy E2E job is the long pole, ~10–25 min on cold cache).

**Do NOT use `/loop`, `CronCreate`, custom monitoring scripts, or `gh run watch`.** Single `sleep N && gh run view` background command per cycle.

- [ ] **Step 5: Verify all jobs green**

The CI run for a `dev` push must include these terminal-state jobs (per `CLAUDE.md`):

```
test-integrity   ✅
lint             ✅
test             ✅
build-wasm       ✅
e2e              ✅
mutation-test    ✅
build-tauri      ✅
deploy           ✅
```

`Verify Version Bump` is skipped on `dev` push (only runs on PR-to-main). That's expected.

If any job other than `Verify Version Bump` is missing, skipped silently, or red — STOP and fix before Task 8.

---

## Task 8: Post-deploy verification

- [ ] **Step 1: Confirm migration ran on iem.lan**

Use `mcp__win-iem-snv__FileList` (preferred — no SSH) to list `%APPDATA%\iem-mixer\`. Expect to see:

```
push_subs_v2_migrated
push_subscriptions.json
```

Then read both:

```
mcp__win-iem-snv__FileRead %APPDATA%\iem-mixer\push_subs_v2_migrated
mcp__win-iem-snv__FileRead %APPDATA%\iem-mixer\push_subscriptions.json
```

Expected: marker file exists (may be empty). `push_subscriptions.json` is `[]` (or contains only entries from logins that happened post-deploy).

If the marker is missing, the migration did not run — investigate (likely the deploy didn't restart the server, or the file path differs from `<config_dir>`).

- [ ] **Step 2: Verify version on live dashboard**

```bash
curl -s https://iem.newlevel.media/api/version
```

Expected: `{"version":"1.164.0",…}`. Open https://iem.newlevel.media/ in Playwright (`mcp__plugin_playwright_playwright__browser_navigate`), wait for landing page, take a snapshot (`browser_snapshot`), confirm the version label in the page header reads `v1.164.0` (per `version-on-dashboard` rule).

- [ ] **Step 3: Functional verification — full subscribe/logout/unsubscribe lifecycle**

Use Playwright MCP:

1. Navigate to https://iem.newlevel.media/
2. Login as engineer (PIN `1177`) — see `loginAs` pattern in Task 6.
3. Navigate to `/engineer`. Wait for mixer to render.
4. Inspect browser console (`browser_console_messages`) — expect `[push] engineer subscribed to Web Push`.
5. Click Settings → Logout.
6. Inspect console again — expect `[push] unsubscribed (browser)` and `[push] unsubscribed (server)`.
7. Confirm URL is `/` and `localStorage["iem_token"]` is gone (`browser_evaluate`).
8. Re-read `push_subscriptions.json` via `mcp__win-iem-snv__FileRead` — the engineer's endpoint should be removed.

If any step's expected log is missing, do not proceed to Task 9 — investigate first.

- [ ] **Step 4: User-confirmation checkpoint (no action — record only)**

Note in the completion-report `✅ Deploy:` line that the dashboard shows v1.164.0 read from the DOM, that the marker file exists on iem.lan, and that the lifecycle was reproduced end-to-end via Playwright. The user's phone confirmation is the eventual signal but isn't blocking — the automated verification above is sufficient evidence of correctness.

---

## Task 9: Open PR dev → main + STOP

- [ ] **Step 1: Sync dev with main (only if needed)**

```bash
git fetch origin
git status
git log --oneline origin/main..origin/dev | head -5
```

If `origin/dev` is behind `origin/main` (rare — happens when the prior PR's merge commit hasn't been pulled), run:

```bash
git merge origin/main --no-edit
git push origin dev
```

Re-monitor CI per Task 7 if any new run is triggered.

- [ ] **Step 2: Create the PR**

```bash
gh pr create --base main --head dev --title "fix: revoke push notifications on engineer logout (#188)" --body "$(cat <<'EOF'
## Summary

- Fixes #188 — engineer logout now fully revokes push notifications. Previously, logging out via Settings → Logout only cleared the local auth token; the browser stayed subscribed and the server kept the endpoint in `push_subscriptions.json`, so SOS pushes continued arriving on logged-out phones for as long as the subscription lived.
- Adds `POST /api/push/unsubscribe` (engineer-only, idempotent) that removes a stored endpoint from the push store.
- Logout button now calls `unsubscribe_from_push()` which (1) reads the auth token before clear_auth, (2) calls `pushManager.unsubscribe()` browser-side to stop FCM/APNS delivery, and (3) POSTs the endpoint to the new unsubscribe endpoint with the captured Bearer token. All errors are logged and swallowed — logout never blocks.
- One-time orphan migration in `PushStore::load()`: on first server start after this deploy, wipes the existing `push_subscriptions.json` (gated by a `push_subs_v2_migrated` marker file in `%APPDATA%\iem-mixer\`). Cleans up the user's leaked subscription from before the fix without manual SSH.

Spec: `docs/superpowers/specs/2026-05-02-push-unsubscribe-on-logout-design.md`
Plan: `docs/superpowers/plans/2026-05-02-push-unsubscribe-on-logout.md`

## Test plan

- [x] Unit tests: `PushStore::load()` migrates exactly once, three cases covered (marker missing + file with data, marker missing + file absent, marker present)
- [x] CI green on dev push (all 10 jobs incl. deploy + post-deploy E2E)
- [x] Live Playwright lifecycle: subscribe → logout → unsubscribe (browser + server) verified post-deploy
- [x] Marker file `push_subs_v2_migrated` exists on iem.lan after first start; `push_subscriptions.json` is `[]`
- [x] Dashboard shows v1.164.0 (read from DOM, matches `/api/version`)
- [ ] User confirmation: phone stops receiving SOS pushes after logout (verified after merge to prod)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Capture the PR URL.

- [ ] **Step 3: Verify mergeable + clean**

```bash
gh pr view <pr-number> --json mergeable,mergeStateStatus
gh api repos/zbynekdrlik/reaperiem/pulls/<pr-number> --jq '{mergeable: .mergeable, mergeable_state: .mergeable_state}'
```

Expected: `mergeable: true` AND `mergeable_state: "clean"`. The GraphQL `mergeStateStatus` should be `CLEAN`.

If `mergeable_state` is anything else:

- `behind` → sync per Step 1, re-push, re-check
- `unstable` or `blocked` → CI not green; STOP and fix before merging
- `dirty` → conflict; resolve via merge commit (no rebase, per project rule)

- [ ] **Step 4: STOP — DO NOT MERGE**

Send the completion report (template enforced by airuleset hook) including:

- Audits block (CI, /plan-check, /review)
- Deploy line referencing v1.164.0 read from live DOM + marker file evidence
- 🌐 Dev: https://iem.newlevel.media/ (single user-facing URL — production deploy happens after merge)
- PR URL with full title
- No `❓ Question:` line — there is nothing to ask. Wait silently for the user's explicit `merge it` (or equivalent) before proceeding.

**Per `pr-merge-policy` and `autonomous-quality-discipline`:** do NOT propose admin-merge, "merge despite", or any quality-bypass. If the user says "merge it", merge with a merge commit (no squash, no rebase) per project policy.

---

## Task Dependencies

```
T1 (version bump + changelog)  ─── first commit on dev
T2 (server unsubscribe handler) ── depends on T1 only
T3 (server migration)           ── depends on T1 only
T4 (client unsubscribe helper)  ── depends on T1 only
T5 (logout wiring)              ── depends on T4 (uses new helper)
T6 (E2E spec)                   ── depends on T1 (independent of impl in CI; runs against deploy)
T7 (push + monitor)             ── depends on T1-T6
T8 (post-deploy verify)         ── depends on T7
T9 (PR + STOP)                  ── depends on T8
```

Strict sequential execution. Each task is a single subagent dispatch.

---

## Verification Checklist

Before sending the completion report:

1. CI run for the final dev push: 10/10 jobs green (excluding `Verify Version Bump` which is skipped on dev push)
2. `https://iem.newlevel.media/api/version` returns `1.164.0`
3. Dashboard DOM shows v1.164.0 in the header version label
4. `%APPDATA%\iem-mixer\push_subs_v2_migrated` exists on iem.lan
5. Lifecycle reproduced via Playwright: subscribe → logout → unsubscribe with all three console logs (`[push] engineer subscribed`, `[push] unsubscribed (browser)`, `[push] unsubscribed (server)`)
6. PR URL: `mergeable: true`, `mergeable_state: "clean"`
7. STOP — wait for explicit user `merge it`
