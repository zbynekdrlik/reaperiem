# REAPER IEM Mixing System Design

**Date:** 2026-02-23
**Status:** Approved

## Overview

Complete REAPER project setup for IEM (In-Ear Monitor) mixing system with web-based self-mix interface for band members.

## Track Structure (39 tracks total)

### Inputs (28 tracks)

**MICS folder (10 tracks):**
| Track | Name | Dante RX |
|-------|------|----------|
| 1 | PETKA mic | 03 |
| 2 | STEVO mic | 04 |
| 3 | MAREK mic | 05 |
| 4 | ZUZKA mic | 06 |
| 5 | ZUZKA gtr | 07 |
| 6 | TINA mic | 08 |
| 7 | MIREC mic | 09 |
| 8 | ALEX mic | 10 |
| 9 | PATRIKA mic | 11 |
| 10 | ANI mic | 12 |

**STEMS folder (14 tracks):**
| Track | Name | Dante RX |
|-------|------|----------|
| 11 | DRUMS L | 21 |
| 12 | DRUMS R | 22 |
| 13 | BASS L | 23 |
| 14 | BASS R | 24 |
| 15 | INST L | 25 |
| 16 | INST R | 26 |
| 17 | OTHER L | 27 |
| 18 | OTHER R | 28 |
| 19 | BGVS L | 29 |
| 20 | BGVS R | 30 |
| 21 | CLICK | 31 |
| 22 | GUIDE | 32 |
| 23 | IEMONLY L | 33 |
| 24 | IEMONLY R | 34 |

**TECH folder (4 tracks):**
| Track | Name | Dante RX |
|-------|------|----------|
| 25 | HAND1 mic | 49 |
| 26 | HAND2 mic | 50 |
| 27 | HAND3 mic | 51 |
| 28 | ENGINEER mic | 52 |

### Outputs (11 tracks)

**BAND folder (9 tracks):**
| Track | Name | Dante TX |
|-------|------|----------|
| 29 | PETKA inear | 03-04 |
| 30 | STEVO inear | 05-06 |
| 31 | MAREK inear | 07-08 |
| 32 | ZUZKA inear | 09-10 |
| 33 | TINA inear | 11-12 |
| 34 | MIREC inear | 13-14 |
| 35 | ALEX inear | 15-16 |
| 36 | PATRIKA inear | 17-18 |
| 37 | ANI inear | 19-20 |

**TECH folder (2 tracks):**
| Track | Name | Dante TX | Notes |
|-------|------|----------|-------|
| 38 | ENGINEER inear | 33-34 | Receives solo bus |
| 39 | TRANSLATOR | 35 | Mono, HAND1 only |

## Routing

### Send Matrix

Every input track (28) has sends to every band output track (9):

- Total sends: 252 (28 × 9)
- Default level: 0dB
- Default pan: Center

### Special Routing

1. **Band outputs**: Direct to hardware, NOT through master (solo doesn't affect them)
2. **ENGINEER**: Receives REAPER's solo bus output (when any track solo'd, engineer hears it)
3. **TRANSLATOR**: Only receives send from HAND1 mic (1 send, mono)

### Hardware Output Mapping

| Output Track   | ASIO Channel | Dante TX |
| -------------- | ------------ | -------- |
| PETKA inear    | 3-4          | 3-4      |
| STEVO inear    | 5-6          | 5-6      |
| MAREK inear    | 7-8          | 7-8      |
| ZUZKA inear    | 9-10         | 9-10     |
| TINA inear     | 11-12        | 11-12    |
| MIREC inear    | 13-14        | 13-14    |
| ALEX inear     | 15-16        | 15-16    |
| PATRIKA inear  | 17-18        | 17-18    |
| ANI inear      | 19-20        | 19-20    |
| ENGINEER inear | 33-34        | 33-34    |
| TRANSLATOR     | 35           | 35       |

## Web Interface

### URL Structure

Each band member accesses their mix at:

```
http://iem.lan:8080/mixer/{name}
```

Examples:

- `http://iem.lan:8080/mixer/petka`
- `http://iem.lan:8080/mixer/marek`

### Controls Per Channel

| Control | Function                      | MCP Tool            |
| ------- | ----------------------------- | ------------------- |
| Meter   | Real-time input level         | `get_track_meter()` |
| Fader   | Send level to member's output | `set_send_level()`  |
| Pan     | Stereo position in mix        | `set_send_pan()`    |
| Mute    | Mute input in mix             | `set_send_mute()`   |
| Solo    | Solo input in mix             | Solo routing        |

### Stereo Linking

Stereo stem pairs (DRUMS L/R, etc.) shown as single fader with linked control.

### Presets

- Save button stores current mix via `save_preset(member, name)`
- Load preset via `load_preset(member, name)`

## New MCP Tools Required

```python
# Pan control for sends
set_send_pan(track_index: int, send_index: int, pan: float)
# pan: -1.0 (left) to 1.0 (right)

# Mute control for sends
set_send_mute(track_index: int, send_index: int, mute: bool)

# Real-time metering
get_track_meter(track_index: int) -> dict
# Returns: {"peak_l": float, "peak_r": float, "rms_l": float, "rms_r": float}
```

## Configuration Updates

### config/input_tracks.yaml (new file)

Full list of input tracks with Dante RX channel mappings.

### config/band_members.yaml

Already correct with current band member definitions.

## Implementation Notes

1. Create project from scratch (delete existing minimal setup)
2. Use REAPER folder tracks for organization
3. All sends default to 0dB, center pan
4. Band outputs bypass master for solo isolation
5. Web interface in `web/` folder, uses existing REAPER HTTP API
