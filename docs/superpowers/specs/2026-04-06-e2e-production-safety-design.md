# Design: Production-Safe Live E2E Tests (#147)

**Date:** 2026-04-06
**Issue:** #147
**Depends on:** #142 (assume removal — completed in PR #146)
**Goal:** Rewrite all 17 live E2E test files so they never modify band member mixes on production REAPER. Re-enable post-deploy E2E in CI.

## Problem

Live E2E tests (in `e2e/tests/live/`) run against production REAPER on iem.lan after CI deploy. Currently:

- **55 tests** log in as `petronela` and drag her faders, change EQ, toggle mute/solo
- **5 tests** log in as `petka` and modify stems faders
- **4 tests** log in as `ani` and change global volume
- **1 test** logs in as `stevo`
- **1 test** triggers SOS alert — sends real push notifications to all band members

When #142 enabled post-deploy E2E, band members received SOS alerts and saw their mixes change during CI. Post-deploy E2E was immediately disabled (PR #146, issue #147 created).

## Safety Rules

1. **Engineer-only writes** — tests that change values (faders, EQ, mute/solo, pan, limiter) must log in as `engineer` (PIN `1177`) and operate on `/engineer` page only
2. **Read-only for band members** — tests that log in as band members may only verify UI renders (check elements exist, read text), never change values
3. **REAPER project save/restore** — CI saves the project before tests and reverts after (pass or fail)
4. **No real SOS/alerts** — mock the `/api/alert` endpoint to prevent push notifications

## Design

### REAPER State Guard (CI-level)

Before post-deploy E2E starts:
```bash
curl -sf "http://iem.lan:8080/_/40026"   # Save project (action 40026)
sleep 2
```

After tests complete (regardless of pass/fail, using `if: always()`):

REAPER has no built-in "revert to saved" action. Instead, we write a small ReaScript (`revert_project.lua`) that reloads the current project file:

```lua
-- revert_project.lua: Reload current project from last saved state
local _, project_path = reaper.EnumProjects(-1)
if project_path ~= "" then
  reaper.Main_openProject(project_path)
  reaper.SetExtState("reaperiem", "revert_result", "OK:" .. project_path, false)
else
  reaper.SetExtState("reaperiem", "revert_result", "ERROR:no_project", false)
end
```

CI triggers it via HTTP API:
```bash
curl -sf "http://iem.lan:8080/_/_RS_REAPERIEM_REVERT_PROJECT"
sleep 3
curl -sf "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/revert_result"
# Expected: OK:C:\Users\newlevel\Documents\reaperiem\reaperiem.RPP
```

This restores all REAPER state: sends, FX params, routing, track volumes, mutes. No surgical API restore needed.

### Test Conversion Pattern

All write tests change from member login to engineer login on engineer's own mixer:

```typescript
// BEFORE (unsafe — modifies petronela's REAPER sends)
await loginAs(page, "petronela");
await page.goto("/petronela");

// AFTER (safe — only modifies engineer's REAPER sends)
await loginAs(page, "engineer", "1177");
await page.goto("/engineer");
```

The engineer's `/engineer` page has identical UI: faders, EQ, channels, mute/solo, pan. Test logic and assertions remain unchanged — only the login and URL change.

### Alert Mock Pattern

For alert.spec.ts, intercept the alert API to prevent real push notifications:

```typescript
await page.route("**/api/alert", (route) => {
  route.fulfill({ status: 200, body: JSON.stringify({ ok: true }) });
});
```

Tests still click the SOS button and verify UI behavior, but no notifications reach band members.

### File-by-File Strategy

**5 files with heavy writes (rewrite to engineer):**

| File | Lines | Current logins | Write operations | Change |
|------|-------|---------------|------------------|--------|
| mixer.spec.ts | 2178 | 51x petronela, 1x petka, 2x ani, 1x stevo | 256 (faders, mute, solo, pan) | All writes → engineer /engineer |
| eq.spec.ts | 1242 | 15x petronela, 1x engineer | 66 (EQ sliders, gain, freq) | All writes → engineer /engineer |
| persistence.spec.ts | 216 | 3x petronela, 2x ani | 24 (global volume fader) | All writes → engineer /engineer |
| stems-volume.spec.ts | 124 | 5x petka | 17 (tab clicks, stems fader) | All writes → engineer /engineer |
| alert.spec.ts | 134 | 2x engineer, 2x dynamic member | 3 (SOS button click) | Mock /api/alert, keep engineer login |

**5 files already safe (verify only):**

| File | Lines | Why safe |
|------|-------|---------|
| engineer-listen-member.spec.ts | 742 | Already uses engineer login |
| engineer-mute-all.spec.ts | 213 | Already uses engineer login |
| limiter.spec.ts | 161 | Already uses engineer login |
| talkback.spec.ts | 93 | Already uses engineer login |
| elevated.spec.ts | 101 | API-only, no UI writes |

**7 files to audit and fix if needed:**

| File | Lines | Audit focus |
|------|-------|------------|
| audio-e2e.spec.ts | 241 | Check for REAPER write operations |
| audio-listen.spec.ts | 262 | Check for REAPER write operations |
| audio-pipeline.spec.ts | 416 | Check for REAPER write operations |
| listen-quality.spec.ts | 139 | Check for REAPER write operations |
| listen-state-sync.spec.ts | 457 | Check for REAPER write operations |
| preset-input.spec.ts | 70 | Check for REAPER write operations |
| smoke-live.spec.ts | 122 | Check for REAPER write operations |

Any member-login writes found during audit get converted to engineer login.

### CI Workflow Changes

In `.github/workflows/ci.yml` deploy job:

```yaml
# Before E2E tests
- name: Save REAPER project
  run: curl -sf "http://iem.lan:8080/_/40026" && sleep 2

# Run E2E
- name: Post-deploy E2E
  run: |
    cd iem-mixer/e2e
    npx playwright test --config=playwright.live.config.ts

# After E2E (always, even on failure)
- name: Restore REAPER project
  if: always()
  run: |
    curl -sf "http://iem.lan:8080/_/_RS_REAPERIEM_REVERT_PROJECT"
    sleep 3
    curl -sf "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/revert_result"
```

### Test-Integrity Enforcement

The existing CI test-integrity job already bans `assume()` patterns. The production safety rules (engineer-only writes, no member writes) are enforced by:

1. This rewrite (removes all unsafe patterns)
2. Code review on future PRs
3. The REAPER save/restore guard as a safety net

### What Changes vs. What Doesn't

**Changes:**
- Login member and page URL in all write tests (member → engineer)
- Alert test gets API mock for push notifications
- CI gets save/restore steps around E2E
- Post-deploy E2E re-enabled

**Doesn't change:**
- Test logic, assertions, UI selectors
- Test file locations (still in `e2e/tests/live/`)
- CI E2E tests (in `e2e/tests/`, run on ubuntu-latest)
- Number of tests or coverage

## Success Criteria

1. All 17 live test files audited and safe
2. Zero tests log in as band members for write operations
3. CI save/restore protects REAPER state on every run
4. Post-deploy E2E re-enabled and passing
5. Band members never see mix changes or alerts from CI
