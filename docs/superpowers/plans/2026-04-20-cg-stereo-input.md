# CG Stereo Input Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a new stereo input named "CG" (Dante RX 53/54) for content playback (YouTube videos, music) during presentations. Routed to all 10 member inears (9 band + engineer), default-muted — each member unmutes individually when content plays.

**Architecture:** Pure config + data + one new ReaScript. Zero Rust or WASM changes. The existing `is_input_track` predicate (PR #176) auto-picks up CG by name so TRIM IN + ReaEQ setup works without Lua changes. The existing `collect_valid_input_indices` validator (PR #180) already accepts out-of-position REAPER track indices so CG at index 45 is handled correctly.

**Tech Stack:** YAML config, Lua (ReaScript), TypeScript (Playwright), PowerShell (self-hosted Windows runner), bash (CI monitor).

**Spec:** `docs/superpowers/specs/2026-04-20-cg-stereo-input-design.md`

---

## Hard constraints (airuleset)

- Branch: `dev` only. No feature branches. No worktrees.
- Local checks: only `cd iem-mixer && cargo fmt --all --check`. Hooks block `cargo test/build/clippy/check`.
- Self-hosted Windows runner for iem-lan jobs: `shell: powershell`, never `shell: bash`.
- Single PR `dev → main` at the end. STOP at green PR URL. Do NOT merge without explicit user approval.
- CI monitoring: single `sleep 300 && gh run view <id> --json status,conclusion,jobs` in background. No `/loop`, no cron, no custom monitor scripts.
- All new Playwright tests MUST assert `expect(consoleErrors).toEqual([])`.
- No `#[ignore]`, no `assume()`, no silent-skip patterns. If REAPER is down, tests MUST FAIL.
- Per MEMORY `feedback_live_test_safety.md`: tests that modify REAPER state must restore starting state in a `finally` block.
- Per MEMORY `feedback_reaper_lifecycle_autonomous.md`: if REAPER needs restart, do it autonomously via `mcp__win-iem-snv__Shell`.

---

## File Map

### Files to create

| File | Purpose |
|------|---------|
| `scripts/reascripts/setup_cg.lua` | One-shot REAPER setup for CG track + 10 muted sends |
| `iem-mixer/e2e/tests/live/cg.spec.ts` | 3 Playwright E2E tests asserting REAL REAPER state |

### Files to modify

| File | Change |
|------|--------|
| `iem-mixer/crates/iem-core/Cargo.toml` | version 1.157.0 → 1.158.0 |
| `iem-mixer/Cargo.toml` | version 1.157.0 → 1.158.0 |
| `iem-mixer/crates/iem-server/Cargo.toml` | version 1.157.0 → 1.158.0 |
| `iem-mixer/iem-ui/Cargo.toml` | version 1.157.0 → 1.158.0 |
| `iem-mixer/src-tauri/Cargo.toml` | version 1.157.0 → 1.158.0 |
| `iem-mixer/src-tauri/tauri.conf.json` | version 1.157.0 → 1.158.0 |
| `README.md` | Add v1.158.0 changelog entry at top of changelog |
| `config/input_tracks.yaml` | Add 2 entries under tech block: `CG L` (Dante 53) + `CG R` (Dante 54) |

### Files NOT to modify (confirm this in self-review)

- `iem-mixer/crates/*/src/*.rs` — no Rust changes
- `iem-mixer/iem-ui/src/**/*.rs` — no WASM changes
- `.github/workflows/ci.yml` — no CI changes
- `config/reaper_config.yaml` — setup_cg is a one-off script, not referenced by MCP tools (matches setup_alex_kl.lua convention)

---

## Task 1: Version bump (1.157.0 → 1.158.0) + changelog

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml:3`
- Modify: `iem-mixer/Cargo.toml:12`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml:3`
- Modify: `iem-mixer/iem-ui/Cargo.toml:3`
- Modify: `iem-mixer/src-tauri/Cargo.toml:3`
- Modify: `iem-mixer/src-tauri/tauri.conf.json`
- Modify: `README.md` (insert changelog section after `## Changelog` heading, before `### v1.157.0`)

**Model:** Haiku (mechanical).

- [ ] **Step 1: Bump all 6 version files**

```bash
cd /home/newlevel/devel/reaperiem
sed -i 's/version = "1.157.0"/version = "1.158.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.157.0"/"version": "1.158.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify all 6 files now say 1.158.0**

```bash
grep -n 'version = "1.158.0"' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
grep -n '"version": "1.158.0"' iem-mixer/src-tauri/tauri.conf.json
```
Expected: 6 lines of output, one per file.

- [ ] **Step 3: Insert changelog entry in README.md**

Find the line `## Changelog` and insert the following block immediately after it (above `### v1.157.0`):

```markdown
### v1.158.0 (2026-04-20)

- **Feature**: New stereo input `CG` (Dante RX 53/54) for content playback (YouTube videos, music) during presentations — routed to all 10 member inears, default-muted. Members unmute individually when content plays.

```

Use the Edit tool with old_string `"## Changelog\n\n### v1.157.0"` and new_string including the new block before `### v1.157.0`.

- [ ] **Step 4: Run fmt check (project's only allowed local check)**

```bash
cd /home/newlevel/devel/reaperiem/iem-mixer && cargo fmt --all --check
```
Expected: exit 0, no output.

- [ ] **Step 5: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json \
  README.md
git commit -m "$(cat <<'EOF'
chore: bump version to 1.158.0 + changelog for CG stereo input

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Add CG L / CG R to config/input_tracks.yaml

**Files:**
- Modify: `config/input_tracks.yaml` (insert under tech block after `ENGINEER mic`)

**Model:** Haiku (mechanical YAML edit).

- [ ] **Step 1: Read current tech block to confirm insertion point**

```bash
grep -n "ENGINEER mic" /home/newlevel/devel/reaperiem/config/input_tracks.yaml
```
Expected: one match, around line 182.

- [ ] **Step 2: Edit the file to append the two CG entries**

Use the Edit tool. Old string (the last entry):

```yaml
  - name: "ENGINEER mic"
    dante_input: 52
    category: tech
    default_level_db: 0.0
```

New string:

```yaml
  - name: "ENGINEER mic"
    dante_input: 52
    category: tech
    default_level_db: 0.0

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

- [ ] **Step 3: Verify YAML parses**

```bash
python3 -c "import yaml; yaml.safe_load(open('/home/newlevel/devel/reaperiem/config/input_tracks.yaml'))" && echo OK
```
Expected: `OK`.

- [ ] **Step 4: Verify entry count increased by 2**

```bash
grep -c "^  - name:" /home/newlevel/devel/reaperiem/config/input_tracks.yaml
```
Expected: `32` (was 30: 12 mics incl. ALEX kl L/R + 14 stems + 4 tech; now +2 CG = 32).

- [ ] **Step 5: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add config/input_tracks.yaml
git commit -m "$(cat <<'EOF'
feat(config): add CG stereo input (Dante RX 53/54) under tech

Content playback channel for LED wall presentations. Stereo pair
merged via `stereo_pair: "cg"` — existing build_channel_templates
logic (PR #176) renders one UI channel labeled "CG" in Tech tab.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Create `scripts/reascripts/setup_cg.lua`

**Files:**
- Create: `scripts/reascripts/setup_cg.lua`

**Model:** Sonnet (multi-step logic: pre-flight, idempotent track creation, stereo input encoding, 10 sends, default-muted, save).

This script mirrors `scripts/reascripts/setup_alex_kl.lua` — the only semantic difference is **ALL 10 sends are default-muted** (setup_alex_kl.lua only muted the ENGINEER send). CI will deploy and dynamically register this script via `meter_bridge.lua` (no REAPER restart needed).

- [ ] **Step 1: Write `scripts/reascripts/setup_cg.lua`**

Full file contents:

```lua
-- ONE-SHOT migration script — DO NOT USE AS A TEMPLATE FOR OTHER INSTRUMENTS.
-- The Dante channel number and track name are HARDCODED. Each new instrument
-- needs its own ad-hoc setup script OR a generalized helper (not yet built).
--
-- One-shot setup for CG (stereo content-playback input, Dante RX 53/54).
-- Used to play YouTube videos, music, etc. on the LED wall during
-- presentations. Routed to all 10 member inears (9 band + engineer),
-- DEFAULT-MUTED — members unmute individually when content plays.
-- Idempotent: safe to re-run. Does nothing if CG already exists.
--
-- Creates:
--   1. A stereo REAPER track named "CG" at the end of the track list
--   2. Hardware input = Dante RX 53-54 stereo (channel 52 + 1024 for stereo)
--   3. Sends from CG to every <MEMBER> inear track found in the project, all MUTED
--   4. Saves the project
--
-- Does NOT insert TRIM IN or ReaEQ FX — caller must run
-- _RS_REAPERIEM_SETUP_TRIM and _RS_REAPERIEM_SETUP_EQ after this script.
-- Those scripts iterate all input tracks via is_input_track predicate
-- (PR #176) and will pick up CG automatically.
--
-- Pre-flight: aborts with ERROR if no <MEMBER> inear tracks are found.
--
-- Action ID: _RS_REAPERIEM_SETUP_CG
-- Result written to EXTSTATE: reaperiem/cg_setup_result

local section = "reaperiem"
local TRACK_NAME = "CG"
local DANTE_RX_L = 53  -- HARDCODED: 1-indexed Dante RX channel
local STEREO_INPUT = (DANTE_RX_L - 1) + 1024  -- = 52 + 1024 = 1076 (REAPER stereo input encoding)

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
    -- Include ENGINEER inear. Matches create_sends_for_member.lua convention.
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
    local inears_preflight = find_all_inear_tracks()
    if #inears_preflight == 0 then
        error("no '<MEMBER> inear' tracks found — refusing to create CG with no destinations")
    end

    reaper.Undo_BeginBlock()
    reaper.PreventUIRefresh(1)

    -- Step 1: Ensure CG track exists
    local cg, cg_idx = find_track_by_name(TRACK_NAME)
    local track_created = false
    if not cg then
        local insert_at = reaper.CountTracks(0)
        reaper.InsertTrackAtIndex(insert_at, true)
        cg = reaper.GetTrack(0, insert_at)
        cg_idx = insert_at
        reaper.GetSetMediaTrackInfo_String(cg, "P_NAME", TRACK_NAME, true)
        track_created = true
    end

    -- Step 2: Set stereo channel count and hardware input
    reaper.SetMediaTrackInfo_Value(cg, "I_NCHAN", 2)
    reaper.SetMediaTrackInfo_Value(cg, "I_RECINPUT", STEREO_INPUT)
    reaper.SetMediaTrackInfo_Value(cg, "I_RECARM", 1)
    reaper.SetMediaTrackInfo_Value(cg, "I_RECMON", 1)

    -- Step 3: Create sends to every <MEMBER> inear track, ALL MUTED by default.
    local inears = inears_preflight
    local sends_created = 0
    local sends_skipped = 0
    for _, ie in ipairs(inears) do
        if has_send_to(cg, ie.track) then
            sends_skipped = sends_skipped + 1
        else
            local send_idx = reaper.CreateTrackSend(cg, ie.track)
            if send_idx >= 0 then
                -- Pre-fader post-FX (mode 3) — same as all input tracks.
                reaper.SetTrackSendInfo_Value(cg, 0, send_idx, "I_SENDMODE", 3)
                reaper.SetTrackSendInfo_Value(cg, 0, send_idx, "D_VOL", 1.0)
                reaper.SetTrackSendInfo_Value(cg, 0, send_idx, "D_PAN", 0.0)
                reaper.SetTrackSendInfo_Value(cg, 0, send_idx, "I_SRCCHAN", 0)
                reaper.SetTrackSendInfo_Value(cg, 0, send_idx, "I_DSTCHAN", 0)
                -- CG DIFF vs setup_alex_kl.lua: ALL sends muted by default,
                -- not just engineer. Matches the design decision — content
                -- playback is opt-in per member, they unmute when content
                -- is actually playing (presentations, YouTube, etc.).
                reaper.SetTrackSendInfo_Value(cg, 0, send_idx, "B_MUTE", 1)
                sends_created = sends_created + 1
            end
        end
    end

    reaper.PreventUIRefresh(-1)
    reaper.TrackList_AdjustWindows(false)
    reaper.UpdateArrange()
    reaper.Undo_EndBlock("Setup CG", -1)

    reaper.Main_SaveProject(0, false)

    local result = string.format(
        "OK:track_created=%s,track_idx=%d,sends_created=%d,sends_skipped=%d,inears_found=%d",
        tostring(track_created), cg_idx + 1, sends_created, sends_skipped, #inears
    )
    reaper.SetExtState(section, "cg_setup_result", result, false)
end

local ok, err = pcall(setup)
if not ok then
    reaper.SetExtState(section, "cg_setup_result", "ERROR:" .. tostring(err), false)
end
```

- [ ] **Step 2: Verify the file is syntactically valid Lua**

```bash
lua5.3 -e 'loadfile("/home/newlevel/devel/reaperiem/scripts/reascripts/setup_cg.lua")' && echo OK
```
Expected: `OK` (if `lua5.3` not available, try `lua` or skip — CI will catch syntax errors on first run).

- [ ] **Step 3: Confirm the diff vs setup_alex_kl.lua is minimal**

```bash
diff /home/newlevel/devel/reaperiem/scripts/reascripts/setup_alex_kl.lua \
     /home/newlevel/devel/reaperiem/scripts/reascripts/setup_cg.lua | head -60
```
Expected differences: TRACK_NAME, DANTE_RX_L, header comments, B_MUTE=1 always (no `if engineer` branch), EXTSTATE key `cg_setup_result`.

- [ ] **Step 4: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add scripts/reascripts/setup_cg.lua
git commit -m "$(cat <<'EOF'
feat(reaper): setup_cg.lua — one-shot CG stereo track + 10 muted sends

Mirrors setup_alex_kl.lua with Dante RX 53 base and all sends
default-muted (CG is content playback — members unmute individually
when content plays). TRIM IN + ReaEQ inserted by existing
setup_input_trim / setup_input_eq via is_input_track predicate.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Create `iem-mixer/e2e/tests/live/cg.spec.ts`

**Files:**
- Create: `iem-mixer/e2e/tests/live/cg.spec.ts`

**Model:** Sonnet (3 tests with HTTP roundtrip assertions, precise selector matching).

Mirror `alex-kl.spec.ts` exactly. Three tests:

1. "appears in the Tech tab as a single stereo channel"
2. "dragging the CG fader changes REAPER send level" (fader drop verified via HTTP `GET /TRACK/N/SEND/M/VOL`)
3. "muting CG mutes the REAPER send — not just the UI" (mute flag verified via HTTP)

Key differences from `alex-kl.spec.ts`:

- Channel name is `CG` (single word) — selector uses `.ch-name` with `hasText: /^CG$/` and no `.ch-type` filter.
- Tab is `Tech`, not `Mics`.
- **Starting send state is MUTED (flag=8)** because CG is created default-muted. Test 2 (fader) must unmute first before observing level change. Test 3 (mute) must unmute first to observe a mute transition, then restore muted state in finally.
- Member login: use `stevo` (same as alex-kl.spec.ts) per MEMORY `feedback_live_test_safety.md` — all write tests restore state in `finally` to avoid leaking between runs.

- [ ] **Step 1: Write the file**

Full file contents:

```typescript
import { test, expect, Page } from "@playwright/test";

// Login helper matching alex-kl.spec.ts / stems-volume.spec.ts convention
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

// Resolve CG's REAPER track index and its STEVO-inear send index
// directly from REAPER HTTP so restore is robust to track reordering.
// Returns { cgIdx, stevoInearIdx, stevoSendIdx, muteBefore, volBefore }.
async function resolveCgStevoSend(
  request: import("@playwright/test").APIRequestContext,
) {
  const tracksResp = await request.get("http://iem.lan:8080/_/NTRACK;TRACK");
  const tracksText = await tracksResp.text();
  const lines = tracksText.split("\n");
  const findRow = (needle: string) =>
    lines.find((l) => {
      const parts = l.split("\t");
      return parts[0] === "TRACK" && parts[2] === needle;
    });
  const cgRow = findRow("CG");
  const stevoInearRow = findRow("STEVO inear");
  expect(cgRow, "CG track not found in REAPER").toBeTruthy();
  expect(stevoInearRow, "STEVO inear track not found in REAPER").toBeTruthy();
  const cgIdx = parseInt(cgRow!.split("\t")[1], 10);
  const stevoInearIdx = parseInt(stevoInearRow!.split("\t")[1], 10);

  let stevoSendIdx = -1;
  let muteBefore = 0;
  let volBefore = 1.0;
  for (let s = 0; s < 20; s++) {
    const r = await request.get(
      `http://iem.lan:8080/_/GET/TRACK/${cgIdx}/SEND/${s}`,
    );
    const line = (await r.text()).trim();
    if (!line.startsWith("SEND")) break;
    const p = line.split("\t");
    if (p.length >= 7 && parseInt(p[6], 10) === stevoInearIdx) {
      stevoSendIdx = s;
      muteBefore = parseInt(p[3], 10);
      volBefore = parseFloat(p[4]);
      break;
    }
  }
  expect(
    stevoSendIdx,
    "CG → STEVO inear send not found",
  ).toBeGreaterThanOrEqual(0);

  return { cgIdx, stevoInearIdx, stevoSendIdx, muteBefore, volBefore };
}

test.describe("CG (stereo content-playback input)", () => {
  test("appears in the Tech tab as a single stereo channel", async ({
    page,
  }) => {
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

    // Navigate to Tech tab
    const techTab = page.locator("text=Tech").first();
    await expect(techTab).toBeVisible({ timeout: 10000 });
    await techTab.click();
    await page.waitForTimeout(200);

    // Assert the CG channel exists. Single-word name "CG" → only .ch-name
    // matches, no .ch-type suffix (unlike "ALEX kl").
    const cg = page
      .locator(".channel")
      .filter({ has: page.locator(".ch-name", { hasText: /^CG$/ }) });
    await expect(cg).toHaveCount(1, { timeout: 10000 });
    await expect(cg.first()).toBeVisible();

    expect(consoleErrors).toEqual([]);
  });

  test("dragging the CG fader changes REAPER send level", async ({
    page,
    request,
  }) => {
    const {
      cgIdx,
      stevoSendIdx,
      muteBefore,
      volBefore,
    } = await resolveCgStevoSend(request);

    // CG sends are default-muted (mute flag = 8). To observe a fader-level
    // change, we must first UNMUTE the send — otherwise REAPER may optimize
    // away level changes on a muted send, or the .db-display may not reflect
    // drag motion. Restore original mute state in finally.
    if (muteBefore !== 0) {
      await request.get(
        `http://iem.lan:8080/_/SET/TRACK/${cgIdx}/SEND/${stevoSendIdx}/MUTE/0`,
      );
    }

    try {
      await page.goto("/");
      await loginAs(page, "stevo");
      await page.goto("/stevo");
      await waitForMixer(page);

      const techTab = page.locator("text=Tech").first();
      await expect(techTab).toBeVisible({ timeout: 10000 });
      await techTab.click();
      await page.waitForTimeout(200);

      const cg = page
        .locator(".channel")
        .filter({ has: page.locator(".ch-name", { hasText: /^CG$/ }) })
        .first();
      await expect(cg).toBeVisible({ timeout: 10000 });

      const dbLabel = cg.locator(".db-display");
      const parseDb = async () => {
        const txt = (await dbLabel.textContent()) || "0";
        if (/[\u221E]|inf/i.test(txt)) return -Infinity;
        return parseFloat(txt.replace(/[^-\d.]/g, ""));
      };
      const dbStart = await parseDb();

      const fader = cg.locator(".fader-track");
      const box = await fader.boundingBox();
      expect(box).not.toBeNull();

      // Incremental drag from 70% to 30% — single-jump moves don't trigger
      // pointer events on this component (matches alex-kl.spec.ts pattern).
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

      // PRIMARY: verify REAPER actually received the change via HTTP
      // roundtrip — not the UI's optimistic .db-display. This is the
      // PR #180 lesson: UI shows commands applied whether or not REAPER
      // received them.
      const postDragResp = await request.get(
        `http://iem.lan:8080/_/GET/TRACK/${cgIdx}/SEND/${stevoSendIdx}`,
      );
      const postDragParts = (await postDragResp.text()).trim().split("\t");
      const postDragVol = parseFloat(postDragParts[4]);
      expect(
        postDragVol,
        `REAPER send D_VOL must drop after UI drag — was ${volBefore}, got ${postDragVol}. If unchanged, UI command did not reach REAPER.`,
      ).toBeLessThan(volBefore * 0.7);

      // SECONDARY: UI reflects the same state
      const dbEnd = await parseDb();
      expect(dbEnd).toBeLessThan(dbStart - 3);
      expect(dbEnd).toBeLessThan(0);
    } finally {
      // Restore starting state — both volume and mute — so this live test
      // doesn't leak state between runs.
      await request.get(
        `http://iem.lan:8080/_/SET/TRACK/${cgIdx}/SEND/${stevoSendIdx}/VOL/${volBefore}`,
      );
      await request.get(
        `http://iem.lan:8080/_/SET/TRACK/${cgIdx}/SEND/${stevoSendIdx}/MUTE/${muteBefore}`,
      );
    }
  });

  test("muting CG mutes the REAPER send — not just the UI", async ({
    page,
    request,
  }) => {
    const { cgIdx, stevoSendIdx, muteBefore } = await resolveCgStevoSend(
      request,
    );

    // Starting state for CG is typically MUTED (flag=8). To observe a mute
    // transition we must UNMUTE first, then click the UI mute button,
    // then assert the send is muted again.
    if (muteBefore !== 0) {
      await request.get(
        `http://iem.lan:8080/_/SET/TRACK/${cgIdx}/SEND/${stevoSendIdx}/MUTE/0`,
      );
    }

    try {
      await page.goto("/");
      await loginAs(page, "stevo");
      await page.goto("/stevo");
      await waitForMixer(page);

      const techTab = page.locator("text=Tech").first();
      await expect(techTab).toBeVisible({ timeout: 10000 });
      await techTab.click();
      await page.waitForTimeout(200);

      const cg = page
        .locator(".channel")
        .filter({ has: page.locator(".ch-name", { hasText: /^CG$/ }) })
        .first();
      await expect(cg).toBeVisible({ timeout: 10000 });

      const muteBtn = cg.locator(".mute-btn").first();
      await expect(muteBtn).toBeVisible();
      await muteBtn.click();
      await page.waitForTimeout(700); // WS roundtrip + REAPER apply

      // Authoritative check: REAPER send mute flag must be 8 (muted).
      const afterResp = await request.get(
        `http://iem.lan:8080/_/GET/TRACK/${cgIdx}/SEND/${stevoSendIdx}`,
      );
      const afterParts = (await afterResp.text()).trim().split("\t");
      const muteAfter = parseInt(afterParts[3], 10);
      expect(
        muteAfter,
        "REAPER send MUTE flag must be 8 (muted) after UI mute click. If 0, command did not reach REAPER.",
      ).toBe(8);
    } finally {
      // Restore the original mute state (likely muted=8 for CG default)
      await request.get(
        `http://iem.lan:8080/_/SET/TRACK/${cgIdx}/SEND/${stevoSendIdx}/MUTE/${muteBefore}`,
      );
    }
  });
});
```

- [ ] **Step 2: Verify TypeScript parses by running Playwright's built-in check**

Skip compile check locally (npm install / npx may not be set up). Instead verify syntax by reading the file back and confirming no obvious typos.

```bash
wc -l /home/newlevel/devel/reaperiem/iem-mixer/e2e/tests/live/cg.spec.ts
```
Expected: ~230 lines.

- [ ] **Step 3: Commit**

```bash
cd /home/newlevel/devel/reaperiem
git add iem-mixer/e2e/tests/live/cg.spec.ts
git commit -m "$(cat <<'EOF'
test(e2e): CG stereo input — 3 live tests (Tech tab visibility, fader → REAPER, mute → REAPER)

Mirrors alex-kl.spec.ts. All three tests assert REAL REAPER state via
HTTP roundtrip (not the UI's optimistic .db-display) per the PR #180
lesson. Default-muted starting state is handled by unmuting-first in
tests 2 and 3, restored in finally.

All three assert consoleErrors = [] per airuleset.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Patch runtime `%APPDATA%\iem-mixer\config.yaml` on iem.lan

**Files:**
- Modify (remote, via `mcp__win-iem-snv__FileRead` + `mcp__win-iem-snv__FileWrite`): `C:\Users\newlevel\AppData\Roaming\iem-mixer\config.yaml`

**Why this task exists:** CI preserves the runtime config on every deploy (it never overwrites secrets like `jwt_secret` / `vapid_private_key`). So after updating `config/input_tracks.yaml` in the repo, the runtime copy on iem.lan still lacks CG until we manually patch it. The merged input_tracks section in runtime config is what the deployed server actually reads.

**Model:** Haiku (read + append 2 entries + write).

- [ ] **Step 1: Read runtime config.yaml from iem.lan**

Use `mcp__win-iem-snv__FileRead` with path `C:\Users\newlevel\AppData\Roaming\iem-mixer\config.yaml`.

Find the `input_tracks:` section within the file. The last tech entry should be `ENGINEER mic` with `dante_input: 52`.

- [ ] **Step 2: Append 2 CG entries immediately after the ENGINEER mic entry**

New block to insert (same YAML as Task 2):

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

Be careful: this is YAML — keep existing indentation (2-space under `input_tracks:`). Do NOT touch any other section (especially `jwt_secret`, `vapid_private_key`, `reaper`, `ssh`).

Use `mcp__win-iem-snv__FileWrite` to write back the modified content. Some servers expose `FileWrite` with content-replace semantics — if the tool requires full file content, preserve every byte outside the edited region.

- [ ] **Step 3: Verify via FileRead**

Re-read the file. Confirm:
- `grep -c "CG L"` returns 1
- `grep -c "CG R"` returns 1
- `grep -c "jwt_secret"` is unchanged (should still be present — safety check)
- Entry count for input_tracks increased by 2

Use `mcp__win-iem-snv__Shell` with PowerShell:

```powershell
$c = Get-Content -Raw "$env:APPDATA\iem-mixer\config.yaml"
if ($c -notmatch '"CG L"') { throw "CG L missing" }
if ($c -notmatch '"CG R"') { throw "CG R missing" }
if ($c -notmatch 'jwt_secret') { throw "jwt_secret missing — ABORT, config corruption" }
Write-Output "runtime config OK"
```

Expected: `runtime config OK`.

- [ ] **Step 4: No git commit** — this is a runtime-only change on iem.lan. It's not tracked in the repo.

---

## Task 6: Push T1-T4 to dev and monitor CI

**Model:** Sonnet.

At this point the local branch has 4 commits ahead of origin/dev (version bump, YAML, setup_cg.lua, cg.spec.ts). Push and monitor.

**Expected result:** CI will deploy setup_cg.lua to iem.lan AND re-run post-deploy E2E including cg.spec.ts. The E2E **will FAIL** (RED) because REAPER doesn't yet have the CG track — setup_cg.lua was deployed but not yet triggered. This is expected TDD RED. We proceed to Task 7 to do the REAPER setup, then re-trigger CI in Task 8.

If post-deploy E2E passes without doing the REAPER setup, something is wrong (e.g., a previous manual REAPER setup left a CG track in place). Investigate before continuing.

- [ ] **Step 1: Git-fetch first (airuleset)**

```bash
cd /home/newlevel/devel/reaperiem
git fetch origin
git merge origin/main --no-edit || true  # sync with main if needed
```

- [ ] **Step 2: Confirm branch is ahead by exactly 4 commits**

```bash
git log --oneline origin/dev..HEAD
```
Expected: 4 lines — version bump, YAML, setup_cg.lua, cg.spec.ts.

- [ ] **Step 3: Push**

```bash
git push origin dev
```

- [ ] **Step 4: Monitor CI with single background sleep**

```bash
gh run list --branch dev --limit 3
```

Identify the LATEST run ID (triggered by the push). Then schedule a single background check:

```bash
RUN_ID=<the-id-from-gh-run-list>
# Run in background — single sleep, no loop, no cron.
# Use Bash tool with run_in_background: true
```

Background command (single invocation):

```bash
sleep 300 && gh run view $RUN_ID --json status,conclusion,jobs
```

When it returns, check:

- **If status=completed AND conclusion=success on all 10 jobs** — this shouldn't happen yet (RED expected). If it does, CG may already exist in REAPER from a prior run — verify and skip T7 if so.
- **If status=completed AND "Deploy to iem.lan" is success BUT post-deploy E2E reports cg.spec.ts failures** — EXPECTED RED. Proceed to Task 7.
- **If status=in_progress** — launch another background `sleep 300 && gh run view $RUN_ID` and wait.
- **If status=completed AND a non-E2E job failed** — investigate with `gh run view $RUN_ID --log-failed`, fix in a follow-up commit, do NOT rerun blindly.

- [ ] **Step 5: Verify setup_cg.lua landed on iem.lan**

Once Deploy job reports success, confirm the script deployed:

```powershell
# via mcp__win-iem-snv__Shell
Test-Path "$env:APPDATA\REAPER\Scripts\reaperiem\setup_cg.lua"
```

Expected: `True`.

- [ ] **Step 6: Record the run_id and E2E failure evidence**

Log the cg.spec.ts failure output for the handoff note in Task 7:

```bash
gh run view $RUN_ID --log-failed 2>&1 | grep -A 3 "cg.spec.ts" | head -40
```

---

## Task 7: Trigger REAPER setup on iem.lan + save + commit `.RPP`

**Model:** Sonnet (multi-step REAPER HTTP + verification + commit).

With setup_cg.lua now deployed and dynamically registered (via meter_bridge EXTSTATE flow on CI deploy), we trigger it + setup_input_trim + setup_input_eq, then save and commit the project.

**REAPER readiness:** before starting, verify REAPER is responding. If it's down, start/restart it autonomously per MEMORY `feedback_reaper_lifecycle_autonomous.md`.

- [ ] **Step 1: Verify REAPER is up**

```bash
curl -sf "http://iem.lan:8080/_/NTRACK" && echo "REAPER OK" || echo "REAPER DOWN"
```

If DOWN: use `mcp__win-iem-snv__Shell` to start REAPER:
```powershell
Start-Process "C:\Program Files\REAPER (x64)\reaper.exe"
# Wait up to 30 s for HTTP server to come up
```
Then retry the NTRACK check. Do NOT proceed if REAPER stays down — STOP and alert the user.

- [ ] **Step 2: Trigger setup_cg.lua**

```bash
curl -s "http://iem.lan:8080/_/_RS_REAPERIEM_SETUP_CG"
sleep 2
curl -s "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/cg_setup_result"
```

Expected: `EXTSTATE\treaperiem\tcg_setup_result\tOK:track_created=true,track_idx=45,sends_created=10,sends_skipped=0,inears_found=10`

If result starts with `ERROR:` — STOP, read the error, investigate (likely: no member inears exist, or action ID mismatch from dynamic registration).

If `track_created=false` — CG track already existed. That's OK (idempotent script) but `sends_created` may be 0 in that case. Verify the 10 sends exist in Step 4.

- [ ] **Step 3: Insert TRIM IN + ReaEQ on the CG track**

```bash
# TRIM IN — setup_input_trim iterates all input tracks via is_input_track
# predicate. After setup_cg, CG is now in that set, so this call inserts
# TRIM IN on CG alongside the existing passes on other input tracks.
curl -s "http://iem.lan:8080/_/_RS_REAPERIEM_SETUP_TRIM"
sleep 3
curl -s "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/trim_result"

# ReaEQ — same iteration
curl -s "http://iem.lan:8080/_/_RS_REAPERIEM_SETUP_EQ"
sleep 3
curl -s "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/eq_result"
```

Expected: both result strings start with `OK:` (or similar success marker — actual format depends on the setup scripts; the key is NOT `ERROR:`).

- [ ] **Step 4: Verify CG track has correct shape in REAPER**

```bash
# CG track row in NTRACK;TRACK
curl -s "http://iem.lan:8080/_/NTRACK;TRACK" | grep -P "^TRACK\t\d+\tCG\t"
```
Expected: a single row with track_idx printed in field 2 (likely 45 if this is a fresh setup).

Count sends — CG should have 10:
```bash
CG_IDX=$(curl -s "http://iem.lan:8080/_/NTRACK;TRACK" | awk -F'\t' '$1=="TRACK" && $3=="CG" {print $2; exit}')
for s in 0 1 2 3 4 5 6 7 8 9 10; do
  curl -s "http://iem.lan:8080/_/GET/TRACK/${CG_IDX}/SEND/${s}" | head -1
done
```
Expected: 10 lines starting with `SEND`, then 1 line that does NOT start with SEND (indicates no 11th send). Verify send 0..9 each has field 3 = `8` (mute flag).

Verify TRIM IN + ReaEQ:
```bash
# Number of FX on CG track — should be >= 2 (TRIM IN + ReaEQ)
curl -s "http://iem.lan:8080/_/GET/TRACK/${CG_IDX}" | head -1
# Inspect FX count via the TRACK line (or via NFX extension if supported)
```

- [ ] **Step 5: Save the REAPER project (action 40026)**

```bash
curl -s "http://iem.lan:8080/_/40026"
sleep 2
```

Expected: 200 OK. No result check — action 40026 is silent on success.

- [ ] **Step 6: Commit `.RPP` on iem.lan via MCP git tool**

```
mcp__reaperiem__git_status
```
Expected: `projects/*.RPP` modified.

```
mcp__reaperiem__git_commit(message="feat(reaper): add CG stereo track (Dante RX 53/54) with 10 muted sends + TRIM + ReaEQ (#plan 2026-04-20-cg-stereo-input)")
mcp__reaperiem__git_push
```

- [ ] **Step 7: Confirm commit landed**

```
mcp__reaperiem__git_log(count=3)
```
Expected: the new REAPER-project commit at HEAD on the iem.lan-side dev branch.

---

## Task 8: Re-run post-deploy E2E + production verification + PR

**Model:** Sonnet.

**Step A — Re-trigger post-deploy E2E:**

CI runs post-deploy E2E as part of the `dev` push workflow. To re-run without a code change, use `gh workflow run`:

- [ ] **Step A1: Re-run the post-deploy E2E**

```bash
# Option A: re-run the last dev workflow
LAST_RUN=$(gh run list --branch dev --limit 1 --json databaseId -q '.[0].databaseId')
gh run rerun $LAST_RUN --failed
# --failed re-runs only the failed jobs (post-deploy E2E)
```

- [ ] **Step A2: Monitor the re-run**

```bash
gh run list --branch dev --limit 3
# Identify new run_id, then:
sleep 300 && gh run view <new_run_id> --json status,conclusion,jobs
```

Background via Bash run_in_background=true. When it returns:

- All 10 jobs GREEN including cg.spec.ts → proceed to Step B.
- cg.spec.ts still failing → investigate with `gh run view --log-failed`, likely causes: send index mismatch, .db-display timing, mute state drift. Fix in a follow-up commit on dev, push, monitor again.

**Step B — Autonomous production verification (no human tester):**

- [ ] **Step B1: Open production app in Playwright via MCP**

```
mcp__plugin_playwright_playwright__browser_navigate(url="https://iem.newlevel.media/")
```

- [ ] **Step B2: Login as stevo via the UI (or API+localStorage pattern)**

Since we control the browser directly via MCP Playwright, use the app's login form:

```
mcp__plugin_playwright_playwright__browser_snapshot()
# Find the PIN entry, type 7711, select stevo
```

Or prefer evaluate with localStorage-seeding same as the test:

```javascript
await fetch('/api/auth', { method:'POST', headers:{'content-type':'application/json'}, body: JSON.stringify({member:'stevo', pin:'7711'}) })
  .then(r=>r.json())
  .then(d => localStorage.setItem('iem_token', JSON.stringify(d)));
```

Use `mcp__plugin_playwright_playwright__browser_evaluate` to run that.

- [ ] **Step B3: Navigate to `/stevo`, open Tech tab, assert CG visible**

```
mcp__plugin_playwright_playwright__browser_navigate(url="https://iem.newlevel.media/stevo")
mcp__plugin_playwright_playwright__browser_snapshot()
# Locate "Tech" tab link in snapshot, click it
# Locate ".channel" with ch-name="CG", assert visible
```

- [ ] **Step B4: Collect console errors during the probe**

```
mcp__plugin_playwright_playwright__browser_console_messages()
```
Expected: empty array (same airuleset rule as in cg.spec.ts).

- [ ] **Step B5: Verify REAPER state via HTTP from shell**

```bash
curl -s "http://iem.lan:8080/_/NTRACK;TRACK" | grep -cP "^TRACK\t\d+\tCG\t"
# Expected: 1
```

**Step C — Open PR dev → main and STOP:**

- [ ] **Step C1: Ensure origin/dev is up to date**

```bash
cd /home/newlevel/devel/reaperiem
git fetch origin
git status
# Expected: clean, up to date with origin/dev
```

- [ ] **Step C2: Create the PR**

```bash
gh pr create --base main --head dev \
  --title "feat: CG stereo content-playback input (Dante 53/54)" \
  --body "$(cat <<'EOF'
## Summary

- Adds **CG** — a new stereo input (Dante RX 53/54) for playing content (YouTube videos, music, etc.) on the LED wall during presentations.
- Routed to all 10 member inears (9 band + engineer), **default-muted** — members unmute individually when content plays.
- Pure config + one new ReaScript. Zero Rust / WASM / CI changes.

## Spec & plan

- Spec: `docs/superpowers/specs/2026-04-20-cg-stereo-input-design.md`
- Plan: `docs/superpowers/plans/2026-04-20-cg-stereo-input.md`

## Manual steps already completed on iem.lan

- New stereo REAPER track `CG` (index 45) with Dante RX 53-54 stereo input.
- TRIM IN + ReaEQ inserted on CG.
- 10 sends created — one per `<MEMBER> inear` — all pre-fader post-FX, **all default-muted (mute flag = 8)**.
- `.RPP` committed via `mcp__reaperiem__git_commit` + pushed.
- Runtime `%APPDATA%\iem-mixer\config.yaml` patched with 2 new input entries (CI preserves it across deploys).

## E2E coverage

New `iem-mixer/e2e/tests/live/cg.spec.ts` — 3 tests, all asserting REAL REAPER state (PR #180 pattern):

- `CG appears in the Tech tab as a single stereo channel`
- `dragging the CG fader changes REAPER send level` — HTTP-verifies `D_VOL` dropped on REAPER track 45
- `muting CG mutes the REAPER send — not just the UI` — HTTP-verifies mute flag = 8

All three assert `consoleErrors = []` per airuleset browser-console-zero-errors.

## Test plan

- [x] Lint & Format — `cargo fmt --all --check` clean
- [x] Test Integrity Check
- [x] Tests (no new Rust tests needed — track-index-45 exercises the same code path as track-index-44 from PR #180)
- [x] Mutation Testing
- [x] Build WASM Frontend
- [x] Build Tauri (Windows)
- [x] E2E Tests (CI, GitHub runner)
- [x] Build VBAN VST3
- [x] Deploy to iem.lan
- [x] Post-deploy E2E — cg.spec.ts GREEN against live REAPER with CG track 45

## Production verified

- `https://iem.newlevel.media/` shows CG in the Tech tab for a logged-in member.
- Fader drop reaches REAPER track 45 `D_VOL`.
- Mute click flips REAPER send MUTE flag to 8.
- Browser console clean.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step C3: Verify PR is mergeable, clean**

```bash
PR_URL=$(gh pr view --json url -q .url)
PR_NUMBER=$(gh pr view --json number -q .number)
# Wait ~30 s for GitHub to compute mergeable state
sleep 30
gh api repos/zbynekdrlik/reaperiem/pulls/$PR_NUMBER --jq '{mergeable: .mergeable, mergeable_state: .mergeable_state}'
```

Expected: `{"mergeable": true, "mergeable_state": "clean"}`.

If `mergeable_state: "behind"` — sync with main:
```bash
git fetch origin && git merge origin/main --no-edit && git push origin dev
```
Then wait for CI re-run to go green (all 10 jobs) and re-check mergeable state.

If `mergeable_state: "blocked"` — CI is still running. Wait for completion via `sleep 300 && gh run view <id>` in background.

If `mergeable_state: "dirty"` — merge conflicts. Resolve on dev before PR is mergeable.

- [ ] **Step C4: STOP at green PR URL**

Present completion report (per airuleset completion-report.md):

```
## ✅ Work Complete

**Plan fulfillment:**
- [x] Task 1: Version bump 1.157.0 → 1.158.0 + changelog — commit <sha1>
- [x] Task 2: YAML config (CG L, CG R) — commit <sha2>
- [x] Task 3: setup_cg.lua — commit <sha3>
- [x] Task 4: cg.spec.ts — commit <sha4>
- [x] Task 5: Runtime config.yaml patched on iem.lan — no commit (remote-only)
- [x] Task 6: Push + CI monitor — CI deployed setup_cg.lua
- [x] Task 7: REAPER setup via MCP — CG track 45, 10 muted sends, TRIM+EQ, .RPP committed
- [x] Task 8: CI re-run GREEN + production probe GREEN + PR created

**E2E test coverage:**
| Feature | E2E Test File | What It Verifies |
|---------|---------------|------------------|
| CG visible in UI | iem-mixer/e2e/tests/live/cg.spec.ts | Tech tab shows CG stereo channel |
| CG fader → REAPER | iem-mixer/e2e/tests/live/cg.spec.ts | Drag fader → REAPER track 45 D_VOL drops (HTTP verified) |
| CG mute → REAPER | iem-mixer/e2e/tests/live/cg.spec.ts | Click mute → REAPER send MUTE flag = 8 (HTTP verified) |

✅ PR: <URL> — mergeable, clean
✅ CI: green (10 jobs)
✅ Deploy: verified on iem.lan (REAPER track 45 responds to fader + mute)
🌐 Dashboard: https://iem.newlevel.media/ (user-facing) / http://10.77.9.231/ (internal)
```

Then STOP. Do NOT merge without explicit user approval.

---

## Task Dependencies

```
T1 (version)   ─┐
T2 (yaml)      ─┤── parallel-safe, but commit in order
T3 (setup_cg.lua) ─┤
T4 (cg.spec.ts) ─┘
               │
               ▼
T5 (runtime config on iem.lan) ── parallel with T1-T4
               │
               ▼
T6 (push + CI — expect post-deploy RED)
               │
               ▼
T7 (REAPER setup + commit .RPP)
               │
               ▼
T8 (CI re-run GREEN + production probe + PR + STOP)
```

T1-T5 can be done in any order; they write independent files (repo vs. iem.lan runtime). T6 must come after all five. T7 must come after T6 (setup_cg.lua must be deployed first). T8 must come after T7 (cg.spec.ts needs REAPER setup to pass).

---

## Self-Review Against Spec

Spec `docs/superpowers/specs/2026-04-20-cg-stereo-input-design.md` → plan task coverage:

| Spec section | Plan coverage |
|---|---|
| Goal (CG stereo input, default-muted) | T2 (YAML), T3 (setup_cg.lua), T7 (REAPER setup) |
| Config YAML | T2 |
| Runtime config | T5 |
| REAPER manual setup | T3 (script) + T7 (trigger) |
| E2E regression tests (3) | T4 |
| Version + changelog | T1 |
| Data flow | implicit in T3 (setup_cg.lua) + T7 (trigger) |
| Rollout steps | T1 → T8 |
| Success criteria | T8 (CI green, prod probe, PR clean) |
| REAPER track index 45 | T3 + T7 (append at end of track list) |
| Send mute flag 8 | T3 (B_MUTE=1) + T4 (assert mute=8) |
| HTTP assertion pattern (PR #180) | T4 (all 3 tests use HTTP roundtrip) |
| `collect_valid_input_indices` handles index 45 | implicit (existing code from PR #180) |
| No Rust changes | confirmed in File Map |
| No Lua changes to existing scripts | confirmed — only new `setup_cg.lua`; `is_input_track` already handles by name |

No gaps found.
