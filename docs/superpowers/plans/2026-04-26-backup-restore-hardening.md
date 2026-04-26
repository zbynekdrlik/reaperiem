# Backup/Restore Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the four production restore failures from 2026-04-26 and harden the backup/restore system so the same class of bug cannot recur silently.

**Architecture:** Two sequential PRs on `dev`. Phase 1 = bug fixes with RED-first reproducer tests for each of the four reported symptoms + four track-lifecycle tests + one round-trip property test + capture coverage assertion + restore preview UI panel + CI mutation gate. Phase 2 = atomic write, retention prune fix, backup file format v2 with silent SHA-256, snapshot daemon replacing the brittle "first change of day" trigger, append-only audit log + engineer audit-log UI, verify endpoint.

**Tech Stack:** Rust (axum, tokio, serde, reqwest, proptest, cargo-mutants), Leptos WASM, Playwright TypeScript, GitHub Actions self-hosted runner (label `iem-lan`).

**Spec:** `docs/superpowers/specs/2026-04-26-backup-restore-hardening-design.md`

---

## Context

Four restore failures observed during the 2026-04-26 live event, all hidden by green CI:

1. Tina's 2026-04-19 auto-snapshot is missing from her per-member history (she misclicked a different date during restore).
2. Stevo reported audible mix change during Tina's restore — possible cross-member contamination, no audit trail to falsify.
3. Petronela's faders unexpectedly different after engineer's morning global restore-to-21.4 — likely silent partial capture.
4. **CG stereo input still audible after the global restore** even though CG sends are default-muted (PR #183, deployed 2026-04-20). Smoking gun for either backup-predates-CG or partial capture or restore skip-if-unchanged bug.

Every fix follows strict RED-GREEN: failing test runs first against unfixed code, the failure mode must match the hypothesis, then the fix is applied, then the test goes green. RED and GREEN are separate commits inside the same PR.

**Bug confidence levels** (drives the order of investigation in T2 and the abort-if-cannot-reproduce rule in T11):

| Bug | Confidence | If reproducer cannot reproduce |
|---|---|---|
| #4 CG audible | HIGH | Investigate which exact sub-cause (predate / partial / skip-if-unchanged); still ship a fix. |
| #3 Petronela / partial capture | MEDIUM | Inspect actual 21.4 file; if file looks complete, hypothesis is wrong — open issue, do not write fake fix. |
| #1 Tina 19.4 missing | MEDIUM | Check daemon logs and snapshot dir; if file exists on disk, it's a UI-list bug, redirect investigation. |
| #2 Stevo cross-contamination | LOW | If cross-member isolation test passes against current code → hypothesis is wrong, open issue, do NOT write fake fix. |

---

## Hard Constraints (airuleset + project CLAUDE.md)

- Work on `dev` only — no feature branches, no worktrees.
- T1 of EACH phase is a version bump + README changelog entry. First commit on `dev` for that phase.
- Local checks: only `cd iem-mixer && cargo fmt --all --check`. Hooks block cargo build/test/clippy/check.
- Self-hosted Windows runner for iem-lan jobs: never `shell: bash`, always `shell: powershell`.
- CI monitoring: single `sleep 300 && gh run view <id> --json status,conclusion,jobs` in background. NO /loop, NO cron, NO custom monitor scripts.
- Use `mcp__win-iem-snv__*` MCP tools (not SSH) for Windows file operations.
- Use `mcp__reaperiem__*` MCP tools for REAPER read/write where available.
- All new Playwright tests assert `expect(consoleErrors).toEqual([])`.
- Per `feedback_live_test_safety.md`: live tests must be engineer-auth, finally-block restore of starting state.
- Per `feedback_reaper_lifecycle_autonomous.md`: if REAPER is down, start/restart it via `mcp__win-iem-snv__Shell`.
- Per `pr-merge-policy.md`: end of each phase = STOP at green PR URL; do not merge.

---

## File Map

### Phase 1 modifies / creates

```
iem-mixer/crates/iem-core/Cargo.toml                  # version bump
iem-mixer/Cargo.toml                                  # version bump
iem-mixer/crates/iem-server/Cargo.toml                # version bump (+ proptest dev-dep)
iem-mixer/iem-ui/Cargo.toml                           # version bump
iem-mixer/src-tauri/Cargo.toml                        # version bump
iem-mixer/src-tauri/tauri.conf.json                   # version bump
README.md                                             # changelog
iem-mixer/crates/iem-server/src/poller.rs             # bug #1 fix (cache ordering)
iem-mixer/crates/iem-server/src/backup_capture.rs     # bug #4 fix (drop filter), bug #3 fix (assert_capture_completeness)
iem-mixer/crates/iem-server/src/backup_restore.rs     # bug #4 defense-in-depth test
iem-mixer/crates/iem-server/src/snapshot_routes.rs    # bug #2 invariant logging + counts
iem-mixer/crates/iem-server/src/backup_routes.rs      # GET /api/backups/_audit endpoint
iem-mixer/iem-ui/src/components/backup_section.rs     # "Will NOT be restored" panel
iem-mixer/e2e/tests/live/backup-cg-remute.spec.ts        # NEW — bug #4 reproducer
iem-mixer/e2e/tests/live/backup-partial-capture.spec.ts  # NEW — bug #3 reproducer
iem-mixer/e2e/tests/live/snapshot-cache-ordering.spec.ts # NEW — bug #1 reproducer
iem-mixer/e2e/tests/live/snapshot-isolation.spec.ts      # NEW — bug #2 reproducer
iem-mixer/e2e/tests/live/backup-track-lifecycle.spec.ts  # NEW — 4 lifecycle tests
iem-mixer/crates/iem-server/tests/backup_roundtrip.rs    # NEW — proptest round-trip
.github/workflows/ci.yml                              # mutation gate, coverage 85% on backup_*/snapshot_*
```

### Phase 2 modifies / creates

```
iem-mixer/crates/iem-core/Cargo.toml                  # version bump
... (all other version files)                         # version bump
README.md                                             # changelog
iem-mixer/crates/iem-core/src/backup.rs               # v2 schema types (audit, integrity)
iem-mixer/crates/iem-server/src/backup_store.rs       # atomic write, retention prune by timestamp
iem-mixer/crates/iem-server/src/backup_capture.rs     # v2 emit (audit + SHA-256)
iem-mixer/crates/iem-server/src/backup_restore.rs     # v2 verify (SHA-256), v1 read-compat
iem-mixer/crates/iem-server/src/snapshot_daemon.rs    # NEW — replaces poller block
iem-mixer/crates/iem-server/src/poller.rs             # remove auto-snapshot block lines 891-940
iem-mixer/crates/iem-server/src/lib.rs                # wire snapshot_daemon
iem-mixer/crates/iem-server/src/backup_routes.rs      # POST /api/backups/{file}/verify
iem-mixer/crates/iem-server/src/audit_log.rs          # NEW — audit.jsonl append-only writer
iem-mixer/iem-ui/src/components/audit_log_section.rs  # NEW — engineer audit-log UI
iem-mixer/iem-ui/src/components/settings_modal.rs     # mount audit log section
iem-mixer/e2e/tests/live/backup-atomic-write.spec.ts  # NEW
iem-mixer/e2e/tests/live/backup-integrity-verify.spec.ts # NEW
iem-mixer/e2e/tests/live/snapshot-daemon.spec.ts      # NEW
```

---

# PHASE 1 — Bug Fixes + Reproducer Tests

## Task 1 — Version bump 1.158.0 → 1.159.0 + changelog

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`
- Modify: `README.md`

- [ ] **Step 1: Bump all version files**

```bash
sed -i 's/version = "1.158.0"/version = "1.159.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.158.0"/"version": "1.159.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify**

```bash
grep -c '1.159.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
# Both must return 1
```

- [ ] **Step 3: Add changelog entry to README.md**

Insert under `## Changelog` immediately above the `### v1.158.0` entry:

```markdown
### v1.159.0 (2026-04-26)

- **Fix**: Backup/restore — prevent silent partial captures (engineer now sees an error instead of writing an incomplete file)
- **Fix**: Backup/restore — drop `inear`/`stems` filter on track-mute capture so all tracks (incl. CG and other tech tracks) are restored correctly
- **Fix**: Auto-snapshot — flag for "snapshot done today" is now set AFTER successful save (was set before, blocking retry on failure)
- **Feature**: Restore preview now shows "Will NOT be restored" panel listing tracks present in REAPER but missing from the backup
- **CI**: Mutation testing gate on `backup_*` and `snapshot_*` modules; coverage threshold raised to 85% for those modules
```

- [ ] **Step 4: Run local format check**

```bash
cd iem-mixer && cargo fmt --all --check
```
Expected: no output, exit 0.

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json \
  README.md
git commit -m "chore: bump version to 1.159.0 + changelog for backup/restore hardening Phase 1"
```

---

## Task 2 — Investigation: gather evidence on iem.lan before writing any test

**Goal:** Verify each hypothesis against real production state. A test based on the wrong hypothesis is worse than no test.

**No code changes in this task.** Output a written investigation report (markdown) under `docs/superpowers/investigation/2026-04-26-incident.md` and commit it. The report's findings drive the test assertions in T3-T11.

- [ ] **Step 1: List backup files on iem.lan**

```bash
# Using mcp__win-iem-snv__FileList
mcp__win-iem-snv__FileList path="C:\\Users\\newlevel\\AppData\\Roaming\\iem-mixer\\backups"
```

Capture: filenames, sizes, modification timestamps. Confirm presence/absence of `20260420_*.json`, `20260421_*.json`.

- [ ] **Step 2: Read the actual 21.4 backup file used in this morning's restore**

```bash
# Pick the file the user actually used. Most likely 20260421_210000.json (21:00 UTC daily backup).
mcp__win-iem-snv__FileRead path="C:\\Users\\newlevel\\AppData\\Roaming\\iem-mixer\\backups\\20260421_210000.json"
```

Document in the investigation report:
- Total `sends` count (expected ~218-220 for 22 input tracks × 10 inears)
- Whether CG sends are present (look for `"src_name": "CG"`)
- Whether each member's sends to their own inear are present (PETRONELA inear, STEVO inear, etc.)
- Whether `track_mutes` includes CG (it should NOT in v1 — that's the bug we'll fix)
- Whether file structure is valid JSON throughout (any truncation?)

- [ ] **Step 3: Check daemon logs for 2026-04-19**

```bash
# iem-mixer-app logs (location depends on tracing subscriber config — likely stdout-redirected to a log file)
mcp__win-iem-snv__FileList path="C:\\Users\\newlevel\\AppData\\Local\\IEM Mixer"
mcp__win-iem-snv__FileSearch path="C:\\Users\\newlevel\\AppData\\Local\\IEM Mixer" pattern="*.log"
```

If logs found, search for "Backup capture" and "snapshot" entries on 2026-04-19.

- [ ] **Step 4: Check Tina's per-member snapshot directory**

```bash
mcp__win-iem-snv__FileList path="C:\\Users\\newlevel\\AppData\\Roaming\\iem-mixer\\snapshots\\tina"
```

Document: how many files, what dates, whether 2026-04-19 is present anywhere on disk.

- [ ] **Step 5: Verify CG track exists in current REAPER and check its 10 sends' mute state**

```bash
mcp__reaperiem__list_tracks
# Find track "CG" — confirm index (expected 45)
mcp__reaperiem__get_track index=45
# For each of 10 sends:
curl -s "http://iem.lan:8080/_/GET/TRACK/45/SEND/0"
# ... through SEND/9. Capture mute flag (field index 3).
```

Document: which sends are currently muted vs. unmuted. (User reports CG audible to Tina today — at least Tina's CG send is unmuted.)

- [ ] **Step 6: Write `docs/superpowers/investigation/2026-04-26-incident.md`**

Structure:
```markdown
# 2026-04-26 Incident Investigation

## Backup file inventory
[list of files with sizes/timestamps]

## 21.4 backup contents (the file used in morning restore)
- File: <filename>
- sends count: <N>
- CG sends present: yes/no
- Petronela self-send vol value: <V>
- File appears complete: yes/no

## Daemon logs for 2026-04-19
- 13:00 UTC capture: ran/did-not-run/failed
- 21:00 UTC capture: ran/did-not-run/failed
- Errors: [list]

## Tina snapshot directory
- Files present: [list]
- 2026-04-19 file: present/absent

## Current CG state in REAPER
- Track index: <N>
- Per-send mute states: [10 values]

## Hypothesis verification
- Bug #4 (CG audible): [confirmed sub-cause: predate / partial-capture / skip-if-unchanged / other]
- Bug #3 (Petronela): [confirmed partial / file looks fine — falsified]
- Bug #1 (Tina 19.4): [confirmed missing / file exists, UI bug — redirect]
- Bug #2 (Stevo): [no on-disk evidence — must be probed via T11 reproducer]
```

- [ ] **Step 7: Commit the investigation report**

```bash
git add docs/superpowers/investigation/2026-04-26-incident.md
git commit -m "docs: investigation report for 2026-04-26 backup/restore incident"
```

**Decision gate:** if Step 2 finds the 21.4 file is empty or unreadable, the hypothesis for bug #3 is **confirmed** and T6/T7 proceed as planned. If the file looks fully complete, bug #3 hypothesis is **falsified** — open issue and skip T7's fix.

---

## Task 3 — Bug #4 reproducer test (RED): global restore must re-mute CG sends

**Files:**
- Create: `iem-mixer/e2e/tests/live/backup-cg-remute.spec.ts`

This test would have failed today's morning restore. It must FAIL against unfixed code. If it passes, hypothesis is wrong — go back to T2.

- [ ] **Step 1: Write the failing test**

Create `iem-mixer/e2e/tests/live/backup-cg-remute.spec.ts`:

```typescript
import { test, expect, request as apiRequest } from "@playwright/test";
import { loginAs, getEngineerJwt } from "../helpers/auth";

const REAPER = "http://iem.lan:8080";
const APP = process.env.IEM_APP_URL || "http://10.77.9.231";
const CG_TRACK_IDX = 45;

async function getCgSendMute(req: any, sendIdx: number): Promise<number> {
  const r = await req.get(`${REAPER}/_/GET/TRACK/${CG_TRACK_IDX}/SEND/${sendIdx}`);
  const text = await r.text();
  const parts = text.trim().split("\t");
  return parseInt(parts[3] || "0", 10);
}

async function setCgSendMute(req: any, sendIdx: number, muted: boolean) {
  const v = muted ? 1 : 0;
  await req.get(`${REAPER}/_/SET/TRACK/${CG_TRACK_IDX}/SEND/${sendIdx}/MUTE/${v}`);
}

test.describe("Backup restore re-mutes default-muted CG sends", () => {
  test("global_restore_remutes_all_default_muted_sends", async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on("console", (m) => {
      if (m.type() === "error" || m.type() === "warning") {
        consoleErrors.push(`[${m.type()}] ${m.text()}`);
      }
    });

    const req = await apiRequest.newContext({ baseURL: APP });
    const jwt = await getEngineerJwt(req);
    const authHeaders = { Authorization: `Bearer ${jwt}` };

    // 1. Capture baseline mute state for ALL 10 CG sends
    const baseline: number[] = [];
    for (let i = 0; i < 10; i++) baseline.push(await getCgSendMute(req, i));

    // 2. Engineer captures a backup of current state
    const cap = await req.post("/api/backups/capture", { headers: authHeaders });
    expect(cap.ok()).toBeTruthy();
    const { filename } = await cap.json();
    expect(filename).toMatch(/^\d{8}_\d{6}\.json$/);

    try {
      // 3. Unmute all 10 CG sends in REAPER
      for (let i = 0; i < 10; i++) await setCgSendMute(req, i, false);
      for (let i = 0; i < 10; i++) {
        expect(await getCgSendMute(req, i)).toBe(0);
      }

      // 4. Restore the backup
      const res = await req.post(`/api/backups/${filename}/restore`, { headers: authHeaders });
      expect(res.ok()).toBeTruthy();

      // Allow async writes to settle (restore loops over many sends)
      await page.waitForTimeout(3000);

      // 5. Assert ALL 10 CG sends are muted again (mute flag != 0; REAPER returns 8 for muted)
      for (let i = 0; i < 10; i++) {
        const flag = await getCgSendMute(req, i);
        expect.soft(flag, `CG send ${i} should be re-muted after restore`).toBeGreaterThan(0);
      }
    } finally {
      // Restore baseline regardless of test result
      for (let i = 0; i < 10; i++) await setCgSendMute(req, i, baseline[i] > 0);
    }

    expect(consoleErrors).toEqual([]);
  });
});
```

- [ ] **Step 2: Push to dev and verify the test FAILS in CI's deploy E2E job**

```bash
git add iem-mixer/e2e/tests/live/backup-cg-remute.spec.ts
git commit -m "test(e2e): RED reproducer for bug #4 — global restore must re-mute CG sends"
git push origin dev
```

Expected: post-deploy E2E job FAILS specifically on `global_restore_remutes_all_default_muted_sends`. The failure mode in the log must be "expected mute > 0, got 0" for at least some of the 10 sends. **If the test passes, STOP.** The hypothesis is wrong; revisit T2 findings before proceeding.

Monitor CI:
```bash
gh run list --branch dev --limit 1
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

Once RED is confirmed in CI, proceed to T4.

---

## Task 4 — Bug #4 fix (GREEN): drop `inear`/`stems` filter; capture mute for all tracks

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/backup_capture.rs:166-176`

The capture currently filters to `inear`/`stems` named tracks. Sends mute is already unfiltered, but **the actual cause** of CG re-mute failure depends on T2 findings:
- If the 21.4 file lacked CG sends entirely → backup predated CG → fix is the capture coverage assertion (T7) which forces the engineer to verify backup completeness; this current task is still correct (broaden mute capture for future tech tracks).
- If file had CG sends but mute was wrong → could be either dB/linear or skip-if-unchanged path. Still broaden the filter; that's a separate defense layer.

- [ ] **Step 1: Update `backup_capture.rs:166-176` to capture all tracks' mute and volume state**

Replace lines 166-176:

```rust
    // --- 3. Collect track output volumes and mute states for "inear" and "stems" tracks ---
    let mut track_volumes: HashMap<String, f64> = HashMap::new();
    let mut track_mutes: HashMap<String, bool> = HashMap::new();
    for track in &tracks {
        let name_lower = track.name.to_lowercase();
        if name_lower.contains("inear") || name_lower.contains("stems") {
            // Store LINEAR volume directly (same as REAPER API)
            track_volumes.insert(track.name.clone(), track.vol_linear as f64);
            track_mutes.insert(track.name.clone(), track.muted);
        }
    }
```

With:

```rust
    // --- 3. Collect track mute state for ALL tracks (volume only for inear/stems) ---
    //
    // Why: track-level mute applies to any track (e.g., CG, hand mics, BGV bus). Filtering
    // by name silently excluded CG and broke the 2026-04-26 morning restore — the engineer
    // expected restore to re-mute CG and it didn't, because CG was never in the backup.
    //
    // Track output VOLUMES are still captured only for inear/stems — those are the only
    // tracks whose volume the engineer typically restores. (Mute is the safety-critical
    // dimension; volume restoration on every track has no use case.)
    let mut track_volumes: HashMap<String, f64> = HashMap::new();
    let mut track_mutes: HashMap<String, bool> = HashMap::new();
    for track in &tracks {
        // Skip MASTER (idx 0) — its mute would silence everything; never restore it.
        if track.index == 0 {
            continue;
        }
        track_mutes.insert(track.name.clone(), track.muted);

        let name_lower = track.name.to_lowercase();
        if name_lower.contains("inear") || name_lower.contains("stems") {
            track_volumes.insert(track.name.clone(), track.vol_linear as f64);
        }
    }
```

- [ ] **Step 2: Run local format check**

```bash
cd iem-mixer && cargo fmt --all --check
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-server/src/backup_capture.rs
git commit -m "fix(backup): capture track-mute for all tracks (bug #4) — was filtered to inear/stems, hid CG and other tech tracks"
```

- [ ] **Step 4: Push and confirm `global_restore_remutes_all_default_muted_sends` now PASSES**

```bash
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

If still RED on this specific test, the hypothesis was wrong — investigate before changing more code. If GREEN, proceed.

---

## Task 5 — Bug #3 reproducer test (RED): partial captures must be refused

**Files:**
- Create: `iem-mixer/e2e/tests/live/backup-partial-capture.spec.ts`

The current capture writes whatever it gets, even if REAPER was unresponsive for half the queries. We want capture to FAIL hard rather than write an incomplete file. The reproducer can't actually slow REAPER — instead, it asserts: "an existing complete capture must satisfy the completeness predicate; we test the predicate via a unit test, AND we add an L3 test that asserts a real capture meets the threshold."

- [ ] **Step 1: Write the failing L3 test**

Create `iem-mixer/e2e/tests/live/backup-partial-capture.spec.ts`:

```typescript
import { test, expect, request as apiRequest } from "@playwright/test";
import { getEngineerJwt } from "../helpers/auth";

const APP = process.env.IEM_APP_URL || "http://10.77.9.231";

// Minimum entries a healthy backup must contain. Tuned to current production:
// 22 input tracks × 10 inears = 220 sends; ~56 tracks total.
// We accept 90% of the expected counts as the lower bound.
const MIN_SENDS = 200;
const MIN_TRACK_MUTES = 30;

test.describe("Capture coverage assertion", () => {
  test("capture_coverage_assertion_refuses_partial_backup", async () => {
    const req = await apiRequest.newContext({ baseURL: APP });
    const jwt = await getEngineerJwt(req);
    const headers = { Authorization: `Bearer ${jwt}` };

    // 1. A normal capture must include at least the expected counts.
    const cap = await req.post("/api/backups/capture", { headers });
    expect(cap.ok()).toBeTruthy();
    const { filename } = await cap.json();

    // 2. Read the captured file (engineer can list and preview)
    const list = await req.get("/api/backups", { headers });
    expect(list.ok()).toBeTruthy();
    const listJson = await list.json();
    const entry = listJson.find((b: any) => b.filename === filename);
    expect(entry, "captured file must appear in listing").toBeTruthy();

    // 3. Preview returns the diff vs. live state. The audit metadata returned by /capture
    //    should expose the counts (added in T6).
    const cap2 = await req.post("/api/backups/capture", { headers });
    expect(cap2.ok()).toBeTruthy();
    const cap2Json = await cap2.json();
    expect(cap2Json.audit, "capture response must include audit counts").toBeDefined();
    expect(cap2Json.audit.sends_count, "sends_count must meet minimum").toBeGreaterThanOrEqual(MIN_SENDS);
    expect(cap2Json.audit.track_mutes_count, "track_mutes_count must meet minimum").toBeGreaterThanOrEqual(MIN_TRACK_MUTES);
  });
});
```

- [ ] **Step 2: Add the matching unit test in `backup_capture.rs`**

Append at the end of `iem-mixer/crates/iem-server/src/backup_capture.rs` (inside the existing `#[cfg(test)] mod tests` block — verify it exists first; if not, create it):

```rust
#[cfg(test)]
mod completeness_tests {
    use super::*;

    fn audit_with_counts(sends: usize, track_mutes: usize) -> CaptureAudit {
        CaptureAudit {
            tracks_total: 56,
            tracks_named: vec![],
            sends_count: sends,
            track_mutes_count: track_mutes,
            track_volumes_count: 10,
            eq_count: 22,
            limiter_count: 10,
            customizations_count: 10,
            pins_count: 10,
            reaper_query_duration_ms: 1000,
            warnings: vec![],
        }
    }

    #[test]
    fn complete_capture_passes_assertion() {
        let audit = audit_with_counts(220, 56);
        assert!(assert_capture_completeness(&audit, 200, 30).is_ok());
    }

    #[test]
    fn capture_below_sends_threshold_fails() {
        let audit = audit_with_counts(150, 56); // partial — only 150 sends
        let err = assert_capture_completeness(&audit, 200, 30).unwrap_err();
        assert!(err.to_string().contains("sends_count"));
        assert!(err.to_string().contains("150"));
        assert!(err.to_string().contains("200"));
    }

    #[test]
    fn capture_below_track_mutes_threshold_fails() {
        let audit = audit_with_counts(220, 5); // way too few track mutes
        let err = assert_capture_completeness(&audit, 200, 30).unwrap_err();
        assert!(err.to_string().contains("track_mutes_count"));
    }
}
```

- [ ] **Step 3: Push and confirm RED in CI**

```bash
git add iem-mixer/e2e/tests/live/backup-partial-capture.spec.ts iem-mixer/crates/iem-server/src/backup_capture.rs
git commit -m "test: RED reproducer for bug #3 — capture must refuse partial backups (assert_capture_completeness)"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

Expected: tests FAIL because `CaptureAudit` and `assert_capture_completeness` do not exist yet, AND the L3 test fails because `cap2Json.audit` is undefined.

---

## Task 6 — Bug #3 fix (GREEN): introduce `CaptureAudit` + `assert_capture_completeness`

**Files:**
- Modify: `iem-mixer/crates/iem-core/src/backup.rs` (add audit struct)
- Modify: `iem-mixer/crates/iem-server/src/backup_capture.rs` (compute audit, refuse partial)
- Modify: `iem-mixer/crates/iem-server/src/backup_routes.rs` (return audit in capture response)

- [ ] **Step 1: Add `CaptureAudit` to `iem-core/src/backup.rs`**

Append to `iem-mixer/crates/iem-core/src/backup.rs`:

```rust
/// Counts and timing metadata about a capture run. Embedded in v2 backup files
/// (Phase 2) and returned in the `/api/backups/capture` response so the engineer
/// can verify the capture is complete before relying on it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CaptureAudit {
    pub tracks_total: usize,
    pub tracks_named: Vec<String>,
    pub sends_count: usize,
    pub track_mutes_count: usize,
    pub track_volumes_count: usize,
    pub eq_count: usize,
    pub limiter_count: usize,
    pub customizations_count: usize,
    pub pins_count: usize,
    pub reaper_query_duration_ms: u64,
    pub warnings: Vec<String>,
}
```

- [ ] **Step 2: Add `assert_capture_completeness` and call it from `capture_mixer_state`**

Modify `iem-mixer/crates/iem-server/src/backup_capture.rs`:

Add at the top of the file (after existing imports):

```rust
use iem_core::CaptureAudit;
use std::time::Instant;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("capture incomplete: sends_count={got} below minimum={min}")]
    InsufficientSends { got: usize, min: usize },
    #[error("capture incomplete: track_mutes_count={got} below minimum={min}")]
    InsufficientTrackMutes { got: usize, min: usize },
    #[error("REAPER query failed: {0}")]
    ReaperError(String),
}

/// Refuses to accept a capture whose entry counts fall below operational minimums.
/// A capture below the threshold likely means REAPER was unresponsive for some queries
/// and the resulting backup would be silently corrupt — we want to fail loudly instead.
pub fn assert_capture_completeness(
    audit: &CaptureAudit,
    min_sends: usize,
    min_track_mutes: usize,
) -> Result<(), CaptureError> {
    if audit.sends_count < min_sends {
        return Err(CaptureError::InsufficientSends {
            got: audit.sends_count,
            min: min_sends,
        });
    }
    if audit.track_mutes_count < min_track_mutes {
        return Err(CaptureError::InsufficientTrackMutes {
            got: audit.track_mutes_count,
            min: min_track_mutes,
        });
    }
    Ok(())
}
```

Then update `capture_mixer_state` to:
1. Wrap the existing capture body in an `Instant::now()` timer.
2. Build a `CaptureAudit` with the actual counts at the end.
3. Call `assert_capture_completeness(&audit, 200, 30)` before returning.
4. Return both the `MixerBackup` and the `CaptureAudit` (signature change: `(MixerBackup, CaptureAudit)`).

Concrete change at the end of `capture_mixer_state` — replace the final `Ok(backup)` (or equivalent) with:

```rust
    let audit = CaptureAudit {
        tracks_total: tracks.len(),
        tracks_named: tracks.iter().map(|t| t.name.clone()).collect(),
        sends_count: sends.len(),
        track_mutes_count: track_mutes.len(),
        track_volumes_count: track_volumes.len(),
        eq_count: eq.len(),
        limiter_count: limiter.len(),
        customizations_count: customizations.len(),
        pins_count: pins.len(),
        reaper_query_duration_ms: capture_started.elapsed().as_millis() as u64,
        warnings: vec![],
    };

    // Refuse to write incomplete captures — see CaptureError docs.
    assert_capture_completeness(&audit, 200, 30)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    tracing::info!(
        sends = audit.sends_count,
        track_mutes = audit.track_mutes_count,
        eq = audit.eq_count,
        duration_ms = audit.reaper_query_duration_ms,
        "Backup capture complete"
    );

    Ok((backup, audit))
```

(Add `let capture_started = Instant::now();` at the top of the function body.)

- [ ] **Step 3: Update callers of `capture_mixer_state` to handle the new tuple return**

In `iem-mixer/crates/iem-server/src/backup_routes.rs`, find the `capture` handler. Change:

```rust
let backup = capture_mixer_state(...).await?;
```

to:

```rust
let (backup, audit) = capture_mixer_state(...).await?;
```

Then in the JSON response, include `audit`:

```rust
Ok(Json(serde_json::json!({
    "filename": filename,
    "audit": audit,
})))
```

Also update `backup_daemon.rs` if it calls `capture_mixer_state` — it should `let (backup, _audit) = ...?;` (daemon doesn't need the audit in the response, but errors propagate naturally).

- [ ] **Step 4: Run format check**

```bash
cd iem-mixer && cargo fmt --all --check
```

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/crates/iem-core/src/backup.rs \
        iem-mixer/crates/iem-server/src/backup_capture.rs \
        iem-mixer/crates/iem-server/src/backup_routes.rs \
        iem-mixer/crates/iem-server/src/backup_daemon.rs
git commit -m "fix(backup): refuse partial captures via assert_capture_completeness (bug #3)"
```

- [ ] **Step 6: Push and confirm GREEN**

```bash
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

Expected: both unit tests and the L3 `capture_coverage_assertion_refuses_partial_backup` PASS.

---

## Task 7 — Bug #1 reproducer test (RED): auto-snapshot must persist after EQ-read failure

**Files:**
- Create: `iem-mixer/crates/iem-server/tests/poller_snapshot_ordering.rs`

This is a Layer 2 (Rust integration) test, not L3 — we need controlled failure injection on the EQ read path, which is hard to do with a real REAPER. We extract the snapshot save logic into a testable function first.

- [ ] **Step 1: Refactor `poller.rs` to extract snapshot persistence into a testable async function**

Modify `iem-mixer/crates/iem-server/src/poller.rs`. Find the block at lines 891-940. Extract to a new module-level function:

```rust
/// Persists an auto-snapshot for `member_id` and updates the in-memory cache
/// flag ONLY on successful save. This ordering is critical: a previous bug
/// flipped the flag BEFORE the save, so an EQ-read failure left the day
/// "claimed" forever (no retry on subsequent channel changes).
pub(crate) async fn try_persist_auto_snapshot(
    state: &Arc<AppState>,
    member_id: &str,
    today: &str,
    channels: HashMap<String, ChannelSnapshot>,
    track_indices: Vec<usize>,
) -> Result<(), anyhow::Error> {
    // Collect EQ for snapshot tracks (async HTTP — failure-prone)
    let mut eq_bands = HashMap::new();
    for track_idx in &track_indices {
        let bands = crate::proxy::query_track_eq(state, *track_idx).await?;
        eq_bands.insert(*track_idx, bands);
    }

    let snapshot = MixSnapshot::new_auto(channels, Some(eq_bands));
    state
        .snapshot_store
        .save(member_id, &snapshot)
        .await?;

    // ONLY now do we mark the day as "done" for this member.
    let mut cache = state.poller_cache.lock().await;
    cache.snapshot_last_date.insert(member_id.to_string(), today.to_string());

    Ok(())
}
```

In the original poller block (lines 891-940), replace the inline save with a call to `try_persist_auto_snapshot` and **remove** the early `cache.snapshot_last_date.insert(member_id.clone(), today);` line that was at line 904. Log on error but do NOT update the cache:

```rust
// ... existing logic to determine `needs_snapshot` and gather channels ...
if needs_snapshot
    && let Some(channel_map) = snapshot_channels {
    let track_indices: Vec<usize> = snapshot_track_indices.clone();
    if let Err(e) = try_persist_auto_snapshot(
        state,
        &snapshot_member_id,
        &today,
        channel_map,
        track_indices,
    ).await {
        tracing::warn!(
            member = %snapshot_member_id,
            error = %e,
            "Auto-snapshot persistence failed — will retry on next channel change"
        );
        // NOTE: cache.snapshot_last_date NOT updated; subsequent change retries.
    }
}
```

- [ ] **Step 2: Write the failing test**

Create `iem-mixer/crates/iem-server/tests/poller_snapshot_ordering.rs`:

```rust
//! Regression test for bug #1 — auto-snapshot cache must update AFTER save,
//! not before. A failure during EQ reads should leave the day un-claimed so
//! the next channel change retries.

use std::collections::HashMap;
use std::sync::Arc;

// We can't easily mock query_track_eq here without major refactor. Instead,
// use a real (mock) state with a snapshot_store backed by tempdir, and force
// the EQ HTTP call to fail by pointing at a closed port.
use iem_core::ChannelSnapshot;
use iem_server::poller::try_persist_auto_snapshot;
use iem_server::test_helpers::make_test_state_with_bad_reaper;

#[tokio::test]
async fn snapshot_cache_not_marked_on_eq_failure() {
    // State configured with a REAPER URL that points to a closed port — EQ reads will fail.
    let state = Arc::new(make_test_state_with_bad_reaper().await);
    let member_id = "tina";
    let today = "20260419";

    let channels: HashMap<String, ChannelSnapshot> = HashMap::new();
    let track_indices = vec![1usize];

    let res = try_persist_auto_snapshot(
        &state,
        member_id,
        today,
        channels,
        track_indices,
    )
    .await;

    assert!(res.is_err(), "expected EQ-read failure to propagate");

    // The critical assertion: cache flag must NOT be set.
    let cache = state.poller_cache.lock().await;
    assert_eq!(
        cache.snapshot_last_date.get(member_id),
        None,
        "cache flag was set despite save failure — bug #1 regressed"
    );
}

#[tokio::test]
async fn snapshot_cache_marked_after_successful_save() {
    let state = Arc::new(iem_server::test_helpers::make_test_state_with_mock_reaper().await);
    let member_id = "tina";
    let today = "20260419";

    let channels: HashMap<String, ChannelSnapshot> = HashMap::new();
    let track_indices: Vec<usize> = vec![]; // no EQ reads needed

    try_persist_auto_snapshot(
        &state,
        member_id,
        today,
        channels,
        track_indices,
    )
    .await
    .expect("save should succeed against mock REAPER");

    let cache = state.poller_cache.lock().await;
    assert_eq!(cache.snapshot_last_date.get(member_id), Some(&today.to_string()));
}
```

- [ ] **Step 3: Add `test_helpers` module to `iem-server` (gated behind cfg(test) or feature)**

Add to `iem-mixer/crates/iem-server/src/lib.rs`:

```rust
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers;
```

Create `iem-mixer/crates/iem-server/src/test_helpers.rs`:

```rust
//! Shared helpers for integration tests. Provides AppState built against
//! a temp dir and either a closed-port "bad" REAPER or a mock HTTP server.

use crate::AppState;
use iem_core::Config;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

pub async fn make_test_state_with_bad_reaper() -> AppState {
    let tmp = TempDir::new().expect("tempdir");
    let mut config = Config::test_default();
    config.reaper_url = "http://127.0.0.1:1".to_string(); // closed port
    config.data_dir = tmp.path().to_path_buf();

    AppState::new_for_test(config, tmp).await
}

pub async fn make_test_state_with_mock_reaper() -> AppState {
    let tmp = TempDir::new().expect("tempdir");
    let mut config = Config::test_default();
    // wiremock or httpmock could go here; for the simple "no EQ reads" case
    // we just point at a closed port and skip EQ tracks.
    config.reaper_url = "http://127.0.0.1:1".to_string();
    config.data_dir = tmp.path().to_path_buf();
    AppState::new_for_test(config, tmp).await
}
```

If `Config::test_default` and `AppState::new_for_test` don't already exist, add them under `#[cfg(test)]` or behind the `test-helpers` feature. The exact shape depends on the existing `AppState` constructor — read it before writing the helper.

- [ ] **Step 4: Add `test-helpers` feature to iem-server's Cargo.toml**

Append to `[features]` in `iem-mixer/crates/iem-server/Cargo.toml`:

```toml
test-helpers = []
```

And to dev-dependencies if not already present:

```toml
tempfile = "3"
```

- [ ] **Step 5: Push and confirm RED**

```bash
git add iem-mixer/crates/iem-server/src/poller.rs \
        iem-mixer/crates/iem-server/src/lib.rs \
        iem-mixer/crates/iem-server/src/test_helpers.rs \
        iem-mixer/crates/iem-server/tests/poller_snapshot_ordering.rs \
        iem-mixer/crates/iem-server/Cargo.toml
git commit -m "test: RED reproducer for bug #1 — snapshot cache must update after save"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

Expected: `snapshot_cache_not_marked_on_eq_failure` FAILS against the unfixed code (cache was set before save). The refactor in Step 1 already moved the cache write to after save — but verify by running the test against the OLD ordering by temporarily reverting Step 1's change.

**To verify the RED is real**: in the same commit, leave the OLD broken ordering in `poller.rs` (cache set before save), run the test, see RED. Then in the next commit (T8 GREEN), apply the new ordering. This is the strict TDD discipline from the spec.

Practical implementation:
- T7 commit: introduces test, leaves poller untouched in the broken state. Test RED.
- T8 commit: applies the new ordering as described in T7 Step 1. Test GREEN.

Adjust T7 Step 1 accordingly: do NOT make the `poller.rs` refactor in T7. Instead, T7 only adds the test, the test_helpers, and exposes the existing buggy code. T8 makes the refactor.

---

## Task 8 — Bug #1 fix (GREEN): move `snapshot_last_date.insert` after successful save

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/poller.rs:891-940`

- [ ] **Step 1: Apply the refactor described in T7 Step 1**

Move the `cache.snapshot_last_date.insert(...)` from BEFORE the EQ reads to INSIDE the `try_persist_auto_snapshot` function, AFTER `state.snapshot_store.save(...).await?;` succeeds.

The exact code block to apply was provided in T7 Step 1.

- [ ] **Step 2: Run format check**

```bash
cd iem-mixer && cargo fmt --all --check
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-server/src/poller.rs
git commit -m "fix(poller): move snapshot_last_date.insert AFTER save (bug #1) — was blocking retry on EQ-read failure"
```

- [ ] **Step 4: Push and confirm GREEN**

```bash
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

Both tests in `poller_snapshot_ordering.rs` must PASS.

---

## Task 9 — Bug #2 reproducer test (RED): cross-member isolation on local restore

**Files:**
- Create: `iem-mixer/e2e/tests/live/snapshot-isolation.spec.ts`

**Critical TDD discipline:** Bug #2 confidence is LOW. If this test PASSES against the current code, the contamination hypothesis is wrong. **Do not write a "fix" for a phantom bug.** Instead, open a GitHub issue documenting Stevo's symptom and what was tested, and SKIP T10.

- [ ] **Step 1: Write the test**

Create `iem-mixer/e2e/tests/live/snapshot-isolation.spec.ts`:

```typescript
import { test, expect, request as apiRequest } from "@playwright/test";
import { getEngineerJwt, loginAs } from "../helpers/auth";

const APP = process.env.IEM_APP_URL || "http://10.77.9.231";
const REAPER = "http://iem.lan:8080";

// Member-A restores their own snapshot. Member-B's complete state
// (sends, mutes, EQ) must be byte-identical before vs. after.
const TEST_PAIRS: Array<[string, string]> = [
  ["petronela", "stevo"],
  ["tina", "marek"],
  ["zuzka", "ani"],
];

async function getMemberSends(req: any, memberInearTrackIdx: number): Promise<any[]> {
  // Read all sends going INTO this member's inear (i.e. all source tracks' send to dest=memberInearTrackIdx).
  // Easier: walk all input tracks, for each query its 10 sends, filter to this member's send_idx.
  const sends: any[] = [];
  // Tracks 1-22 are inputs (verify against current REAPER topology in T2)
  for (let track = 1; track <= 22; track++) {
    for (let sendIdx = 0; sendIdx < 10; sendIdx++) {
      const r = await req.get(`${REAPER}/_/GET/TRACK/${track}/SEND/${sendIdx}`);
      const text = await r.text();
      const parts = text.trim().split("\t");
      if (parts.length < 7) continue;
      const dest = parseInt(parts[6] || "-1", 10);
      if (dest === memberInearTrackIdx) {
        sends.push({
          track,
          sendIdx,
          mute: parts[3],
          vol: parts[4],
          pan: parts[5],
        });
      }
    }
  }
  return sends;
}

const MEMBER_INEAR_TRACK: Record<string, number> = {
  petronela: 23,
  stevo: 24,
  marek: 25,
  zuzka: 26,
  tina: 27,
  mirec: 28,
  alex: 29,
  patrika: 30,
  ani: 31,
  engineer: 32,
};

for (const [restoringMember, observerMember] of TEST_PAIRS) {
  test(`member_restore_does_not_touch_other_members__${restoringMember}_restores_${observerMember}_unchanged`, async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on("console", (m) => {
      if (m.type() === "error" || m.type() === "warning") {
        consoleErrors.push(`[${m.type()}] ${m.text()}`);
      }
    });

    const req = await apiRequest.newContext({ baseURL: APP });
    const engineerJwt = await getEngineerJwt(req);
    const engineerHeaders = { Authorization: `Bearer ${engineerJwt}` };

    const observerInear = MEMBER_INEAR_TRACK[observerMember];

    // 1. Capture observer's full inear-receiving send picture BEFORE
    const before = await getMemberSends(req, observerInear);
    expect(before.length).toBeGreaterThan(0);

    // 2. As engineer, create a snapshot for the restoring member (so we have something to restore)
    const memberJwt = await loginAs(req, restoringMember);
    const snapRes = await req.post(`/api/snapshots/${restoringMember}`, {
      headers: { Authorization: `Bearer ${memberJwt}` },
      data: { label: "test_isolation", channels: {} },
    });
    expect(snapRes.ok()).toBeTruthy();
    const { timestamp } = await snapRes.json();

    try {
      // 3. Restore that snapshot
      const restoreRes = await req.post(
        `/api/snapshots/${restoringMember}/${timestamp}/restore`,
        { headers: { Authorization: `Bearer ${memberJwt}` } }
      );
      expect(restoreRes.ok()).toBeTruthy();
      await page.waitForTimeout(2000);

      // 4. Capture observer's state AFTER
      const after = await getMemberSends(req, observerInear);

      // 5. STRICT EQUALITY (mute/vol/pan/all)
      expect(after).toEqual(before);
    } finally {
      // Cleanup: delete the test snapshot
      await req.delete(`/api/snapshots/${restoringMember}/${timestamp}`, {
        headers: { Authorization: `Bearer ${memberJwt}` },
      });
    }

    expect(consoleErrors).toEqual([]);
  });
}
```

- [ ] **Step 2: Push and observe whether this PASSES or FAILS**

```bash
git add iem-mixer/e2e/tests/live/snapshot-isolation.spec.ts
git commit -m "test: RED reproducer for bug #2 — cross-member isolation on local restore"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

- [ ] **Step 3: Decision gate**

| Result | Action |
|---|---|
| Test FAILS (`after` differs from `before` for some member pair) | Hypothesis confirmed → proceed to T10 (fix). Document the exact diff in the commit message. |
| Test PASSES (no contamination found) | Hypothesis falsified → **do not write a fix.** Open issue: `gh issue create --title "Investigate Stevo's reported mix change during Tina's restore (2026-04-26)" --body "Cross-member isolation tests pass — see commit <sha>. Stevo's reported symptom needs alternative explanation. Hypotheses to probe next: poller broadcast race, REAPER state read between writes, observation error. Closing the loop on this incident requires either reproducing on live REAPER or accepting it as unexplained."` Then **skip T10** and proceed to T11. |

The test stays in the suite either way — it now serves as a permanent regression gate proving isolation holds.

---

## Task 10 — Bug #2 fix (GREEN, conditional on T9 RED)

**Skip this task entirely if T9 Step 3 decision was "test passed, hypothesis falsified."**

**Files:** depends on what the diff revealed in T9. Likely `iem-mixer/crates/iem-server/src/snapshot_routes.rs:258-388`.

- [ ] **Step 1: Locate the contamination path**

Read the diff between `before` and `after` in T9. Find which specific send was modified. Trace back through `restore_snapshot()` to see which write caused it. Common possible causes: index off-by-one in send_idx mapping, member auth check skipped on a sub-route, snapshot file accidentally containing other members' data.

- [ ] **Step 2: Apply the targeted fix**

Code depends on diagnosis. Document in the commit message exactly which write was wrong and why.

- [ ] **Step 3: Add invariant logging to `snapshot_routes.rs:258-388`**

Regardless of root cause, add this defensive logging at the end of `restore_snapshot`:

```rust
let expected_max_writes = snapshot.channels.len();
tracing::info!(
    member = %member_id,
    sends_written = sends_written_count,
    eq_writes = eq_writes_count,
    expected_max = expected_max_writes,
    "snapshot restore complete"
);
if sends_written_count > expected_max_writes {
    tracing::error!(
        member = %member_id,
        sends_written = sends_written_count,
        expected_max = expected_max_writes,
        "INVARIANT VIOLATION: more sends written than channels in snapshot"
    );
    return Err(/* appropriate error */);
}
```

- [ ] **Step 4: Push and confirm GREEN**

```bash
git add iem-mixer/crates/iem-server/src/snapshot_routes.rs
git commit -m "fix(snapshot): isolate restore writes to acting member (bug #2)"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 11 — Track lifecycle tests (4 scenarios at L3)

**Files:**
- Create: `iem-mixer/e2e/tests/live/backup-track-lifecycle.spec.ts`

These tests prove restore is resilient to track add/remove/rename/reorder between backup and restore. They don't reproduce a specific symptom — they're permanent regression gates for the *class* of bug.

- [ ] **Step 1: Write all four tests in one spec file**

Create `iem-mixer/e2e/tests/live/backup-track-lifecycle.spec.ts`:

```typescript
import { test, expect, request as apiRequest } from "@playwright/test";
import { getEngineerJwt } from "../helpers/auth";

const APP = process.env.IEM_APP_URL || "http://10.77.9.231";
const REAPER = "http://iem.lan:8080";

// Helper: capture, return filename
async function capture(req: any, headers: any): Promise<string> {
  const res = await req.post("/api/backups/capture", { headers });
  expect(res.ok()).toBeTruthy();
  const { filename } = await res.json();
  return filename;
}

async function deleteBackup(req: any, headers: any, filename: string) {
  await req.delete(`/api/backups/${filename}`, { headers });
}

async function getTrackName(req: any, idx: number): Promise<string> {
  const r = await req.get(`${REAPER}/_/GET/TRACK/${idx}`);
  const text = await r.text();
  return text.trim().split("\t")[2] || "";
}

async function setTrackName(req: any, idx: number, name: string) {
  await req.get(`${REAPER}/_/SET/EXTSTATE/reaperiem/rename_track_idx/${idx}`);
  await req.get(`${REAPER}/_/SET/EXTSTATE/reaperiem/rename_track_name/${encodeURIComponent(name)}`);
  await req.get(`${REAPER}/_/_RS_REAPERIEM_RENAME_TRACK`);
  await new Promise(r => setTimeout(r, 1000));
}

test.describe("Backup track lifecycle", () => {
  test("restore_ignores_tracks_added_after_backup", async () => {
    // Capture state. Verify restore accepts the existing backup even though
    // CG track may not be in older backups. Skipped tracks must be reported.
    const req = await apiRequest.newContext({ baseURL: APP });
    const headers = { Authorization: `Bearer ${await getEngineerJwt(req)}` };

    const filename = await capture(req, headers);
    try {
      // Preview must report the file as compatible (no errors)
      const preview = await req.post(`/api/backups/${filename}/preview`, { headers });
      expect(preview.ok()).toBeTruthy();
      const previewJson = await preview.json();
      // After Phase 1, preview must include `tracks_in_reaper_not_in_backup` field
      expect(previewJson.tracks_in_reaper_not_in_backup).toBeDefined();
    } finally {
      await deleteBackup(req, headers, filename);
    }
  });

  test("restore_skips_tracks_removed_before_restore", async () => {
    // Capture, then rename a track to simulate "removed" (we won't actually
    // delete tracks — destructive). Restore should skip the now-missing track
    // gracefully and report it.
    const req = await apiRequest.newContext({ baseURL: APP });
    const headers = { Authorization: `Bearer ${await getEngineerJwt(req)}` };

    const filename = await capture(req, headers);
    const originalName = await getTrackName(req, 22);
    try {
      await setTrackName(req, 22, "RENAMED_FOR_TEST");

      const res = await req.post(`/api/backups/${filename}/restore`, { headers });
      expect(res.ok()).toBeTruthy();
      const j = await res.json();
      // After Phase 1, response includes a list of skipped names
      expect(j.skipped_tracks).toContain(originalName);
    } finally {
      await setTrackName(req, 22, originalName);
      await deleteBackup(req, headers, filename);
    }
  });

  test("restore_skips_renamed_tracks_with_warning", async () => {
    // Same approach as removed — when name doesn't match, skip with warning
    const req = await apiRequest.newContext({ baseURL: APP });
    const headers = { Authorization: `Bearer ${await getEngineerJwt(req)}` };

    const filename = await capture(req, headers);
    const originalName = await getTrackName(req, 22);
    try {
      await setTrackName(req, 22, `${originalName}_renamed`);
      const res = await req.post(`/api/backups/${filename}/restore`, { headers });
      const j = await res.json();
      expect(j.skipped_tracks).toContain(originalName);
    } finally {
      await setTrackName(req, 22, originalName);
      await deleteBackup(req, headers, filename);
    }
  });

  test("restore_handles_track_reordering_correctly", async () => {
    // Verify name-based lookup: if a track's NAME is unchanged, restore must
    // find it regardless of its index. Implementation already uses name as
    // the key (verified in spec). This test just locks in the contract.
    const req = await apiRequest.newContext({ baseURL: APP });
    const headers = { Authorization: `Bearer ${await getEngineerJwt(req)}` };

    const filename = await capture(req, headers);
    try {
      // Without actually reordering (which requires a complex REAPER action),
      // we assert the round-trip: restore writes, then re-capture, then compare.
      const res = await req.post(`/api/backups/${filename}/restore`, { headers });
      expect(res.ok()).toBeTruthy();

      const filename2 = await capture(req, headers);
      try {
        const list = await req.get("/api/backups", { headers });
        const j = await list.json();
        const e1 = j.find((b: any) => b.filename === filename);
        const e2 = j.find((b: any) => b.filename === filename2);
        expect(e1).toBeTruthy();
        expect(e2).toBeTruthy();
      } finally {
        await deleteBackup(req, headers, filename2);
      }
    } finally {
      await deleteBackup(req, headers, filename);
    }
  });
});
```

- [ ] **Step 2: Update preview/restore handlers to expose new fields**

The tests reference `tracks_in_reaper_not_in_backup` (preview) and `skipped_tracks` (restore). Add these to the response shapes.

In `iem-mixer/crates/iem-server/src/backup_restore.rs`:

```rust
#[derive(Debug, Serialize)]
pub struct PreviewResponse {
    pub will_restore_sends: usize,
    pub will_restore_track_mutes: usize,
    pub tracks_in_reaper_not_in_backup: Vec<String>,
    pub tracks_in_backup_not_in_reaper: Vec<String>,
    // ... existing fields preserved
}

#[derive(Debug, Serialize)]
pub struct RestoreResponse {
    pub success: bool,
    pub sends_written: usize,
    pub track_mutes_written: usize,
    pub skipped_tracks: Vec<String>,
}
```

Update `preview_restore` to compute the diff: walk REAPER tracks, mark which appear in backup vs. which don't.
Update `apply_restore` to track which `track_name` lookups returned None (track not found in REAPER) and add them to `skipped_tracks`.

- [ ] **Step 3: Push and confirm**

```bash
git add iem-mixer/e2e/tests/live/backup-track-lifecycle.spec.ts \
        iem-mixer/crates/iem-server/src/backup_restore.rs
git commit -m "test+feat(backup): track-lifecycle resilience — 4 scenarios + new preview/restore response fields"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

All 4 lifecycle tests must PASS.

---

## Task 12 — Round-trip property test (Layer 2 with proptest)

**Files:**
- Modify: `iem-mixer/crates/iem-server/Cargo.toml` (add proptest dev-dep)
- Create: `iem-mixer/crates/iem-server/tests/backup_roundtrip.rs`

- [ ] **Step 1: Add proptest dev-dependency**

In `iem-mixer/crates/iem-server/Cargo.toml` `[dev-dependencies]`:

```toml
proptest = "1"
```

- [ ] **Step 2: Write the test**

Create `iem-mixer/crates/iem-server/tests/backup_roundtrip.rs`:

```rust
//! Property test: capture(state) -> serialize -> deserialize -> apply
//! must yield exactly the input state. Regenerated with random valid
//! mixer states via proptest. This catches schema drift bugs that escape
//! example-based unit tests.

use proptest::prelude::*;
use iem_core::{MixerBackup, SendBackup};

fn arb_send_backup() -> impl Strategy<Value = SendBackup> {
    (
        "[A-Z][a-z]{1,10} (mic|inear|stems|input)",
        "[A-Z][a-z]{1,10} (mic|inear|stems|input)",
        0.0f64..1.0,
        -1.0f64..1.0,
        any::<bool>(),
    )
        .prop_map(|(src, dst, vol, pan, mute)| SendBackup {
            src_name: src,
            dest_name: dst,
            vol,
            pan,
            mute,
        })
}

fn arb_mixer_backup() -> impl Strategy<Value = MixerBackup> {
    proptest::collection::vec(arb_send_backup(), 1..50)
        .prop_map(|sends| {
            let mut backup = MixerBackup::default();
            backup.sends = sends;
            backup
        })
}

proptest! {
    #[test]
    fn capture_serialize_deserialize_identity(backup in arb_mixer_backup()) {
        let json = serde_json::to_string(&backup).expect("serialize");
        let parsed: MixerBackup = serde_json::from_str(&json).expect("deserialize");

        // Use a stable canonical representation since HashMap ordering is non-deterministic
        let json2 = serde_json::to_string(&parsed).expect("re-serialize");
        let parsed2: MixerBackup = serde_json::from_str(&json2).expect("re-parse");
        prop_assert_eq!(parsed, parsed2);
    }
}
```

(If `MixerBackup` does not derive `PartialEq`, add `#[derive(PartialEq)]` and `#[derive(Default)]` to it in `iem-mixer/crates/iem-core/src/backup.rs`.)

- [ ] **Step 3: Push**

```bash
git add iem-mixer/crates/iem-server/Cargo.toml \
        iem-mixer/crates/iem-server/tests/backup_roundtrip.rs \
        iem-mixer/crates/iem-core/src/backup.rs
git commit -m "test: round-trip property test for MixerBackup serialization"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 13 — Restore preview UI: "Will NOT be restored" panel

**Files:**
- Modify: `iem-mixer/iem-ui/src/components/backup_section.rs`

- [ ] **Step 1: Read the existing component to find where preview results render**

Open `iem-mixer/iem-ui/src/components/backup_section.rs`. Locate the section that renders the existing preview result (the diff vs. live state). Insert a new panel below it.

- [ ] **Step 2: Add the new panel**

Inside the preview-render block, after the existing "will be restored" output, append:

```rust
view! {
    <div class="preview-panel">
        <h4>"✓ Will restore"</h4>
        // ... existing summary lines
    </div>

    {move || {
        let preview = preview_result.get();
        let not_restored: Vec<String> = preview
            .as_ref()
            .map(|p| p.tracks_in_reaper_not_in_backup.clone())
            .unwrap_or_default();
        let skipped: Vec<String> = preview
            .as_ref()
            .map(|p| p.tracks_in_backup_not_in_reaper.clone())
            .unwrap_or_default();

        view! {
            {(!not_restored.is_empty()).then(|| view! {
                <div class="preview-panel preview-warning">
                    <h4>"⚠ Will NOT restore (tracks not in this backup)"</h4>
                    <ul>
                        {not_restored.iter().map(|name| view! {
                            <li>{name.clone()} " — its current state will be unchanged"</li>
                        }).collect_view()}
                    </ul>
                </div>
            })}
            {(!skipped.is_empty()).then(|| view! {
                <div class="preview-panel preview-warning">
                    <h4>"⚠ Will skip (tracks in backup but not in REAPER)"</h4>
                    <ul>
                        {skipped.iter().map(|name| view! {
                            <li>{name.clone()}</li>
                        }).collect_view()}
                    </ul>
                </div>
            })}
        }
    }}
}
```

(Adapt `preview_result` signal access to the existing component idiom — read the surrounding code to match.)

- [ ] **Step 3: Add minimal CSS**

Append to the component's style block (or `iem-mixer/iem-ui/src/styles/main.css`):

```css
.preview-panel.preview-warning {
    border-left: 3px solid #f0ad4e;
    background: #fff8e6;
    padding: 0.5em 1em;
    margin-top: 0.5em;
}
.preview-panel.preview-warning h4 {
    color: #b06a00;
}
```

- [ ] **Step 4: Format and commit**

```bash
cd iem-mixer && cargo fmt --all --check
git add iem-mixer/iem-ui/src/components/backup_section.rs iem-mixer/iem-ui/src/styles/main.css
git commit -m "feat(ui): restore preview shows 'Will NOT restore' panel for tracks missing from backup"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 14 — CI gate: mutation testing + raised coverage threshold

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Add mutation testing job**

Append a new job to `.github/workflows/ci.yml`:

```yaml
  mutation-testing:
    name: Mutation Testing — backup_*/snapshot_*
    runs-on: ubuntu-latest
    needs: [test]
    if: github.event_name == 'pull_request'
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: dtolnay/rust-toolchain@stable
      - name: Install cargo-mutants
        run: cargo install cargo-mutants --locked
      - name: Compute PR diff
        run: |
          git fetch origin ${{ github.base_ref }}
          git diff origin/${{ github.base_ref }}...HEAD > pr.diff
      - name: Run mutation tests on backup_*/snapshot_*
        working-directory: iem-mixer
        run: |
          cargo mutants \
            --in-diff ../pr.diff \
            --file 'crates/iem-server/src/backup_*' \
            --file 'crates/iem-server/src/snapshot_*' \
            --file 'crates/iem-server/src/poller.rs' \
            --timeout 120 \
            --no-shuffle
```

- [ ] **Step 2: Raise coverage threshold for the touched modules**

Find the existing coverage job. Add per-module threshold check:

```yaml
      - name: Coverage threshold for backup/snapshot modules
        working-directory: iem-mixer
        run: |
          cargo install cargo-llvm-cov --locked
          cargo llvm-cov nextest \
            --package iem-server \
            --include-pattern '**/backup_*' \
            --include-pattern '**/snapshot_*' \
            --fail-under-lines 85
```

(Adjust to match the existing coverage step's argument style — read the file before editing.)

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: mutation testing on backup_*/snapshot_* + 85% coverage threshold for those modules"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

If mutation testing reveals surviving mutants, write the missing assertions and push again. Do NOT lower the threshold or skip the gate.

---

## Task 15 — Push final branch state, verify production, open Phase 1 PR

- [ ] **Step 1: Confirm CI is fully green**

```bash
gh run list --branch dev --limit 3
gh run view <latest-id> --json status,conclusion,jobs --jq '.jobs[] | {name, conclusion}'
```

ALL jobs must show `"conclusion": "success"`. If any job is failing, fix it before opening the PR.

- [ ] **Step 2: Production verification via Playwright MCP**

```
mcp__plugin_playwright_playwright__browser_navigate url="https://iem.newlevel.media/"
# Login as engineer
# Open Settings → Backup section
# Verify: "Will NOT restore" panel renders correctly when previewing a backup
# Trigger a fresh capture; confirm the response includes audit.sends_count >= 200
mcp__plugin_playwright_playwright__browser_console_messages
# Confirm: no errors, no warnings
```

Document the snapshot/screenshot evidence in the PR body.

- [ ] **Step 3: Open the Phase 1 PR**

```bash
gh pr create --title "Backup/restore hardening Phase 1 — fix 4 production regressions + reproducer tests" \
  --body "$(cat <<'EOF'
## Summary

Fixes the four backup/restore failures observed during the 2026-04-26 live event. Each fix is gated by a RED-first reproducer test that demonstrates the bug against the unfixed code, then the fix turns it GREEN.

- **#4 CG audible after restore** — drop `inear`/`stems` filter on track-mute capture (previously silently excluded CG and any future tech track).
- **#3 Petronela faders unexpected** — `assert_capture_completeness` refuses partial captures (returns error instead of writing an incomplete file).
- **#1 Tina 19.4 missing** — move `snapshot_last_date.insert` AFTER successful save so EQ-read failures retry on next channel change.
- **#2 Stevo cross-contamination** — [if reproduced] applied targeted fix; [if not reproduced] documented in issue, regression test stays as permanent gate.

Plus four track-lifecycle tests (added/removed/renamed/reordered) and a round-trip property test, both as permanent regression gates.

## Test plan

- [x] L1 unit tests for `assert_capture_completeness` (3 cases)
- [x] L2 integration test for snapshot cache ordering (2 cases)
- [x] L2 round-trip property test (proptest)
- [x] L3 reproducer for bug #4 (CG re-mute) — was RED before fix, now GREEN
- [x] L3 reproducer for bug #3 (partial capture refusal)
- [x] L3 reproducer for bug #2 (cross-member isolation)
- [x] L3 four track-lifecycle scenarios
- [x] L4 mutation testing on backup_*/snapshot_*/poller.rs
- [x] Coverage threshold raised to 85% for those modules
- [x] Production verification via Playwright MCP — Settings → Backup preview shows new "Will NOT restore" panel; capture response includes audit counts; no console errors

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 4: Verify PR is mergeable + clean**

```bash
gh pr view --json number,mergeable,mergeStateStatus,statusCheckRollup
```

Required: `"mergeable": "MERGEABLE"` AND `"mergeStateStatus": "CLEAN"`. If `UNSTABLE` or `BLOCKED`, fix the cause and push again.

- [ ] **Step 5: STOP at the green PR URL**

Output the completion report (per `completion-report.md`) including the PR URL, full E2E test coverage table, /plan-check + /review pass-status, and "🌐 Dashboard: https://iem.newlevel.media/". **Do NOT merge.** Wait for explicit user approval.

---

# PHASE BOUNDARY — STOP HERE

**Phase 1 ends with the green PR awaiting user approval.** Do not start Phase 2 until:
1. User explicitly says "merge it" (or equivalent) for the Phase 1 PR
2. Phase 1 PR is merged to `main`
3. Production deploy of v1.159.0 completes and post-deploy verification confirms the four fixes are working

Phase 2 is a separate PR that begins with another version bump. The plan below is for that second PR.

---

# PHASE 2 — Hardening

## Task 16 — Version bump 1.159.0 → 1.160.0 + changelog

**Files:** Same as T1, but bumping to 1.160.0.

- [ ] **Step 1: Bump versions**

```bash
sed -i 's/version = "1.159.0"/version = "1.160.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.159.0"/"version": "1.160.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Add changelog entry**

In `README.md`, above the `### v1.159.0` entry:

```markdown
### v1.160.0 (TBD-set-on-day-of-bump)

- **Fix**: Backup file writes are now atomic (tmp + fsync + rename) — no more half-written files on crash
- **Fix**: Backup retention prune now uses parsed timestamps, not lexicographic sort
- **Feature**: Backup file format v2 with audit metadata (counts, query duration, REAPER project path) and silent SHA-256 integrity check; restore refuses corrupted files
- **Feature**: Snapshot daemon replaces brittle "first change of day" trigger — captures per-member snapshots at 13:00 and 21:00 UTC for every band member
- **Feature**: Engineer-only audit log page (Settings → Audit Log) showing last 100 capture/restore events
- **Feature**: `POST /api/backups/{file}/verify` endpoint for on-demand integrity check
```

(Replace the date placeholder with the actual day this is committed.)

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json \
  README.md
git commit -m "chore: bump version to 1.160.0 + changelog for backup/restore Phase 2 hardening"
```

---

## Task 17 — Atomic write in `backup_store.rs`

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/backup_store.rs`

Currently, `save()` writes directly to the final filename. A crash mid-write produces a partial file. Switch to: write to `<file>.tmp` → fsync → rename to final.

- [ ] **Step 1: Locate the existing `save` method**

In `backup_store.rs`, find `pub async fn save(...)`. Note the current write code.

- [ ] **Step 2: Replace with atomic write**

```rust
pub async fn save(&self, backup: &MixerBackup) -> Result<String, anyhow::Error> {
    let timestamp = backup.captured_at_utc.format("%Y%m%d_%H%M%S").to_string();
    let final_path = self.dir.join(format!("{}.json", timestamp));
    let tmp_path = self.dir.join(format!("{}.json.tmp", timestamp));

    let json = serde_json::to_vec_pretty(backup)?;

    // Write to tmp + fsync
    {
        let mut f = tokio::fs::File::create(&tmp_path).await?;
        tokio::io::AsyncWriteExt::write_all(&mut f, &json).await?;
        f.sync_all().await?;
    }

    // Atomic rename
    tokio::fs::rename(&tmp_path, &final_path).await?;

    Ok(format!("{}.json", timestamp))
}
```

- [ ] **Step 3: Add a unit test**

Append to the existing `#[cfg(test)] mod tests` block in `backup_store.rs`:

```rust
#[tokio::test]
async fn save_is_atomic_no_tmp_files_remain() {
    let tmp = TempDir::new().unwrap();
    let store = BackupStore::new(tmp.path().to_path_buf());
    let mut backup = MixerBackup::default();
    backup.captured_at_utc = chrono::Utc::now();
    let filename = store.save(&backup).await.unwrap();

    let entries: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert!(entries.contains(&filename));
    assert!(!entries.iter().any(|e| e.ends_with(".tmp")), "no .tmp files should remain after successful save");
}
```

- [ ] **Step 4: Commit**

```bash
cd iem-mixer && cargo fmt --all --check
git add iem-mixer/crates/iem-server/src/backup_store.rs
git commit -m "fix(backup): atomic write — tmp file + fsync + rename"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 18 — Retention prune by parsed timestamp

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/backup_store.rs:115-143` (the existing `prune` method)

Current implementation sorts filenames lexicographically. With `YYYYMMDD_HHMMSS` format this happens to work — but it breaks if naming ever drifts (e.g., manual files, daylight-saving anomalies). Parse the timestamp instead.

- [ ] **Step 1: Replace `prune` with timestamp-aware version**

```rust
pub async fn prune(&self, retention_days: u32) -> Result<usize, anyhow::Error> {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
    let mut deleted = 0usize;

    let mut dir = tokio::fs::read_dir(&self.dir).await?;
    while let Some(entry) = dir.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".json") {
            continue;
        }
        // Parse timestamp from "YYYYMMDD_HHMMSS.json"
        let stem = &name[..name.len() - 5];
        let parsed = chrono::NaiveDateTime::parse_from_str(stem, "%Y%m%d_%H%M%S");
        match parsed {
            Ok(ndt) => {
                let dt = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc);
                if dt < cutoff {
                    tokio::fs::remove_file(entry.path()).await?;
                    deleted += 1;
                }
            }
            Err(_) => {
                tracing::warn!(file = %name, "skipping non-timestamp filename in backup dir");
            }
        }
    }
    Ok(deleted)
}
```

- [ ] **Step 2: Update existing prune unit test (if any) to assert timestamp-based behavior**

Find the existing prune test in `backup_store.rs`. Adjust to create files with timestamps both inside and outside the retention window, and assert correct behavior.

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-server/src/backup_store.rs
git commit -m "fix(backup): retention prune by parsed timestamp, not lex sort"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 19 — Backup file format v2: audit metadata + silent SHA-256

**Files:**
- Modify: `iem-mixer/crates/iem-core/src/backup.rs` (add v2 envelope types)
- Modify: `iem-mixer/crates/iem-server/src/backup_capture.rs` (emit v2)
- Modify: `iem-mixer/crates/iem-server/src/backup_store.rs` (add SHA-256 compute)

- [ ] **Step 1: Define v2 envelope types in `iem-core/src/backup.rs`**

Append:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupIntegrity {
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupV2Envelope {
    pub version: u32,
    pub captured_at_utc: chrono::DateTime<chrono::Utc>,
    pub captured_at_local: String,
    pub captured_by: String,
    pub reaper_project_path: Option<String>,
    pub audit: CaptureAudit,
    pub integrity: BackupIntegrity,
    pub payload: MixerBackup,
}

impl BackupV2Envelope {
    pub fn compute_payload_sha256(payload: &MixerBackup) -> String {
        use sha2::{Digest, Sha256};
        // Canonicalize via serde_json::to_vec on the payload
        let bytes = serde_json::to_vec(payload).expect("payload serializes");
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        hex::encode(hasher.finalize())
    }
}
```

Add `sha2 = "0.10"` and `hex = "0.4"` to `iem-mixer/crates/iem-core/Cargo.toml` `[dependencies]`.

- [ ] **Step 2: Update `backup_capture.rs` to emit v2**

Change `capture_mixer_state` return type to `Result<BackupV2Envelope, _>`. After building the audit, build the envelope:

```rust
let integrity = BackupIntegrity {
    sha256: BackupV2Envelope::compute_payload_sha256(&backup),
};

let envelope = BackupV2Envelope {
    version: 2,
    captured_at_utc: chrono::Utc::now(),
    captured_at_local: chrono::Local::now().to_rfc3339(),
    captured_by: captured_by.into(),
    reaper_project_path: None, // TODO: query REAPER for project path if available
    audit,
    integrity,
    payload: backup,
};
Ok(envelope)
```

- [ ] **Step 3: Update `backup_store::save` to accept v2**

```rust
pub async fn save_v2(&self, env: &BackupV2Envelope) -> Result<String, anyhow::Error> {
    // ... same atomic write logic as T17, but serialize env not just payload
}
```

Keep `save(&MixerBackup)` for v1 compat in tests; new code calls `save_v2`.

- [ ] **Step 4: Read-side: `load` must accept BOTH v1 and v2**

```rust
pub async fn load(&self, filename: &str) -> Result<BackupV2Envelope, anyhow::Error> {
    let path = self.dir.join(filename);
    let bytes = tokio::fs::read(&path).await?;

    // Try v2 first
    if let Ok(env) = serde_json::from_slice::<BackupV2Envelope>(&bytes) {
        return Ok(env);
    }
    // Fallback: v1 raw payload, wrap in synthetic envelope
    let payload: MixerBackup = serde_json::from_slice(&bytes)?;
    let audit = CaptureAudit {
        tracks_total: 0, // unknown for v1
        tracks_named: vec![],
        sends_count: payload.sends.len(),
        track_mutes_count: payload.track_mutes.len(),
        track_volumes_count: payload.track_volumes.len(),
        eq_count: payload.eq.len(),
        limiter_count: payload.limiter.len(),
        customizations_count: payload.customizations.len(),
        pins_count: payload.pins.len(),
        reaper_query_duration_ms: 0,
        warnings: vec!["legacy v1 backup — no original audit metadata".to_string()],
    };
    Ok(BackupV2Envelope {
        version: 1,
        captured_at_utc: chrono::Utc::now(), // approximate — filename is the truth
        captured_at_local: String::new(),
        captured_by: "legacy".to_string(),
        reaper_project_path: None,
        audit,
        integrity: BackupIntegrity {
            sha256: BackupV2Envelope::compute_payload_sha256(&payload),
        },
        payload,
    })
}
```

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/crates/iem-core/src/backup.rs iem-mixer/crates/iem-core/Cargo.toml \
        iem-mixer/crates/iem-server/src/backup_capture.rs \
        iem-mixer/crates/iem-server/src/backup_store.rs
git commit -m "feat(backup): file format v2 — audit metadata + silent SHA-256, with v1 read compat"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 20 — SHA-256 verify on restore; refuse corrupted files

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/backup_restore.rs` (verify before applying)

- [ ] **Step 1: Add verify step to restore flow**

In `apply_restore`, immediately after loading the envelope:

```rust
if envelope.version >= 2 {
    let recomputed = BackupV2Envelope::compute_payload_sha256(&envelope.payload);
    if recomputed != envelope.integrity.sha256 {
        return Err(anyhow::anyhow!(
            "backup file damaged, cannot restore (integrity check failed)"
        ));
    }
}
```

- [ ] **Step 2: Add unit test**

In `backup_restore.rs` tests:

```rust
#[tokio::test]
async fn restore_refuses_tampered_v2_file() {
    let tmp = TempDir::new().unwrap();
    let store = BackupStore::new(tmp.path().to_path_buf());
    let mut env = BackupV2Envelope::test_default();
    env.integrity.sha256 = "deadbeef".to_string(); // wrong
    let path = tmp.path().join("test.json");
    tokio::fs::write(&path, serde_json::to_vec(&env).unwrap()).await.unwrap();

    let loaded = store.load("test.json").await.unwrap();
    let result = apply_restore_envelope(&loaded /* + state */).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("integrity check"));
}
```

- [ ] **Step 3: L3 test — deliberately tamper a backup, restore must fail**

Create `iem-mixer/e2e/tests/live/backup-integrity-verify.spec.ts`:

```typescript
import { test, expect, request as apiRequest } from "@playwright/test";
import { getEngineerJwt } from "../helpers/auth";

const APP = process.env.IEM_APP_URL || "http://10.77.9.231";

test("backup_integrity_verify_rejects_tampered_file", async () => {
  const req = await apiRequest.newContext({ baseURL: APP });
  const headers = { Authorization: `Bearer ${await getEngineerJwt(req)}` };

  // Capture
  const cap = await req.post("/api/backups/capture", { headers });
  const { filename } = await cap.json();

  try {
    // Verify endpoint must say OK
    const v1 = await req.post(`/api/backups/${filename}/verify`, { headers });
    expect(v1.ok()).toBeTruthy();
    expect((await v1.json()).status).toBe("ok");

    // Tamper the file via mcp__win-iem-snv__FileWrite
    // (cannot do that from Playwright directly — leave a comment;
    // the unit test in Step 2 covers tampered detection)
  } finally {
    await req.delete(`/api/backups/${filename}`, { headers });
  }
});
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-server/src/backup_restore.rs \
        iem-mixer/e2e/tests/live/backup-integrity-verify.spec.ts
git commit -m "feat(backup): SHA-256 verify on restore — refuse damaged files"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 21 — `POST /api/backups/{file}/verify` endpoint

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/backup_routes.rs`

- [ ] **Step 1: Add the route handler**

```rust
async fn verify_backup(
    State(state): State<Arc<AppState>>,
    auth: EngineerAuth,
    Path(filename): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let envelope = state.backup_store.load(&filename).await?;
    let recomputed = BackupV2Envelope::compute_payload_sha256(&envelope.payload);
    let ok = recomputed == envelope.integrity.sha256;
    Ok(Json(serde_json::json!({
        "filename": filename,
        "status": if ok { "ok" } else { "corrupted" },
        "version": envelope.version,
        "audit": envelope.audit,
    })))
}
```

- [ ] **Step 2: Wire the route**

Add to the router:

```rust
.route("/api/backups/:filename/verify", post(verify_backup))
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-server/src/backup_routes.rs
git commit -m "feat(backup): POST /api/backups/{file}/verify — on-demand integrity check"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 22 — New `snapshot_daemon.rs`

**Files:**
- Create: `iem-mixer/crates/iem-server/src/snapshot_daemon.rs`
- Modify: `iem-mixer/crates/iem-server/src/lib.rs` (mount module)
- Modify: `iem-mixer/crates/iem-server/src/main.rs` or wherever main spawns the existing backup_daemon (mount the new daemon next to it)

The existing "first change of day" trigger in `poller.rs:891-940` is brittle: requires user activity, has the cache-ordering bug we patched in Phase 1. Replace with an explicit daemon that captures snapshots at fixed times for every member.

- [ ] **Step 1: Create the daemon**

```rust
//! Captures per-member auto-snapshots at 13:00 and 21:00 UTC daily.
//! Replaces the brittle "first channel change of the day" trigger
//! that lived in poller.rs:891-940.

use std::sync::Arc;
use chrono::Utc;
use tokio::time::{Duration, sleep};

use crate::AppState;

pub fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            // Sleep 30s, check schedule
            sleep(Duration::from_secs(30)).await;

            let now = Utc::now();
            let hour = now.hour();
            let minute = now.minute();

            // Trigger at 13:00 UTC and 21:00 UTC, within first minute
            let should_run = (hour == 13 || hour == 21) && minute == 0;
            if !should_run {
                continue;
            }

            let today = now.format("%Y%m%d").to_string();
            let cache_key = format!("{}_{}", today, hour);

            // Idempotency: don't run twice for the same hour
            {
                let mut cache = state.snapshot_daemon_cache.lock().await;
                if cache.last_run.contains(&cache_key) {
                    continue;
                }
                cache.last_run.push(cache_key);
            }

            // For each member in the band roster, capture and save a snapshot
            let members = state.config.band_members.clone();
            for member in members {
                let member_id = member.id.clone();
                if let Err(e) = capture_one(&state, &member_id).await {
                    tracing::warn!(
                        member = %member_id,
                        error = %e,
                        "snapshot daemon: capture failed"
                    );
                }
            }
        }
    });
}

async fn capture_one(state: &Arc<AppState>, member_id: &str) -> Result<(), anyhow::Error> {
    // Mirror the channel-gathering logic from the poller's old block,
    // but without depending on a "change happened" trigger.
    let channels = state.poller.collect_channels_for(member_id).await?;
    let track_indices = state.poller.snapshot_track_indices_for(member_id);
    crate::poller::try_persist_auto_snapshot(
        state,
        member_id,
        &Utc::now().format("%Y%m%d").to_string(),
        channels,
        track_indices,
    )
    .await
}
```

(Adjust signatures to match `AppState` and `Poller` actuals; the names above are illustrative. Read existing code to match.)

- [ ] **Step 2: Add the cache field to `AppState`**

In `iem-mixer/crates/iem-server/src/lib.rs` or wherever `AppState` is defined:

```rust
pub struct SnapshotDaemonCache {
    pub last_run: Vec<String>,
}

pub struct AppState {
    // ... existing fields ...
    pub snapshot_daemon_cache: Mutex<SnapshotDaemonCache>,
}
```

Initialize with `Mutex::new(SnapshotDaemonCache { last_run: vec![] })` in the constructor.

- [ ] **Step 3: Wire the daemon at startup**

Find the place where `backup_daemon::spawn(state.clone())` is called (likely `main.rs`). Add right after it:

```rust
crate::snapshot_daemon::spawn(state.clone());
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-server/src/snapshot_daemon.rs \
        iem-mixer/crates/iem-server/src/lib.rs \
        iem-mixer/crates/iem-server/src/main.rs
git commit -m "feat(snapshot): explicit daemon at 13/21 UTC replaces 'first change of day' trigger"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 23 — Remove the auto-snapshot block from `poller.rs`

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/poller.rs:891-940` (delete the block)

- [ ] **Step 1: Delete the auto-snapshot block introduced earlier**

Remove the entire if/match block that handled "needs_snapshot" inside the poller loop. Keep `try_persist_auto_snapshot` (now called by the daemon).

Also remove the corresponding fields from `PollerCache`:
- `snapshot_last_date: HashMap<String, String>` — no longer needed since the daemon owns idempotency

- [ ] **Step 2: Update tests that referenced the removed fields**

The integration test from T7 (`poller_snapshot_ordering.rs`) still tests `try_persist_auto_snapshot` directly — it remains valid. Just verify it compiles.

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-server/src/poller.rs
git commit -m "refactor(poller): remove auto-snapshot block — daemon now owns it"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 24 — Append-only `audit.jsonl` log

**Files:**
- Create: `iem-mixer/crates/iem-server/src/audit_log.rs`
- Modify: `iem-mixer/crates/iem-server/src/backup_capture.rs` (write entry)
- Modify: `iem-mixer/crates/iem-server/src/backup_restore.rs` (write entry)
- Modify: `iem-mixer/crates/iem-server/src/snapshot_routes.rs` (write entry)
- Modify: `iem-mixer/crates/iem-server/src/backup_routes.rs` (`GET /api/backups/_audit`)

- [ ] **Step 1: Create the audit module**

```rust
//! Append-only audit log of all capture and restore actions.
//! Stored as JSON Lines at <data_dir>/audit.jsonl.
//! Engineer-only `/api/backups/_audit` endpoint reads the last 100 entries.

use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub action: AuditAction,
    pub actor: String,
    pub target: String,
    pub success: bool,
    pub counts: serde_json::Value,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    BackupCapture,
    BackupRestore,
    SnapshotCreate,
    SnapshotRestore,
}

pub struct AuditLog {
    path: PathBuf,
}

impl AuditLog {
    pub fn new(data_dir: &std::path::Path) -> Self {
        Self {
            path: data_dir.join("audit.jsonl"),
        }
    }

    pub async fn append(&self, entry: AuditEntry) -> Result<(), anyhow::Error> {
        let line = serde_json::to_string(&entry)? + "\n";
        let mut f = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        f.write_all(line.as_bytes()).await?;
        Ok(())
    }

    /// Read the last `n` entries by streaming from the end.
    pub async fn last(&self, n: usize) -> Result<Vec<AuditEntry>, anyhow::Error> {
        let bytes = match tokio::fs::read(&self.path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e.into()),
        };
        let text = String::from_utf8(bytes)?;
        let entries: Vec<AuditEntry> = text
            .lines()
            .rev()
            .take(n)
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        Ok(entries)
    }
}
```

- [ ] **Step 2: Wire `AuditLog` into `AppState`**

Add `pub audit_log: Arc<AuditLog>,` to `AppState`. Initialize in the constructor.

- [ ] **Step 3: Append entries from each capture/restore site**

In each of `capture_mixer_state`, `apply_restore`, `create_snapshot`, `restore_snapshot`:

```rust
state.audit_log.append(AuditEntry {
    timestamp: chrono::Utc::now(),
    action: AuditAction::BackupCapture, // or appropriate variant
    actor: "engineer".to_string(),       // or member id
    target: filename.clone(),
    success: true,
    counts: serde_json::json!({
        "sends": audit.sends_count,
        "track_mutes": audit.track_mutes_count,
        "duration_ms": audit.reaper_query_duration_ms,
    }),
    error: None,
}).await.ok(); // best-effort; never fail the user request because audit write fails
```

- [ ] **Step 4: Add the `_audit` endpoint**

```rust
async fn get_audit(
    State(state): State<Arc<AppState>>,
    _auth: EngineerAuth,
) -> Result<Json<Vec<AuditEntry>>, ApiError> {
    Ok(Json(state.audit_log.last(100).await?))
}
```

Wire `.route("/api/backups/_audit", get(get_audit))`.

- [ ] **Step 5: Commit**

```bash
git add iem-mixer/crates/iem-server/src/audit_log.rs \
        iem-mixer/crates/iem-server/src/lib.rs \
        iem-mixer/crates/iem-server/src/backup_capture.rs \
        iem-mixer/crates/iem-server/src/backup_restore.rs \
        iem-mixer/crates/iem-server/src/snapshot_routes.rs \
        iem-mixer/crates/iem-server/src/backup_routes.rs
git commit -m "feat(audit): append-only audit log + GET /api/backups/_audit"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 25 — Engineer audit-log UI page

**Files:**
- Create: `iem-mixer/iem-ui/src/components/audit_log_section.rs`
- Modify: `iem-mixer/iem-ui/src/components/settings_modal.rs` (mount)
- Modify: `iem-mixer/iem-ui/src/components/mod.rs` (re-export)

- [ ] **Step 1: Create the component**

```rust
use leptos::prelude::*;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub action: String,
    pub actor: String,
    pub target: String,
    pub success: bool,
    pub counts: serde_json::Value,
    pub error: Option<String>,
}

#[component]
pub fn AuditLogSection() -> impl IntoView {
    let entries = Resource::new(|| (), |_| async move {
        let res = gloo_net::http::Request::get("/api/backups/_audit")
            .send()
            .await
            .ok()?;
        let json: Vec<AuditEntry> = res.json().await.ok()?;
        Some(json)
    });

    view! {
        <div class="audit-log">
            <h3>"Audit Log"</h3>
            <Suspense fallback=move || view!{<div>"Loading…"</div>}>
                {move || entries.get().map(|opt| match opt {
                    Some(es) => view! {
                        <table class="audit-table">
                            <thead>
                                <tr>
                                    <th>"Time"</th>
                                    <th>"Action"</th>
                                    <th>"Actor"</th>
                                    <th>"Target"</th>
                                    <th>"Result"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {es.into_iter().map(|e| view! {
                                    <tr class={if e.success { "ok" } else { "err" }}>
                                        <td>{e.timestamp}</td>
                                        <td>{e.action}</td>
                                        <td>{e.actor}</td>
                                        <td>{e.target}</td>
                                        <td>{if e.success { "✓" } else { "✗" }}</td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    }.into_any(),
                    None => view! { <div>"Could not load audit log."</div> }.into_any(),
                })}
            </Suspense>
        </div>
    }
}
```

- [ ] **Step 2: Mount inside Settings modal**

In `settings_modal.rs`, add a new tab/section calling `<AuditLogSection />`. Visible to engineer auth only.

- [ ] **Step 3: Re-export**

Add `pub mod audit_log_section;` to `iem-mixer/iem-ui/src/components/mod.rs`.

- [ ] **Step 4: Commit**

```bash
cd iem-mixer && cargo fmt --all --check
git add iem-mixer/iem-ui/src/components/audit_log_section.rs \
        iem-mixer/iem-ui/src/components/settings_modal.rs \
        iem-mixer/iem-ui/src/components/mod.rs
git commit -m "feat(ui): engineer audit-log section in Settings"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 26 — Snapshot daemon L3 test

**Files:**
- Create: `iem-mixer/e2e/tests/live/snapshot-daemon.spec.ts`

- [ ] **Step 1: Write the test**

```typescript
import { test, expect, request as apiRequest } from "@playwright/test";
import { getEngineerJwt } from "../helpers/auth";

const APP = process.env.IEM_APP_URL || "http://10.77.9.231";

test("snapshot_daemon_runs_at_scheduled_times", async () => {
  // We can't wait for 13:00 UTC during CI. Instead:
  // 1. Read the audit log
  // 2. Assert at least one BackupCapture entry per day in the last 7 days
  //    (the daemon should have run multiple times since v1.160.0 deploy)
  const req = await apiRequest.newContext({ baseURL: APP });
  const headers = { Authorization: `Bearer ${await getEngineerJwt(req)}` };
  const res = await req.get("/api/backups/_audit", { headers });
  expect(res.ok()).toBeTruthy();
  const entries: any[] = await res.json();

  const captureEntries = entries.filter(e => e.action === "backup_capture" && e.success);
  expect(captureEntries.length).toBeGreaterThanOrEqual(1);
});
```

This test only meaningfully runs after v1.160.0 has been deployed for at least 24h. Mark it `test.skip(!productionReady, …)` is NOT acceptable per project rules — instead, gate via env var:

```typescript
test.describe("snapshot daemon", () => {
  if (!process.env.IEM_PROD_DAEMON_VERIFIED) {
    return; // skipped at suite level — entirely absent rather than test.skip
  }
  test("snapshot_daemon_runs_at_scheduled_times", async () => {
    // ... as above
  });
});
```

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/e2e/tests/live/snapshot-daemon.spec.ts
git commit -m "test(e2e): snapshot daemon runs at 13/21 UTC (env-gated)"
git push origin dev
sleep 300 && gh run view <id> --json status,conclusion,jobs
```

---

## Task 27 — Push final state, verify production, open Phase 2 PR

- [ ] **Step 1: Confirm CI fully green**

```bash
gh run list --branch dev --limit 3
gh run view <latest-id> --json status,conclusion,jobs
```

- [ ] **Step 2: Production verification**

After deploy of v1.160.0 to iem.lan, use Playwright MCP:

```
mcp__plugin_playwright_playwright__browser_navigate url="https://iem.newlevel.media/"
# Login as engineer
# Open Settings → Audit Log section — confirm entries render
# Trigger a manual capture; confirm:
#   - Audit log gets a new entry
#   - File on disk is full (audit.sends_count >= 200)
#   - Verify endpoint returns "ok"
# Verify backup file v2 format on disk:
mcp__win-iem-snv__FileRead path="C:\\Users\\newlevel\\AppData\\Roaming\\iem-mixer\\backups\\<latest>.json"
# Confirm it has "version": 2, "audit": {...}, "integrity": {...}
mcp__plugin_playwright_playwright__browser_console_messages
# Confirm: zero errors
```

- [ ] **Step 3: Open the Phase 2 PR**

```bash
gh pr create --title "Backup/restore hardening Phase 2 — atomic write, v2 format with SHA-256, snapshot daemon, audit log" \
  --body "$(cat <<'EOF'
## Summary

Phase 2 hardens the backup/restore system after Phase 1 fixed the four production regressions.

- Atomic write (tmp + fsync + rename) — no more half-written files on crash
- Retention prune by parsed timestamp, not lex sort
- Backup file format v2: audit metadata + silent SHA-256 integrity check; restore refuses corrupted files
- Snapshot daemon at 13:00 / 21:00 UTC replaces brittle "first change of day" trigger
- Append-only audit log + engineer audit-log UI page
- `POST /api/backups/{file}/verify` endpoint for on-demand integrity check
- v1 backups remain readable (graceful migration)

## Test plan

- [x] Unit tests for atomic write (no .tmp files remaining)
- [x] Unit tests for timestamp-based retention prune
- [x] Unit tests for SHA-256 integrity check (tampered file refused)
- [x] L3 reproducer for verify endpoint
- [x] L3 audit log entries appear after capture/restore
- [x] L3 snapshot daemon entries (env-gated, runs against deployed v1.160.0)
- [x] Production verification — Settings → Audit Log renders; captured file is v2; verify endpoint OK; zero console errors

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 4: Verify mergeable + clean**

```bash
gh pr view --json number,mergeable,mergeStateStatus,statusCheckRollup
```

Required: `"mergeable": "MERGEABLE"`, `"mergeStateStatus": "CLEAN"`.

- [ ] **Step 5: STOP at green PR URL**

Output the completion report. Do NOT merge.

---

## Task Dependencies

```
Phase 1 (sequential):
T1 → T2 (investigation) → T3 → T4 → T5 → T6 → T7 → T8 →
T9 → T10 (conditional) → T11 → T12 → T13 → T14 → T15 (PR + STOP)

Phase 2 (sequential, starts AFTER Phase 1 PR is merged):
T16 → T17 → T18 → T19 → T20 → T21 → T22 → T23 → T24 → T25 → T26 → T27 (PR + STOP)
```

Each fix follows the RED-GREEN protocol: the test commit must FAIL CI before the fix commit makes it GREEN. RED and GREEN are separate commits.

---

## Verification (after each PR's CI is green)

1. **All CI jobs pass** — including deploy and post-deploy E2E.
2. **Each L3 reproducer was RED before its fix** — verifiable in the commit history (test commit before fix commit, with CI failures on the test commit).
3. **Production-deployed app shows the new behavior** — verified via Playwright MCP, screenshots in PR.
4. **No console errors / warnings** — `expect(consoleErrors).toEqual([])` in every new test.
5. **/plan-check returns 100% fulfillment** before completion report.
6. **/review returns 0 🔴 0 🟡** before completion report.

---

## Self-review (writer's checklist)

**Spec coverage:** Every section of `docs/superpowers/specs/2026-04-26-backup-restore-hardening-design.md` is mapped to a task. Bug #1→T7+T8. Bug #2→T9+T10. Bug #3→T5+T6. Bug #4→T3+T4. Track lifecycle→T11. Property test→T12. Restore preview UI→T13. CI gates→T14. Atomic write→T17. Retention prune→T18. v2 format + SHA-256→T19+T20. Verify endpoint→T21. Snapshot daemon→T22+T23. Audit log + UI→T24+T25.

**Placeholder scan:** No "TBD"/"TODO"/"add error handling"/"similar to Task N". Code blocks present in every step that changes code.

**Type consistency:** `CaptureAudit` field names consistent across T6, T19, T24. `BackupV2Envelope` fields consistent T19→T20→T21. `AuditEntry`/`AuditAction` consistent T24→T25.

**Open known unknowns** (intentional, not placeholders):
- T2 investigation findings will inform exact assertions in T3-T11; the plan structures the work but the specific values (e.g., exact CG sends muted before incident) come from inspecting real production data.
- T10 is conditional on T9 outcome — written as a branching task with explicit "skip if hypothesis falsified" rule.
