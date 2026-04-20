# Listen "No Source" Regression Fix + E2E Hardening — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Root-cause-fix the Listen button regression (engineer hears "No Source" because server forwards zero binary frames over `/ws/audio`) AND close the 7 silent-skip E2E gaps that let this ship green for months.

**Architecture:** One PR on branch `dev`. Five parts in TDD order — version bump, hardened test-integrity CI scan, removal of silent-skip violations, new binary-frames-or-die E2E test, CI reordering, server instrumentation, then root-cause fix as the final commit after reading the instrumented logs from the RED CI run.

**Tech Stack:** Rust (tokio, axum, tracing, std::sync::atomic), Playwright TypeScript, GitHub Actions bash + PowerShell.

**Spec:** `docs/superpowers/specs/2026-04-20-listen-no-source-regression-design.md`

---

## Hard gates (airuleset)

- **T1 version bump (1.156.0 → 1.157.0) + README changelog MUST be first commit on `dev`.**
- **Work on `dev` only** — no feature branches, no worktrees. Project root: `/home/newlevel/devel/reaperiem`.
- **Local checks:** only `cd iem-mixer && cargo fmt --all --check` runs locally. Hooks block `cargo test`, `cargo build`, `cargo clippy`, `cargo check`. Never bypass.
- **Self-hosted Windows runner** for `iem-lan` jobs — never use `shell: bash` for steps that run on the self-hosted runner. `shell: powershell` only.
- **Single PR dev → main** at the end. Verify `{mergeable: true, mergeable_state: "clean"}`. **STOP at green PR URL. DO NOT merge** without explicit user approval.
- **CI monitoring:** single background `sleep 300 && gh run view <id> --json status,conclusion,jobs` pattern. **NO `/loop`, no `CronCreate`, no custom bash monitor scripts.**
- **TDD red → green:** T4's new E2E test WILL FAIL on first CI run against production (proving the regression exists and is now detected). T7 adds the root-cause fix as a second commit in the SAME PR, turning CI green before merge.
- **All new Playwright tests assert `expect(consoleMessages).toEqual([])`** per airuleset `browser-console-zero-errors.md`.
- **Production safety:** tests that write REAPER mute state (member-target Listen) MUST send `ListenStop` in a `finally` block. The server also restores mutes on WS disconnect, but belt-and-suspenders per MEMORY `feedback_live_test_safety.md`.

---

## Task complexity and model selection

- **T1** (version bump + README changelog): Haiku — mechanical sed + README insert.
- **T2** (CI test-integrity scan extension): Sonnet — regex needs to catch the 7 real violations without false positives.
- **T3** (remove 7 silent-skip violations, convert to hard asserts): Sonnet — 3 files, must preserve legitimate waits, add console-errors check.
- **T4** (new `audio-listen-e2e.spec.ts`): Sonnet — careful WebSocket test + production-safe finally block.
- **T5** (CI reorder): Haiku — move one step block.
- **T6** (server instrumentation + `frames_forwarded` diagnostics field): Sonnet — Rust concurrency, `Arc<AtomicU64>`, thread through `handle_audio_ws`.
- **T7** (push → monitor → read RED logs → root-cause fix): Sonnet with escalation to Opus if the cause turns out to involve deep async semantics.
- **T8** (PR creation + mergeable verification + STOP): Sonnet — `gh pr create`, verify `mergeable_state=clean`, report URL.

---

## File Map

### Code files

- `iem-mixer/crates/iem-core/Cargo.toml` — version bump
- `iem-mixer/Cargo.toml` — version bump
- `iem-mixer/crates/iem-server/Cargo.toml` — version bump
- `iem-mixer/iem-ui/Cargo.toml` — version bump
- `iem-mixer/src-tauri/Cargo.toml` — version bump
- `iem-mixer/src-tauri/tauri.conf.json` — version bump
- `README.md` — changelog entry
- `.github/workflows/ci.yml` — extend test-integrity scan (around line 148), reorder tone-deactivate step (move block at lines 2142-2159 to after step at line 2316-2321)
- `iem-mixer/e2e/tests/live/audio-listen.spec.ts` — delete silent-skip block at lines 93-102; add hard asserts
- `iem-mixer/e2e/tests/live/audio-e2e.spec.ts` — delete 4 silent-skip blocks (lines 38-41, 63-67, 73-79, 137-140, 170-186); convert to hard asserts
- `iem-mixer/e2e/tests/live/listen-quality.spec.ts` — delete silent-skip at line 69
- `iem-mixer/e2e/tests/live/audio-listen-e2e.spec.ts` — **NEW** binary-frames-or-die test
- `iem-mixer/crates/iem-server/src/audio_stream.rs` — add `frames_forwarded: u64` to `AudioDiagnostics`
- `iem-mixer/crates/iem-server/src/proxy.rs` — 4 `tracing::info!` in `handle_audio_ws` + `frames_forwarded` counter update at line 3166 (after successful binary send); root-cause fix commit (unknown exact location until logs read)

---

## Task 1: Version bump 1.156.0 → 1.157.0 + README changelog

**Files:**
- Modify: all 5 × `Cargo.toml`, `iem-mixer/src-tauri/tauri.conf.json`, `README.md`

- [ ] **Step 1: Bump version in all Rust manifests and tauri config**

```bash
cd /home/newlevel/devel/reaperiem
sed -i 's/version = "1.156.0"/version = "1.157.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.156.0"/"version": "1.157.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify**

```bash
grep -c '1.157.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
# Expected: both output "1"
```

- [ ] **Step 3: Insert changelog block in `README.md`**

Open `README.md` and locate the line `### v1.156.0 (2026-04-19)` (at line 9 currently). Insert the following block IMMEDIATELY BEFORE that line (so the new version appears above v1.156.0):

```markdown
### v1.157.0 (2026-04-20)

- **Fix**: Engineer "Listen" button — restore binary audio streaming to the browser (regression: server accepted ListenStart but forwarded zero Opus frames, leaving the button stuck on "No Source" after 5 s timeout).
- **CI hardening**: extend `test-integrity` to reject silent-skip patterns in live E2E tests (`console.log("[SKIP]"`, `return;` after `count()`/auth guards, `catch {}` around `waitForFunction`).
- **E2E**: new binary-frames-or-die test `audio-listen-e2e.spec.ts` asserts ≥30 Opus frames within a 3 s ListenStart window against live REAPER with the tone generator active.
- **Diagnostics**: `/api/audio/diagnostics` now reports `frames_forwarded` (count of Opus frames sent on `/ws/audio` since app start) to catch pipeline breaks in production between deploys.

```

- [ ] **Step 4: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json README.md
git commit -m "$(cat <<'EOF'
chore: bump version to 1.157.0 + changelog for Listen regression fix

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Harden `test-integrity` CI scan to reject silent-skip patterns

**Files:**
- Modify: `.github/workflows/ci.yml` — replace the "Ban assume()/skip patterns in E2E tests" step (currently at lines 132-148)

- [ ] **Step 1: Replace the existing step with an expanded scan**

Open `.github/workflows/ci.yml`, locate the existing block:

```yaml
      - name: Ban assume()/skip patterns in E2E tests
        run: |
          echo "Checking for banned skip patterns in E2E tests..."
          FOUND=0
          for pattern in "function assume(" "if (!assume(" "if (!(await waitForMixer" "ASSUME SKIP"; do
            HITS=$(grep -rn "$pattern" iem-mixer/e2e/tests/ 2>/dev/null || true)
            if [ -n "$HITS" ]; then
              echo "BANNED pattern found: $pattern"
              echo "$HITS"
              FOUND=1
            fi
          done
          if [ "$FOUND" = "1" ]; then
            echo "::error::E2E tests contain banned assume/skip patterns. Tests must use expect() and fail loudly."
            exit 1
          fi
          echo "OK: No assume/skip patterns found in E2E tests"
```

and replace it in-place with the extended scan below:

```yaml
      - name: Ban assume()/skip patterns in E2E tests
        run: |
          echo "Checking for banned skip patterns in E2E tests..."
          FOUND=0

          # Literal-string patterns (fast grep pass)
          for pattern in "function assume(" "if (!assume(" "if (!(await waitForMixer" "ASSUME SKIP" 'console.log("[SKIP]' 'console.log("[INFO] Listen button did not transition'; do
            HITS=$(grep -rn -F "$pattern" iem-mixer/e2e/tests/ 2>/dev/null || true)
            if [ -n "$HITS" ]; then
              echo "BANNED pattern found: $pattern"
              echo "$HITS"
              FOUND=1
            fi
          done

          # Regex patterns — silent "return;" after guard expressions
          # Matches: if (!(await X.count())) return;  | if (btnCount === 0) { ... return; }  | if (!resp.ok) return;
          REGEX_HITS=$(grep -rnE 'if \(!\(await [A-Za-z_]+\.(count|status)\(\)\)\) return;' iem-mixer/e2e/tests/ 2>/dev/null || true)
          if [ -n "$REGEX_HITS" ]; then
            echo "BANNED pattern: silent return after count()/status() guard"
            echo "$REGEX_HITS"
            FOUND=1
          fi

          # Regex: catch { ... } swallowing a waitForFunction timeout (look for 'catch' within 5 lines after 'waitForFunction(')
          CATCH_HITS=$(grep -rnPzo '(?s)waitForFunction\([^)]*\)[^}]{0,400}\}\s*catch\s*\{[^}]*\}' iem-mixer/e2e/tests/ 2>/dev/null || true)
          if [ -n "$CATCH_HITS" ]; then
            echo "BANNED pattern: catch {} around waitForFunction() swallows timeout"
            echo "$CATCH_HITS"
            FOUND=1
          fi

          # Regex: unused hasStateChange-style variables (pattern check on button class without assertion)
          UNUSED_HITS=$(grep -rnE 'const hasStateChange\s*=' iem-mixer/e2e/tests/ 2>/dev/null || true)
          if [ -n "$UNUSED_HITS" ]; then
            echo "BANNED pattern: hasStateChange computed without expect() assertion"
            echo "$UNUSED_HITS"
            FOUND=1
          fi

          if [ "$FOUND" = "1" ]; then
            echo "::error::E2E tests contain banned assume/skip/silent-guard patterns. Tests must use expect() and fail loudly."
            exit 1
          fi
          echo "OK: No assume/skip/silent-guard patterns found in E2E tests"
```

- [ ] **Step 2: Self-test the new scan before committing**

Temporarily introduce a violation to prove the scan catches it:

```bash
cd /home/newlevel/devel/reaperiem
# Copy the scan block out of the workflow and run it locally against the tests as-is
# Expected: FAILS because the 7 existing violations in audio-listen.spec.ts, audio-e2e.spec.ts,
# listen-quality.spec.ts are still present (they get removed in T3).
bash -c 'FOUND=0
for pattern in "function assume(" "if (!assume(" "if (!(await waitForMixer" "ASSUME SKIP" "console.log(\"[SKIP]" "console.log(\"[INFO] Listen button did not transition"; do
  HITS=$(grep -rn -F "$pattern" iem-mixer/e2e/tests/ 2>/dev/null || true)
  if [ -n "$HITS" ]; then echo "HIT: $pattern"; echo "$HITS"; FOUND=1; fi
done
REGEX_HITS=$(grep -rnE "if \(!\(await [A-Za-z_]+\.(count|status)\(\)\)\) return;" iem-mixer/e2e/tests/ 2>/dev/null || true)
if [ -n "$REGEX_HITS" ]; then echo "HIT regex count/status"; echo "$REGEX_HITS"; FOUND=1; fi
UNUSED_HITS=$(grep -rnE "const hasStateChange\s*=" iem-mixer/e2e/tests/ 2>/dev/null || true)
if [ -n "$UNUSED_HITS" ]; then echo "HIT hasStateChange"; echo "$UNUSED_HITS"; FOUND=1; fi
echo "FOUND=$FOUND"'
```

Expected output: FOUND=1 with at least hits in `audio-listen.spec.ts`, `audio-e2e.spec.ts`, and `listen-quality.spec.ts`. This confirms the scan WILL detect the existing violations. T3 removes them. **Do not modify any test file in this task.** The scan must continue to show FOUND=1 until T3 lands; that's what proves the regex works.

- [ ] **Step 3: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add .github/workflows/ci.yml
git commit -m "$(cat <<'EOF'
ci: extend test-integrity to reject silent-skip patterns in E2E tests

Scan now rejects:
- console.log("[SKIP]
- console.log("[INFO] Listen button did not transition
- if (!(await X.count())) return;  /  if (!(await X.status())) return;
- catch {} wrapping waitForFunction()
- const hasStateChange = ... (computed without expect())

These are the patterns that let the Listen "No Source" regression
(engineer button perpetually shows No Source despite server receiving
OIEM audio at 50 pps) ship green for months. Aligns the gate with the
existing airuleset test-strictness.md rule that already bans them.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Remove 7 silent-skip violations in existing E2E tests

**Files:**
- Modify: `iem-mixer/e2e/tests/live/audio-listen.spec.ts` (lines 60-103)
- Modify: `iem-mixer/e2e/tests/live/audio-e2e.spec.ts` (lines 35-113 and 115-241)
- Modify: `iem-mixer/e2e/tests/live/listen-quality.spec.ts` (line 68-88)

- [ ] **Step 1: Fix `audio-listen.spec.ts` — "clicking Listen button opens audio WebSocket without errors"**

Locate the block at lines 60-103. Replace its body (from the `async ({ page })` opening to the closing `});`) with:

```typescript
  test("clicking Listen button opens audio WebSocket without errors", async ({
    page,
  }) => {
    // Collect ALL browser console errors and warnings (airuleset browser-console-zero-errors)
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        // Ignore known-benign warnings that appear on every page load
        if (text.includes("apple-mobile-web-app-capable")) return;
        if (text.includes("[push] subscribe await failed")) return;
        if (text.includes("integrity")) return;
        if (text.includes("vapid-key fetch error")) return;
        consoleMessages.push(`[${msg.type()}] ${text}`);
      }
    });

    await page.goto("/");
    await loginAs(page, "engineer", "1177");
    await page.goto("/engineer");
    await waitForMixer(page);

    await expect(page.locator(".toolbar")).toBeVisible({ timeout: 10000 });

    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });

    // Click the Listen button — this should open a WebSocket to /ws/audio
    await listenBtn.click();

    // Wait up to 8s for the button to reach a terminal state (listening OR no-source).
    // No catch{} — if the button never transitions, the test MUST fail.
    await page.waitForFunction(
      () => {
        const b = document.querySelector(".toolbar-btn-listen");
        if (!b) return false;
        return (
          b.classList.contains("listening") ||
          b.classList.contains("no-source")
        );
      },
      { timeout: 8000 },
    );

    // Hard assert: button MUST be in `listening` state — tone generator is active
    // during post-deploy E2E (see ci.yml). A real signal source should reach the browser.
    const finalClass = await listenBtn.getAttribute("class");
    expect(finalClass).toContain("listening");

    // Zero browser console errors/warnings during the flow
    expect(consoleMessages).toEqual([]);
  });
```

- [ ] **Step 2: Fix `audio-e2e.spec.ts` — replace silent skips with hard asserts in "Audio Pipeline Diagnostics" describe block**

In `iem-mixer/e2e/tests/live/audio-e2e.spec.ts`, find the `getEngineerToken` helper (lines 15-33). Replace the three `return null` / silent-return paths inside the three tests in the `Audio Pipeline Diagnostics` describe (lines 35-113) with `expect()`s.

Find this block at around line 36-46:

```typescript
  test("diagnostics endpoint returns valid structure", async ({ request }) => {
    const token = await getEngineerToken(request);
    if (!token) {
      console.log("[SKIP] Cannot authenticate as engineer");
      return;
    }
```

and replace it with:

```typescript
  test("diagnostics endpoint returns valid structure", async ({ request }) => {
    const token = await getEngineerToken(request);
    expect(token).toBeTruthy();
```

Find this block at around line 60-78:

```typescript
  test("when OIEM is active, audio signal is not silence", async ({
    request,
  }) => {
    const token = await getEngineerToken(request);
    if (!token) {
      console.log("[SKIP] Cannot authenticate as engineer");
      return;
    }

    const response = await request.get(`${BASE_URL}/api/audio/diagnostics`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    const diag = await response.json();

    if (!diag.receiving_oiem) {
      console.log(
        "[SKIP] No OIEM packets — REAPER not running or VST not active",
      );
      return;
    }
```

and replace it with:

```typescript
  test("when OIEM is active, audio signal is not silence", async ({
    request,
  }) => {
    const token = await getEngineerToken(request);
    expect(token).toBeTruthy();

    const response = await request.get(`${BASE_URL}/api/audio/diagnostics`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    const diag = await response.json();

    // Tone generator is active during post-deploy E2E (see ci.yml).
    // OIEM packets MUST be flowing — if not, the VST / pipeline / tone trigger is broken.
    expect(diag.receiving_oiem).toBe(true);
```

Find this block at around line 96-105:

```typescript
  test("non-engineer token rejected for diagnostics", async ({ request }) => {
    // Login as regular member
    const authResp = await request.post(`${BASE_URL}/api/auth`, {
      data: { member: "petronela", pin: "7711" },
    });
    if (authResp.status() !== 200) {
      console.log("[SKIP] Cannot authenticate as member");
      return;
    }
    const { token } = await authResp.json();
```

and replace it with:

```typescript
  test("non-engineer token rejected for diagnostics", async ({ request }) => {
    // Login as regular member
    const authResp = await request.post(`${BASE_URL}/api/auth`, {
      data: { member: "petronela", pin: "7711" },
    });
    expect(authResp.status()).toBe(200);
    const { token } = await authResp.json();
```

- [ ] **Step 3: Fix `audio-e2e.spec.ts` — replace silent skip + try/catch in "Browser Audio Playback" test**

In the same file, find the block at around lines 134-186:

```typescript
    // Check if Listen button exists
    const listenBtn = page.locator(".toolbar-btn-listen");
    const btnCount = await listenBtn.count();
    if (btnCount === 0) {
      console.log("[SKIP] Listen button not found on engineer page");
      return;
    }

    await expect(listenBtn).toBeVisible();
    const btnText = await listenBtn.textContent();

    // WebCodecs must be supported in Chromium
    expect(btnText).not.toContain("Unsupported");
```

and replace it with:

```typescript
    // Listen button MUST exist on engineer page — no silent skip
    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });
    const btnText = await listenBtn.textContent();

    // WebCodecs must be supported in Chromium
    expect(btnText).not.toContain("Unsupported");
```

Then find the try/catch block at lines 171-186:

```typescript
    try {
      await page.waitForFunction(
        () => {
          const btn = document.querySelector(".toolbar-btn-listen");
          return (
            btn &&
            (btn.classList.contains("listening") ||
              btn.textContent?.includes("No Source"))
          );
        },
        { timeout: 10000 },
      );
    } catch {
      // Timeout is OK — might not have audio source in CI
      console.log("[INFO] Listen button did not transition within 10s");
    }

    const afterClick = await listenBtn.textContent();
    console.log(`Listen button state after click: "${afterClick}"`);

    // Audio source must be available (REAPER running)
    expect(afterClick).not.toContain("No Source");
```

and replace the whole block with:

```typescript
    // No catch — waitForFunction MUST succeed. Tone generator is active during
    // post-deploy E2E, so the button must reach `listening` state within 10 s.
    await page.waitForFunction(
      () => {
        const btn = document.querySelector(".toolbar-btn-listen");
        if (!btn) return false;
        return (
          btn.classList.contains("listening") ||
          btn.textContent?.includes("No Source")
        );
      },
      { timeout: 10000 },
    );

    const afterClick = await listenBtn.textContent();
    // Audio source MUST be available (REAPER + tone generator running during E2E)
    expect(afterClick).not.toContain("No Source");
```

- [ ] **Step 4: Fix `listen-quality.spec.ts` — line 69**

In `iem-mixer/e2e/tests/live/listen-quality.spec.ts`, find this block at lines 67-74:

```typescript
    // Click listen button
    const listenBtn = page.locator(".toolbar-btn-listen");
    if (!(await listenBtn.count())) return;

    await listenBtn.click();

    // Wait for listening state (needs REAPER audio pipeline)
    await expect(page.locator(".toolbar-btn-listen.listening")).toBeVisible({ timeout: 5000 });
```

and replace it with:

```typescript
    // Click listen button — MUST exist on engineer page
    const listenBtn = page.locator(".toolbar-btn-listen");
    await expect(listenBtn).toBeVisible({ timeout: 5000 });

    await listenBtn.click();

    // Wait for listening state (needs REAPER audio pipeline — tone generator active during E2E)
    await expect(page.locator(".toolbar-btn-listen.listening")).toBeVisible({ timeout: 8000 });
```

- [ ] **Step 5: Self-verify locally**

The hardened scan from T2 should now pass against the current tree:

```bash
cd /home/newlevel/devel/reaperiem
bash -c 'FOUND=0
for pattern in "function assume(" "if (!assume(" "if (!(await waitForMixer" "ASSUME SKIP" "console.log(\"[SKIP]" "console.log(\"[INFO] Listen button did not transition"; do
  HITS=$(grep -rn -F "$pattern" iem-mixer/e2e/tests/ 2>/dev/null || true)
  if [ -n "$HITS" ]; then echo "HIT: $pattern"; FOUND=1; fi
done
REGEX_HITS=$(grep -rnE "if \(!\(await [A-Za-z_]+\.(count|status)\(\)\)\) return;" iem-mixer/e2e/tests/ 2>/dev/null || true)
if [ -n "$REGEX_HITS" ]; then echo "HIT regex"; FOUND=1; fi
UNUSED_HITS=$(grep -rnE "const hasStateChange\s*=" iem-mixer/e2e/tests/ 2>/dev/null || true)
if [ -n "$UNUSED_HITS" ]; then echo "HIT unused"; FOUND=1; fi
echo "FOUND=$FOUND"'
```

Expected output: `FOUND=0` (no violations remaining).

- [ ] **Step 6: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/e2e/tests/live/audio-listen.spec.ts \
        iem-mixer/e2e/tests/live/audio-e2e.spec.ts \
        iem-mixer/e2e/tests/live/listen-quality.spec.ts
git commit -m "$(cat <<'EOF'
test: remove 7 silent-skip violations in live listen tests

The hardened test-integrity scan would reject all of these. Tests now
fail loudly when preconditions are not met:
- engineer login MUST succeed (auth failure no longer silently returns)
- Listen button MUST exist (no "[SKIP] Listen button not found")
- diagnostics MUST show receiving_oiem=true (tone generator active during E2E)
- waitForFunction MUST resolve (no catch {} around it)
- Listen button MUST reach `listening` state (no_source is now a failure)
- All tests assert expect(consoleMessages).toEqual([]) per airuleset
  browser-console-zero-errors.md

These tests will now fail hard against the current regression, which is
exactly what we want.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: New binary-frames-or-die E2E test

**Files:**
- Create: `iem-mixer/e2e/tests/live/audio-listen-e2e.spec.ts`

- [ ] **Step 1: Create the new test file**

Create `iem-mixer/e2e/tests/live/audio-listen-e2e.spec.ts` with exactly this content:

```typescript
/**
 * #179.5 — Binary-frames-or-die E2E test for the Listen button.
 *
 * Opens /ws/audio as engineer, sends ListenStart, counts binary Opus frames
 * received over 3 seconds. The tone generator is active during post-deploy
 * E2E (see ci.yml), so the audio pipeline MUST deliver frames. Any silence
 * is a regression.
 *
 * This test is intentionally minimal and zero-tolerance — no try/catch,
 * no silent skips, no tolerance of "no_source".
 */

import { test, expect, Page } from "@playwright/test";

async function loginAsEngineer(page: Page): Promise<void> {
  const response = await page.request.post("/api/auth", {
    data: { member: "engineer", pin: "1177" },
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

async function probeWsAudio(
  page: Page,
  memberId: string,
  probeMs: number,
): Promise<{
  binCount: number;
  totalBytes: number;
  firstBinLatency: number | null;
  textMsgs: string[];
}> {
  return await page.evaluate(
    async ({ memberId, probeMs }) => {
      const auth = JSON.parse(localStorage.getItem("iem_token")!);
      const proto = location.protocol === "https:" ? "wss:" : "ws:";
      const url = `${proto}//${location.host}/ws/audio?token=${auth.token}`;
      const ws = new WebSocket(url);
      ws.binaryType = "arraybuffer";

      let binCount = 0;
      let totalBytes = 0;
      let firstBinMs: number | null = null;
      const textMsgs: string[] = [];

      ws.onmessage = (e: MessageEvent) => {
        if (e.data instanceof ArrayBuffer) {
          if (firstBinMs === null) firstBinMs = Date.now();
          binCount++;
          totalBytes += e.data.byteLength;
        } else if (typeof e.data === "string") {
          textMsgs.push(e.data);
        }
      };

      await new Promise<void>((res, rej) => {
        ws.onopen = () => res();
        ws.onerror = () => rej(new Error("ws connect failed"));
        setTimeout(() => rej(new Error("ws open timeout")), 3000);
      });

      const sentAt = Date.now();
      ws.send(JSON.stringify({ cmd: "ListenStart", member_id: memberId }));

      try {
        await new Promise((r) => setTimeout(r, probeMs));
      } finally {
        // Production-safe: always send ListenStop so the server restores
        // any saved member mute state (belt-and-suspenders with WS-disconnect
        // cleanup) per MEMORY feedback_live_test_safety.md.
        try {
          ws.send(JSON.stringify({ cmd: "ListenStop" }));
        } catch {
          /* send may fail if socket is already closed; disconnect cleanup covers us */
        }
        ws.close();
      }

      return {
        binCount,
        totalBytes,
        firstBinLatency: firstBinMs !== null ? firstBinMs - sentAt : null,
        textMsgs,
      };
    },
    { memberId, probeMs },
  );
}

test.describe("Listen /ws/audio binary-frames-or-die", () => {
  test("engineer ListenStart delivers binary Opus frames within 1s and >=30 frames in 3s", async ({
    page,
  }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        if (text.includes("apple-mobile-web-app-capable")) return;
        if (text.includes("[push] subscribe await failed")) return;
        if (text.includes("integrity")) return;
        if (text.includes("vapid-key fetch error")) return;
        consoleMessages.push(`[${msg.type()}] ${text}`);
      }
    });

    await page.goto("/");
    await loginAsEngineer(page);
    await page.goto("/engineer");
    await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible(
      { timeout: 10000 },
    );

    const result = await probeWsAudio(page, "engineer", 3000);

    // Hard assertions — zero tolerance
    expect(result.textMsgs.some((m) => m.includes('"status":"listening"'))).toBe(
      true,
    );
    expect(result.textMsgs.some((m) => m.includes('"status":"no_source"'))).toBe(
      false,
    );
    expect(result.binCount).toBeGreaterThanOrEqual(30);
    expect(result.totalBytes).toBeGreaterThan(1000);
    expect(result.firstBinLatency).not.toBeNull();
    expect(result.firstBinLatency!).toBeLessThan(1000);

    expect(consoleMessages).toEqual([]);
  });

  test("engineer ListenStart member_id=petronela delivers binary Opus frames (solo-mute path)", async ({
    page,
  }) => {
    const consoleMessages: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        const text = msg.text();
        if (text.includes("apple-mobile-web-app-capable")) return;
        if (text.includes("[push] subscribe await failed")) return;
        if (text.includes("integrity")) return;
        if (text.includes("vapid-key fetch error")) return;
        consoleMessages.push(`[${msg.type()}] ${text}`);
      }
    });

    await page.goto("/");
    await loginAsEngineer(page);
    await page.goto("/engineer");
    await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible(
      { timeout: 10000 },
    );

    // probeWsAudio handles the ListenStop-in-finally production-safety contract
    const result = await probeWsAudio(page, "petronela", 3000);

    expect(result.textMsgs.some((m) => m.includes('"status":"listening"'))).toBe(
      true,
    );
    expect(result.textMsgs.some((m) => m.includes('"status":"no_source"'))).toBe(
      false,
    );
    expect(result.binCount).toBeGreaterThanOrEqual(30);
    expect(result.totalBytes).toBeGreaterThan(1000);
    expect(result.firstBinLatency).not.toBeNull();
    expect(result.firstBinLatency!).toBeLessThan(1000);

    expect(consoleMessages).toEqual([]);
  });
});
```

- [ ] **Step 2: Verify the hardened scan still passes**

```bash
cd /home/newlevel/devel/reaperiem
# Same scan block as T3 step 5. Expected: FOUND=0.
```

- [ ] **Step 3: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/e2e/tests/live/audio-listen-e2e.spec.ts
git commit -m "$(cat <<'EOF'
test: binary-frames-or-die E2E for Listen /ws/audio

Opens WebSocket as engineer, sends ListenStart, asserts:
- first binary frame arrives within 1 s
- >=30 binary frames in 3 s (50 pps × 3 s ≈ 150; 30 = generous floor)
- >1 KB total bytes received
- no_source text message is NEVER emitted
- zero browser console errors/warnings

Two tests: engineer own-mix target, and petronela member target
(solo-mute path; ListenStop sent in finally for production-safe
mute restore).

This will FAIL on the current deploy (reproducing the regression),
and pass after the root-cause fix in the same PR.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Reorder CI — move `Deactivate tone generator` after post-deploy E2E

**Files:**
- Modify: `.github/workflows/ci.yml` — move the block at lines 2142-2159 to after the block at lines 2316-2321

- [ ] **Step 1: Cut the `Deactivate tone generator` step block**

Open `.github/workflows/ci.yml`. Locate the block starting at `- name: Deactivate tone generator` (line 2142) and ending at `Write-Host "Tone generator cleanup: $result"` (line 2159). Cut this entire 18-line block out of the file.

The block to cut:

```yaml
      - name: Deactivate tone generator
        if: always() && steps.reaper-pre.outputs.was_running == 'true'
        shell: powershell
        run: |
          Invoke-WebRequest -Uri "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/tone_gen_action/stop" -UseBasicParsing | Out-Null
          try {
            # Use dynamically-registered action ID
            $toneIdResp = (Invoke-WebRequest -Uri "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/action_tone_generator" -UseBasicParsing).Content
            $toneActionId = ($toneIdResp -split "`t")[-1]
            if ($toneActionId) {
              Invoke-WebRequest -Uri "http://iem.lan:8080/_/$toneActionId" -UseBasicParsing | Out-Null
            }
          } catch {
            Write-Host "::warning::Could not trigger tone gen stop: $_"
          }
          Start-Sleep -Seconds 1
          $result = (Invoke-WebRequest -Uri "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/tone_gen_result" -UseBasicParsing).Content
          Write-Host "Tone generator cleanup: $result"
```

- [ ] **Step 2: Paste the block after `Run post-deploy E2E tests`**

Paste the block IMMEDIATELY AFTER the `Run post-deploy E2E tests` step (ends at line 2321 with `E2E_BASE_URL: http://localhost`) and BEFORE `Restore REAPER project after E2E` (line 2323).

After the edit, the sequence in the file should be:

```yaml
      - name: Run post-deploy E2E tests
        shell: powershell
        working-directory: iem-mixer/e2e
        run: npx playwright test --config=playwright.live.config.ts --reporter=list
        env:
          E2E_BASE_URL: http://localhost

      - name: Deactivate tone generator
        if: always() && steps.reaper-pre.outputs.was_running == 'true'
        shell: powershell
        run: |
          Invoke-WebRequest -Uri "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/tone_gen_action/stop" -UseBasicParsing | Out-Null
          try {
            # Use dynamically-registered action ID
            $toneIdResp = (Invoke-WebRequest -Uri "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/action_tone_generator" -UseBasicParsing).Content
            $toneActionId = ($toneIdResp -split "`t")[-1]
            if ($toneActionId) {
              Invoke-WebRequest -Uri "http://iem.lan:8080/_/$toneActionId" -UseBasicParsing | Out-Null
            }
          } catch {
            Write-Host "::warning::Could not trigger tone gen stop: $_"
          }
          Start-Sleep -Seconds 1
          $result = (Invoke-WebRequest -Uri "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/tone_gen_result" -UseBasicParsing).Content
          Write-Host "Tone generator cleanup: $result"

      - name: Restore REAPER project after E2E
```

The `if: always()` guard on the deactivate step preserves cleanup even if the E2E step fails. **Do not change `shell: powershell`** — this is the self-hosted Windows runner, airuleset forbids `shell: bash` here.

- [ ] **Step 3: Verify the reorder**

```bash
cd /home/newlevel/devel/reaperiem
# Deactivate must come AFTER 'Run post-deploy E2E tests' in the file
awk '/- name: Run post-deploy E2E tests/{p=NR} /- name: Deactivate tone generator/{d=NR} END{print "run-e2e:", p, "deactivate:", d, "ok:", (d>p?"YES":"NO")}' .github/workflows/ci.yml
```

Expected: `ok: YES` and the two line numbers are close (within ~15 lines of each other).

- [ ] **Step 4: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add .github/workflows/ci.yml
git commit -m "$(cat <<'EOF'
ci: run post-deploy E2E before deactivating tone generator

Previously, the tone generator was turned off BEFORE post-deploy E2E
ran. That left Listen E2E tests with no audio signal source — meaning
"No Source" responses looked identical whether the pipeline was broken
or the tone was simply off. The binary-frames-or-die test needs a live
signal to make its assertions meaningful.

No logic changes; pure step reorder. if: always() on the deactivate
step preserves cleanup even when E2E fails.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Server instrumentation + `frames_forwarded` diagnostics counter

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/audio_stream.rs` — add `frames_forwarded: u64` to `AudioDiagnostics` + `Default`
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs::handle_audio_ws` — 4 `tracing::info!` lines + counter update after successful Binary send

- [ ] **Step 1: Add `frames_forwarded` field to `AudioDiagnostics`**

Edit `iem-mixer/crates/iem-server/src/audio_stream.rs`. Locate the `AudioDiagnostics` struct at lines 21-40. Add one new field AFTER `sequence_gaps`:

Change:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioDiagnostics {
    /// Whether OIEM packets are being received
    pub receiving_oiem: bool,
    /// Also exposed as receiving_vban for backwards compatibility with CI
    pub receiving_vban: bool,
    /// OIEM packets received per second
    pub packets_per_second: f32,
    /// Opus frames relayed per second (same as packets for OIEM)
    pub opus_frames_per_second: f32,
    /// Size of last Opus frame in bytes
    pub last_frame_size_bytes: usize,
    /// Peak dB of last packet (estimated from Opus frame size)
    pub peak_db: f32,
    /// Last received sequence number
    pub last_sequence: u16,
    /// Sequence gaps detected (dropped UDP packets)
    pub sequence_gaps: u64,
}
```

to:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioDiagnostics {
    /// Whether OIEM packets are being received
    pub receiving_oiem: bool,
    /// Also exposed as receiving_vban for backwards compatibility with CI
    pub receiving_vban: bool,
    /// OIEM packets received per second
    pub packets_per_second: f32,
    /// Opus frames relayed per second (same as packets for OIEM)
    pub opus_frames_per_second: f32,
    /// Size of last Opus frame in bytes
    pub last_frame_size_bytes: usize,
    /// Peak dB of last packet (estimated from Opus frame size)
    pub peak_db: f32,
    /// Last received sequence number
    pub last_sequence: u16,
    /// Sequence gaps detected (dropped UDP packets)
    pub sequence_gaps: u64,
    /// Total Opus frames forwarded over /ws/audio since app start (counts
    /// successful `Message::Binary` sends across all concurrent listeners).
    /// Used to detect pipeline breaks between deploys.
    pub frames_forwarded: u64,
}
```

Then locate the `Default` impl at lines 42-55 and add `frames_forwarded: 0,` at the end. Change:

```rust
impl Default for AudioDiagnostics {
    fn default() -> Self {
        Self {
            receiving_oiem: false,
            receiving_vban: false,
            packets_per_second: 0.0,
            opus_frames_per_second: 0.0,
            last_frame_size_bytes: 0,
            peak_db: -150.0,
            last_sequence: 0,
            sequence_gaps: 0,
        }
    }
}
```

to:

```rust
impl Default for AudioDiagnostics {
    fn default() -> Self {
        Self {
            receiving_oiem: false,
            receiving_vban: false,
            packets_per_second: 0.0,
            opus_frames_per_second: 0.0,
            last_frame_size_bytes: 0,
            peak_db: -150.0,
            last_sequence: 0,
            sequence_gaps: 0,
            frames_forwarded: 0,
        }
    }
}
```

- [ ] **Step 2: Add 4 `tracing::info!` lines + counter update in `handle_audio_ws`**

Edit `iem-mixer/crates/iem-server/src/proxy.rs`.

**Change A — at the producer spawn** (line 3105, inside the `ClientMsg::ListenStart` arm, just before `producer_handle = Some(tokio::spawn(async move {`):

Find:

```rust
                                    // Spawn frame dropper producer: reads broadcast, drops stale on backpressure
                                    let mut broadcast_rx = state.audio_tx.subscribe();
                                    let dropper_tx = frame_tx.clone();
                                    producer_handle = Some(tokio::spawn(async move {
                                        loop {
                                            match broadcast_rx.recv().await {
                                                Ok(frame) => {
                                                    // try_send: if channel full (TCP backpressure), drop this frame
                                                    let _ = dropper_tx.try_send(frame);
                                                }
                                                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                                    tracing::debug!("Audio dropper skipped {} stale frames", n);
                                                }
                                                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                            }
                                        }
                                    }));
```

Replace with (promote debug → info; add info on spawn; log closed):

```rust
                                    // Spawn frame dropper producer: reads broadcast, drops stale on backpressure
                                    let mut broadcast_rx = state.audio_tx.subscribe();
                                    let dropper_tx = frame_tx.clone();
                                    let sub_count = state.audio_tx.receiver_count();
                                    tracing::info!(
                                        subscriber_count = sub_count,
                                        "audio producer spawned for /ws/audio listener"
                                    );
                                    producer_handle = Some(tokio::spawn(async move {
                                        let mut first_frame_logged = false;
                                        loop {
                                            match broadcast_rx.recv().await {
                                                Ok(frame) => {
                                                    if !first_frame_logged {
                                                        tracing::info!(
                                                            frame_size = frame.len(),
                                                            "audio producer received first broadcast frame"
                                                        );
                                                        first_frame_logged = true;
                                                    }
                                                    // try_send: if channel full (TCP backpressure), drop this frame
                                                    if let Err(e) = dropper_tx.try_send(frame) {
                                                        tracing::debug!("audio dropper try_send failed: {}", e);
                                                    }
                                                }
                                                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                                    tracing::info!("audio broadcast Lagged: skipped {} stale frames", n);
                                                }
                                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                                    tracing::info!("audio broadcast Closed — producer exiting");
                                                    break;
                                                }
                                            }
                                        }
                                    }));
```

**Change B — at the successful binary send + first-frame-forwarded log + counter update** (line 3162-3174, the frame_rx branch):

Find:

```rust
            // Audio frames from dropper channel — only when listening
            frame = frame_rx.recv(), if is_listening => {
                match frame {
                    Some(data) => {
                        last_audio_time = Instant::now();
                        if socket.send(Message::Binary(data.to_vec().into())).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        // Channel closed — producer died
                        break;
                    }
                }
            }
```

Replace with (add info on first forwarded frame, update counter, log binary send error):

```rust
            // Audio frames from dropper channel — only when listening
            frame = frame_rx.recv(), if is_listening => {
                match frame {
                    Some(data) => {
                        last_audio_time = Instant::now();
                        let size = data.len();
                        if let Err(e) = socket.send(Message::Binary(data.to_vec().into())).await {
                            tracing::info!("audio binary send failed: {} — closing /ws/audio", e);
                            break;
                        }
                        if !first_frame_forwarded_logged {
                            tracing::info!(
                                frame_size = size,
                                "first binary frame forwarded on /ws/audio"
                            );
                            first_frame_forwarded_logged = true;
                        }
                        // Bump diagnostic counter (shared across all listeners).
                        if let Ok(mut diag) = state.audio_diagnostics.lock() {
                            diag.frames_forwarded = diag.frames_forwarded.saturating_add(1);
                        }
                    }
                    None => {
                        // Channel closed — producer died
                        tracing::info!("audio frame_rx closed — producer task died");
                        break;
                    }
                }
            }
```

**Change C — declare `first_frame_forwarded_logged` at the top of `handle_audio_ws`** (line 3044, next to the other mutables):

Find:

```rust
    let (frame_tx, mut frame_rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(5);
    let mut producer_handle: Option<tokio::task::JoinHandle<()>> = None;
    let mut last_audio_time = Instant::now();
    let mut is_listening = false;
```

Replace with:

```rust
    let (frame_tx, mut frame_rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(5);
    let mut producer_handle: Option<tokio::task::JoinHandle<()>> = None;
    let mut last_audio_time = Instant::now();
    let mut is_listening = false;
    let mut first_frame_forwarded_logged = false;
```

- [ ] **Step 3: Run local formatter check**

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer && cargo fmt --all --check
```

Expected: no output, exit code 0. If it fails, run `cargo fmt --all` and re-run `--check`. **Do not run `cargo test`, `cargo build`, `cargo clippy`, or `cargo check` — hooks block them.**

- [ ] **Step 4: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/crates/iem-server/src/audio_stream.rs \
        iem-mixer/crates/iem-server/src/proxy.rs
git commit -m "$(cat <<'EOF'
feat(server): instrument handle_audio_ws for Listen pipeline diagnosis

Adds 4 INFO-level tracing lines covering the four failure modes where
binary frames can get lost between OIEM broadcast and /ws/audio client:
- producer spawn (with subscriber_count)
- first broadcast frame received by producer
- first binary frame forwarded to client
- broadcast Lagged/Closed promoted from debug to info
- socket.send(Binary) error (new; previously silent break)

Adds frames_forwarded AtomicU64 counter surfaced in
/api/audio/diagnostics. Lets us tell from the outside whether any
client ever got a frame on a given deploy.

Prep for reading the RED CI run and landing the root-cause fix as a
second commit in the same PR.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Push, monitor RED CI, read logs, root-cause fix

This task is where TDD turns RED → GREEN. The commits from T1-T6 make the E2E gap visible. The test-integrity scan passes; the new binary-frames-or-die test fails on post-deploy E2E because the regression still exists. After landing the root cause, CI goes green.

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs` (exact location depends on what logs reveal)
- Possibly modify: `iem-mixer/crates/iem-server/src/lib.rs` (if the root cause is the `broadcast::channel(8)` capacity or similar config)

- [ ] **Step 1: Push T1-T6 commits**

```bash
cd /home/newlevel/devel/reaperiem
git push origin dev
```

- [ ] **Step 2: Identify the run and monitor with the ONE correct pattern**

```bash
# Find the latest run
gh run list --branch dev --limit 3
# Pick the run id from the output (the most recent 'push' event run)
# Then monitor — single background sleep, NOT /loop, NOT cron, NOT bash while-loop:
gh run view <RUN_ID> --json status,conclusion,jobs
# If still in_progress:
sleep 300 && gh run view <RUN_ID> --json status,conclusion,jobs
```

Expected result: **the run fails on the post-deploy E2E job** because the new `audio-listen-e2e.spec.ts` asserts ≥30 binary frames and gets 0. This is the RED state. Other 9 jobs (lint, test, build-wasm, build-tauri, e2e, test-integrity, verify-version-bump, build-vban, deploy) should pass — lint-check is `cargo fmt` only, test/build/e2e do not need audio, test-integrity scan PASSES because T3 removed all violations.

- [ ] **Step 3: Retrieve logs + diagnostics from the iem.lan app**

After the run reaches a terminal state (fails, as expected), connect to iem.lan via the `mcp__win-iem-snv__Shell` MCP tool (NOT SSH) and read the latest app log:

```powershell
$logDir = [Environment]::GetFolderPath('ApplicationData') + "\iem-mixer\logs"
$latest = Get-ChildItem $logDir -File | Sort-Object LastWriteTime -Descending | Select-Object -First 1
Write-Host "=== $($latest.Name) ==="
Select-String -Path $latest.FullName -Pattern "audio (producer|broadcast|binary|frame_rx|first|Lagged|Closed)" | Select-Object -Last 60 | ForEach-Object { Write-Host $_.Line }
```

Also fetch diagnostics to see `frames_forwarded`:

```bash
# From the dev workstation:
TOKEN=$(curl -sS -X POST http://10.77.9.231/api/auth -H 'Content-Type: application/json' -d '{"member":"engineer","pin":"1177"}' | python3 -c 'import json,sys;print(json.load(sys.stdin)["token"])')
curl -sS http://10.77.9.231/api/audio/diagnostics -H "Authorization: Bearer $TOKEN" | python3 -m json.tool
```

- [ ] **Step 4: Interpret the logs and pick the root cause**

Match what the logs say to one of these four scenarios. Each has a different one-line fix. **Only one of these is correct — pick based on what the logs actually show, don't guess.**

**Scenario A: producer spawned, first broadcast frame received, but no "first binary frame forwarded" log + `frames_forwarded` stays 0.**
Cause: `frame_rx.recv()` branch isn't firing despite `is_listening == true`. This means the select loop's `, if is_listening` guard is being evaluated against a stale value at branch-selection time. Fix: remove the `if is_listening` guard on the `frame_rx.recv()` branch (frames can only arrive when a producer is spawned, which only happens on ListenStart) — in `proxy.rs`, change:

```rust
frame = frame_rx.recv(), if is_listening => {
```

to:

```rust
frame = frame_rx.recv() => {
```

**Scenario B: producer spawned but NO "first broadcast frame received" log (or logs show repeated `Lagged`).**
Cause: producer's `broadcast_rx.recv()` is starving. `broadcast::channel(8)` (lib.rs:223) is too shallow given 50 pps — new subscribers get `Lagged` immediately and `recv()` internally retries but the application sees gaps. Fix: raise capacity back to 64 in `iem-mixer/crates/iem-server/src/lib.rs` line 223:

```rust
let (audio_tx, _) = broadcast::channel(64);
```

(Revert the reduction made in commit `70df365` — the stated rationale was burst cap, but 8 frames × 20 ms = 160 ms is too tight for subscriber startup.)

**Scenario C: "first binary frame forwarded" logged, `frames_forwarded` increments, but client still reports binCount=0 and button shows "No Source".**
Cause: server sends binary but client never receives — likely `Message::Binary(data.to_vec().into())` producing a zero-length payload OR axum ws dropping binary. Fix: add `debug_assert!(!data.is_empty())` and log `frame_size` on send; if frames ARE sent but not received, check axum `Config::max_message_size` on the upgrade. Specific change to be determined from the `frame_size` values in logs.

**Scenario D: "audio binary send failed" error logged.**
Cause: `socket.send(Binary)` returns `Err`. The error message in the log (`{}`) will say why — backpressure, reset, or payload-size limit. Fix: depends on the error variant.

- [ ] **Step 5: Implement the fix**

Apply the exact edit corresponding to the scenario from Step 4. Keep the change SMALL — one location, no refactor. Run local fmt check:

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer && cargo fmt --all --check
```

- [ ] **Step 6: Commit the root-cause fix**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/crates/iem-server/src/proxy.rs  # or lib.rs depending on scenario
git commit -m "$(cat <<'EOF'
fix(server): restore binary Opus frame forwarding on /ws/audio

Scenario <A|B|C|D> confirmed from instrumented logs on RED run:
<1-2 lines summarizing what the log showed>.

The fix is <describe>. Verified by the binary-frames-or-die E2E test
(audio-listen-e2e.spec.ts) landing green in the next CI cycle.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Replace the `<A|B|C|D>` and `<describe>` placeholders with the actual values BEFORE committing.

- [ ] **Step 7: Push and monitor until green**

```bash
cd /home/newlevel/devel/reaperiem
git push origin dev
gh run list --branch dev --limit 3
# Pick the new run id; then:
sleep 300 && gh run view <NEW_RUN_ID> --json status,conclusion,jobs
```

Expected: all 10 jobs green including post-deploy E2E. If still failing, `gh run view <id> --log-failed`, identify the cause, fix in ONE new commit, push, monitor. One rerun is acceptable for transient failures (airuleset `test-strictness.md`); two means something is really wrong — diagnose, don't retry.

---

## Task 8: Open PR dev → main and STOP

**Files:**
- None (git + `gh` operations only)

- [ ] **Step 1: Fetch origin + verify dev is ahead of main**

```bash
cd /home/newlevel/devel/reaperiem
git fetch origin
git log --oneline origin/main..origin/dev | head
# Expected: the T1-T7 commits listed
```

- [ ] **Step 2: Create the PR**

```bash
cd /home/newlevel/devel/reaperiem
gh pr create --base main --head dev --title "fix: restore Listen binary audio + harden E2E against silent-skip regressions" --body "$(cat <<'EOF'
## Summary
- Root-cause fix for the engineer 🔊 Listen button showing "No Source" despite server receiving OIEM audio at 50 pps — server was forwarding zero binary frames on `/ws/audio`.
- Hardened `test-integrity` CI scan to reject the 7 silent-skip patterns that let this regression ship green for months.
- New binary-frames-or-die E2E test that asserts ≥30 Opus frames in a 3 s ListenStart window against live REAPER with tone generator active.
- Server instrumentation: 4 INFO-level tracing lines in `handle_audio_ws` + new `frames_forwarded` counter in `/api/audio/diagnostics`.
- Reordered CI so the tone generator is deactivated AFTER post-deploy E2E (was before), giving the new test a real signal source.

## Test plan
- [ ] All 10 CI jobs green on `dev` (including post-deploy E2E on self-hosted Windows runner)
- [ ] `audio-listen-e2e.spec.ts` asserts ≥30 binary frames and passes
- [ ] Test-integrity scan fails if any of the 7 banned patterns are reintroduced
- [ ] Production Listen works after merge + deploy: open `https://iem.newlevel.media/engineer` → click 🔊 Listen → button reaches `listening` state, not `no-source`
- [ ] `GET /api/audio/diagnostics` shows `frames_forwarded` incrementing

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Verify the PR is mergeable and clean**

```bash
# Get the PR number from step 2 output. Then:
gh api repos/zbynekdrlik/reaperiem/pulls/<PR_NUMBER> --jq '{mergeable: .mergeable, mergeable_state: .mergeable_state}'
```

Expected: `{"mergeable": true, "mergeable_state": "clean"}`.

If `mergeable_state` is `behind`: run `git fetch origin && git merge origin/main --no-edit && git push origin dev`, re-check.
If `dirty` or `blocked`: investigate the specific check that's red, fix, push. Do not present an unclean PR URL to the user.

- [ ] **Step 4: STOP and present**

Output the completion report to the user in the airuleset format (see `completion-report.md`). **Do NOT merge.** The user merges explicitly per airuleset `pr-merge-policy.md`. Wait for their "merge it" / "approved" before any further action.

Include in the report:
- Plan fulfillment checklist (T1-T8)
- E2E test coverage table with:
  - `audio-listen-e2e.spec.ts` — engineer ListenStart delivers ≥30 binary Opus frames in 3 s
  - `audio-listen-e2e.spec.ts` — petronela ListenStart delivers ≥30 binary Opus frames (solo-mute path)
  - `audio-listen.spec.ts` — click Listen → button reaches `listening` (no-source is failure)
  - `audio-e2e.spec.ts` — diagnostics endpoint requires engineer; receiving_oiem MUST be true
- ✅ PR URL, ✅ CI green count, ✅ deploy verified (production version 1.157.0, Playwright confirmed binary frames arriving)

---

## Task Dependencies

```
T1 (version bump) ─► T2 (CI scan) ─► T3 (remove violations) ─► T4 (new E2E) ─► T5 (CI reorder) ─► T6 (instrumentation) ─► T7 (push + RED + fix) ─► T8 (PR + STOP)
```

Strict sequential order. T1 must be first on `dev` (airuleset). T2 must come before T3 (scan must exist to validate the removal). T3 must come before T4 (can't add new test if scan would reject other tests). T5 before T7 (reorder must land before the new E2E runs). T7 is the RED→GREEN bridge — do not open the PR (T8) before CI is green.

---

## Verification

After T8 reports completion:

1. **10 CI jobs green** — lint, test, test-integrity, build-wasm, build-tauri, build-vban, e2e, verify-version-bump, deploy, post-deploy E2E.
2. **PR is mergeable and clean** — `mergeable: true, mergeable_state: "clean"`.
3. **Production verification** — independently open `https://iem.newlevel.media/engineer` in Playwright, click 🔊 Listen, assert binary frames arrive (same probe pattern as `audio-listen-e2e.spec.ts`). This is done AFTER user-approved merge, before closing the thread.
4. **Regression-proof:** attempt to reintroduce any of the 7 silent-skip patterns in a dummy branch; `test-integrity` must fail.
5. **Production telemetry:** `/api/audio/diagnostics` returns `frames_forwarded > 0` after any engineer clicks Listen in production.
