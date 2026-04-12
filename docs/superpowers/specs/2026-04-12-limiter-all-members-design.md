# Limiter Button for All Members — Design Spec (#156)

## Problem

The LIM button on the Global IEM Volume fader is engineer-only (`is_engineer.then(|| ...)` guard in `mixer.rs:1832`). Band members report they cannot see or access the limiter for their own inear output. The limiter controls hearing protection — every member should be able to control their own threshold.

## Decision

Members get full limiter control (ON/OFF + threshold) on their own inear output. No safety floor enforced by the engineer. The engineer retains the ability to control any member's limiter.

## Changes

### 1. UI: Remove engineer guard on LIM button

**File:** `iem-mixer/iem-ui/src/pages/mixer.rs:1832`

Remove the `is_engineer.then(|| ...)` wrapper around the LIM button in `GlobalVolumeFader`. The button renders for all authenticated users. `output_track_idx` is already populated for every member from the WebSocket `State` message, so no additional data flow changes are needed.

### 2. Server: Add track-ownership validation for limiter commands

**File:** `iem-mixer/crates/iem-server/src/proxy.rs`

The three limiter WebSocket commands (`GetLimiterParams`, `SetLimiterParam`, `SetLimiterEnabled`) currently accept any `track_index` without validation. Add a scope check:

- **Engineer:** Can get/set limiter params for any track (unchanged behavior).
- **Member:** Can only get/set limiter params for their own output track. The member's output track index is available from `MixerCache::output_track_indices` keyed by `member_id`.
- If a member sends a limiter command for a track that isn't theirs, ignore the command silently (no error response needed — this can only happen from a tampered client).

### 3. E2E test: Non-engineer limiter access

**File:** `iem-mixer/e2e/tests/live/limiter.spec.ts`

Add a test that:
1. Logs in as a non-engineer member (e.g., `petronela`)
2. Navigates to their mixer page
3. Asserts the LIM button is visible on the Global IEM Volume fader
4. Clicks the LIM button
5. Asserts the limiter modal opens with the MAX LEVEL slider visible

## Files touched

| File | Change |
|------|--------|
| `iem-mixer/iem-ui/src/pages/mixer.rs` | Remove `is_engineer` guard (~3 lines) |
| `iem-mixer/crates/iem-server/src/proxy.rs` | Add track-ownership check for 3 limiter commands (~15 lines) |
| `iem-mixer/e2e/tests/live/limiter.spec.ts` | Add non-engineer LIM visibility + modal test |

## Out of scope

- Engineer-enforced minimum threshold (user chose full member control)
- Per-channel limiter buttons (only Global IEM Volume fader gets LIM)
- Limiter activation logging (#145 — separate issue)
