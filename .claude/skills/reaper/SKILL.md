---
name: reaper
description: REAPER HTTP API gotchas, ReaScript lifecycle, autonomous REAPER management, engineer mix routing, ReaEQ parameter layout. Load whenever touching REAPER HTTP API parsing, ReaScripts, or iem.lan REAPER operations.
---

# REAPER IEM — REAPER Operations Skill

## REAPER HTTP API — CRITICAL Gotchas

**ALL REAPER API commands MUST use `/_/` prefix!**

```
CORRECT:   http://iem.lan:8080/_/SET/TRACK/1/SEND/0/VOL/0.5
WRONG:     http://iem.lan:8080/SET/TRACK/1/SEND/0/VOL/0.5  (returns empty!)
```

### Index Rules (VERIFIED EMPIRICALLY)

| Entity     | Index Base  | Example                                       |
| ---------- | ----------- | --------------------------------------------- |
| **Tracks** | **1-based** | Track 1 = first input track, Track 0 = MASTER |
| **Sends**  | **0-based** | Send 0 = first destination, Send 1 = second   |

### TRACK Response Format

`/_/NTRACK;TRACK` returns tab-separated fields:

```
TRACK  idx  name  flags  vol  pan  last_meter_peak  last_meter_pos  width  panmode  sendcnt  recvcnt  hwout  color
  0     1    2     3      4    5          6                7          8       9        10       11      12     13
```

- **Fields [6] and [7] are NOT L/R stereo** — they are two measurements of the SAME combined signal
- Values are dB×10 integers. Convert: `10^(value / 10.0 / 20.0)`. Floor: `-1500` = -150 dB = silence

### SEND Response Format

`/_/GET/TRACK/{t}/SEND/{s}` returns:

```
SEND  track  send  mute_flag  volume  pan  destination_track
  0     1     2       3         4      5          6
```

- **Mute flag**: 0 = unmuted, **8** = muted (bitfield, not boolean!)
- **Pan range**: -1.0 (left) to 1.0 (right), 0.0 = center

### Meter Bridge (true L/R stereo)

For per-channel peaks, use `meter_bridge.lua` via EXTSTATE, NOT the TRACK fields.

- Action: `_RS_REAPERIEM_METER_BRIDGE`
- Writes to `REAPERIEM_METERS/peaks` as `1:L_db10,R_db10;2:...`
- Poller reads EXTSTATE first, falls back to TRACK fields if bridge not running

### Before ANY REAPER HTTP API parsing change

```bash
curl -s "http://iem.lan:8080/_/NTRACK;TRACK"          # see real field layout
curl -s "http://iem.lan:8080/_/GET/TRACK/1/SEND/0"    # see real SEND format
```

NEVER assume field counts, value ranges, or sentinel values — always verify on live REAPER.

---

## Engineer Mix Channel Routing (v1.57.0+)

**NEVER hardcode send_index=0 for mix channels on member inear tracks.**

Member inear tracks (track 23–31) have two different send types:

- **Send 0 = Hardware output** (Dante stereo pair to speakers)
- **Send N = Send to ENGINEER inear** (N discovered dynamically at startup)

**Why:** `poller.rs:discover_members()` probes sends to find which one targets the engineer track. Result stored in `DiscoveredMember.mix_send_index: Option<usize>`. MUTING Send/0 on a member inear track kills their hardware output — production incident 2026-03-15.

**All code touching mix channels must use `mix_send_index`:** `batch_control` MuteAll, `get_mixer_state`, `build_full_state`, `apply_command_to_cache`, poller mix channel polling.

---

## ReaEQ Parameter Layout (VERIFIED on live REAPER)

ReaEQ (Cockos) — 19 total params: 5 bands × 3 (Freq/Gain/BW) + 4 global.

| Param | Name              | Default Norm | Meaning              |
| ----- | ----------------- | ------------ | -------------------- |
| 0     | Freq-Low Shelf    | 0.283        | 287.5 Hz             |
| 1     | Gain-Low Shelf    | 0.250        | 0.0 dB (0.25=center) |
| 2     | BW-Low Shelf      | 0.295        | 1.18 oct             |
| 3     | Freq-Band 2       | 0.290        | 300.0 Hz             |
| 4     | Gain-Band 2       | 0.250        | 0.0 dB               |
| 5     | BW-Band 2         | 0.500        | 2.00 oct             |
| 6     | Freq-Band 3       | 0.476        | 1000.0 Hz            |
| 7     | Gain-Band 3       | 0.250        | 0.0 dB               |
| 8     | BW-Band 3         | 0.500        | 2.00 oct             |
| 9     | Freq-High Shelf 4 | 0.726        | 4624.6 Hz            |
| 10    | Gain-High Shelf 4 | 0.250        | 0.0 dB               |
| 11    | BW-High Shelf 4   | 0.200        | 0.80 oct             |
| 12    | Freq-High Pass 5  | 0.193        | 150.7 Hz             |
| 13    | Gain-High Pass 5  | 0.250        | 0.0 dB               |
| 14    | BW-High Pass 5    | 0.500        | 2.00 oct             |
| 15    | Global Gain       | 1.000        | 0.0 dB               |
| 16    | Bypass            | 0.000        | normal               |
| 17    | Wet               | 1.000        | 100%                 |
| 18    | Delta             | 0.000        | normal               |

**Gain mapping**: 0.25 normalized = 0.0 dB. Use `TrackFX_GetFormattedParamValue()` for accurate dB/Hz conversion.

Band types (Low Shelf, Band, Band, High Shelf, High Pass) are baked into the preset — NOT separate parameters.

---

## Autonomous REAPER Lifecycle

Claude is the sole operator of REAPER on iem.lan. **Do NOT ask the user to start/stop REAPER.**

- If REAPER is unreachable (HTTP 8080 times out, or `Get-Process reaper` returns empty) → start it yourself.
- After starting, verify `/_/NTRACK` returns 200 before reporting up (REAPER takes a few seconds to finish project load).
- **ALWAYS SAVE BEFORE RESTARTING:** `curl "http://iem.lan:8080/_/40026"`

### Start REAPER via SSH (schtasks pattern)

```bash
ssh newlevel@iem.lan "taskkill /IM reaper.exe /F 2>nul"
sleep 2
ssh newlevel@iem.lan "schtasks /create /tn StartREAPER /tr \"\\\"C:\\Program Files\\REAPER (x64)\\reaper.exe\\\"\" /sc once /st 00:00 /ru newlevel /it /f && schtasks /run /tn StartREAPER && schtasks /delete /tn StartREAPER /f"
```

**Why schtasks?** SSH runs in session 0 (service) which can't launch GUI apps. `schtasks /ru newlevel /it` runs in the desktop session. REAPER auto-opens last project without arguments.

### NEVER KILL REAPER carelessly

`taskkill /F /IM reaper.exe` has crashed the Windows machine at a remote location (requires physical power cycle). Always save first (action 40026). If a dialog is blocking HTTP — tell the user rather than killing REAPER.

---

## EXTSTATE Communication Pattern

EXTSTATE is the bridge between HTTP API and ReaScripts:

```
1. Set params:   curl "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/param_key/param_value"
2. Trigger:      curl "http://iem.lan:8080/_/_RS_REAPERIEM_SCRIPT_NAME"
3. Wait:         sleep 2-3 seconds (script needs time to execute)
4. Read result:  curl "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/result_key"
```

Every ReaScript writes results to EXTSTATE. **Never assume a script succeeded — always read the result.**

### Dynamic Script Registration (no restart)

```bash
curl "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/register_scripts/setup_vban.lua|check_vban.lua"
sleep 3
curl "http://iem.lan:8080/_/GET/EXTSTATE/reaperiem/register_result"
# Expected: OK:2
```

Filenames only (not full paths) — meter_bridge constructs path via `reaper.GetResourcePath()`.
