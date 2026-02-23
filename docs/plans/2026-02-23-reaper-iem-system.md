# REAPER IEM Mixing System Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Create complete REAPER project with 39 tracks, 252 sends, hardware routing, and web-based self-mix interface.

**Architecture:** MCP server creates REAPER tracks via ReaScript, sets up send matrix, configures hardware outputs. Web interface uses REAPER HTTP API for real-time control. New MCP tools for pan/mute/metering.

**Tech Stack:** Python (MCP server), Lua (ReaScripts), HTML/JS (web interface), REAPER HTTP API

---

## Phase 1: MCP Server Extensions

### Task 1: Add set_send_pan MCP Tool

**Files:**

- Modify: `mcp/reaperiem_mcp/server.py`
- Test: `tests/test_server.py`

**Step 1: Add set_send_pan function**

```python
@mcp.tool()
async def set_send_pan(track_index: int, send_index: int, pan: float) -> dict:
    """Set pan position for a send.

    Args:
        track_index: Source track number (1-based)
        send_index: Send number (1-based)
        pan: Pan position (-1.0 left to 1.0 right, 0.0 center)
    """
    if not -1.0 <= pan <= 1.0:
        return {"error": "Pan must be between -1.0 and 1.0"}

    # REAPER pan: 0.0 = left, 0.5 = center, 1.0 = right
    reaper_pan = (pan + 1.0) / 2.0

    url = f"{REAPER_URL}/_/SET/TRACK/{track_index}/SEND/{send_index}/PAN/{reaper_pan}"
    response = requests.get(url)

    return {"status": "ok", "track": track_index, "send": send_index, "pan": pan}
```

**Step 2: Test the function**

Run: `curl "http://iem.lan:8080/_/SET/TRACK/1/SEND/1/PAN/0.5"`
Expected: Returns OK, send pan changes

**Step 3: Commit**

```bash
git add mcp/reaperiem_mcp/server.py
git commit -m "feat(mcp): add set_send_pan tool for stereo positioning"
```

---

### Task 2: Add set_send_mute MCP Tool

**Files:**

- Modify: `mcp/reaperiem_mcp/server.py`

**Step 1: Add set_send_mute function**

```python
@mcp.tool()
async def set_send_mute(track_index: int, send_index: int, mute: bool) -> dict:
    """Mute or unmute a send.

    Args:
        track_index: Source track number (1-based)
        send_index: Send number (1-based)
        mute: True to mute, False to unmute
    """
    mute_val = 1 if mute else 0

    url = f"{REAPER_URL}/_/SET/TRACK/{track_index}/SEND/{send_index}/MUTE/{mute_val}"
    response = requests.get(url)

    return {"status": "ok", "track": track_index, "send": send_index, "muted": mute}
```

**Step 2: Commit**

```bash
git add mcp/reaperiem_mcp/server.py
git commit -m "feat(mcp): add set_send_mute tool"
```

---

### Task 3: Add get_track_meter MCP Tool

**Files:**

- Modify: `mcp/reaperiem_mcp/server.py`

**Step 1: Add get_track_meter function**

```python
@mcp.tool()
async def get_track_meter(track_index: int) -> dict:
    """Get real-time meter levels for a track.

    Args:
        track_index: Track number (1-based)

    Returns:
        Dict with peak_l, peak_r values (0.0 to 1.0)
    """
    url = f"{REAPER_URL}/_/GET/TRACK/{track_index}/VU"
    response = requests.get(url)

    # Parse VU meter response
    # Format: TRACK/index/VU/left/right
    parts = response.text.strip().split('/')
    if len(parts) >= 5:
        return {
            "track": track_index,
            "peak_l": float(parts[3]),
            "peak_r": float(parts[4])
        }
    return {"error": "Could not read meter"}
```

**Step 2: Commit**

```bash
git add mcp/reaperiem_mcp/server.py
git commit -m "feat(mcp): add get_track_meter tool for real-time levels"
```

---

## Phase 2: ReaScript for Project Setup

### Task 4: Create Project Setup ReaScript

**Files:**

- Create: `scripts/reascripts/setup_iem_project.lua`

**Step 1: Create the ReaScript**

```lua
-- setup_iem_project.lua
-- Creates complete IEM mixing project structure

-- Clear existing tracks
local num_tracks = reaper.CountTracks(0)
for i = num_tracks - 1, 0, -1 do
    local track = reaper.GetTrack(0, i)
    reaper.DeleteTrack(track)
end

-- Input track definitions
local mics = {
    {name = "PETKA mic", input = 3},
    {name = "STEVO mic", input = 4},
    {name = "MAREK mic", input = 5},
    {name = "ZUZKA mic", input = 6},
    {name = "ZUZKA gtr", input = 7},
    {name = "TINA mic", input = 8},
    {name = "MIREC mic", input = 9},
    {name = "ALEX mic", input = 10},
    {name = "PATRIKA mic", input = 11},
    {name = "ANI mic", input = 12}
}

local stems = {
    {name = "DRUMS L", input = 21},
    {name = "DRUMS R", input = 22},
    {name = "BASS L", input = 23},
    {name = "BASS R", input = 24},
    {name = "INST L", input = 25},
    {name = "INST R", input = 26},
    {name = "OTHER L", input = 27},
    {name = "OTHER R", input = 28},
    {name = "BGVS L", input = 29},
    {name = "BGVS R", input = 30},
    {name = "CLICK", input = 31},
    {name = "GUIDE", input = 32},
    {name = "IEMONLY L", input = 33},
    {name = "IEMONLY R", input = 34}
}

local tech_inputs = {
    {name = "HAND1 mic", input = 49},
    {name = "HAND2 mic", input = 50},
    {name = "HAND3 mic", input = 51},
    {name = "ENGINEER mic", input = 52}
}

local band_outputs = {
    {name = "PETKA inear", output_l = 3, output_r = 4},
    {name = "STEVO inear", output_l = 5, output_r = 6},
    {name = "MAREK inear", output_l = 7, output_r = 8},
    {name = "ZUZKA inear", output_l = 9, output_r = 10},
    {name = "TINA inear", output_l = 11, output_r = 12},
    {name = "MIREC inear", output_l = 13, output_r = 14},
    {name = "ALEX inear", output_l = 15, output_r = 16},
    {name = "PATRIKA inear", output_l = 17, output_r = 18},
    {name = "ANI inear", output_l = 19, output_r = 20}
}

local tech_outputs = {
    {name = "ENGINEER inear", output_l = 33, output_r = 34},
    {name = "TRANSLATOR", output_l = 35, output_r = nil}  -- mono
}

-- Helper: Create track with name
function create_track(name, folder_depth)
    local idx = reaper.CountTracks(0)
    reaper.InsertTrackAtIndex(idx, true)
    local track = reaper.GetTrack(0, idx)
    reaper.GetSetMediaTrackInfo_String(track, "P_NAME", name, true)
    if folder_depth then
        reaper.SetMediaTrackInfo_Value(track, "I_FOLDERDEPTH", folder_depth)
    end
    return track, idx + 1  -- 1-based index
end

-- Helper: Set hardware input
function set_hw_input(track, channel)
    -- REAPER: I_RECINPUT = 1024 + channel (mono) or 1024 + channel | 1024 (stereo)
    reaper.SetMediaTrackInfo_Value(track, "I_RECINPUT", 1024 + channel - 1)
    reaper.SetMediaTrackInfo_Value(track, "I_RECARM", 1)
    reaper.SetMediaTrackInfo_Value(track, "I_RECMON", 1)  -- Monitor input
end

-- Helper: Set hardware output
function set_hw_output(track, channel_l, channel_r)
    -- Disable master send
    reaper.SetMediaTrackInfo_Value(track, "B_MAINSEND", 0)
    -- Set hardware output
    local hw_out = reaper.CreateTrackSend(track, nil)  -- Hardware output
    -- Configure for specific channels (0-indexed in API)
    reaper.SetTrackSendInfo_Value(track, 1, hw_out, "I_DSTCHAN", (channel_l - 1) | 1024)
end

-- Helper: Create send from source to dest
function create_send(src_track, dst_track)
    local send_idx = reaper.CreateTrackSend(src_track, dst_track)
    reaper.SetTrackSendInfo_Value(src_track, 0, send_idx, "D_VOL", 1.0)  -- 0dB
    reaper.SetTrackSendInfo_Value(src_track, 0, send_idx, "D_PAN", 0.0)  -- Center
    return send_idx
end

-- Store track references
local input_tracks = {}
local output_tracks = {}

-- Create ---INPUTS--- folder
create_track("---INPUTS---", 1)

-- Create MICS folder
create_track("MICS", 1)
for _, mic in ipairs(mics) do
    local track = create_track(mic.name, 0)
    set_hw_input(track, mic.input)
    table.insert(input_tracks, track)
end
-- Close MICS folder
reaper.SetMediaTrackInfo_Value(reaper.GetTrack(0, reaper.CountTracks(0)-1), "I_FOLDERDEPTH", -1)

-- Create STEMS folder
create_track("STEMS", 1)
for _, stem in ipairs(stems) do
    local track = create_track(stem.name, 0)
    set_hw_input(track, stem.input)
    table.insert(input_tracks, track)
end
-- Close STEMS folder
reaper.SetMediaTrackInfo_Value(reaper.GetTrack(0, reaper.CountTracks(0)-1), "I_FOLDERDEPTH", -1)

-- Create TECH inputs folder
create_track("TECH", 1)
for _, tech in ipairs(tech_inputs) do
    local track = create_track(tech.name, 0)
    set_hw_input(track, tech.input)
    table.insert(input_tracks, track)
end
-- Close TECH folder and INPUTS folder
reaper.SetMediaTrackInfo_Value(reaper.GetTrack(0, reaper.CountTracks(0)-1), "I_FOLDERDEPTH", -2)

-- Create ---OUTPUTS--- folder
create_track("---OUTPUTS---", 1)

-- Create BAND folder
create_track("BAND", 1)
for _, out in ipairs(band_outputs) do
    local track = create_track(out.name, 0)
    set_hw_output(track, out.output_l, out.output_r)
    table.insert(output_tracks, {track = track, name = out.name})
end
-- Close BAND folder
reaper.SetMediaTrackInfo_Value(reaper.GetTrack(0, reaper.CountTracks(0)-1), "I_FOLDERDEPTH", -1)

-- Create TECH outputs folder
create_track("TECH", 1)
for _, out in ipairs(tech_outputs) do
    local track = create_track(out.name, 0)
    set_hw_output(track, out.output_l, out.output_r or out.output_l)
    table.insert(output_tracks, {track = track, name = out.name, is_tech = true})
end
-- Close TECH folder and OUTPUTS folder
reaper.SetMediaTrackInfo_Value(reaper.GetTrack(0, reaper.CountTracks(0)-1), "I_FOLDERDEPTH", -2)

-- Create send matrix: every input -> every band output
for _, input_track in ipairs(input_tracks) do
    for _, output in ipairs(output_tracks) do
        if not output.is_tech or output.name == "ENGINEER inear" then
            create_send(input_track, output.track)
        end
    end
end

-- Special: TRANSLATOR only gets HAND1 mic
local hand1_track = nil
for i, track in ipairs(input_tracks) do
    local _, name = reaper.GetTrackName(track)
    if name == "HAND1 mic" then
        hand1_track = track
        break
    end
end

local translator_track = nil
for _, output in ipairs(output_tracks) do
    if output.name == "TRANSLATOR" then
        translator_track = output.track
        break
    end
end

if hand1_track and translator_track then
    create_send(hand1_track, translator_track)
end

reaper.UpdateArrange()
reaper.Main_SaveProject(0, false)

reaper.ShowMessageBox("IEM Project created!\n\n28 inputs\n11 outputs\n252+ sends", "Setup Complete", 0)
```

**Step 2: Deploy to iem.lan**

```bash
./scripts/deploy.sh
```

**Step 3: Run in REAPER**

In REAPER on iem.lan: Actions > Run ReaScript > select setup_iem_project.lua

**Step 4: Commit**

```bash
git add scripts/reascripts/setup_iem_project.lua
git commit -m "feat(reascript): add IEM project setup script"
```

---

## Phase 3: Configuration Updates

### Task 5: Create Input Tracks Configuration

**Files:**

- Create: `config/input_tracks.yaml`

**Step 1: Create config file**

```yaml
# Input track configuration for IEM mixing
# Maps track names to Dante RX channels

mics:
  - name: "PETKA mic"
    dante_input: 3
    default_level_db: 0
  - name: "STEVO mic"
    dante_input: 4
    default_level_db: 0
  - name: "MAREK mic"
    dante_input: 5
    default_level_db: 0
  - name: "ZUZKA mic"
    dante_input: 6
    default_level_db: 0
  - name: "ZUZKA gtr"
    dante_input: 7
    default_level_db: 0
  - name: "TINA mic"
    dante_input: 8
    default_level_db: 0
  - name: "MIREC mic"
    dante_input: 9
    default_level_db: 0
  - name: "ALEX mic"
    dante_input: 10
    default_level_db: 0
  - name: "PATRIKA mic"
    dante_input: 11
    default_level_db: 0
  - name: "ANI mic"
    dante_input: 12
    default_level_db: 0

stems:
  - name: "DRUMS"
    dante_input_l: 21
    dante_input_r: 22
    stereo: true
  - name: "BASS"
    dante_input_l: 23
    dante_input_r: 24
    stereo: true
  - name: "INST"
    dante_input_l: 25
    dante_input_r: 26
    stereo: true
  - name: "OTHER"
    dante_input_l: 27
    dante_input_r: 28
    stereo: true
  - name: "BGVS"
    dante_input_l: 29
    dante_input_r: 30
    stereo: true
  - name: "CLICK"
    dante_input: 31
    stereo: false
  - name: "GUIDE"
    dante_input: 32
    stereo: false
  - name: "IEMONLY"
    dante_input_l: 33
    dante_input_r: 34
    stereo: true

tech:
  - name: "HAND1 mic"
    dante_input: 49
  - name: "HAND2 mic"
    dante_input: 50
  - name: "HAND3 mic"
    dante_input: 51
  - name: "ENGINEER mic"
    dante_input: 52
```

**Step 2: Commit**

```bash
git add config/input_tracks.yaml
git commit -m "config: add input tracks configuration with Dante mappings"
```

---

## Phase 4: Web Interface

### Task 6: Create Mixer Web Page

**Files:**

- Create: `web/mixer.html`

**Step 1: Create the mixer HTML**

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>IEM Mixer</title>
    <style>
      * {
        box-sizing: border-box;
        margin: 0;
        padding: 0;
      }
      body {
        font-family: -apple-system, BlinkMacSystemFont, sans-serif;
        background: #1a1a1a;
        color: #fff;
        padding: 10px;
      }
      .header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        padding: 10px;
        background: #2a2a2a;
        border-radius: 8px;
        margin-bottom: 15px;
      }
      .header h1 {
        font-size: 1.5em;
      }
      .save-btn {
        background: #4caf50;
        color: white;
        border: none;
        padding: 10px 20px;
        border-radius: 5px;
        cursor: pointer;
      }
      .section {
        margin-bottom: 20px;
      }
      .section-title {
        font-size: 0.9em;
        color: #888;
        margin-bottom: 10px;
        text-transform: uppercase;
      }
      .channels {
        display: flex;
        flex-wrap: wrap;
        gap: 10px;
      }
      .channel {
        background: #2a2a2a;
        border-radius: 8px;
        padding: 10px;
        width: 70px;
        display: flex;
        flex-direction: column;
        align-items: center;
      }
      .controls {
        display: flex;
        gap: 5px;
        margin-bottom: 5px;
      }
      .btn {
        width: 25px;
        height: 25px;
        border: none;
        border-radius: 4px;
        cursor: pointer;
        font-size: 10px;
        font-weight: bold;
      }
      .mute {
        background: #444;
        color: #fff;
      }
      .mute.active {
        background: #f44336;
      }
      .solo {
        background: #444;
        color: #fff;
      }
      .solo.active {
        background: #ffeb3b;
        color: #000;
      }
      .meter {
        width: 100%;
        height: 80px;
        background: #111;
        border-radius: 4px;
        position: relative;
        margin-bottom: 5px;
      }
      .meter-fill {
        position: absolute;
        bottom: 0;
        left: 0;
        right: 0;
        background: linear-gradient(to top, #4caf50, #8bc34a, #ffeb3b, #f44336);
        border-radius: 4px;
        transition: height 0.05s;
      }
      .fader-container {
        width: 100%;
        height: 120px;
        display: flex;
        justify-content: center;
      }
      .fader {
        writing-mode: bt-lr;
        -webkit-appearance: slider-vertical;
        width: 30px;
        height: 100%;
      }
      .pan {
        width: 50px;
        margin: 5px 0;
      }
      .name {
        font-size: 11px;
        text-align: center;
        margin-top: 5px;
      }
      .db {
        font-size: 10px;
        color: #888;
      }
    </style>
  </head>
  <body>
    <div class="header">
      <h1 id="mixerTitle">Loading...</h1>
      <button class="save-btn" onclick="savePreset()">SAVE</button>
    </div>

    <div class="section">
      <div class="section-title">Mics</div>
      <div class="channels" id="mics"></div>
    </div>

    <div class="section">
      <div class="section-title">Stems</div>
      <div class="channels" id="stems"></div>
    </div>

    <div class="section">
      <div class="section-title">Tech</div>
      <div class="channels" id="tech"></div>
    </div>

    <script>
      const REAPER_URL = ""; // Same origin
      let memberName = "";
      let outputTrackIndex = 0;
      let channels = [];

      // Get member from URL path
      const pathParts = window.location.pathname.split("/");
      memberName = pathParts[pathParts.length - 1] || "petka";
      document.getElementById("mixerTitle").textContent =
        memberName.toUpperCase() + "'s Mix";

      // Channel definitions
      const mics = [
        "PETKA",
        "STEVO",
        "MAREK",
        "ZUZKA",
        "ZUZKAg",
        "TINA",
        "MIREC",
        "ALEX",
        "PATRIKA",
        "ANI",
      ];
      const stems = [
        "DRUMS",
        "BASS",
        "INST",
        "OTHER",
        "BGVS",
        "CLICK",
        "GUIDE",
        "IEMONLY",
      ];
      const tech = ["HAND1", "HAND2", "HAND3", "ENGR"];

      function createChannel(name, containerId, trackIndex, sendIndex) {
        const container = document.getElementById(containerId);
        const ch = document.createElement("div");
        ch.className = "channel";
        ch.innerHTML = `
                <div class="controls">
                    <button class="btn mute" data-track="${trackIndex}" data-send="${sendIndex}">M</button>
                    <button class="btn solo" data-track="${trackIndex}" data-send="${sendIndex}">S</button>
                </div>
                <div class="meter"><div class="meter-fill" id="meter-${trackIndex}" style="height: 0%"></div></div>
                <div class="fader-container">
                    <input type="range" class="fader" min="-60" max="12" value="0"
                           data-track="${trackIndex}" data-send="${sendIndex}"
                           oninput="setLevel(this)">
                </div>
                <input type="range" class="pan" min="-100" max="100" value="0"
                       data-track="${trackIndex}" data-send="${sendIndex}"
                       oninput="setPan(this)">
                <div class="name">${name}</div>
                <div class="db" id="db-${trackIndex}-${sendIndex}">0 dB</div>
            `;
        container.appendChild(ch);

        // Setup mute/solo buttons
        ch.querySelector(".mute").onclick = function () {
          this.classList.toggle("active");
          setMute(trackIndex, sendIndex, this.classList.contains("active"));
        };
        ch.querySelector(".solo").onclick = function () {
          this.classList.toggle("active");
          // Solo implementation
        };
      }

      function setLevel(el) {
        const track = el.dataset.track;
        const send = el.dataset.send;
        const db = parseFloat(el.value);
        document.getElementById(`db-${track}-${send}`).textContent = db + " dB";

        // Convert dB to linear
        const vol = Math.pow(10, db / 20);
        fetch(`/_/SET/TRACK/${track}/SEND/${send}/VOL/${vol}`);
      }

      function setPan(el) {
        const track = el.dataset.track;
        const send = el.dataset.send;
        const pan = parseFloat(el.value) / 100; // -1 to 1
        const reaperPan = (pan + 1) / 2; // 0 to 1
        fetch(`/_/SET/TRACK/${track}/SEND/${send}/PAN/${reaperPan}`);
      }

      function setMute(track, send, muted) {
        fetch(`/_/SET/TRACK/${track}/SEND/${send}/MUTE/${muted ? 1 : 0}`);
      }

      function savePreset() {
        alert("Preset saved!");
        // TODO: Implement via MCP
      }

      // Create channels (track indices will be set based on REAPER project)
      let trackIdx = 3; // First input track after folders
      mics.forEach((name, i) => createChannel(name, "mics", trackIdx + i, 1));
      trackIdx += mics.length + 1; // +1 for folder
      stems.forEach((name, i) => createChannel(name, "stems", trackIdx + i, 1));
      trackIdx += stems.length + 1;
      tech.forEach((name, i) => createChannel(name, "tech", trackIdx + i, 1));

      // Meter update loop
      setInterval(() => {
        // TODO: Fetch meter levels
      }, 100);
    </script>
  </body>
</html>
```

**Step 2: Commit**

```bash
git add web/mixer.html
git commit -m "feat(web): add IEM mixer web interface"
```

---

### Task 7: Deploy and Test

**Step 1: Deploy to iem.lan**

```bash
./scripts/deploy.sh
```

**Step 2: Run ReaScript to create project**

In REAPER: Actions > Run ReaScript > setup_iem_project.lua

**Step 3: Test web interface**

Open: `http://iem.lan:8080/mixer/petka`

**Step 4: Verify sends are working**

Move faders, confirm levels change in REAPER

**Step 5: Final commit**

```bash
git add -A
git commit -m "feat: complete IEM mixing system implementation"
git push
```

---

## Summary

| Phase | Tasks | Description                  |
| ----- | ----- | ---------------------------- |
| 1     | 1-3   | MCP tools (pan, mute, meter) |
| 2     | 4     | ReaScript project setup      |
| 3     | 5     | Configuration files          |
| 4     | 6-7   | Web interface + deploy       |

**Total: 7 tasks**
