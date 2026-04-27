# Backup/Restore Hardening — Design Spec

**Date:** 2026-04-26
**Status:** Approved (brainstorming complete)
**Driver:** Production incident on 2026-04-26 — four distinct restore failures during/after live event.

## Problem statement

Four production failures in the backup/restore system, all undetected by CI:

1. **Tina's 2026-04-19 auto-snapshot is missing from her per-member history.** She tried to restore it via the history+restore button on `/tina`, picked a different date by mistake, and the restore wrote unintended state.
2. **Stevo reported audible change in his mix while Tina was restoring.** Suggests possible cross-member contamination during local restore (or audit-trail gap that prevents falsifying the hypothesis).
3. **Petronela's faders were unexpectedly different after the engineer's morning global restore to 2026-04-21.** Likely the 21.4 backup file itself was a silent partial capture.
4. **CG stereo input remained audible to Tina after the global restore-to-21.4** — even though CG sends were default-muted on 2026-04-20 (PR #183). Smoking gun for either silent partial capture or a backup that predated CG.

Meta-failure: **CI was green throughout.** No test asserts (a) cross-member isolation on local restore, (b) default-muted tech tracks re-mute after global restore, (c) auto-snapshot scheduler reliability, (d) capture+restore round-trip identity.

## Goals

- For each of the four reported symptoms, there exists a regression test that would have failed before it hit production.
- Capture refuses to write partial backups silently (raises a hard error instead).
- Restore makes "what will NOT be restored" visible to the engineer in the preview UI before applying.
- CI gates (mutation testing + raised coverage threshold) protect the modules going forward.

## Non-goals

- Member manual preset system (`preset_*` files) — not implicated in any report.
- REAPER `.RPP` save/commit pipeline — orthogonal.
- Backup encryption — backups stored on iem.lan only, no exfiltration risk.
- Off-site backup replication.

## System map

Three independent restore systems exist (currently easy to confuse):

| System | Endpoint | Owner | Trigger |
|---|---|---|---|
| **Per-member snapshots** (the "history" with restore button on `/tina` etc.) | `/api/snapshots/{member}` | `snapshot_routes.rs`, `snapshot_store.rs` | First channel change of the day for that member (`poller.rs:891-940`) — **brittle, replaced in Phase 2** |
| **Engineer global backup** (the morning "restore to 21.4") | `/api/backups/{filename}` | `backup_routes.rs`, `backup_capture.rs`, `backup_restore.rs`, `backup_store.rs`, `backup_daemon.rs` | Daily daemon at 13:00 / 21:00 UTC |
| **Manual member presets** | `/api/presets/{member}` | `preset_routes.rs`, `preset_store.rs` | Manual "save preset" button |

## Root-cause hypotheses

| # | Symptom | Hypothesis | Code |
|---|---|---|---|
| 1 | Tina 19.4 missing | `cache.snapshot_last_date.insert(member, today)` runs **before** the actual save. If async EQ reads fail or process restarts mid-flight, the day is "claimed" in memory but no file is written, blocking retry on subsequent channel changes. Trigger is also "first change of day" — if Tina didn't move anything that day, no snapshot at all. | `poller.rs:891-940` |
| 2 | Stevo affected by Tina restore | No contamination path obvious in code; **zero test asserts member-isolation**. Could also be observation error. Need real-REAPER reproduction + audit logging to falsify. | `snapshot_routes.rs:258-388` |
| 3 | Petronela faders unexpected | **The 21.4 backup file itself is incomplete.** Capture has no integrity assertion. A slow/unresponsive REAPER during the daemon run silently records partial state; the file looks valid on disk. | `backup_capture.rs` (no coverage assertion) |
| 4 | CG audible after restore | (a) The 21.4 backup may have been captured before CG track existed (CG shipped 2026-04-20). (b) Silent partial capture during 21.4 daemon run. (c) Track-mute filter at `backup_capture.rs:171` excludes everything not named `inear`/`stems` — a related bug for any future tech track with non-inear/stems naming. | `backup_capture.rs:171`, `backup_restore.rs:490-498` |

## Test architecture

Four-layer strategy, each layer fails CI independently:

```
L1 — Unit (Rust, fast, no REAPER): pruning, parsing, hash, audit-log accounting, cache ordering
L2 — Integration (Rust, mock REAPER HTTP): canned responses → expected JSON; round-trip identity; cross-member isolation
L3 — Live REAPER E2E (deploy job, real iem.lan): 4 reproducers + 4 track-lifecycle scenarios — all production-safe (engineer-only writes, finally-restore)
L4 — Mutation (cargo-mutants on backup_*, snapshot_*): blocks CI if any mutant survives in PR diff
```

### Named regression tests — one per reported symptom

| Symptom | Test name | Layer | Asserts |
|---|---|---|---|
| Tina 19.4 missing | `auto_snapshot_persists_after_eq_read_failure` | L2 + L3 | Inject EQ-read failure mid-capture → assert `snapshot_last_date` flag NOT set, AND next channel change retries successfully. |
| Stevo cross-contamination | `member_restore_does_not_touch_other_members` | L2 + L3 | Capture full mixer state. Modify member A's snapshot. Restore. Read member B's complete state from REAPER. Assert byte-identical to pre-restore. Matrix across all 10 members. |
| Petronela faders unexpected | `capture_coverage_assertion_refuses_partial_backup` | L2 + L3 | Capture with simulated REAPER unresponsiveness for some tracks → capture FAILS (returns error, file NOT written) instead of writing partial JSON. |
| CG audible after restore | `global_restore_remutes_all_default_muted_sends` | L3 | Capture clean state with CG sends muted. Unmute all 10 CG sends in REAPER. Restore. Assert all 10 CG sends mute=true via direct REAPER query. **This test would have caught the 2026-04-26 production failure.** |

### Track lifecycle tests (Layer 3)

All backup entries are name-keyed (verified — see `backup_capture.rs:149-155`, `:168-176`). These tests prove restore is resilient to track changes between backup and restore:

| Scenario | Test name | Asserts |
|---|---|---|
| Track added after backup | `restore_ignores_tracks_added_after_backup` | New track has no entry in backup → its state untouched. Old tracks restored. Audit reports the new track as "ignored — not in backup". |
| Track removed before restore | `restore_skips_tracks_removed_before_restore` | Backup entry for deleted track silently skipped + logged. Other tracks unaffected. |
| Track renamed before restore | `restore_skips_renamed_tracks_with_warning` | Old name not found by lookup → skip + audit warning. Other tracks unaffected. |
| Track reordered (index shift) | `restore_handles_track_reordering_correctly` | Lookup by name finds CG at its current index regardless of REAPER reordering. |

### Property test (Layer 2)

| Test name | Asserts |
|---|---|
| `capture_restore_identity_for_random_state` | Generate random valid mixer state via `proptest`. Capture → modify everything → restore → query full state → assert IDENTITY (every send vol/pan/mute, every track mute, every EQ band, every limiter). |

### Test isolation discipline (per `feedback_live_test_safety.md`)

Every L3 test:
1. Reads starting state into memory before any write.
2. All writes happen as **engineer** auth — never modify a member's mix directly.
3. `finally` block ALWAYS restores starting state, even on test failure.
4. Test fails if cleanup fails — never silently leak modified state to production.

### CI gate additions

- Mutation testing (`cargo-mutants`) on `backup_capture`, `backup_restore`, `snapshot_routes`, `snapshot_store`, `backup_store` — block CI if any mutant survives in PR diff.
- New test-integrity rule: any new test in `tests/live/` MUST include `finally`-style cleanup or CI rejects it.
- Coverage threshold for `backup_*` and `snapshot_*` modules raised to **85%** (project-wide stays at 60%).

## TDD discipline (non-negotiable)

The user's concern: previous PRs shipped green CI but missed real bugs ("same issue next time"). To prevent that, every fix in this spec follows strict RED-first TDD:

1. **Write the test first.** Name it after the symptom (e.g., `global_restore_remutes_all_default_muted_sends`).
2. **Run it against the CURRENT code, unfixed.** It MUST fail. The failure mode MUST match the hypothesis (e.g., assert reports `mute=false` after restore, proving CG sends were not re-muted).
3. **If the test passes against unfixed code → the hypothesis is wrong.** STOP. Do not write the "fix". Go back to investigation. Document the falsified hypothesis. Look at other causes (e.g., for bug #2: shared inear destinations, poller broadcast timing, or observation error).
4. **Only after RED is confirmed**, write the fix. Run the test again — must turn GREEN.
5. **Commit RED and GREEN as separate commits** in the PR so the reviewer can see the failing test ran first.

### Confidence levels per reported bug

Honest assessment of how sure we are each hypothesis is the actual cause. Drives investigation order.

| Bug | Confidence | Why |
|---|---|---|
| #4 CG audible after restore | **HIGH** | Either backup predates CG (verifiable: timestamp the file vs PR #183 deploy), OR partial capture (verifiable: count sends in 21.4 file vs expected ~220), OR track-mute filter excluded CG track-level mute (verified: `backup_capture.rs:171`). At least one of these is true. |
| #3 Petronela faders unexpected | **MEDIUM** | "Silent partial capture" is plausible but unverified. Investigation step: open the actual 21.4 backup file, count entries, check Petronela's expected sends are present and have plausible values. If they look fine in the file → hypothesis is wrong, look elsewhere (e.g., dB/linear conversion regression, restore URL bug). |
| #1 Tina 19.4 missing | **MEDIUM** | Cache-ordering is a real bug in `poller.rs:891-940`, but it may not be the actual cause of this incident. Investigation step: check if Tina's snapshot files exist on disk for 19.4 (could be a UI-list bug, not a save bug). Check daemon logs for 19.4. |
| #2 Stevo cross-contamination | **LOW** | No code path obvious. Could be observation error (Stevo heard something else). Test must run against live REAPER and try to reproduce; if can't reproduce after honest effort → hypothesis is wrong, document and move on, do not ship a "fix" for a phantom bug. |

### Investigation phase (Phase 1 prerequisite)

Before writing any Phase 1 fix, the implementer must:

1. **Open the actual `20260421_130000.json` (or whichever was used today's morning) on iem.lan and inspect it.** Count sends, look for CG entries, look for Petronela's sends. Document findings.
2. **Check daemon log entries for 19.4 at 13:00 and 21:00 UTC.** Did capture run? Did it succeed?
3. **Check `snapshot_store` directory on disk for Tina's 19.4 file.** Present-but-not-listed vs absent are different bugs.

Only with that evidence in hand do we write the failing tests with the right assertions. **A test based on the wrong hypothesis is worse than no test** — it gives false confidence.

## Phase 1 — bug fixes (ships first PR, ~3-5 days)

Each item is one targeted fix. Tests come first (RED), then fix (GREEN).

| File | Change | Bug fixed |
|---|---|---|
| `iem-mixer/crates/iem-server/src/poller.rs:891-940` | Move `cache.snapshot_last_date.insert()` to AFTER successful save. On EQ-read failure, log warning and skip insert so next change retries. | #1 (Tina 19.4 missing) |
| `iem-mixer/crates/iem-server/src/backup_capture.rs:166-176` | Remove the `name_lower.contains("inear") || name_lower.contains("stems")` filter — capture mute state for **all** tracks (both send-mute and track-mute paths). | #4 (CG re-mute), prevents recurrence for any future tech track |
| `iem-mixer/crates/iem-server/src/backup_capture.rs` (new function) | `assert_capture_completeness(audit) -> Result<(), CaptureError>` — refuses to return a backup if `sends_count < min_expected` (configurable, defaults to 90% of band-members × visible-tracks). | #3 (Petronela / silent partial capture) |
| `iem-mixer/crates/iem-server/src/backup_restore.rs:490-498` | Add explicit unit test for "skip if unchanged" path; verify `mute=true` in backup vs `mute=false` in REAPER triggers write (not skip). | #4 (defense-in-depth) |
| `iem-mixer/crates/iem-server/src/snapshot_routes.rs:258-388` | Invariant logging on every restore: which member's data was touched + total sends/EQ writes. Asymmetric counts (e.g., wrote N sends, expected ≤10) fail the request. | #2 (Stevo / audit trail) |
| `iem-mixer/crates/iem-server/src/backup_routes.rs` (new endpoint) | `GET /api/backups/_audit` → returns last 100 capture/restore events. | Diagnostics |
| `iem-mixer/iem-ui/src/components/backup_modal.rs` (or equivalent) | Restore preview: add "**Will NOT be restored**" panel listing tracks present in REAPER but missing from backup. | Engineer foresees gaps |

## Phase 2 — hardening (second PR, ~5-7 days)

| File | Change |
|---|---|
| `iem-mixer/crates/iem-server/src/backup_store.rs` | Atomic write: serialize → write `<file>.tmp` → fsync → rename. Retention prune by **parsed timestamp**, not lex sort. |
| `iem-mixer/crates/iem-server/src/backup_capture.rs` | Compute `integrity.sha256` over canonicalized payload; embed in v2 file. **Silent** — never surfaces in UI unless corruption detected. |
| `iem-mixer/crates/iem-server/src/backup_restore.rs` | On load: recompute SHA-256, compare to embedded. Mismatch → refuse restore with "backup file damaged, cannot restore". |
| `iem-mixer/crates/iem-server/src/snapshot_daemon.rs` (NEW) | Replaces brittle "first change of day" trigger in poller. Captures per-member snapshots **at 13:00 and 21:00 UTC** (matches engineer backup cadence) for every member in `band_members.yaml`, regardless of activity. |
| `iem-mixer/crates/iem-server/src/poller.rs` | Remove the auto-snapshot block (lines 891-940). Daemon owns it. |
| `iem-mixer/crates/iem-server/src/backup_capture.rs` (existing) | Per-action audit entries appended to `audit.jsonl` (append-only). |
| `iem-mixer/crates/iem-server/src/backup_routes.rs` | `POST /api/backups/{filename}/verify` — recomputes hash, returns ok/corrupted. |
| `iem-mixer/iem-ui/src/components/audit_log.rs` (NEW) | Engineer-only page showing rolling audit log (last 100 events): captures + restores with counts, timestamps, success/failure. |

## Backup file format v2

```jsonc
{
  "version": 2,
  "captured_at_utc": "2026-04-21T13:00:00Z",
  "captured_at_local": "2026-04-21 15:00:00 +02:00",
  "captured_by": "daemon" | "engineer:<id>",
  "reaper_project_path": "C:\\Users\\newlevel\\Documents\\reaperiem\\iem.RPP",
  "audit": {
    "tracks_total": 56,
    "tracks_named": ["MASTER", "PETRONELA mic", "...", "CG", "..."],
    "sends_count": 220,
    "track_mutes_count": 56,
    "track_volumes_count": 10,
    "eq_count": 22,
    "limiter_count": 10,
    "customizations_count": 10,
    "pins_count": 10,
    "reaper_query_duration_ms": 4200,
    "warnings": []
  },
  "integrity": { "sha256": "<hex>" },
  "payload": { "sends": [...], "track_mutes": {...}, "track_volumes": {...},
               "eq": {...}, "limiter": {...}, "customizations": {...}, "pins": {...} }
}
```

**Migration:** v1 files keep working (read-only). New captures write v2. Restore handler accepts both.

## Restore preview UI (Phase 1)

```
┌─────────────────────────────────────────────────────────────┐
│  Restore preview — backup_20260421_130000.json              │
│  Captured 2026-04-21 15:00 (5 days old)                     │
│                                                             │
│  ✓ Will restore                                             │
│    - 218 sends across 22 tracks                             │
│    - Track mutes for 56 tracks                              │
│    - EQ for 22 tracks · 10 limiters                         │
│                                                             │
│  ⚠ Will NOT restore (tracks not in this backup)            │
│    - "CG" (added 2026-04-20, after this backup)            │
│       → its current state will be unchanged                 │
│                                                             │
│  ⚠ Will skip (tracks in backup but not in REAPER)           │
│    - none                                                   │
│                                                             │
│  [Cancel]  [Restore]                                        │
└─────────────────────────────────────────────────────────────┘
```

## Migration & rollout

- **Schema:** v1 → v2 is additive. v1 files continue to be readable; new captures emit v2.
- **Daemon swap (Phase 2):** removing the poller auto-snapshot block and adding `snapshot_daemon.rs` are atomic in a single PR. No data loss; existing snapshots remain valid.
- **Capture coverage assertion (Phase 1):** Possible operational impact — if REAPER is genuinely degraded, a daemon run will fail and write nothing rather than write partial. The audit log + alerting will surface this; it is the correct behavior.
- **Coverage threshold raise (Phase 1):** measure current coverage on `backup_*` / `snapshot_*` first; if below 85%, write the missing tests as part of Phase 1 before raising the gate (no instant CI break).

## Tech stack

- Backend: Rust (axum, tokio, serde, reqwest), `proptest` for property tests, `cargo-mutants` for mutation testing.
- Frontend: Leptos WASM (existing).
- E2E: Playwright (TypeScript) — engineer-auth tests, finally-cleanup discipline.
- CI: GitHub Actions, self-hosted Windows runner for deploy/E2E jobs (label: `iem-lan`).
