# Listen "No Source" Regression — Design Spec

**Date:** 2026-04-20
**Issue:** Engineer's 🔊 Listen button always shows "No Source" in production. The bug has shipped green on multiple CI cycles because the E2E test suite has silent-skip paths that violate airuleset `test-strictness.md`.

---

## Reproduction (verified live)

Opened `https://iem.newlevel.media/engineer` as engineer, clicked 🔊 Listen. Over 6 seconds the browser's `/ws/audio` WebSocket received:

- 2 text messages: `{"status":"listening"}` then `{"status":"no_source"}`
- **0 binary frames, 0 bytes**

Same result direct to `http://10.77.9.231` — rules out Cloudflare.

Server-side diagnostics at the same moment: `receiving_oiem: true, 50 pps, peak_db: -34.18 dB, frame_size: 130 B`. OIEM UDP → server broadcast is healthy.

**Fault is inside `proxy.rs::handle_audio_ws`** — between `state.audio_tx.subscribe()` and `socket.send(Message::Binary)`. Frames enter the broadcast, never reach the WebSocket.

---

## How this shipped green (existing E2E anti-patterns)

`test-integrity` (ci.yml:132-148) only scans for `.skip(`, `#[ignore]`, `function assume(`, `if (!assume(`, and one hardcoded `waitForMixer` string. It does NOT catch the 7 silent-skip patterns that hid the Listen regression:

| File:line | Anti-pattern |
|---|---|
| `audio-listen.spec.ts:93-102` | Computes `hasStateChange` but never asserts; accepts `no-source` class as "OK" |
| `audio-e2e.spec.ts:74-78` | `if (!diag.receiving_oiem) { console.log("[SKIP]..."); return; }` |
| `audio-e2e.spec.ts:137-140` | `if (btnCount === 0) { console.log("[SKIP]..."); return; }` |
| `audio-e2e.spec.ts:171-186` | `try { waitForFunction(...) } catch { /* Timeout is OK */ }` |
| `audio-e2e.spec.ts:39-40, 65-66, 102-103` | Silent `return` after auth failure |
| `listen-quality.spec.ts:69` | `if (!(await listenBtn.count())) return;` |
| `ci.yml:2142` | Tone generator deactivated BEFORE post-deploy E2E → Listen test has no signal source to verify against |

Airuleset `test-strictness.md` and project CLAUDE.md both ban these. The gate is broken.

---

## Design (5 parts, one PR)

### 1. Harden `test-integrity` CI job

Extend the scan in `.github/workflows/ci.yml` (the step "Ban assume()/skip patterns in E2E tests") to also fail on:

- `console.log("[SKIP]` in any `.spec.ts` file
- `return;` on a line following a `.count()` / `.status()` / `!resp.ok` / `!auth` guard within a `test(` block
- `catch {` within ~5 lines of `waitForFunction(` (swallowed timeouts)
- Unused `expect(…)`-less boolean computations near `getAttribute("class")` (pattern check on `hasStateChange`-style variables)

Regex-based grep, same style as existing. CI must fail if any current listen test still contains these patterns after Part 2.

### 2. Delete the 7 silent-skip violations

Rewrite `audio-listen.spec.ts`, `audio-e2e.spec.ts`, `listen-quality.spec.ts` with hard assertions:

- Replace `if (btnCount === 0) return;` → `await expect(listenBtn).toBeVisible({ timeout: 5000 });`
- Replace `if (!diag.receiving_oiem) return;` → `expect(diag.receiving_oiem).toBe(true);`
- Replace `try/catch` around `waitForFunction` → remove the catch; let it fail.
- Remove the "No Source is acceptable in CI" comments; the tone-generator-active window (Part 4) guarantees a source.
- Every test ends with `expect(consoleMessages).toEqual([])` per airuleset `browser-console-zero-errors.md`.

### 3. One new hard E2E test: binary-frames-or-die

New file `iem-mixer/e2e/tests/live/audio-listen-e2e.spec.ts` (single test):

```ts
test("engineer Listen receives live binary Opus frames end-to-end", async ({ page, request }) => {
  // Precondition: tone generator must be active (Part 4 guarantees this)
  await loginAsEngineer(page);
  await page.goto("/engineer");

  // Open /ws/audio in the page context (uses page origin's WSS)
  const result = await page.evaluate(async () => {
    const auth = JSON.parse(localStorage.getItem("iem_token")!);
    const url = `${location.protocol === "https:" ? "wss:" : "ws:"}//${location.host}/ws/audio?token=${auth.token}`;
    const ws = new WebSocket(url);
    ws.binaryType = "arraybuffer";
    let binCount = 0, totalBytes = 0, firstBinMs: number | null = null;
    const textMsgs: string[] = [];
    ws.onmessage = (e) => {
      if (e.data instanceof ArrayBuffer) {
        if (firstBinMs === null) firstBinMs = Date.now();
        binCount++;
        totalBytes += e.data.byteLength;
      } else {
        textMsgs.push(e.data as string);
      }
    };
    await new Promise<void>((res, rej) => {
      ws.onopen = () => res();
      ws.onerror = () => rej(new Error("ws connect failed"));
      setTimeout(() => rej(new Error("ws open timeout")), 3000);
    });
    const sentAt = Date.now();
    ws.send(JSON.stringify({ cmd: "ListenStart", member_id: "engineer" }));
    await new Promise((r) => setTimeout(r, 3000));
    ws.close();
    return { binCount, totalBytes, firstBinLatency: firstBinMs ? firstBinMs - sentAt : null, textMsgs };
  });

  // Hard assertions — zero tolerance
  expect(result.textMsgs.some((m) => m.includes('"status":"listening"'))).toBe(true);
  expect(result.textMsgs.some((m) => m.includes('"status":"no_source"'))).toBe(false);
  expect(result.binCount).toBeGreaterThanOrEqual(30); // 50 pps × 3s ≈ 150, 30 = generous floor
  expect(result.totalBytes).toBeGreaterThan(1000);
  expect(result.firstBinLatency).not.toBeNull();
  expect(result.firstBinLatency!).toBeLessThan(1000); // first frame within 1s of ListenStart
});
```

Plus a second test that flips the same check for a band-member target (`ListenStart member_id="petronela"`) to cover the solo-mute path. This path writes to REAPER send-mutes; the handler restores saved mutes on `ListenStop` AND on WS disconnect cleanup, so the test must always send `ListenStop` in a `finally` block (production-safe per MEMORY.md `feedback_live_test_safety.md`).

### 4. CI ordering fix

In `.github/workflows/ci.yml`, move the step `"Deactivate tone generator"` (currently line 2142, runs BEFORE `"Run post-deploy E2E tests"`) to AFTER the post-deploy E2E step. The new Listen E2E test from Part 3 requires a live signal source to make its assertions meaningful.

### 5. Server instrumentation + root-cause fix

Add 4 `tracing::info!` calls to `iem-mixer/crates/iem-server/src/proxy.rs::handle_audio_ws`:

- On producer spawn: `"audio producer spawned"`
- On first frame forwarded: `"first binary frame forwarded bytes={}"`
- On broadcast `Err(Lagged)` and `Err(Closed)`: already `debug`, promote to `info`
- On `socket.send(Binary)` error before `break`: `"binary send failed err={}"`

Add `frames_forwarded: u64` to `AudioDiagnostics` (a single `AtomicU64` updated per successful Binary send), surface in `/api/audio/diagnostics` response.

Deploy this together with Parts 1-4. Read the logs + diagnostics during the new E2E run. Whatever the logs reveal (producer panic, send error, broadcast closed, etc.) drives the actual one-line fix as a second commit in the same PR before merge.

---

## File Map

| File | Change |
|---|---|
| `.github/workflows/ci.yml` | Extend test-integrity scan (step at line 132); reorder Deactivate-tone step after E2E (line 2142) |
| `iem-mixer/e2e/tests/live/audio-listen.spec.ts` | Remove silent-skip patterns; add hard asserts |
| `iem-mixer/e2e/tests/live/audio-e2e.spec.ts` | Remove 4 silent-skip patterns; convert to hard asserts |
| `iem-mixer/e2e/tests/live/listen-quality.spec.ts` | Remove silent-skip on line 69 |
| `iem-mixer/e2e/tests/live/audio-listen-e2e.spec.ts` | **New** — binary-frames-or-die test (engineer + member target) |
| `iem-mixer/crates/iem-server/src/proxy.rs` | 4 tracing::info! in `handle_audio_ws`; `frames_forwarded` counter |
| `iem-mixer/crates/iem-server/src/audio_stream.rs` | Add `frames_forwarded: u64` to `AudioDiagnostics` |
| 5 × Cargo.toml + tauri.conf.json | Version bump 1.156.0 → 1.157.0 |
| `README.md` | Changelog entry for v1.157.0 |

---

## Test Strategy

**RED:** Parts 1 + 2 + 3 + 4 land together in the first commit sequence. The new `audio-listen-e2e.spec.ts` **will fail** on post-deploy CI (proving the regression exists and is now caught). The hardened `test-integrity` scan passes because Part 2 removed the 7 violations it would otherwise flag.

**GREEN:** Part 5 adds the instrumentation. Monitor the failing CI run, read logs, push the root-cause fix as a second commit. CI goes green. Merge.

**No mocks, no synthetic UDP.** The tone generator is already part of CI setup (registered via `_RS_REAPERIEM_TONE_GEN` at ci.yml:1239) — activate it once before E2E, deactivate after, same as today but in the correct order.

---

## Out of Scope

- Refactoring `handle_audio_ws` beyond the instrumentation + root-cause fix.
- Changing broadcast channel capacity (currently 8 — commit `70df365` reduced from 64 with documented reason; don't revert blindly).
- Refactoring other live-test files beyond the 3 listed.
- Adding Listen UI changes.

---

## Success Criteria

1. `cargo fmt --check` passes (only local check per airuleset).
2. All 10 CI jobs green including post-deploy E2E on self-hosted Windows runner.
3. New `audio-listen-e2e.spec.ts` asserts ≥30 binary frames per 3-second ListenStart window — passes against live REAPER.
4. `test-integrity` fails a PR that reintroduces any of the 7 anti-patterns in `live/audio*.spec.ts` or `live/listen*.spec.ts`.
5. Clicking 🔊 Listen on `https://iem.newlevel.media/engineer` plays audio — verified by Playwright post-deploy (binary frames observed, button reaches `listening` class).
6. PR from `dev` → `main` is mergeable + clean. Wait for explicit user merge approval.
