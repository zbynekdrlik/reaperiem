# ALEX kl Keyboard Input Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add ALEX kl (stereo keyboard, Dante RX 13/14) as a new input track, after first fixing the categorization architecture so future instruments don't need code changes.

**Architecture:** Three phases. Phase 1 makes the YAML `category`/`stereo_pair` fields authoritative by deserializing them into `InputTrack` (they were silently ignored). Phase 2 generalises 3 Lua FX scripts from `mic`-or-`gtr` name matching to an `is_input_track` predicate. Phase 3 adds ALEX kl config entries, REAPER track manual creation on iem.lan, and a permanent E2E regression test.

**Tech Stack:** Rust (Axum/serde/tokio), Lua (ReaScripts), YAML configs, Playwright E2E tests, REAPER HTTP API.

**Spec:** `docs/superpowers/specs/2026-04-16-alex-kl-keyboard-design.md`

---

## Constraints (airuleset)

- **Version bump must be FIRST commit** (1.152.0 → 1.153.0). CI fails otherwise.
- **No local cargo test/build/clippy/check** — hooks block these. Only `cargo fmt --all --check` runs locally.
- **Self-hosted Windows runner** — never use `shell: bash` in any new `.yml` (not applicable here; no workflow changes).
- **Two-branch workflow** — stay on `dev`, push to `dev`, create PR to `main`.
- **Final step** — green PR URL, STOP, do not merge.
- **REAPER track creation on iem.lan happens BEFORE pushing code** (so E2E test passes on first CI run, not after a rerun).

---

## File Map

### Code files (Phase 1)
- Modify: `iem-mixer/crates/iem-core/src/config.rs` — extend `InputTrack` struct
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs` — use config-first categorization + unit tests

### Lua scripts (Phase 2)
- Modify: `scripts/reascripts/setup_input_trim.lua` — replace `is_mic_or_gtr` with `is_input_track`
- Modify: `scripts/reascripts/check_input_trim.lua` — replace `is_mic_or_gtr` with `is_input_track`
- Modify: `scripts/reascripts/setup_input_eq.lua` — replace `needs_eq` with `is_input_track`-based predicate

### Config & track files (Phase 3)
- Modify: `config/input_tracks.yaml` — add ALEX kl L and R entries
- Modify: `iem-mixer/config/config.production.yaml` — add ALEX kl fallback entry
- Modify: `scripts/reascripts/setup_iem_project.lua` — add ALEX kl L/R to INPUT_MICS
- Modify: `scripts/reascripts/merge_stereo_inputs.lua` — add "ALEX kl" to base_names
- Create: `iem-mixer/e2e/tests/live/alex-kl.spec.ts` — permanent regression test

### Version files (Phase 0 — first commit)
- Modify: `iem-mixer/crates/iem-core/Cargo.toml`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`

### Changelog (after CI green, before PR)
- Modify: `README.md` — add v1.153.0 entry

### Manual REAPER operations (not in code)
- Create 2 new REAPER tracks via HTTP API on iem.lan
- Run `_RS_REAPERIEM_SETUP_TRIM`, `_RS_REAPERIEM_SETUP_EQ` via HTTP API
- Run `merge_stereo_inputs` via HTTP API
- Create 10 sends via HTTP API
- Save project via action 40026

---

## Task 1: Version bump 1.152.0 → 1.153.0

**Files:**
- Modify: 5 Cargo.toml + 1 tauri.conf.json

- [ ] **Step 1: Bump all version files**

```bash
cd /home/newlevel/devel/reaperiem
sed -i 's/version = "1.152.0"/version = "1.153.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.152.0"/"version": "1.153.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify all 6 files updated**

```bash
grep -c '1.153.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
```

Expected: each file returns `1`.

- [ ] **Step 3: Verify no trace of old version left**

```bash
grep '1.152.0' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json || echo "CLEAN"
```

Expected: `CLEAN` (no matches).

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.153.0"
```

---

## Task 2: Extend `InputTrack` struct with `category` and `stereo_pair`

**Files:**
- Modify: `iem-mixer/crates/iem-core/src/config.rs:371-383`

- [ ] **Step 1: Read current struct to confirm exact contents**

```bash
sed -n '371,384p' iem-mixer/crates/iem-core/src/config.rs
```

Expected: shows the 3-field `InputTrack` struct as designed in the spec.

- [ ] **Step 2: Replace the struct with the extended version**

Edit `iem-mixer/crates/iem-core/src/config.rs`. Replace the existing `InputTrack` definition (lines 371-383) with:

```rust
/// Input track configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputTrack {
    /// Track name (e.g., "MAREK mic")
    pub name: String,

    /// Dante input channel (1-indexed)
    pub dante_input: u8,

    /// Default send level in dB
    #[serde(default)]
    pub default_level_db: f32,

    /// Category override: "mics", "stems", or "tech".
    /// When present, takes precedence over name-based derivation in proxy.rs.
    #[serde(default)]
    pub category: Option<String>,

    /// Stereo pair key. Tracks sharing this key are merged into one stereo
    /// REAPER track (e.g., "alex kl" for "ALEX kl L" + "ALEX kl R").
    #[serde(default)]
    pub stereo_pair: Option<String>,
}
```

- [ ] **Step 3: Format and commit**

```bash
cd iem-mixer && cargo fmt --all --check
```

If formatting passes, commit:

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/crates/iem-core/src/config.rs
git commit -m "feat(core): InputTrack accepts category and stereo_pair fields

Previously InputTrack only deserialized name/dante_input/default_level_db.
The category and stereo_pair fields present in input_tracks.yaml were
silently dropped by serde, forcing proxy.rs::categorize_track to re-derive
them from name substring matching.

Backward-compatible: both fields use #[serde(default)] so existing configs
without them continue to work (Option::None = use name-based fallback)."
```

---

## Task 3: Config-first categorization in `proxy.rs` + unit tests

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs:620-645` (build_channel_templates)
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs` (add helper function + tests)

- [ ] **Step 1: Write the failing unit tests first**

Edit `iem-mixer/crates/iem-server/src/proxy.rs`. Find the test module (near line 3463, where `test_categorize_track_mics` lives). Insert these three tests immediately after `test_categorize_hand_mic_as_tech` (around line 3927):

```rust
#[test]
fn test_build_channel_templates_uses_config_category() {
    // When InputTrack has Some(category), it wins over name-based derivation.
    let inputs = vec![iem_core::config::InputTrack {
        name: "ALEX kl L".to_string(),
        dante_input: 13,
        default_level_db: 0.0,
        category: Some("mics".to_string()),
        stereo_pair: Some("alex kl".to_string()),
    }];
    let channels = build_channel_templates(&inputs, None);
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].category, "mics");
    assert_eq!(channels[0].stereo_pair, Some("alex kl".to_string()));
    assert_eq!(channels[0].stereo_side, Some("L".to_string()));
}

#[test]
fn test_build_channel_templates_fallback_when_no_config_category() {
    // When InputTrack has None category, falls back to categorize_track().
    let inputs = vec![iem_core::config::InputTrack {
        name: "MAREK mic".to_string(),
        dante_input: 5,
        default_level_db: 0.0,
        category: None,
        stereo_pair: None,
    }];
    let channels = build_channel_templates(&inputs, None);
    assert_eq!(channels[0].category, "mics");
    assert_eq!(channels[0].stereo_pair, None);
    assert_eq!(channels[0].stereo_side, None);
}

#[test]
fn test_derive_stereo_side() {
    assert_eq!(derive_stereo_side("ALEX kl L"), Some("L".to_string()));
    assert_eq!(derive_stereo_side("ALEX kl R"), Some("R".to_string()));
    assert_eq!(derive_stereo_side("ALEX kl"), None);
    assert_eq!(derive_stereo_side("MAREK mic"), None);
    assert_eq!(derive_stereo_side("DRUMS L"), Some("L".to_string()));
}
```

- [ ] **Step 2: Confirm tests would fail (document expected failure)**

Cannot run cargo test locally. Expected failures if pushed now:

```
error[E0599]: no function or associated item named `derive_stereo_side` found
error: missing field `category` / `stereo_pair` in initializer of `InputTrack`
```

(The second error won't fire because Task 2 already added those fields; only `derive_stereo_side` is undefined.)

- [ ] **Step 3: Add the `derive_stereo_side` helper function**

In `proxy.rs`, immediately BEFORE `categorize_track` (which starts at line 678-679), add:

```rust
/// Extract trailing " L" or " R" stereo-side suffix from a track name.
/// Returns None when no suffix is present.
pub(crate) fn derive_stereo_side(name: &str) -> Option<String> {
    if name.ends_with(" L") {
        Some("L".to_string())
    } else if name.ends_with(" R") {
        Some("R".to_string())
    } else {
        None
    }
}
```

- [ ] **Step 4: Rewire `build_channel_templates` to prefer config values**

In `proxy.rs`, find the body of `build_channel_templates` (line 625-644). Replace the `.map` closure body so the existing line 632:

```rust
let (category, stereo_pair, stereo_side) = categorize_track(&input.name);
```

is replaced with:

```rust
// Prefer explicit config fields; fall back to name-based derivation for
// configs that lack category/stereo_pair (REAPER-discovered tracks,
// legacy configs).
let (category, stereo_pair, stereo_side) = if let Some(cat) = &input.category {
    (
        cat.clone(),
        input.stereo_pair.clone(),
        derive_stereo_side(&input.name),
    )
} else {
    categorize_track(&input.name)
};
```

- [ ] **Step 5: Format**

```bash
cd iem-mixer && cargo fmt --all --check
```

If fmt fails, run `cd iem-mixer && cargo fmt --all` and re-check.

- [ ] **Step 6: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/crates/iem-server/src/proxy.rs
git commit -m "feat(server): use InputTrack.category when present

build_channel_templates now prefers the explicit category/stereo_pair
fields on InputTrack over name-based derivation via categorize_track().
Falls back to categorize_track() when category is None (backward compat).

Adds derive_stereo_side() helper. Adds three unit tests covering both
the config-first path and the fallback path."
```

---

## Task 4: Generalise `setup_input_trim.lua` — use `is_input_track`

**Files:**
- Modify: `scripts/reascripts/setup_input_trim.lua:14-17`

- [ ] **Step 1: Read the current function to confirm exact contents**

```bash
sed -n '14,17p' scripts/reascripts/setup_input_trim.lua
```

Expected output:

```lua
local function is_mic_or_gtr(name)
    local lower = name:lower()
    return lower:match("mic") or lower:match("gtr")
end
```

- [ ] **Step 2: Replace the predicate and its call sites**

Edit `scripts/reascripts/setup_input_trim.lua`. Change lines 14-17 from:

```lua
local function is_mic_or_gtr(name)
    local lower = name:lower()
    return lower:match("mic") or lower:match("gtr")
end
```

to:

```lua
-- Returns true if the track name identifies an input (instrument/mic) track
-- as opposed to an output (inear), submix (stems), routing (MASTER,
-- TRANSLATOR), or tech (HAND*, ENGINEER*) track.
local function is_input_track(name)
    local lower = name:lower()
    if lower:match("inear$") or lower:match("stems$") then return false end
    if name == "MASTER" or name == "TRANSLATOR" then return false end
    if lower:match("^hand") or lower:match("^engineer") then return false end
    return true
end
```

- [ ] **Step 3: Update the one call site in the same file**

Find the call `if is_mic_or_gtr(name) then` (line 41). Replace with:

```lua
        if is_input_track(name) then
```

- [ ] **Step 4: Verify no stale references remain**

```bash
grep -n 'is_mic_or_gtr' scripts/reascripts/setup_input_trim.lua || echo "CLEAN"
```

Expected: `CLEAN`.

- [ ] **Step 5: Commit**

```bash
git add scripts/reascripts/setup_input_trim.lua
git commit -m "refactor(reascript): setup_input_trim uses is_input_track

Replaces mic-or-gtr substring match with an is_input_track() predicate
that matches anything that isn't an output (inear/stems), routing
(MASTER/TRANSLATOR), or tech (HAND*/ENGINEER*) track.

Future instruments (keyboard, violin, bass2, etc.) now work without
Lua changes — the YAML config alone decides."
```

---

## Task 5: Generalise `check_input_trim.lua` — use `is_input_track`

**Files:**
- Modify: `scripts/reascripts/check_input_trim.lua:12-15`

- [ ] **Step 1: Read the current function**

```bash
sed -n '12,15p' scripts/reascripts/check_input_trim.lua
```

Expected:

```lua
local function is_mic_or_gtr(name)
    local lower = name:lower()
    return lower:match("mic") or lower:match("gtr")
end
```

- [ ] **Step 2: Replace the predicate**

Edit `scripts/reascripts/check_input_trim.lua`. Change lines 12-15 to:

```lua
local function is_input_track(name)
    local lower = name:lower()
    if lower:match("inear$") or lower:match("stems$") then return false end
    if name == "MASTER" or name == "TRANSLATOR" then return false end
    if lower:match("^hand") or lower:match("^engineer") then return false end
    return true
end
```

- [ ] **Step 3: Update the call site**

Find `if is_mic_or_gtr(name) then` (line 26). Replace with:

```lua
        if is_input_track(name) then
```

- [ ] **Step 4: Verify**

```bash
grep -n 'is_mic_or_gtr' scripts/reascripts/check_input_trim.lua || echo "CLEAN"
```

Expected: `CLEAN`.

- [ ] **Step 5: Commit**

```bash
git add scripts/reascripts/check_input_trim.lua
git commit -m "refactor(reascript): check_input_trim uses is_input_track

Mirrors the setup_input_trim change so the health check covers the same
set of tracks that the setup script installs trim on."
```

---

## Task 6: Generalise `setup_input_eq.lua` — use `is_input_track` plus inear/stems

**Files:**
- Modify: `scripts/reascripts/setup_input_eq.lua:10-14`

- [ ] **Step 1: Read the current function**

```bash
sed -n '10,14p' scripts/reascripts/setup_input_eq.lua
```

Expected:

```lua
local function needs_eq(name)
    local lower = name:lower()
    return lower:match("mic") or lower:match("gtr")
        or lower:match("inear") or lower:match("stems")
end
```

- [ ] **Step 2: Replace with predicate that covers inputs AND outputs**

EQ is applied to inputs (mic/instrument tracks) AND to outputs (inear/stems) — so we need `is_input_track` OR `inear$` OR `stems$`. Edit `scripts/reascripts/setup_input_eq.lua`. Change lines 10-14 to:

```lua
-- EQ applies to all input tracks AND to output submixes (inear, stems).
-- Tech tracks (HAND*, ENGINEER*) and routing tracks (MASTER, TRANSLATOR)
-- do not get EQ.
local function is_input_track(name)
    local lower = name:lower()
    if lower:match("inear$") or lower:match("stems$") then return false end
    if name == "MASTER" or name == "TRANSLATOR" then return false end
    if lower:match("^hand") or lower:match("^engineer") then return false end
    return true
end

local function needs_eq(name)
    local lower = name:lower()
    return is_input_track(name) or lower:match("inear$") or lower:match("stems$")
end
```

- [ ] **Step 3: Verify**

```bash
grep -n 'mic") or lower:match("gtr' scripts/reascripts/setup_input_eq.lua || echo "CLEAN"
```

Expected: `CLEAN`.

- [ ] **Step 4: Commit**

```bash
git add scripts/reascripts/setup_input_eq.lua
git commit -m "refactor(reascript): setup_input_eq uses is_input_track

EQ applies to input tracks (mic/instrument/keyboard/etc.) AND to output
submixes (inear, stems). Use is_input_track() for the input half and
keep the explicit inear$/stems$ match for the output half — same track
coverage as before, just name-convention-agnostic for inputs."
```

---

## Task 7: Add ALEX kl entries to `config/input_tracks.yaml`

**Files:**
- Modify: `config/input_tracks.yaml` (insert after line 54, after `ALEX mic`)

- [ ] **Step 1: Read the current ALEX mic block to find insertion point**

```bash
sed -n '51,60p' config/input_tracks.yaml
```

Expected: shows ALEX mic (lines 51-54) followed by blank line, then PATRIKA mic (line 56-59).

- [ ] **Step 2: Insert ALEX kl L and ALEX kl R after ALEX mic**

Edit `config/input_tracks.yaml`. Replace the range starting at line 51:

```yaml
  - name: "ALEX mic"
    dante_input: 10
    category: mics
    default_level_db: 0.0

  - name: "PATRIKA mic"
```

with:

```yaml
  - name: "ALEX mic"
    dante_input: 10
    category: mics
    default_level_db: 0.0

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

  - name: "PATRIKA mic"
```

- [ ] **Step 3: Verify YAML is valid**

```bash
python3 -c "import yaml; yaml.safe_load(open('config/input_tracks.yaml'))" && echo "VALID"
```

Expected: `VALID`.

- [ ] **Step 4: Verify both entries present**

```bash
grep -c 'ALEX kl' config/input_tracks.yaml
```

Expected: `2`.

- [ ] **Step 5: Commit**

```bash
git add config/input_tracks.yaml
git commit -m "feat(config): add ALEX kl stereo input (Dante RX 13/14)

Stereo keyboard input for Alex alongside his existing ALEX mic.
Pair key 'alex kl' groups L+R for merging and linked control in UI."
```

---

## Task 8: Add ALEX kl fallback entry to `iem-mixer/config/config.production.yaml`

**Files:**
- Modify: `iem-mixer/config/config.production.yaml` (insert after `ALEX mic` line ~103)

- [ ] **Step 1: Read the current ALEX mic block**

```bash
sed -n '101,108p' iem-mixer/config/config.production.yaml
```

Expected: ALEX mic entry followed by blank line and PATRIKA mic.

- [ ] **Step 2: Insert ALEX kl fallback entry (merged-name form)**

The production config lists stereo tracks by merged name (DRUMS not DRUMS L/R). Edit `iem-mixer/config/config.production.yaml`. Replace:

```yaml
  - name: "ALEX mic"
    dante_input: 10
    default_level_db: 0.0

  - name: "PATRIKA mic"
```

with:

```yaml
  - name: "ALEX mic"
    dante_input: 10
    default_level_db: 0.0

  - name: "ALEX kl"
    dante_input: 13
    default_level_db: 0.0

  - name: "PATRIKA mic"
```

- [ ] **Step 3: Verify YAML is valid**

```bash
python3 -c "import yaml; yaml.safe_load(open('iem-mixer/config/config.production.yaml'))" && echo "VALID"
```

Expected: `VALID`.

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/config/config.production.yaml
git commit -m "feat(config): ALEX kl fallback for web UI config

Fallback entry used when REAPER is unreachable (CI, offline dev).
Follows the existing convention: stereo tracks listed by merged name
(e.g., DRUMS, not DRUMS L / DRUMS R)."
```

---

## Task 9: Add ALEX kl L/R to `setup_iem_project.lua` INPUT_MICS table

**Files:**
- Modify: `scripts/reascripts/setup_iem_project.lua:26-37`

This script is only used when recreating the project from scratch. Updating it keeps the "fresh install" path in sync with live state.

- [ ] **Step 1: Read the current table**

```bash
sed -n '26,37p' scripts/reascripts/setup_iem_project.lua
```

Expected:

```lua
local INPUT_MICS = {
    { name = "PETKA mic",    dante_rx = 3 },
    { name = "STEVO mic",    dante_rx = 4 },
    { name = "MAREK mic",    dante_rx = 5 },
    { name = "ZUZKA mic",    dante_rx = 6 },
    { name = "ZUZKA gtr",    dante_rx = 7 },
    { name = "TINA mic",     dante_rx = 8 },
    { name = "MIREC mic",    dante_rx = 9 },
    { name = "ALEX mic",     dante_rx = 10 },
    { name = "PATRIKA mic",  dante_rx = 11 },
    { name = "ANI mic",      dante_rx = 12 },
}
```

- [ ] **Step 2: Insert L/R entries after ALEX mic**

Replace the existing ALEX mic line and PATRIKA mic line:

```lua
    { name = "ALEX mic",     dante_rx = 10 },
    { name = "PATRIKA mic",  dante_rx = 11 },
```

with:

```lua
    { name = "ALEX mic",     dante_rx = 10 },
    { name = "ALEX kl L",    dante_rx = 13 },
    { name = "ALEX kl R",    dante_rx = 14 },
    { name = "PATRIKA mic",  dante_rx = 11 },
```

- [ ] **Step 3: Verify**

```bash
grep -n 'ALEX kl' scripts/reascripts/setup_iem_project.lua
```

Expected: 2 lines, dante_rx=13 and dante_rx=14.

- [ ] **Step 4: Commit**

```bash
git add scripts/reascripts/setup_iem_project.lua
git commit -m "feat(reascript): add ALEX kl L/R to fresh-project setup

Only relevant when recreating the REAPER project from scratch. Live
production project gets these tracks via manual MCP/curl setup
documented in the plan."
```

---

## Task 10: Add "ALEX kl" to `merge_stereo_inputs.lua` base_names

**Files:**
- Modify: `scripts/reascripts/merge_stereo_inputs.lua:48`

- [ ] **Step 1: Read the current base_names list**

```bash
sed -n '48p' scripts/reascripts/merge_stereo_inputs.lua
```

Expected:

```lua
    local base_names = {"DRUMS", "BASS", "INST", "OTHER", "BGVS", "IEMONLY"}
```

- [ ] **Step 2: Append "ALEX kl"**

The script's `find_track_pair` does exact concatenation `base_name .. " L"`, so the entry MUST match the exact casing of the track prefix. Our tracks are `"ALEX kl L"` / `"ALEX kl R"`, so the entry is `"ALEX kl"` (lowercase `kl`).

Edit `scripts/reascripts/merge_stereo_inputs.lua` line 48. Replace:

```lua
    local base_names = {"DRUMS", "BASS", "INST", "OTHER", "BGVS", "IEMONLY"}
```

with:

```lua
    local base_names = {"DRUMS", "BASS", "INST", "OTHER", "BGVS", "IEMONLY", "ALEX kl"}
```

- [ ] **Step 3: Verify**

```bash
grep -n 'ALEX kl' scripts/reascripts/merge_stereo_inputs.lua
```

Expected: 1 match on line 48.

- [ ] **Step 4: Commit**

```bash
git add scripts/reascripts/merge_stereo_inputs.lua
git commit -m "feat(reascript): include ALEX kl in stereo merge list

Track prefix is 'ALEX kl' (lowercase kl) matching track names
'ALEX kl L' / 'ALEX kl R'. Script uses exact-case concatenation
(base_name .. ' L'), so the entry must preserve casing."
```

---

## Task 11: Write the Playwright E2E regression test

**Files:**
- Create: `iem-mixer/e2e/tests/live/alex-kl.spec.ts`

This test is committed as permanent regression coverage. It will fail if REAPER tracks haven't been created yet — that's why Task 12 (manual REAPER setup) must happen BEFORE pushing.

- [ ] **Step 1: Read an existing live test for reference conventions**

```bash
head -25 iem-mixer/e2e/tests/live/stems-volume.spec.ts
```

Note the `loginAs` helper and `waitForMixer` pattern — the new test reuses the same shape.

- [ ] **Step 2: Create the new test file**

Create `iem-mixer/e2e/tests/live/alex-kl.spec.ts` with:

```typescript
import { test, expect, Page } from "@playwright/test";

// Login helper matching stems-volume.spec.ts / mixer.spec.ts convention
async function loginAs(page: Page, member: string) {
  const response = await page.request.post("/api/auth", {
    data: { member, pin: "7711" },
  });
  if (response.status() === 200) {
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
}

async function waitForMixer(page: Page) {
  await expect(page.locator(".app.mixer, .mixer-header").first()).toBeVisible({
    timeout: 10000,
  });
}

test.describe("ALEX kl (keyboard stereo input)", () => {
  test("appears in the Mics tab as a single stereo channel", async ({
    page,
  }) => {
    // Collect console errors for zero-error assertion
    const consoleErrors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        consoleErrors.push(`[error] ${msg.text()}`);
      }
    });

    await page.goto("/");
    await loginAs(page, "stevo");
    await page.goto("/stevo");
    await waitForMixer(page);

    // Navigate to Mics tab (may already be default)
    const micsTab = page.locator("text=Mics").first();
    if ((await micsTab.count()) > 0) {
      await micsTab.click();
      await page.waitForTimeout(200);
    }

    // Assert the channel exists with exact name "ALEX kl"
    const alexKl = page
      .locator(".channel")
      .filter({ has: page.locator(".ch-name", { hasText: /^ALEX kl$/ }) });
    await expect(alexKl).toHaveCount(1, { timeout: 10000 });
    await expect(alexKl.first()).toBeVisible();

    // Console must be clean for the feature to count as working
    expect(consoleErrors).toEqual([]);
  });

  test("dragging the ALEX kl fader changes REAPER send level", async ({
    page,
    request,
  }) => {
    await page.goto("/");
    await loginAs(page, "stevo");
    await page.goto("/stevo");
    await waitForMixer(page);

    const micsTab = page.locator("text=Mics").first();
    if ((await micsTab.count()) > 0) {
      await micsTab.click();
      await page.waitForTimeout(200);
    }

    // Locate the ALEX kl channel and its fader track
    const alexKl = page
      .locator(".channel")
      .filter({ has: page.locator(".ch-name", { hasText: /^ALEX kl$/ }) })
      .first();
    await expect(alexKl).toBeVisible({ timeout: 10000 });

    const fader = alexKl.locator(".fader-track");
    const box = await fader.boundingBox();
    expect(box).not.toBeNull();

    // Drag fader from current position toward the LEFT (lower volume).
    // Incremental moves are required — single-jump moves don't trigger
    // pointer events on this fader component.
    const startX = box!.x + box!.width * 0.7;
    const endX = box!.x + box!.width * 0.3;
    const y = box!.y + box!.height / 2;

    await page.mouse.move(startX, y);
    await page.mouse.down();
    await page.waitForTimeout(200);
    const steps = 10;
    for (let i = 1; i <= steps; i++) {
      await page.mouse.move(startX + (endX - startX) * (i / steps), y);
      await page.waitForTimeout(40);
    }
    await page.mouse.up();
    await page.waitForTimeout(500);

    // Verify the level dropped from the UI's perspective by re-reading
    // the channel's dB label.
    const dbLabel = alexKl.locator(".ch-db");
    const dbText = await dbLabel.textContent();
    const dbValue = parseFloat((dbText || "0").replace(/[^-\d.]/g, ""));
    // Moving left on the fader means lower dB. Anything < 0 dB proves
    // the drag was registered.
    expect(dbValue).toBeLessThan(0);
  });
});
```

- [ ] **Step 3: Verify test file syntax**

```bash
cd iem-mixer/e2e && npx tsc --noEmit tests/live/alex-kl.spec.ts 2>&1 | head -20
```

Expected: no compile errors. If the project uses a different Playwright TS config, rely on CI to report errors.

- [ ] **Step 4: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/e2e/tests/live/alex-kl.spec.ts
git commit -m "test(e2e): ALEX kl channel visibility and fader drag

Permanent regression test. Logs in as stevo, navigates to Mics tab,
asserts exactly one ALEX kl channel exists, drags its fader left,
asserts resulting dB dropped below 0.

Fails until REAPER tracks are created on iem.lan — that setup is a
required deployment step, not a code change."
```

---

## Task 12: Create a dedicated `setup_alex_kl.lua` ReaScript + run it on iem.lan

Instead of composing many fragile curl calls, write ONE idempotent ReaScript that does the complete ALEX kl setup in a single action. This script can be re-run safely and survives partial failures.

**Files:**
- Create: `scripts/reascripts/setup_alex_kl.lua`

- [ ] **Step 1: Create the setup script**

Create `scripts/reascripts/setup_alex_kl.lua` with:

```lua
-- One-shot setup for ALEX kl (stereo keyboard input).
-- Idempotent: safe to re-run. Does nothing if ALEX kl already exists.
--
-- Creates:
--   1. A stereo REAPER track named "ALEX kl" at the end of the track list
--      (operator can reposition later if desired)
--   2. Hardware input = Dante RX 13-14 stereo (channel 12 + 1024 for stereo)
--   3. Sends from ALEX kl to every <MEMBER> inear track found in the project
--   4. Saves the project
--
-- Does NOT insert TRIM IN or ReaEQ FX — caller must run
-- _RS_REAPERIEM_SETUP_TRIM and _RS_REAPERIEM_SETUP_EQ after this script.
--
-- Action ID: _RS_REAPERIEM_SETUP_ALEX_KL
-- Result written to EXTSTATE: reaperiem/alex_kl_setup_result

local section = "reaperiem"
local TRACK_NAME = "ALEX kl"
local DANTE_RX_L = 13  -- 1-indexed Dante channel; REAPER input is (N-1) + 1024 for stereo
local STEREO_INPUT = (DANTE_RX_L - 1) + 1024  -- = 12 + 1024 = 1036

local function find_track_by_name(name)
    local count = reaper.CountTracks(0)
    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, n = reaper.GetTrackName(track)
        if n == name then return track, i end
    end
    return nil, -1
end

local function find_all_inear_tracks()
    local result = {}
    local count = reaper.CountTracks(0)
    for i = 0, count - 1 do
        local track = reaper.GetTrack(0, i)
        local _, n = reaper.GetTrackName(track)
        if n:lower():match("inear$") then
            table.insert(result, { track = track, name = n, idx = i })
        end
    end
    return result
end

local function has_send_to(src_track, dest_track)
    local send_count = reaper.GetTrackNumSends(src_track, 0)  -- 0 = sends
    for s = 0, send_count - 1 do
        local d = reaper.GetTrackSendInfo_Value(src_track, 0, s, "P_DESTTRACK")
        if d == dest_track then return true end
    end
    return false
end

local function setup()
    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    -- Step 1: Ensure ALEX kl track exists
    local alex_kl, alex_kl_idx = find_track_by_name(TRACK_NAME)
    local track_created = false
    if not alex_kl then
        -- Insert a new track at the end
        local insert_at = reaper.CountTracks(0)
        reaper.InsertTrackAtIndex(insert_at, true)
        alex_kl = reaper.GetTrack(0, insert_at)
        alex_kl_idx = insert_at
        reaper.GetSetMediaTrackInfo_String(alex_kl, "P_NAME", TRACK_NAME, true)
        track_created = true
    end

    -- Step 2: Set stereo channel count and hardware input
    reaper.SetMediaTrackInfo_Value(alex_kl, "I_NCHAN", 2)
    reaper.SetMediaTrackInfo_Value(alex_kl, "I_RECINPUT", STEREO_INPUT)
    -- Arm for recording so input levels are visible on meters
    reaper.SetMediaTrackInfo_Value(alex_kl, "I_RECARM", 1)
    -- Monitor off (input monitor not needed for send pipeline)
    reaper.SetMediaTrackInfo_Value(alex_kl, "I_RECMON", 0)

    -- Step 3: Create sends to every <MEMBER> inear track
    local inears = find_all_inear_tracks()
    local sends_created = 0
    local sends_skipped = 0
    for _, ie in ipairs(inears) do
        if has_send_to(alex_kl, ie.track) then
            sends_skipped = sends_skipped + 1
        else
            local send_idx = reaper.CreateTrackSend(alex_kl, ie.track)
            if send_idx >= 0 then
                -- Pre-FX (I_SENDMODE = 1 means pre-FX post-envelopes; 3 means pre-fader)
                -- Use pre-FX post-envelopes (1) to match existing sends convention.
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "I_SENDMODE", 1)
                -- Volume = unity (1.0)
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "D_VOL", 1.0)
                -- Pan = center (0.0)
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "D_PAN", 0.0)
                -- Source channel = stereo 1-2 (0 = stereo L/R in REAPER send chan spec)
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "I_SRCCHAN", 0)
                -- Dest channel = stereo 1-2 (same convention)
                reaper.SetTrackSendInfo_Value(alex_kl, 0, send_idx, "I_DSTCHAN", 0)
                sends_created = sends_created + 1
            end
        end
    end

    reaper.PreventUIRefresh(-1)
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("Setup ALEX kl", -1)

    -- Save project
    reaper.Main_SaveProject(0, false)

    local result = string.format(
        "OK:track_created=%s,track_idx=%d,sends_created=%d,sends_skipped=%d,inears_found=%d",
        tostring(track_created), alex_kl_idx + 1, sends_created, sends_skipped, #inears
    )
    reaper.SetExtState(section, "alex_kl_setup_result", result, false)
end

local ok, err = pcall(setup)
if not ok then
    reaper.SetExtState(section, "alex_kl_setup_result", "ERROR:" .. tostring(err), false)
end
```

- [ ] **Step 2: Deploy the script to iem.lan + register its action ID**

Copy the script to iem.lan's REAPER scripts folder. The `deploy.sh` usually handles this in CI, but for the manual setup we need the script available on iem.lan NOW. Use MCP tools or scp. Using SCP (adjust path if different):

```bash
scp scripts/reascripts/setup_alex_kl.lua newlevel@iem.lan:"C:/Users/newlevel/AppData/Roaming/REAPER/Scripts/reaperiem/setup_alex_kl.lua"
```

Then register it dynamically via meter_bridge (no REAPER restart needed):

```bash
curl -s "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/register_scripts/setup_alex_kl.lua"
sleep 3
curl -s "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/register_result"
```

Expected: `OK:1` (one script registered).

After registration, REAPER assigns the script an action ID. Query the ID:

```bash
# The assigned action ID pattern is _RS_REAPERIEM_SETUP_ALEX_KL (derived
# from script name), but may vary. Check reaper-kb.ini or via AddRemoveReaScript
# result surfaced by meter_bridge.
ssh newlevel@iem.lan "type C:\\Users\\newlevel\\AppData\\Roaming\\REAPER\\reaper-kb.ini" | grep -i alex_kl
```

Expected: a line showing the custom action with command ID. If the action ID convention matches other scripts, it will be `_RS_REAPERIEM_SETUP_ALEX_KL`.

- [ ] **Step 3: Confirm REAPER is reachable**

```bash
curl -s "http://iem.lan:8080/_/NTRACK" | head -1
```

Expected: `NTRACK	<N>`.

- [ ] **Step 4: Save current REAPER project before any edits**

```bash
curl -s "http://iem.lan:8080/_/40026"
```

- [ ] **Step 5: Run the setup script**

```bash
curl -s "http://iem.lan:8080/_/_RS_REAPERIEM_SETUP_ALEX_KL"
sleep 3
curl -s "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/alex_kl_setup_result"
```

Expected result (first run): `OK:track_created=true,track_idx=44,sends_created=10,sends_skipped=0,inears_found=10`.

Expected result (re-run, idempotent): `OK:track_created=false,track_idx=<N>,sends_created=0,sends_skipped=10,inears_found=10`.

If the result starts with `ERROR:`, read the message and investigate. Likely causes: script syntax error, REAPER API not exposing InsertTrackAtIndex in this version.

- [ ] **Step 6: Run setup_input_trim to insert TRIM IN on ALEX kl**

```bash
curl -s "http://iem.lan:8080/_/_RS_REAPERIEM_SETUP_TRIM"
sleep 2
curl -s "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/trim_setup_result"
```

Expected: result contains `inserted_tracks=...ALEX kl...`.

- [ ] **Step 7: Run setup_input_eq to insert ReaEQ on ALEX kl**

```bash
curl -s "http://iem.lan:8080/_/_RS_REAPERIEM_SETUP_EQ"
sleep 2
curl -s "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/eq_setup_result"
```

Expected: result contains `inserted_tracks=...ALEX kl...`.

- [ ] **Step 8: Verify track exists and has correct state**

```bash
# Single stereo track named "ALEX kl"
curl -s "http://iem.lan:8080/_/NTRACK;TRACK" | grep "ALEX kl"
```

Expected: exactly ONE line. Field 10 (0-indexed: `sendcnt` at column 10, counting from TRACK at column 0) equals `10`.

- [ ] **Step 9: Verify trim check passes**

```bash
curl -s "http://iem.lan:8080/_/_RS_REAPERIEM_CHECK_TRIM"
sleep 2
curl -s "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/trim_check" | tr '|' '\n' | grep "ALEX kl"
```

Expected: `ALEX kl=<value>dB` appears, no `missing=ALEX kl` elsewhere in the result.

- [ ] **Step 10: Save REAPER project**

```bash
curl -s "http://iem.lan:8080/_/40026"
```

- [ ] **Step 11: Commit the new setup script**

```bash
git add scripts/reascripts/setup_alex_kl.lua
git commit -m "feat(reascript): one-shot setup_alex_kl for live migration

Idempotent ReaScript that creates the ALEX kl stereo track (Dante RX
13-14 stereo input), arms it, and creates sends to every <MEMBER> inear
track found in the project. Writes result to EXTSTATE.

Used once per deployment for the manual live-REAPER migration. Safe to
re-run — skips existing track and existing sends."
```

---

## Task 13: Update README.md changelog with v1.153.0 entry

**Files:**
- Modify: `README.md` — add changelog entry

- [ ] **Step 1: Locate the changelog section**

```bash
grep -n '^## Changelog\|^### v1\.' README.md | head -5
```

Expected: `## Changelog` on one line, followed by `### v1.152.0` (most recent).

- [ ] **Step 2: Insert v1.153.0 entry immediately after `## Changelog`**

Edit `README.md`. Find the line `### v1.152.0 (...)` and insert a new entry just above it:

```markdown
### v1.153.0 (2026-04-16)

- **Feature**: Added ALEX kl stereo keyboard input (Dante RX 13/14) routable to all band member mixes.
- **Refactor**: `InputTrack` config struct now honors `category` and `stereo_pair` fields from `input_tracks.yaml` (were previously ignored by serde).
- **Refactor**: REAPER FX setup scripts (`setup_input_trim`, `setup_input_eq`, `check_input_trim`) use a category-agnostic `is_input_track` predicate — future instruments need no Lua changes.
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: changelog entry for v1.153.0 (ALEX kl + categorization refactor)"
```

---

## Task 14: Pre-push checks

- [ ] **Step 1: Confirm tree is clean aside from our commits**

```bash
git status
```

Expected: `nothing to commit, working tree clean`.

- [ ] **Step 2: Show commit log**

```bash
git log --oneline origin/main..HEAD
```

Expected: 13 commits covering tasks 1-13, oldest = version bump, newest = changelog. (T14-T16 don't add commits.)

- [ ] **Step 3: Run cargo fmt check**

```bash
cd iem-mixer && cargo fmt --all --check
```

Expected: no output, exit 0.

- [ ] **Step 4: Verify REAPER tracks are ready (Task 12 completed)**

```bash
curl -s "http://iem.lan:8080/_/NTRACK;TRACK" | grep -c "ALEX kl"
```

Expected: `1` (one stereo merged track named "ALEX kl"). If 0 or 2, Task 12 is incomplete — STOP and finish it before pushing.

---

## Task 15: Push to dev and monitor CI

- [ ] **Step 1: Fetch origin before push**

```bash
cd /home/newlevel/devel/reaperiem
git fetch origin
git log --oneline origin/dev..HEAD | head
```

Expected: your local commits ahead of origin/dev. If not, investigate before pushing.

- [ ] **Step 2: Push**

```bash
git push origin dev
```

- [ ] **Step 3: List recent CI runs to find this push's run**

```bash
gh run list --branch dev --limit 3
```

Record the run ID of the most recent in_progress run.

- [ ] **Step 4: Wait for CI to reach terminal state (single background poll)**

```bash
# Use a single backgrounded sleep+view pattern. CI typically takes 15-20 min.
RUN_ID=<paste run ID from step 3>
# Run this in the background using run_in_background=true:
sleep 900 && gh run view $RUN_ID --json status,conclusion,jobs
```

Expected when complete: `"status": "completed"` and `"conclusion": "success"`. If `"conclusion": "failure"`, proceed to Step 5.

- [ ] **Step 5: If any job failed, investigate root cause and fix in ONE commit**

```bash
gh run view $RUN_ID --log-failed
```

Read the output. Common failures:
- `cargo fmt` — run `cargo fmt --all` and commit
- Rust unit test failure — read the assertion, verify code against the plan
- E2E test failure — Task 12 may be incomplete; re-verify REAPER state
- clippy — fix the warning; do not suppress with `#[allow(...)]`

Fix the issue, stage changes, commit with a message that explains the fix, and push:

```bash
git add <files>
git commit -m "fix: <concise description of root cause>"
git push origin dev
```

Repeat Step 3-5 until all CI jobs are green.

- [ ] **Step 6: Confirm ALL jobs passed (not just some)**

```bash
gh run view $RUN_ID --json jobs --jq '.jobs[] | {name: .name, conclusion: .conclusion}'
```

Expected: every job has `"conclusion": "success"`. Deploy-related jobs must be green, not skipped.

---

## Task 16: Create PR from dev to main

- [ ] **Step 1: Create PR**

```bash
cd /home/newlevel/devel/reaperiem
gh pr create --base main --head dev --title "feat: ALEX kl stereo keyboard input + config-driven categorization" --body "$(cat <<'EOF'
## Summary

- Add ALEX kl (stereo keyboard, Dante RX 13/14) as a new input track routable to all band member mixes.
- Fix the long-standing architectural bug where `InputTrack` struct silently dropped the `category` and `stereo_pair` YAML fields via serde.
- Generalise three Lua FX scripts (`setup_input_trim`, `setup_input_eq`, `check_input_trim`) to use an `is_input_track` predicate — future instruments need no Lua changes.

## Spec and plan

- Spec: `docs/superpowers/specs/2026-04-16-alex-kl-keyboard-design.md`
- Plan: `docs/superpowers/plans/2026-04-16-alex-kl-keyboard.md`

## Manual pre-deploy steps (already completed on iem.lan)

- Two new REAPER tracks created (`ALEX kl L`, `ALEX kl R`), merged into one stereo `ALEX kl` track.
- `setup_input_trim`, `setup_input_eq` run — TRIM IN and ReaEQ applied.
- 10 sends created (to each `<MEMBER> inear` track).
- Project saved.

## Test plan

- [ ] CI green on all jobs (lint, tests, build WASM, e2e CI, build Tauri, deploy, version bump check)
- [ ] Post-deploy E2E on iem.lan: `alex-kl.spec.ts` passes against live REAPER
- [ ] Browser console has zero errors when loading any member's mix with ALEX kl visible

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Verify the PR is mergeable**

```bash
gh pr view --json number --jq '.number' > /tmp/pr_num
PR=$(cat /tmp/pr_num)
gh api repos/zbynekdrlik/reaperiem/pulls/$PR --jq '{mergeable: .mergeable, mergeable_state: .mergeable_state, state: .state}'
```

Expected: `mergeable: true`, `mergeable_state: "clean"`.

If `mergeable_state` is `"behind"`:

```bash
git fetch origin
git merge origin/main
git push origin dev
# Wait for CI again, re-verify
```

If `mergeable_state` is `"blocked"` or `"dirty"`, investigate: open PR page, read the required checks that aren't passing.

- [ ] **Step 3: Confirm all required status checks are green**

```bash
gh pr checks $PR
```

Expected: every required check shows `pass`.

- [ ] **Step 4: Print PR URL for the user**

```bash
gh pr view $PR --json url --jq '.url'
```

**STOP HERE.** Do NOT merge. Present the PR URL and completion report to the user; wait for explicit merge instruction.

---

## Completion criteria

Before sending the completion report, verify:

- [ ] All 16 tasks' checkboxes are `[x]`
- [ ] `git log origin/main..dev` shows at least 13 commits (one per code task T1-T13; extra fix commits for CI allowed)
- [ ] CI run on the latest `dev` push: ALL jobs `conclusion: success`
- [ ] PR: `mergeable: true`, `mergeable_state: "clean"`
- [ ] `alex-kl.spec.ts` was part of the E2E CI run and passed
- [ ] `cargo fmt --all --check` passes locally
- [ ] REAPER on iem.lan has a single `ALEX kl` stereo track with TRIM IN, EQ, and 10 sends

If any item is not `[x]`, do not send the report. Fix first.

## Task dependencies

```
T1 (version bump)     ─┐
                       ▼
T2 (InputTrack struct) ───► T3 (proxy.rs + unit tests)
                       │
T4,5,6 (Lua FX scripts, parallelizable)
                       ▼
T7,8 (YAML configs, parallelizable)
                       ▼
T9 (setup_iem_project) ─┐
T10 (merge_stereo)      ─┤
                        ▼
T11 (E2E test file)     ─┐
                         ▼
T12 (setup_alex_kl.lua + REAPER live setup) ← MUST complete before T15 push
                         ▼
T13 (changelog)
                         ▼
T14 (pre-push checks)
                         ▼
T15 (push + monitor CI)
                         ▼
T16 (PR + mergeable)
```

T2→T3 is sequential (T3 depends on T2's new fields). T4/T5/T6 are fully independent. T7/T8 are independent. T9/T10 are independent but should run after T7/T8 for context. T11 depends on nothing but the Playwright config. T12 is external (REAPER state) — happens in parallel with code work but MUST complete before T15. T15 monitors until green; T16 creates PR and stops.
