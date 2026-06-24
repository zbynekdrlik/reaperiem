---
name: testing
description: IEM Mixer test patterns — live E2E safety rules, audio pipeline signal verification, post-deploy verification on iem.lan, test architecture. Load when writing tests, debugging CI failures, or planning test coverage.
---

# REAPER IEM — Testing Skill

## Live E2E Test Safety (Production-Safe Writes)

Live E2E tests run against REAL REAPER and REAL band member data. Follow these rules:

**Use member logins only — NEVER engineer login.** Engineer page has different UI (no stems bus, no "Me" fader, different channel ordering, EQ visibility differs) — 38 of 171 tests failed when switched to engineer login.

**Safe test members (known default PIN "7711"):**
- `petronela` and `stevo` only — these two have the default PIN
- ANI and MAREK have custom PINs (not "7711") — never hardcode for them
- Member "petka" no longer exists — do not use

**REAPER state protection via CI save/restore:**
```yaml
# In CI deploy job — ALWAYS
- run: curl -sf "http://iem.lan:8080/_/40026"    # Save before tests
  name: Save REAPER project
- run: <run tests>
- run: <run revert_project.lua>
  if: always()    # Restore even if tests fail
```

This is the safety net — not login isolation. Member PINs are production data; never reset them.

---

## Audio Pipeline Testing — Verify Signal, Not Just Bytes

**Every audio pipeline test must verify decoded audio contains real signal, not just that bytes arrive.**

The failure mode: CI passed 20+ times while audio was broken — tests only checked binary WebSocket frames arrived, never that they contained valid audio. User's phone showed "Listening" but played nothing.

**Required checks:**
1. `/api/audio/diagnostics` must show `peak_db > -40` (not silence)
2. `opus_frames_per_second > 40` (pipeline actively encoding)
3. Use tone generator ReaScript (`_RS_REAPERIEM_TONE_GEN`) to produce known 440 Hz signal for deterministic testing
4. CI deploy **MUST FAIL** if audio diagnostics show silence during tone gen test

**Never claim "audio pipeline verified" based on log grepping** — verify actual signal level.

---

## Post-Deploy E2E Tests on iem.lan (MANDATORY)

Every feature that deploys to iem.lan **MUST** have a post-deploy verification step in the CI deploy job that runs ON iem.lan against the live app.

**Why:** Claude delivered "tests" on GitHub Actions Linux runner with synthetic data 20+ times — all CI green, feature broken on real deployment. User had to test manually each time.

**Pattern for deploy job:**
```yaml
- name: Verify feature on live system
  run: |
    # Test against http://localhost:80 on iem.lan (NOT the CI runner)
    curl -sf "http://localhost:80/api/version"  # liveness
    # Feature-specific verification against LIVE app
  # continue-on-error: FORBIDDEN — must fail deploy if broken
```

**Audio pipeline deploy verification:**
```yaml
- name: Verify audio pipeline
  run: |
    # Connect WebSocket, send ListenStart, verify binary Opus frames arrive
    # NOT just checking log grepping
```

`continue-on-error: true` is **FORBIDDEN** on deploy verification steps.

---

## Testing Quality — Playwright Verification

**NEVER claim a UI feature works based only on CI passing.** CI tests are synthetic.

After deploy, use Playwright MCP (`mcp__plugin_playwright_playwright__*`) to:
1. Open `https://iem.newlevel.media/` or `http://10.77.9.231/` in real browser
2. Navigate to the feature, interact with it, take screenshots
3. For slider/controls: verify value changes on drag, persists, curve updates, changes reach REAPER
4. Report VERIFIED results with evidence (screenshots, API responses)

**For user-reported bugs — Phase 1 before any code fix:**
1. Write E2E test against REAL deployed system (iem.lan) that FAILS (reproduces the bug)
2. Phase 2: write fix
3. Phase 3: deploy and confirm E2E passes on live system

**CI E2E vs Deploy E2E distinction:**
- CI E2E (GitHub runner): Page loads, UI rendering, basic interactions — MUST NOT use `assume()` or gracefully skip. If REAPER is needed, the test belongs in the deploy E2E job.
- Deploy E2E (self-hosted runner on iem.lan): Full REAPER integration — fader→REAPER, mute→REAPER, EQ→REAPER.

**A test that passes when its dependency is down is a lie.** Skipped tests are lies.

---

## Test Architecture

**Test file locations:**
- `iem-mixer/crates/*/src/*.rs` — Unit tests (inline `#[cfg(test)]`)
- `iem-mixer/tests/` — Integration tests
- `iem-mixer/e2e/` — Playwright e2e tests

**E2E `assume()` helper:**
- `assume(condition, message)` — logs `[ASSUME SKIP]` and returns false when REAPER-dependent precondition not met
- Does NOT trigger test integrity check (no `.skip(` pattern)
- Allows E2E tests to pass in CI without REAPER while being explicit about skips
- Only valid in E2E tests — never in unit/integration tests

**Test integrity scan (CI blocks these):**
- `assume()`, `test.skip()`, and graceful-skip patterns in non-E2E tests → CI fails
- `continue-on-error: true` in any CI workflow step → CI fails
