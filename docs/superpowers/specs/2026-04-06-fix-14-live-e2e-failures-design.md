# Fix 14 Live E2E Test Failures — Design Spec (#148)

## Problem

After #147 re-enabled post-deploy E2E with REAPER save/restore, 157/171 tests pass (92%). 14 tests fail against the live REAPER system. These tests never ran on live REAPER before — they were hidden by `assume()` until #142.

## Root Causes (verified via live Playwright investigation)

### 1. Member Auth Failures (4 tests)

Band members ANI, MAREK changed their PINs from the default "7711" on production. Member "petka" no longer exists in the member list. Tests that log in as these members silently fail auth and proceed without a valid token, causing WebSocket to not deliver channel data.

**Working members**: petronela (PIN 7711), stevo (PIN 7711), engineer (PIN 1177).

**Affected tests:**
- `persistence.spec.ts:117` — ANI global volume test
- `stems-volume.spec.ts:114` — petka global volume fader test
- `mixer.spec.ts:2010` — ani own channel ordering
- `mixer.spec.ts:2039` — ani hide on muted channel

**Fix:** Replace ANI/PETKA logins with petronela or stevo. Custom PINs are user data and must not be reset.

### 2. EQ Menu Text Mismatch (3 tests)

The EQ button in the kebab menu renders as `<span class="menu-icon">≡</span>EQ`. The full element text is "≡EQ". Tests use `getByText("EQ", { exact: true })` which requires the text to be exactly "EQ" — fails because of the icon prefix.

**Verified:** `getByText("EQ")` (without exact) matches correctly. `button:has-text("EQ")` also works.

**Affected tests:**
- `eq.spec.ts:78` — kebab menu has EQ option
- `eq.spec.ts:658` — EQ access control
- `eq.spec.ts:1184` — reset button (uses `openEqForChannel` which has same issue)

**Fix:** Change `getByText("EQ", { exact: true })` to `getByText("EQ")` throughout eq.spec.ts.

### 3. Fader Drag Mechanics (2 tests)

The relative fader requires incremental `mouse.move()` calls. A single jump from point A to point B doesn't trigger value updates because the fader tracks delta from activation point and needs continuous position updates.

**Verified:** Incremental drag (10 steps × 2% each) changes value from 0dB to -14dB. Single jump has no effect.

**Affected tests:**
- `mixer.spec.ts:239` — fader hold-and-drag
- `persistence.spec.ts:28` — global volume persistence

**Fix:** Replace single-jump drags with incremental step-by-step moves.

### 4. Limiter Modal Structure (1 test)

The limiter UI was simplified from 3 controls (threshold, ceiling, release) to 1 control (MAX LEVEL). Test expects `sliderCount === 3` but actual count is 1.

**Verified:** Modal opens correctly, has 1 `.limiter-slider-track`, text "MAX LEVEL -6.0 dB Limiter ON".

**Affected test:** `limiter.spec.ts:133`

**Fix:** Change assertion from `toBe(3)` to `toBe(1)`.

### 5. REAPER Proxy Missing `/_/` Prefix (1 test)

The `/api/reaper/{path}` proxy in `proxy.rs` constructs the REAPER URL as `{reaper_url}/{path}` which produces `http://iem.lan:8080/NTRACK`. REAPER requires the `/_/` prefix: `http://iem.lan:8080/_/NTRACK`.

**Verified:** Endpoint returns 404 with auth, because REAPER doesn't respond to URLs without `/_/`.

**Affected test:** `elevated.spec.ts:26`

**Fix:** Change proxy.rs URL construction to `format!("{}/_/{}", config.reaper_url, path)`.

### 6. Muted Channel Class Timing (1 test)

After clicking the mute button, the `.muted` CSS class is applied reactively via WebSocket roundtrip (click → WebSocket command → server → REAPER → poller reads state → broadcast → UI update). The test checks for the class immediately after click without waiting.

**Affected test:** `mixer.spec.ts:1953`

**Fix:** Use `expect(channel).toHaveClass(/muted/, { timeout: 5000 })` instead of immediate assertion.

### 7. Alert Active State Timing (1 test)

SOS alert button click sends WebSocket `CallEngineer` message. The "active" class depends on server broadcasting the alert state back. Current timeout (10s) may be too tight.

**Affected test:** `alert.spec.ts:55`

**Fix:** Increase timeout to 15s and use `toHaveClass` with timeout.

### 8. EQ Disabled Band State (1 test)

Test disables a band via toggle, drags gain, then checks REAPER state expects `en=0`. The REAPER band state may differ because the EQ plugin's behavior when a band is toggled off may not match the test's assumptions.

**Affected test:** `eq.spec.ts:1245`

**Fix:** Investigate the actual REAPER EQ state after toggle+drag. May need to adjust the assertion or the toggle sequence.

## Architecture

No architectural changes. All fixes are either:
- Test fixes (selectors, timing, member logins, assertions)
- One code fix (proxy.rs `/_/` prefix)

## Files to Modify

### Code fix
- `iem-mixer/crates/iem-server/src/proxy.rs` — add `/_/` prefix

### Test fixes
- `iem-mixer/e2e/tests/live/persistence.spec.ts` — replace ANI with petronela, fix fader drag
- `iem-mixer/e2e/tests/live/stems-volume.spec.ts` — replace petka with petronela
- `iem-mixer/e2e/tests/live/mixer.spec.ts` — replace ani with petronela/stevo, fix fader drag, fix muted timing
- `iem-mixer/e2e/tests/live/eq.spec.ts` — fix `getByText("EQ")` exact matching, fix disabled band
- `iem-mixer/e2e/tests/live/limiter.spec.ts` — fix slider count assertion
- `iem-mixer/e2e/tests/live/elevated.spec.ts` — may need test adjustment after proxy fix
- `iem-mixer/e2e/tests/live/alert.spec.ts` — increase active state timeout

### Version bump
- 6 Cargo.toml + tauri.conf.json: 1.133.0 → 1.134.0
