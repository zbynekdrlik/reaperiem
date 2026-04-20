# CG Stereo Input — Design

**Date:** 2026-04-20
**Status:** Approved
**Related PRs:** mirrors #176 (ALEX kl), hardened by #180 (track-index validation)

## Goal

Add a new stereo input **CG** for playing additional content (YouTube videos, music, etc.) on the LED wall during presentations. Routed to all 10 member inears (9 band members + engineer), default-muted — each member unmutes individually when content plays.

## Non-goals

- Not a synced FOH stem (plays independently, not in time with DRUMS/BASS/INST/…).
- Not category `content` (user chose `tech` — simpler, no new UI tab).
- Not adding a new UI category, page, or route.
- No Rust, Lua, or WASM frontend code changes.

## Architecture

Pure config + manual REAPER setup. Zero code changes. The existing `is_input_track` predicate from PR #176 automatically picks up any new input track by name, so TRIM IN + ReaEQ get inserted on the Lua-run FX setup pass. The existing `build_channel_templates` + `derive_stereo_side` logic from PR #176 merges L and R entries sharing `stereo_pair: "cg"` into a single stereo UI channel labeled "CG" in the Tech tab.

The regression fix from PR #180 (`collect_valid_input_indices` returning REAPER-resolved indices, not `inputs.len()`) means CG's out-of-position REAPER track index (45) is already handled correctly — no code change needed.

## Components

### 1. Config

**File:** `config/input_tracks.yaml`

Add two new entries to the **tech** block, after `ENGINEER mic`:

```yaml
- name: "CG L"
  dante_input: 53
  category: tech
  default_level_db: 0.0
  stereo_pair: "cg"

- name: "CG R"
  dante_input: 54
  category: tech
  default_level_db: 0.0
  stereo_pair: "cg"
```

**Runtime config on iem.lan:** `%APPDATA%\iem-mixer\config.yaml` must be patched the same way. CI preserves the runtime config across deploys (never overwrites it), so this is a manual one-time edit during deploy.

### 2. REAPER setup (manual, one-time on iem.lan)

Performed once via REAPER UI or MCP tools on iem.lan:

1. **Create two input tracks:** `CG L` (I_RECINPUT = Dante RX 53, 1-indexed ASIO), `CG R` (Dante RX 54).
2. **Merge to one stereo track `CG`:** NCHAN=2, stereo input mode = channel + 1024, base channel = Dante 53 (L=53, R=54). Resulting REAPER track index = **45** (appended after ALEX kl at 44).
3. **Insert FX chain:** TRIM IN + ReaEQ (both flat defaults). Same chain as every other input track.
4. **Create 10 sends** — one to each member inear track: PETKA, STEVO, MAREK, ZUZKA, TINA, MIREC, ALEX, PATRIKA, ANI, ENGINEER. All sends pre-fader post-FX. **All sends default-muted (mute flag = 8).** Members unmute individually when content plays.
5. **Save project:** `curl "http://iem.lan:8080/_/40026"` (action 40026).
6. **Commit `.RPP` on iem.lan** via `mcp__reaperiem__git_commit`.

### 3. E2E regression tests

**File:** `iem-mixer/e2e/tests/live/cg.spec.ts` (new, mirrors `alex-kl.spec.ts`).

Three tests, each asserting REAL REAPER state (not the UI's optimistic display — this is the exact lesson from PR #180):

1. **"CG appears in the Tech tab"** — login as any member, open Tech tab, assert one stereo channel labeled "CG" is visible.
2. **"dragging CG fader changes REAPER send level"** — drag the CG fader, then `curl "http://iem.lan:8080/_/GET/TRACK/45/SEND/N/VOL"` and assert `D_VOL` dropped from the starting value. N is the send index for the logged-in member.
3. **"muting CG mutes the REAPER send — not just the UI"** — click the CG mute button, then assert REAPER's send MUTE flag = 8 via HTTP roundtrip.

All three assert `consoleMessages = []` per airuleset browser-console-zero-errors. Filters match the convention already used in `alert.spec.ts` and `alex-kl.spec.ts`.

### 4. Version + changelog

- Bump version 1.157.0 → **1.158.0** across 5× Cargo.toml + `iem-mixer/src-tauri/tauri.conf.json`. First commit on `dev`.
- Add to `README.md` changelog:
  ```
  ### v1.158.0 (2026-04-20)
  - **Feature**: New stereo input `CG` (Dante RX 53/54) for content playback (YouTube, music) during presentations — routed to all 10 member inears, default-muted.
  ```

## Data flow

```
Dante RX 53/54 (CG L/R)
  → REAPER ASIO channels 53/54
  → REAPER stereo track "CG" (index 45, FX: TRIM IN → ReaEQ)
  → 10 muted sends → <MEMBER> inear tracks (indices 23…32)
  → [hardware output, unchanged per-member routing]
  → Dante TX to personal monitors
```

## Testing strategy

- **CI E2E (GitHub runner):** no CG tests — CG requires REAPER.
- **Deploy E2E (iem-lan self-hosted runner, after deploy):** the three `cg.spec.ts` tests run against real REAPER.
- **Regression coverage from PR #180:** `scripts/check_track_index_validator.py` already fails if anyone re-introduces an `inputs.len()` track-index validator. The existing Rust unit tests (`collect_valid_input_indices`, `is_valid_track_index`, `validate_track_index`, `lookup_input_name`) already cover the "arbitrary REAPER index" case — no new unit tests needed; track index 45 is the same code path as 44.
- **Production verification:** autonomous Playwright probe after deploy confirms CG channel visible in Tech tab, fader moves REAPER D_VOL, mute flips REAPER MUTE flag. No human tester.

## Error handling

None new. Existing error paths cover:

- Missing REAPER track named "CG" → `build_channel_templates` skips the entry; UI simply doesn't show CG. Caught by test 1.
- Send to a member inear missing → existing `find_send_by_destination` returns None; server rejects the command with a structured error. Caught by tests 2 and 3.
- Config drift between YAML repo and `%APPDATA%\iem-mixer\config.yaml` → deploy verification catches (if runtime config lacks CG, test 1 fails post-deploy).

## Rollout

1. Version bump first commit on `dev`.
2. Config YAML change committed.
3. Manual REAPER setup on iem.lan (L/R tracks, merge, FX, 10 muted sends, save, commit `.RPP`).
4. `%APPDATA%\iem-mixer\config.yaml` patched on iem.lan.
5. `cg.spec.ts` committed.
6. Push, monitor CI to all 10 jobs green.
7. Post-deploy E2E runs `cg.spec.ts` against live REAPER.
8. Autonomous production probe — Playwright against `https://iem.newlevel.media/` confirms feature works.
9. PR `dev → main`, STOP at green PR URL for explicit user merge approval.

## Success criteria

- All 10 CI jobs green, including Deploy to iem.lan.
- Post-deploy `cg.spec.ts` passes all 3 tests (CG visible in Tech tab; fader drop reaches REAPER track 45; mute flag flip reaches REAPER).
- Production Playwright probe: CG channel visible, commands propagate to REAPER, browser console clean.
- README changelog entry shipped on `main`.
