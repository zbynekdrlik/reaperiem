# ALEX kl — Keyboard Stereo Input Design

**Date:** 2026-04-16
**Status:** Design approved, pending plan
**Context:** Adding a new stereo keyboard input ("ALEX kl") for Alex alongside his existing mic (ALEX mic).

---

## Goal

Add `ALEX kl` as a stereo keyboard input on Dante RX 13 (L) and 14 (R), routable to every band member's personal mix. The new input must behave identically to existing inputs: TRIM IN + EQ FX, correct categorization in the web UI (Mics tab), stereo pair merging, sends to all 10 output tracks.

## Motivation

Alex plays keyboard during services. Band members need independent volume control over his keyboard in their personal mixes, just like any other instrument. Currently his keyboard audio is not routed into the IEM system at all.

## Root-Cause Analysis (why this isn't a trivial change)

Adding a new instrument to this codebase currently requires updates in **8 places** because name-based pattern matching is scattered across Rust server code and Lua ReaScripts. The `config/input_tracks.yaml` file already has `category` and `stereo_pair` fields that look authoritative — but they are **silently ignored** by serde because the `InputTrack` struct in `iem-core/src/config.rs` only deserializes `name`, `dante_input`, `default_level_db`.

Instead, categorization is re-derived from substring checks in:

| Location | Current pattern |
|----------|----------------|
| `proxy.rs:684` `categorize_track()` | contains `mic` or `gtr` → "mics" |
| `setup_input_trim.lua:14` `is_mic_or_gtr()` | matches `mic` or `gtr` |
| `setup_input_eq.lua:10` `needs_eq()` | matches `mic` or `gtr` or `inear` or `stems` |
| `check_input_trim.lua:12` `is_mic_or_gtr()` | matches `mic` or `gtr` |

None of these match `ALEX kl`. Without fixing them, the new track would:
- Appear under the **Stems tab** instead of **Mics** in the web UI
- Have **no TRIM IN** (engineer can't normalize gain)
- Have **no EQ** (band members can't EQ the keyboard in their mix)
- Be **missed by the trim health check** in CI

The fragility is the problem. Each new instrument (violin, bass2, etc.) would require the same four pattern edits. This design fixes the architecture first, then adds ALEX kl cleanly.

---

## Architecture

Three-phase change:

### Phase 1 — Make `config/input_tracks.yaml` the source of truth for categorization

Extend `InputTrack` struct to deserialize the `category` and `stereo_pair` fields that already exist in YAML. Make `proxy.rs` prefer config values and fall back to `categorize_track()` only for REAPER-discovered tracks without config entries.

### Phase 2 — Make Lua FX scripts category-agnostic

Replace `is_mic_or_gtr(name)` name matching with `is_input_track(name)` (matches anything that isn't an output/routing/tech track). This means any future instrument works without Lua changes.

### Phase 3 — Add ALEX kl (config + REAPER actions only, no code)

With the architecture fixed, the actual feature is a config entry + REAPER track creation + running existing FX setup scripts.

---

## Detailed Design

### Phase 1 — Config-driven categorization

**File:** `iem-mixer/crates/iem-core/src/config.rs`

Extend `InputTrack`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputTrack {
    pub name: String,
    pub dante_input: u8,
    #[serde(default)]
    pub default_level_db: f32,

    /// Category override: "mics", "stems", or "tech". If None, falls back to
    /// name-based derivation in categorize_track().
    #[serde(default)]
    pub category: Option<String>,

    /// Stereo pair key. Tracks sharing this key are merged into one stereo
    /// REAPER track (e.g., "alex kl" for ALEX kl L + ALEX kl R).
    #[serde(default)]
    pub stereo_pair: Option<String>,
}
```

**File:** `iem-mixer/crates/iem-server/src/proxy.rs` around line 632

```rust
// Prefer config values; fall back to name matching.
let (category, stereo_pair, stereo_side) = if let Some(cat) = &input.category {
    let side = derive_stereo_side(&input.name);
    (cat.clone(), input.stereo_pair.clone(), side)
} else {
    categorize_track(&input.name)
};
```

Helper `derive_stereo_side(name)` extracts trailing ` L`/` R` suffix; returns `None` when absent.

**Unit tests:** assert that an `InputTrack { category: Some("mics"), stereo_pair: Some("alex kl"), name: "ALEX kl L", .. }` categorizes as "mics" with pair="alex kl", side="L". Ensure fallback still works for tracks without config (REAPER-discovered).

**Backward compatibility:** existing configs without `category`/`stereo_pair` fields keep working — serde `#[serde(default)]` yields `None`, which triggers the fallback path.

### Phase 2 — Lua helper for input track detection

**New file:** `scripts/reascripts/lib_track_filter.lua` (or inline into each of the 3 scripts — prefer inline to avoid Lua `require` path complications on REAPER).

```lua
-- Returns true if the track name identifies an input (mic/instrument) track
-- as opposed to an output (inear), submix (stems), routing (MASTER, TRANSLATOR),
-- or tech (HAND*, ENGINEER*) track.
local function is_input_track(name)
    local lower = name:lower()
    -- Outputs and submixes
    if lower:match("inear$") or lower:match("stems$") then return false end
    -- Routing tracks
    if name == "MASTER" or name == "TRANSLATOR" then return false end
    -- Tech (handhelds, engineer mic)
    if lower:match("^hand") or lower:match("^engineer") then return false end
    return true
end
```

Update 3 scripts:

| Script | Line | Replace |
|--------|------|---------|
| `setup_input_trim.lua` | 14-17 | `is_mic_or_gtr` → `is_input_track` |
| `check_input_trim.lua` | 12-15 | `is_mic_or_gtr` → `is_input_track` |
| `setup_input_eq.lua` | 10-14 | `needs_eq` → `is_input_track(name) or name:lower():match("inear$") or name:lower():match("stems$")` (EQ also applies to inear and stems outputs) |

**Behavior change:** these scripts will now also match any future input track named e.g. `PETRONELA violin`, `BASS keys`, `ALEX kl`. They still skip outputs/tech correctly.

**Risk:** any input-like track that exists in REAPER but shouldn't get TRIM/EQ would now receive them. Mitigation: review the current track list — everything not ending in `inear`/`stems`, not `MASTER`/`TRANSLATOR`, not starting with `HAND`/`ENGINEER` is already expected to have trim+EQ.

### Phase 3 — Add ALEX kl

#### Config files

**File:** `config/input_tracks.yaml` — add two entries in the `mics` section:

```yaml
- name: "ALEX kl L"
  dante_input: 13
  category: mics
  default_level_db: 0.0
  stereo_pair: "alex kl"

- name: "ALEX kl R"
  dante_input: 14
  category: mics
  default_level_db: 0.0
  stereo_pair: "alex kl"
```

Placement: after `ALEX mic` (Dante RX 10), before `PATRIKA mic`.

**File:** `iem-mixer/config/config.production.yaml` — add fallback entry (used when REAPER is unreachable):

```yaml
- name: "ALEX kl"
  dante_input: 13
```

(The production config lists stereo tracks by merged name, matching how it handles DRUMS/BASS/etc.)

#### Project creation script

**File:** `scripts/reascripts/setup_iem_project.lua` at `INPUT_MICS` table (line 26-37):

```lua
{ name = "ALEX kl L",   dante_rx = 13 },
{ name = "ALEX kl R",   dante_rx = 14 },
```

Placement: after `ALEX mic`. Only used when recreating the project from scratch; not part of the production migration path.

#### Stereo merge script

**File:** `scripts/reascripts/merge_stereo_inputs.lua:48`

```lua
local base_names = {"DRUMS", "BASS", "INST", "OTHER", "BGVS", "IEMONLY", "ALEX KL"}
```

(Uppercase `ALEX KL` because the script matches the actual track names `ALEX kl L` and `ALEX kl R` case-insensitively via the pattern match, but the `base_names` entry must match the exact casing used in `find_track_pair`.)

Verify: read `merge_stereo_inputs.lua:18-23` — matching is by exact string `base_name .. " L"` so the `base_names` entry must match track prefix casing. Track prefix is `ALEX kl`, so use `"ALEX kl"` not `"ALEX KL"`.

#### REAPER production migration (manual, one-time, via MCP/curl)

Before merging the PR that changes code, the live REAPER project needs these additions:

1. **Create two new tracks** in REAPER named `ALEX kl L` and `ALEX kl R` with hardware inputs set to Dante RX 13 (mono) and Dante RX 14 (mono) respectively, inserted at positions 11 and 12 (after `ALEX mic`, shifting `PATRIKA mic` and `ANI mic` down).
2. **Run `_RS_REAPERIEM_SETUP_TRIM`** — now matches via `is_input_track` → inserts TRIM IN on both.
3. **Run `_RS_REAPERIEM_SETUP_EQ`** — same mechanism, inserts ReaEQ on both.
4. **Run `merge_stereo_inputs`** — merges L+R into single stereo `ALEX kl` track, sets `I_NCHAN=2`, deletes R track.
5. **Create 10 sends** from `ALEX kl` to each `<MEMBER> inear` track (PETRONELA, STEVO, MAREK, ZUZKA, TINA, MIREC, ALEX, PATRIKA, ANI, ENGINEER). Send volume 1.0 (unity), pre-FX (mode=1), pan=0.
6. **Save project** via action 40026.
7. *Optional:* rename Dante RX channels on `iem-yamaha` to `ALEX kl L` / `ALEX kl R` for FOH visibility (via `netaudio config --device-name iem-yamaha --set-channel-name 13 "ALEX kl L"`).

These REAPER actions cannot be automated in CI because REAPER state is live-edited and backup/restore doesn't cover track creation. They must be performed manually with MCP tools before the code change reaches production, or the deployed server will reference tracks that don't exist.

### No changes needed

- **`band_members.yaml`** — Alex already exists (ID 7, Dante TX 15-16).
- **Poller** (`poller.rs`) — dynamically discovers tracks by name from REAPER, no hardcoded counts.
- **Frontend** (Leptos) — renders channels from server-supplied vector, no hardcoded counts.
- **Backup/preset/snapshot** (`preset.rs`, `snapshot.rs`, `backup.rs`) — indexed by send index, adapts dynamically.
- **E2E tests** — `backup.spec.ts:59` only asserts `track_count > 0`; no tests assume specific counts.
- **Limiter script** (`setup_output_limiter.lua`) — only matches `inear` outputs; unaffected.
- **MCP tool** `add_input_track()` — generic, no changes.

---

## Data Flow

```
FOH stage box (Alex's keyboard jack) → Dante TX on stagebox →
  Dante subscription → iem-yamaha RX 13 (L), RX 14 (R) →
  REAPER hardware input (stereo) on track "ALEX kl" →
    [TRIM IN (JS:volume_pan)] → [ReaEQ] →
  REAPER sends (10 × unity) →
    → PETRONELA inear, STEVO inear, ..., ENGINEER inear →
    Dante TX 3-4, 5-6, ..., 33-34 →
  Yamaha MRX7 or in-ear receiver
```

Web UI data flow unchanged: poller queries REAPER, sends WebSocket events to clients, clients render a channel for "ALEX kl" in the Mics tab.

---

## Testing

### Unit tests (Rust)

Add to `proxy.rs` test module:

- `test_categorize_track_uses_config_when_present` — InputTrack with `category: Some("mics")` yields "mics" even if name would trigger a different category.
- `test_categorize_track_fallback_when_config_missing` — InputTrack with `category: None` falls back to name-based `categorize_track`.
- `test_stereo_side_derivation` — "ALEX kl L" → Some("L"), "ALEX kl R" → Some("R"), "ALEX kl" → None.

### Unit tests (Lua)

Not practical in CI (no Lua test harness configured). Validate via integration:

- After running `setup_input_trim` in live REAPER, assert `ALEX kl` is in the `inserted_tracks` EXTSTATE result.
- After running `setup_input_eq`, same check.
- Run `check_input_trim` → EXTSTATE `trim_check` must contain `ALEX kl=0.0dB` (or similar), no `missing=ALEX kl`.

### E2E tests (Playwright, against live iem.lan)

Add `iem-mixer/e2e/tests/live/alex-kl.spec.ts`:

1. Log in as any band member (e.g., stevo).
2. Navigate to the Mics tab.
3. Assert a channel with name "ALEX kl" is visible.
4. Drag its fader to -10 dB.
5. Query REAPER directly (`/_/GET/TRACK/{alex_kl_track}/SEND/{stevo_send_idx}/VOL`) and assert the value is ~0.316 (linear for -10 dB).
6. Open the EQ modal on ALEX kl — assert 5 bands render.
7. Console must have zero errors.

This test FAILS before the feature ships (no ALEX kl track) and PASSES after. Committed as permanent regression coverage.

---

## Error Handling

- **Missing ReaEQ plugin** (unlikely — ships with REAPER) → `setup_input_eq` EXTSTATE result will contain `errors:Failed to insert ReaEQ on: ALEX kl`. CI integration check surfaces this.
- **Missing JS:volume_pan** (ships with REAPER JS plugins) → same failure surface as above, via `trim_setup_result`.
- **Dante RX 13/14 not subscribed** → `ALEX kl` track exists in REAPER but receives silence. Not a code bug; FOH must patch the stage keyboard to RX 13/14 on the IEM accelerator. Detected operationally by level meter showing -1500 dB (digital silence) on ALEX kl.
- **Stereo merge race** — if `merge_stereo_inputs` runs while the R track is still being created, pair is incomplete. Mitigation: run merge script *after* both tracks exist and are saved. The script's `find_track_pair` tolerates partial state (logs WARNING, skips merge).

---

## Migration / Rollout

1. Land Phase 1 + 2 (code changes) first, merged to main. These are backward-compatible — no behavior change for existing tracks because all existing YAML entries have no `category` field, falling back to name-based matching.
2. Land Phase 3 (config changes) in a separate PR after Phase 1+2 are deployed.
3. **Before merging Phase 3**, manually create the REAPER tracks on iem.lan (see "REAPER production migration" above). Otherwise the deployed server will expose "ALEX kl" in the UI but clicking its fader will fail (track doesn't exist in REAPER).
4. Run `_RS_REAPERIEM_CHECK_TRIM` post-deploy to verify ALEX kl has TRIM IN.
5. Verify the E2E test passes against live iem.lan.

### Rollback

Phase 3 alone: revert the 4 files listed — ALEX kl vanishes from UI. The REAPER track can stay (it just won't be exposed). No data loss.

Phase 1+2: revert the struct extension and Lua helper. No behavior change for existing tracks.

---

## Open Questions

None. Design is fully specified.

---

## File Change Summary

| File | Phase | Change |
|------|-------|--------|
| `iem-mixer/crates/iem-core/src/config.rs` | 1 | Add `category`, `stereo_pair` fields to `InputTrack` |
| `iem-mixer/crates/iem-server/src/proxy.rs` | 1 | Use config category when present; helper `derive_stereo_side`; unit tests |
| `scripts/reascripts/setup_input_trim.lua` | 2 | Replace `is_mic_or_gtr` with `is_input_track` |
| `scripts/reascripts/setup_input_eq.lua` | 2 | Replace `needs_eq` with `is_input_track` + inear/stems |
| `scripts/reascripts/check_input_trim.lua` | 2 | Replace `is_mic_or_gtr` with `is_input_track` |
| `config/input_tracks.yaml` | 3 | Add ALEX kl L and ALEX kl R entries |
| `iem-mixer/config/config.production.yaml` | 3 | Add ALEX kl fallback entry |
| `scripts/reascripts/setup_iem_project.lua` | 3 | Add L/R entries to `INPUT_MICS` table |
| `scripts/reascripts/merge_stereo_inputs.lua` | 3 | Add `"ALEX kl"` to `base_names` |
| `iem-mixer/e2e/tests/live/alex-kl.spec.ts` | 3 | New E2E test for ALEX kl channel |
| REAPER project on iem.lan | 3 | Manual: create tracks, run FX setup + merge + sends + save |

Version bump: 1.152.0 → 1.153.0 (Phase 1+2 PR), then 1.153.0 → 1.154.0 (Phase 3 PR).
