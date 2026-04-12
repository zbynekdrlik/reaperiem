# Limiter Button for All Members — Implementation Plan (#156)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the LIM button visible and functional for all band members on their own IEM Volume fader, not just the engineer.

**Architecture:** Remove the `is_engineer` UI guard on the LIM button (3 lines), add server-side track-ownership validation so members can only control their own limiter, update the existing E2E test that asserts members do NOT see LIM, and add a new test that confirms members CAN use it.

**Tech Stack:** Rust (Leptos WASM frontend, Axum backend), Playwright E2E

**Spec:** `docs/superpowers/specs/2026-04-12-limiter-all-members-design.md`

---

## File Map

| File | Change |
|------|--------|
| `iem-mixer/crates/iem-core/Cargo.toml` + 4 others + tauri.conf.json | Version bump 1.144.0 → 1.145.0 |
| `iem-mixer/iem-ui/src/pages/mixer.rs:1832` | Remove `is_engineer.then()` wrapper on LIM button |
| `iem-mixer/crates/iem-server/src/proxy.rs:1093,1097,1234-1282` | Pass `is_engineer` to `handle_ws`, add track-ownership check on limiter commands |
| `iem-mixer/e2e/tests/live/limiter.spec.ts:95-131` | Flip "member does NOT see LIMIT" test to assert visibility + modal open |

---

## Task 1: Version Bump (1.144.0 → 1.145.0)

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml:3`
- Modify: `iem-mixer/Cargo.toml:12`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml:3`
- Modify: `iem-mixer/iem-ui/Cargo.toml:3`
- Modify: `iem-mixer/src-tauri/Cargo.toml:3`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`

- [ ] **Step 1: Bump all version files**

```bash
sed -i 's/version = "1.144.0"/version = "1.145.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.144.0"/"version": "1.145.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify**

```bash
grep -c '1.145.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
# Both should return 1
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.145.0"
```

---

## Task 2: Server — Add track-ownership validation for limiter commands

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs:1093,1097-1101,1234-1282`

Currently the three limiter WebSocket commands (`GetLimiterParams`, `SetLimiterParam`, `SetLimiterEnabled`) accept any `track_index` from any connected member without validation. The engineer connecting to another member's mixer can legitimately control that member's limiter, but a non-engineer member should only be able to control their own output track's limiter.

- [ ] **Step 1: Pass `is_engineer` bool into `handle_ws`**

In `proxy.rs`, the `ws_upgrade()` function (line 1093) calls `handle_ws` via the WebSocket upgrade. Add `claims.engineer` to the closure and function signature.

Change line 1093:

```rust
// BEFORE:
Ok(ws.on_upgrade(move |socket| handle_ws(socket, state, member_id, network_mode)))

// AFTER:
let is_engineer = claims.engineer;
Ok(ws.on_upgrade(move |socket| handle_ws(socket, state, member_id, network_mode, is_engineer)))
```

Change the `handle_ws` signature at line 1097-1101:

```rust
// BEFORE:
async fn handle_ws(
    mut socket: axum::extract::ws::WebSocket,
    state: AppState,
    member_id: String,
    network_mode: String,
) {

// AFTER:
async fn handle_ws(
    mut socket: axum::extract::ws::WebSocket,
    state: AppState,
    member_id: String,
    network_mode: String,
    is_engineer: bool,
) {
```

- [ ] **Step 2: Add ownership check helper and guard limiter commands**

Add an inline helper before the limiter command block (before line 1234). Then wrap each of the three limiter command handlers with the ownership check.

Insert before line 1234 (above `// Handle Limiter commands`):

```rust
// Limiter ownership check: non-engineer members can only
// control the limiter on their own output track (#156).
let owns_limiter_track = |track_index: usize| -> bool {
    if is_engineer {
        return true;
    }
    // Check synchronously using try_read to avoid holding
    // the lock across the await point.
    if let Ok(cache) = state.mixer_cache.try_read() {
        cache
            .output_track_indices
            .get(&member_id)
            .map_or(false, |&idx| idx == track_index)
    } else {
        false // Lock contended — deny conservatively
    }
};
```

Then wrap each limiter command with the check. Replace lines 1235-1282 with:

```rust
// Handle Limiter commands (async EXTSTATE + ReaScript flow) (#72)
if let iem_core::ClientMsg::GetLimiterParams { track_index } = cmd {
    if owns_limiter_track(track_index) {
        let state_clone = state.clone();
        let member_clone = member_id.clone();
        tokio::spawn(async move {
            if let Some(lim_msg) =
                handle_get_limiter_params(&state_clone, track_index).await
            {
                let _ = state_clone.event_tx.send((member_clone, lim_msg));
            }
        });
    }
    continue;
}
if let iem_core::ClientMsg::SetLimiterParam {
    track_index,
    ref param,
    value,
} = cmd
{
    if owns_limiter_track(track_index) {
        let state_clone = state.clone();
        let param_clone = param.clone();
        tokio::spawn(async move {
            handle_set_limiter_param(
                &state_clone,
                track_index,
                &param_clone,
                value,
            )
            .await;
        });
    }
    continue;
}
if let iem_core::ClientMsg::SetLimiterEnabled {
    track_index,
    enabled,
} = cmd
{
    if owns_limiter_track(track_index) {
        let state_clone = state.clone();
        tokio::spawn(async move {
            handle_set_limiter_param(
                &state_clone,
                track_index,
                "enabled",
                if enabled { 1.0 } else { 0.0 },
            )
            .await;
        });
    }
    continue;
}
```

- [ ] **Step 3: Run `cargo fmt`**

```bash
cd iem-mixer && cargo fmt --all --check
# Fix if needed:
cd iem-mixer && cargo fmt --all
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-server/src/proxy.rs
git commit -m "fix: add track-ownership validation for limiter WebSocket commands (#156)"
```

---

## Task 3: UI — Remove engineer guard on LIM button

**Files:**
- Modify: `iem-mixer/iem-ui/src/pages/mixer.rs:1832-1848`

- [ ] **Step 1: Remove `is_engineer.then()` wrapper**

Replace lines 1832-1848:

```rust
// BEFORE:
{is_engineer.then(|| view! {
    <button
        class="limiter-btn-small"
        on:click=move |_| {
            if let Some(idx) = output_track_idx.get() {
                let _ = set_limiter_loading.try_set(true);
                let _ = set_limiter_open.try_set(Some((idx, "IEM VOL".to_string())));
                ws_send(
                    ws,
                    &iem_core::ClientMsg::GetLimiterParams { track_index: idx },
                );
            }
        }
    >
        "LIM"
    </button>
})}

// AFTER:
<button
    class="limiter-btn-small"
    on:click=move |_| {
        if let Some(idx) = output_track_idx.get() {
            let _ = set_limiter_loading.try_set(true);
            let _ = set_limiter_open.try_set(Some((idx, "IEM VOL".to_string())));
            ws_send(
                ws,
                &iem_core::ClientMsg::GetLimiterParams { track_index: idx },
            );
        }
    }
>
    "LIM"
</button>
```

- [ ] **Step 2: Remove `is_engineer` prop from `GlobalVolumeFader` function signature**

The `is_engineer` parameter at line 1637 is now unused by `GlobalVolumeFader`. However, check if `is_engineer` is used elsewhere in the function body first.

Search for `is_engineer` usage within the `GlobalVolumeFader` function (lines 1625-1860). If the LIM button was the ONLY use, remove:
- The parameter `is_engineer: bool,` from the function signature (line 1637)
- The `is_engineer=is_engineer` prop from the call site (line 1407)

If `is_engineer` is used for something else in the function (e.g., the EQ button), keep it.

- [ ] **Step 3: Run `cargo fmt`**

```bash
cd iem-mixer && cargo fmt --all --check
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/iem-ui/src/pages/mixer.rs
git commit -m "feat: show LIM button for all members, not just engineer (#156)"
```

---

## Task 4: E2E Test — Update limiter tests for member access

**Files:**
- Modify: `iem-mixer/e2e/tests/live/limiter.spec.ts:95-131`

The existing test at line 95 asserts `expect(count).toBe(0)` — that members do NOT see the LIM button. This must be flipped to assert they DO see it and can open the modal.

- [ ] **Step 1: Replace the "member does NOT see LIMIT button" test**

Replace lines 95-131 with a new test that verifies member access:

```typescript
test("member sees LIMIT button and can open modal", async ({ page }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        if (msg.text().includes("subscribe await failed")) return;
        if (msg.text().includes("Push API in incognito")) return;
        if (msg.text().includes("navigator.vibrate")) return;
        if (msg.text().includes("closure invoked recursively")) return;
        if (msg.text().includes("[vite]")) return;
        if (msg.text().includes("favicon")) return;
        if (msg.text().includes("integrity")) return;
        if (msg.text().includes("WebSocket connection")) return;
        consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
      }
    });

    // Login as regular member (petronela, PIN 7711)
    await page.goto("/");
    await loginAsMember(page, "petronela");
    await page.goto("/petronela");

    await waitForMixer(page);

    await expect(page.locator(".channel").first()).toBeVisible({ timeout: 10000 });

    // LIMIT button SHOULD be visible to regular members (#156)
    const limitBtn = page.locator(".limiter-btn-small");
    await expect(limitBtn.first()).toBeVisible({ timeout: 5000 });

    // Click it and verify modal opens
    await limitBtn.first().click();

    const modal = page.locator(".limiter-modal");
    await expect(modal).toBeVisible({ timeout: 5000 });

    // Verify modal has the MAX LEVEL slider
    const sliders = page.locator(".limiter-slider-track");
    await expect(sliders.first()).toBeVisible({ timeout: 3000 });
    const sliderCount = await sliders.count();
    expect(sliderCount).toBe(1);

    // Close modal
    const closeBtn = page.locator(".limiter-close-btn");
    await closeBtn.click();
    await expect(modal).not.toBeVisible({ timeout: 2000 });

    expect(consoleMessages).toEqual([]);
  });
```

- [ ] **Step 2: Update the test description comment at top of file**

Replace the top-of-file docstring (lines 1-7) to reflect the new behavior:

```typescript
/**
 * Limiter Tests — output bus limiter controls (#72, #156).
 *
 * These tests verify the LIMIT button visibility and modal behavior
 * for both engineers and regular band members.
 * The mixer page requires a REAPER connection to render channel strips.
 */
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/e2e/tests/live/limiter.spec.ts
git commit -m "test: update limiter E2E — member now sees and uses LIM button (#156)"
```

---

## Task 5: Changelog + Push + Monitor CI

**Files:**
- Modify: `README.md` (changelog section)

- [ ] **Step 1: Add changelog entry for v1.145.0**

Add to the changelog section in README.md:

```markdown
### v1.145.0 (2026-04-12)
- **Feature**: LIM button now visible to all band members on their IEM Volume fader, not just the engineer (#156)
- **Security**: Server-side track-ownership validation ensures members can only control their own limiter
```

- [ ] **Step 2: Commit changelog**

```bash
git add README.md
git commit -m "docs: changelog for v1.145.0 limiter for all members (#156)"
```

- [ ] **Step 3: Run `cargo fmt --all --check`**

```bash
cd iem-mixer && cargo fmt --all --check
```

- [ ] **Step 4: Push and monitor CI**

```bash
git push origin dev
gh run list --limit 3
# Monitor until ALL jobs reach terminal state
```

- [ ] **Step 5: If CI fails, investigate with `gh run view <id> --log-failed` and fix all issues in ONE commit**

---

## Task Dependencies

```
Task 1 (version bump)     ─── first
Task 2 (server validation) ┐
Task 3 (UI change)         ├── parallel, after Task 1
Task 4 (E2E test update)   ┘
Task 5 (changelog + push)  ─── last, after Tasks 2-4
```

Tasks 2, 3, and 4 are independent and can be done in parallel after Task 1.

---

## Verification

After CI is green:

1. **All CI jobs** pass (including deploy)
2. **Post-deploy E2E** — limiter tests pass: engineer sees LIM, member sees LIM, modal opens for both
3. **Server validation** — member can only control own limiter track (engineer can control any)
