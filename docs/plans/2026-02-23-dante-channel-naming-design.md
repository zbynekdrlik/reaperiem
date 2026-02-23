# Dante IEM Channel Naming Design

**Date:** 2026-02-23
**Status:** Approved
**Device:** Y001-Yamaha-DANTE-ACCEL-0821C4 (IEM Accelerator)

## Problem

Current channel naming is inconsistent (Slovak "sluchatka" mixed with numbers), unordered (stable and irregular members mixed), and doesn't follow Ableton's L=odd channel constraint.

## Design Goals

1. Consistent naming: `NAME inear L/R` format
2. Logical ordering: stable members → irregular → tech roles
3. Ableton compatibility: L channel always on odd number
4. Future-proof: zone-based allocation with room to grow
5. English names for tech roles (ENGINEER, TRANSLATOR)

## Zone Structure

| Zone     | Channels | Purpose                    | Capacity        |
| -------- | -------- | -------------------------- | --------------- |
| Reserved | 1-2      | System/spare               | -               |
| Band     | 3-32     | Band members               | 15 stereo pairs |
| Tech     | 33-48    | Engineer, Translator, etc. | 8 stereo pairs  |
| Spare    | 49-128   | Future expansion           | 80 channels     |

## Channel Assignments

### Band Zone (Channels 3-32)

**Stable members (always present):**

| Channel | Name          |
| ------- | ------------- |
| 3       | PETKA inear L |
| 4       | PETKA inear R |
| 5       | STEVO inear L |
| 6       | STEVO inear R |
| 7       | MAREK inear L |
| 8       | MAREK inear R |
| 9       | ZUZKA inear L |
| 10      | ZUZKA inear R |
| 11      | TINA inear L  |
| 12      | TINA inear R  |
| 13      | MIREC inear L |
| 14      | MIREC inear R |
| 15      | ALEX inear L  |
| 16      | ALEX inear R  |

**Irregular members (sometimes present):**

| Channel | Name            |
| ------- | --------------- |
| 17      | PATRIKA inear L |
| 18      | PATRIKA inear R |
| 19      | ANI inear L     |
| 20      | ANI inear R     |

**Channels 21-32:** Reserved for future band members

### Tech Zone (Channels 33-48)

| Channel | Name             | Notes                                  |
| ------- | ---------------- | -------------------------------------- |
| 33      | ENGINEER inear L | FOH monitoring                         |
| 34      | ENGINEER inear R | FOH monitoring                         |
| 35      | TRANSLATOR inear | Mono (sufficient for translation feed) |
| 36-48   | [Spare]          | Future tech roles                      |

## Naming Convention

- **Format:** `NAME inear L` / `NAME inear R`
- **Mono format:** `NAME inear` (no L/R suffix)
- **Case:** First word UPPERCASE, second lowercase
- **Language:** English for tech roles, names for band members
- **Constraint:** L channel always on odd number (Ableton requirement)

## Channels to Remove

The following current channels will be cleared (reset to default numbered names):

- Mato sluchatka L/R (channels 21-22)
- Host Iem Repro (channel 23)
- IEM to Sluchatka Zvukar L/R (channels 15-16) - moved to Tech zone as ENGINEER
- IEM Sluchatka Prekladac (channel 19) - moved to Tech zone as TRANSLATOR

## Implementation Notes

1. Use `netaudio config --device-name <IEM_DEVICE> --set-channel-name <ch> <name>`
2. Clear old names first, then apply new names
3. Update `config/band_members.yaml` to reflect new channel assignments
4. Dante channel names have 31-character limit (all names fit)

## Verification

After implementation:

1. `netaudio channel list --device-name <IEM_DEVICE>` shows new names
2. Dante Controller displays correct names
3. REAPER/Ableton sees correct channel labels
