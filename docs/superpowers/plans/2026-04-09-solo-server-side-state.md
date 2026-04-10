# Server-Side Solo State — Implementation Plan (#155)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix solo leaving everything muted after PWA crash by moving solo muting to server-side with pre-solo state persistence.

**Architecture:** Solo muting moves from UI→individual SetMute commands to server-side batch muting on SetSolo. Server saves pre-solo mute states and restores them on unsolo or disconnect recovery. Solo state persists across WebSocket disconnects until explicitly turned off.

**Tech Stack:** Rust (Axum server), Leptos (WASM UI), Playwright (E2E tests)

**Spec:** `docs/superpowers/specs/2026-04-09-solo-server-side-state-design.md`

---

## File Map

### Server changes
- `iem-mixer/crates/iem-server/src/lib.rs:161` — Add `pre_solo_mutes` field to `MixerCache`
- `iem-mixer/crates/iem-server/src/proxy.rs:1346-1364` — Rewrite SetSolo handler (move into `apply_command_to_cache`)
- `iem-mixer/crates/iem-server/src/proxy.rs:1440-1448` — Remove solo cleanup from disconnect handler
- `iem-mixer/crates/iem-server/src/proxy.rs:1594` — Add SetSolo arm to `apply_command_to_cache`

### UI changes
- `iem-mixer/iem-ui/src/pages/mixer.rs:2391-2499` — Remove SetMute calls from solo click handler, keep only SetSolo

### E2E tests
- `iem-mixer/e2e/tests/live/mixer.spec.ts:2134-2241` — Update existing solo tests, add crash recovery test

### Version bump
- 5 Cargo.toml + 1 tauri.conf.json: 1.136.0 → 1.137.0

---

## Task 1: Version Bump (1.136.0 → 1.137.0)

- [ ] **Step 1: Bump all version files**

```bash
sed -i 's/version = "1.136.0"/version = "1.137.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.136.0"/"version": "1.137.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify**

```bash
grep -c '1.137.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
# Both should return 1
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.137.0"
```

---

## Task 2: Add `pre_solo_mutes` to MixerCache

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/lib.rs:161`

- [ ] **Step 1: Add field after `solo_states`**

In `lib.rs`, after line 161 (`pub solo_states: HashMap<String, Vec<usize>>`), add:

```rust
    /// Pre-solo mute states per member — saved when solo activates, restored on unsolo
    /// (member_id -> (track_index, send_index, was_muted))
    pub pre_solo_mutes: HashMap<String, Vec<(usize, usize, bool)>>,
```

We store `(track_index, send_index, was_muted)` tuples so the restore can send REAPER commands without needing to recalculate send indices.

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/crates/iem-server/src/lib.rs
git commit -m "feat: add pre_solo_mutes field to MixerCache (#155)"
```

---

## Task 3: Move SetSolo handling into `apply_command_to_cache`

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs`

This is the core change. We remove the early `SetSolo` handler (lines 1346-1364) and add a new match arm in `apply_command_to_cache` (after line 1656) that handles solo with REAPER mute commands.

- [ ] **Step 1: Remove the early SetSolo handler**

Remove the block at lines 1346-1364:

```rust
// REMOVE THIS ENTIRE BLOCK:
// Handle solo state updates (no REAPER command — solo is UI-only sync)
if let ClientMsg::SetSolo { ref soloed } = cmd {
    {
        let mut cache = state.mixer_cache.write().await;
        if soloed.is_empty() {
            cache.solo_states.remove(&member_id);
        } else {
            cache.solo_states
                .insert(member_id.clone(), soloed.clone());
        }
    }
    let _ = state.event_tx.send((
        member_id.clone(),
        ServerMsg::SoloUpdate {
            soloed: soloed.clone(),
        },
    ));
    continue;
}
```

- [ ] **Step 2: Add SetSolo validation in the match block**

In the `match cmd` validation block inside `apply_command_to_cache` (around line 1658), add a new arm after `SetMute`:

```rust
        iem_core::ClientMsg::SetSolo { ref soloed } => {
            for ti in soloed {
                if !is_valid_track(*ti) {
                    return Err(format!("solo track_index {} out of range", ti));
                }
            }
        }
```

- [ ] **Step 3: Add SetSolo dispatch arm**

After the `SetMute` dispatch arm (around line 1789), add the new `SetSolo` arm. This is the core logic:

```rust
        iem_core::ClientMsg::SetSolo { ref soloed } => {
            let soloed_set: std::collections::HashSet<usize> = soloed.iter().copied().collect();
            let mut cache = state.mixer_cache.write().await;
            let current_solo = cache.solo_states.get(member_id).cloned().unwrap_or_default();
            let had_solo = !current_solo.is_empty();
            let wants_solo = !soloed_set.is_empty();

            // Collect mute commands to send to REAPER
            let mut reaper_urls: Vec<String> = Vec::new();
            let mut events: Vec<iem_core::ServerMsg> = Vec::new();

            if wants_solo && !had_solo {
                // ENTERING SOLO: save current mute states, then mute everything except soloed
                let mut saved = Vec::new();
                if let Some(channels) = cache.member_states.get(member_id) {
                    for ch in channels {
                        if let Ok(si) = send_index_for(ch.track_index) {
                            saved.push((ch.track_index, si, ch.muted));
                        }
                    }
                }
                cache.pre_solo_mutes.insert(member_id.to_string(), saved);

                // Mute/unmute channels
                if let Some(channels) = cache.member_states.get_mut(member_id) {
                    for ch in channels.iter_mut() {
                        let should_mute = !soloed_set.contains(&ch.track_index);
                        if ch.muted != should_mute {
                            ch.muted = should_mute;
                            if let Ok(si) = send_index_for(ch.track_index) {
                                let mute_val: u8 = if should_mute { 1 } else { 0 };
                                reaper_urls.push(reaper_api::set_send_mute(
                                    &reaper_url, ch.track_index, si, mute_val,
                                ));
                            }
                            events.push(iem_core::ServerMsg::ChannelUpdate {
                                track_index: ch.track_index,
                                level_db: ch.level_db,
                                muted: ch.muted,
                                pan: ch.pan,
                            });
                        }
                    }
                }
            } else if wants_solo && had_solo {
                // SWITCHING SOLO: keep pre_solo_mutes, update mute to new target
                if let Some(channels) = cache.member_states.get_mut(member_id) {
                    for ch in channels.iter_mut() {
                        let should_mute = !soloed_set.contains(&ch.track_index);
                        if ch.muted != should_mute {
                            ch.muted = should_mute;
                            if let Ok(si) = send_index_for(ch.track_index) {
                                let mute_val: u8 = if should_mute { 1 } else { 0 };
                                reaper_urls.push(reaper_api::set_send_mute(
                                    &reaper_url, ch.track_index, si, mute_val,
                                ));
                            }
                            events.push(iem_core::ServerMsg::ChannelUpdate {
                                track_index: ch.track_index,
                                level_db: ch.level_db,
                                muted: ch.muted,
                                pan: ch.pan,
                            });
                        }
                    }
                }
            } else if !wants_solo && had_solo {
                // EXITING SOLO: restore pre-solo mute states
                if let Some(saved) = cache.pre_solo_mutes.remove(member_id) {
                    if let Some(channels) = cache.member_states.get_mut(member_id) {
                        for (track_idx, send_idx, was_muted) in &saved {
                            if let Some(ch) = channels.iter_mut().find(|c| c.track_index == *track_idx) {
                                if ch.muted != *was_muted {
                                    ch.muted = *was_muted;
                                    let mute_val: u8 = if *was_muted { 1 } else { 0 };
                                    reaper_urls.push(reaper_api::set_send_mute(
                                        &reaper_url, *track_idx, *send_idx, mute_val,
                                    ));
                                    events.push(iem_core::ServerMsg::ChannelUpdate {
                                        track_index: *track_idx,
                                        level_db: ch.level_db,
                                        muted: ch.muted,
                                        pan: ch.pan,
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // Update solo state
            if wants_solo {
                cache.solo_states.insert(member_id.to_string(), soloed.clone());
            } else {
                cache.solo_states.remove(member_id);
            }

            // Mark command timestamps for all affected tracks
            let now = std::time::Instant::now();
            if let Some(channels) = cache.member_states.get(member_id) {
                for ch in channels {
                    cache.command_timestamps.insert(
                        (member_id.to_string(), ch.track_index),
                        now,
                    );
                }
            }

            drop(cache);

            // Send all REAPER commands
            for url in &reaper_urls {
                let _ = state.http_client.get(url).send().await;
            }

            // Broadcast solo update
            let _ = state.event_tx.send((
                member_id.to_string(),
                ServerMsg::SoloUpdate { soloed: soloed.clone() },
            ));

            // Broadcast channel updates for changed mutes
            for event in events {
                let _ = state.event_tx.send((member_id.to_string(), event));
            }

            // Return a dummy URL (no single REAPER command — already sent above)
            return Ok(("".to_string(), None));
        }
```

- [ ] **Step 4: Handle the empty URL return**

The caller of `apply_command_to_cache` sends the returned URL to REAPER. Since SetSolo already sent its own REAPER commands, it returns an empty string. Check the caller at line 1366-1380 to make sure an empty URL is handled:

```rust
match apply_command_to_cache(&state, &member_id, &cmd).await {
    Ok((url, broadcast)) => {
        // ...
        if !url.is_empty() {
            match state.http_client.get(&url).send().await {
```

If the caller already checks `!url.is_empty()`, no change needed. If not, add the check.

Read lines 1366-1395 to verify and add the guard if needed.

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/crates/iem-server/src/proxy.rs
git commit -m "feat: server-side solo muting with pre-solo state persistence (#155)"
```

---

## Task 4: Remove solo cleanup from disconnect handler

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs:1439-1448`

- [ ] **Step 1: Remove `solo_states.remove()` from disconnect cleanup**

At line 1446, remove this line:

```rust
// REMOVE THIS LINE:
cache.solo_states.remove(&member_id);
```

The surrounding code stays — we still remove `active_members` and `member_states` on last disconnect.

**Important:** Also do NOT remove `pre_solo_mutes` on disconnect. Solo state persists until explicit unsolo.

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/crates/iem-server/src/proxy.rs
git commit -m "fix: solo state persists across WebSocket disconnects (#155)"
```

---

## Task 5: Remove SetMute calls from UI solo handler

**Files:**
- Modify: `iem-mixer/iem-ui/src/pages/mixer.rs:2391-2499`

The UI should only send `SetSolo` — the server handles muting. Keep local `set_channels.update()` for optimistic UI rendering.

- [ ] **Step 1: Simplify the solo click handler**

Replace the entire `on_solo_click` closure (lines 2391-2499) with:

```rust
                    let on_solo_click = move |_| {
                        if !connected.get() {
                            return;
                        }

                        let all_channels = channels.get();
                        let current_soloed = soloed.get();
                        let is_currently_soloed = current_soloed.contains(&track_idx);

                        if is_currently_soloed {
                            // UN-SOLO this track
                            let mut new_soloed = current_soloed.clone();
                            new_soloed.remove(&track_idx);
                            if let Some(partner) = partner_idx {
                                new_soloed.remove(&partner);
                            }

                            if new_soloed.is_empty() {
                                // Restore pre-solo mutes (optimistic UI)
                                let saved = pre_solo_mutes.get();
                                set_channels.update(|chs| {
                                    for c in chs.iter_mut() {
                                        let should_be_muted = saved.get(&c.track_index).copied().unwrap_or(false);
                                        c.muted = should_be_muted;
                                    }
                                });
                                set_pre_solo_mutes.set(HashMap::new());
                            } else {
                                // Partial unsolo — mute the desoloed track(s)
                                set_channels.update(|chs| {
                                    if let Some(ch) = chs.iter_mut().find(|c| c.track_index == track_idx) {
                                        ch.muted = true;
                                    }
                                    if let Some(partner) = partner_idx {
                                        if let Some(ch) = chs.iter_mut().find(|c| c.track_index == partner) {
                                            ch.muted = true;
                                        }
                                    }
                                });
                            }

                            let soloed_vec: Vec<usize> = new_soloed.iter().copied().collect();
                            set_soloed.set(new_soloed);
                            ws_send(ws, &iem_core::ClientMsg::SetSolo { soloed: soloed_vec });
                        } else {
                            // SOLO this track
                            let was_empty = current_soloed.is_empty();

                            if was_empty {
                                // Save pre-solo mutes for optimistic UI restore
                                let mut saved_mutes = HashMap::new();
                                for ch in &all_channels {
                                    saved_mutes.insert(ch.track_index, ch.muted);
                                }
                                set_pre_solo_mutes.set(saved_mutes);
                            }

                            // Optimistic UI: mute everything except solo target
                            set_channels.update(|chs| {
                                for c in chs.iter_mut() {
                                    c.muted = c.track_index != track_idx
                                        && partner_idx != Some(c.track_index);
                                }
                            });

                            // Build soloed set — exclusive (only new track + partner)
                            let mut new_soloed = std::collections::HashSet::new();
                            new_soloed.insert(track_idx);
                            if let Some(partner) = partner_idx {
                                new_soloed.insert(partner);
                            }
                            let soloed_vec: Vec<usize> = new_soloed.iter().copied().collect();
                            set_soloed.set(new_soloed);
                            ws_send(ws, &iem_core::ClientMsg::SetSolo { soloed: soloed_vec });
                        }
                    };
```

Key differences from old code:
- **No `ws_send(ws, &iem_core::ClientMsg::SetMute { ... })` calls** — server handles muting
- **Keep `set_channels.update()`** — optimistic UI rendering
- **Keep `pre_solo_mutes`** — UI-side copy for optimistic restore display
- **Only `ws_send` is `SetSolo`** — single message to server

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/iem-ui/src/pages/mixer.rs
git commit -m "refactor: UI sends only SetSolo — server handles muting (#155)"
```

---

## Task 6: Update E2E solo tests

**Files:**
- Modify: `iem-mixer/e2e/tests/live/mixer.spec.ts`

The existing solo tests should still pass since the behavior is the same (solo activates, exclusive mode works). We need to add a new test for crash recovery.

- [ ] **Step 1: Add solo crash recovery test**

After the "solo is exclusive" test (line 2241), add:

```typescript
  test("solo persists after disconnect and reconnect", async ({
    browser,
  }) => {
    // This tests the crash recovery: solo on → disconnect → reconnect → solo still active
    const ctx1 = await browser.newContext();
    const page1 = await ctx1.newPage();

    await page1.goto("/");
    await loginAs(page1, "petronela");
    await page1.goto("/petronela");
    await waitForMixer(page1);

    await expect(page1.locator(".channel").first()).toBeVisible({ timeout: 15000 });

    const micsTab = page1.locator(".category-tab.mics");
    if ((await micsTab.count()) > 0) await micsTab.click();
    await page1.waitForTimeout(500);

    const soloBtn = page1.locator(".solo-btn").first();
    expect(await soloBtn.count()).toBeGreaterThan(0);

    // Activate solo
    await soloBtn.click({ force: true });
    await expect(soloBtn).toHaveClass(/on/, { timeout: 3000 });

    // Close the page (simulates crash/disconnect)
    await ctx1.close();

    // Wait for server to process disconnect
    await new Promise((r) => setTimeout(r, 1000));

    // Reconnect with a new context (simulates reopening PWA)
    const ctx2 = await browser.newContext();
    const page2 = await ctx2.newPage();

    await page2.goto("/");
    await loginAs(page2, "petronela");
    await page2.goto("/petronela");
    await waitForMixer(page2);

    await expect(page2.locator(".channel").first()).toBeVisible({ timeout: 15000 });

    const micsTab2 = page2.locator(".category-tab.mics");
    if ((await micsTab2.count()) > 0) await micsTab2.click();
    await page2.waitForTimeout(500);

    // Solo should still be active after reconnect
    const soloBtn2 = page2.locator(".solo-btn").first();
    await expect(soloBtn2).toHaveClass(/on/, { timeout: 5000 });

    // Unsolo to clean up — should restore pre-solo mutes
    await soloBtn2.click({ force: true });
    await expect(soloBtn2).toHaveClass(/off/, { timeout: 3000 });

    await ctx2.close();
  });
```

- [ ] **Step 2: Add solo unsolo restores mutes test**

After the crash recovery test, add a test that verifies mute states are properly restored:

```typescript
  test("unsolo restores original mute states", async ({ browser }) => {
    const ctx = await browser.newContext();
    const page = await ctx.newPage();

    await page.goto("/");
    await loginAs(page, "petronela");
    await page.goto("/petronela");
    await waitForMixer(page);

    await expect(page.locator(".channel").first()).toBeVisible({ timeout: 15000 });

    const micsTab = page.locator(".category-tab.mics");
    if ((await micsTab.count()) > 0) await micsTab.click();
    await page.waitForTimeout(500);

    const channels = page.locator(".channel");
    const count = await channels.count();
    expect(count).toBeGreaterThanOrEqual(2);

    // Record initial mute states
    const initialMutes: boolean[] = [];
    for (let i = 0; i < count; i++) {
      const cls = (await channels.nth(i).getAttribute("class")) || "";
      initialMutes.push(cls.includes("muted"));
    }

    // Solo first channel
    const soloBtn = page.locator(".solo-btn").first();
    await soloBtn.click({ force: true });
    await expect(soloBtn).toHaveClass(/on/, { timeout: 3000 });
    await page.waitForTimeout(300);

    // Verify non-solo channels are muted
    for (let i = 1; i < count; i++) {
      await expect(channels.nth(i)).toHaveClass(/muted/, { timeout: 2000 });
    }

    // Unsolo
    await soloBtn.click({ force: true });
    await expect(soloBtn).toHaveClass(/off/, { timeout: 3000 });
    await page.waitForTimeout(500);

    // Verify mute states match initial
    for (let i = 0; i < count; i++) {
      const cls = (await channels.nth(i).getAttribute("class")) || "";
      const nowMuted = cls.includes("muted");
      expect(nowMuted).toBe(initialMutes[i]);
    }

    await ctx.close();
  });
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/e2e/tests/live/mixer.spec.ts
git commit -m "test: add solo crash recovery and mute restore E2E tests (#155)"
```

---

## Task 7: Handle empty URL return from apply_command_to_cache

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs`

- [ ] **Step 1: Read the caller and add empty URL guard**

Read lines 1366-1395 of `proxy.rs` where `apply_command_to_cache` result is used. If the caller sends the URL to REAPER unconditionally, wrap the REAPER HTTP call in an `if !url.is_empty()` check.

The SetSolo arm returns `Ok(("".to_string(), None))` since it already sent its own REAPER commands internally.

- [ ] **Step 2: Commit (if changes needed)**

```bash
git add iem-mixer/crates/iem-server/src/proxy.rs
git commit -m "fix: skip REAPER request for empty URL (SetSolo handles internally) (#155)"
```

---

## Task 8: Format check + Push + Monitor CI

- [ ] **Step 1: Run local format check**

```bash
cd iem-mixer && cargo fmt --all --check
cd iem-mixer/iem-ui && cargo fmt --all --check
```

Fix any formatting issues with `cargo fmt --all`.

- [ ] **Step 2: Push and monitor**

```bash
git push origin dev
gh run list --limit 3
```

- [ ] **Step 3: Monitor CI until ALL jobs reach terminal state**

Poll with `gh run view <run-id>` every 5 minutes. All 9 jobs must pass including deploy and post-deploy E2E.

- [ ] **Step 4: If CI fails, investigate with `gh run view <id> --log-failed` and fix all issues in ONE commit**

---

## Task 9: Update changelog + Create PR

- [ ] **Step 1: Add changelog entry to README.md**

```markdown
### v1.137.0 (2026-04-09)
- **Fix**: Solo no longer leaves tracks muted after PWA crash/disconnect (#155)
- **Feature**: Server-managed solo state persists across reconnects
```

- [ ] **Step 2: Commit and push**

```bash
git add README.md
git commit -m "docs: add changelog for v1.137.0 server-side solo state (#155)"
git push origin dev
```

- [ ] **Step 3: Create PR**

```bash
gh pr create --title "fix: solo crash recovery — server-side mute state (#155)" \
  --body "$(cat <<'EOF'
## Summary
- Solo muting moved from UI to server — prevents orphaned mutes on crash
- Pre-solo mute states saved server-side, restored on unsolo
- Solo persists across WebSocket disconnects until explicitly turned off
- Fixes #155

## Test plan
- [ ] Solo on → unsolo → mute states restored correctly
- [ ] Solo on → kill PWA → reopen → solo still active → unsolo → mutes restored
- [ ] Solo exclusive mode still works (solo A, then solo B replaces A)
- [ ] Multi-tab: solo syncs across tabs
- [ ] Post-deploy E2E passes 171+ tests

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 4: Monitor PR CI until green, verify mergeable**

---

## Task Dependencies

```
Task 1 (version bump)                  ──┐
Task 2 (pre_solo_mutes field)          ──┤── Sequential (each builds on previous)
Task 3 (SetSolo server logic)          ──┤
Task 4 (remove disconnect cleanup)     ──┤
Task 5 (UI simplification)            ──┤
Task 6 (E2E tests)                     ──┤
Task 7 (empty URL guard)              ──┘
         │
         ▼
Task 8 (format + push + CI)
         │
         ▼
Task 9 (changelog + PR)
```

Tasks 1-7 are sequential (each modifies files that depend on previous changes). Tasks 8-9 are post-implementation.
