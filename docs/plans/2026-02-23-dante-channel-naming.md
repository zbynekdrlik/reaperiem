# Dante IEM Channel Naming Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Rename TX channels on IEM Yamaha Accelerator to follow zone-based naming with `NAME inear L/R` convention.

**Architecture:** Use netaudio CLI to rename channels on the IEM device. Clear old names first, then apply new names in order. Update config/band_members.yaml to reflect new assignments.

**Tech Stack:** netaudio CLI, YAML config files

---

## Pre-Implementation

### Task 0: Verify Device Availability

**Step 1: Check IEM device is online**

Run: `netaudio device list`
Expected: Shows `Y001-Yamaha-DANTE-ACCEL-0821C4` (or current IEM device name)

**Step 2: Note current device name**

Store the IEM device name for all subsequent commands. If name differs from expected, use the actual name.

---

## Phase 1: Clear Old Channel Names

### Task 1: Clear Channels 3-23 (Old Names)

**Step 1: Clear old band member names (channels 3-23)**

```bash
# Get current IEM device name
IEM_DEVICE="Y001-Yamaha-DANTE-ACCEL-0821C4"

# Clear channels 3-23 to default numbered names
for ch in {3..23}; do
  netaudio config --device-name "$IEM_DEVICE" --set-channel-name $ch "$ch"
done
```

**Step 2: Verify channels cleared**

Run: `netaudio channel list --device-name "Y001-Yamaha-DANTE-ACCEL-0821C4" | head -30`
Expected: Channels 3-23 show just numbers (03, 04, etc.)

---

## Phase 2: Apply New Band Zone Names (Ch 3-20)

### Task 2: Rename Stable Members (Channels 3-16)

**Step 1: Apply PETKA channels 3-4**

```bash
IEM_DEVICE="Y001-Yamaha-DANTE-ACCEL-0821C4"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 3 "PETKA inear L"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 4 "PETKA inear R"
```

**Step 2: Apply STEVO channels 5-6**

```bash
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 5 "STEVO inear L"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 6 "STEVO inear R"
```

**Step 3: Apply MAREK channels 7-8**

```bash
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 7 "MAREK inear L"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 8 "MAREK inear R"
```

**Step 4: Apply ZUZKA channels 9-10**

```bash
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 9 "ZUZKA inear L"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 10 "ZUZKA inear R"
```

**Step 5: Apply TINA channels 11-12**

```bash
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 11 "TINA inear L"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 12 "TINA inear R"
```

**Step 6: Apply MIREC channels 13-14**

```bash
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 13 "MIREC inear L"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 14 "MIREC inear R"
```

**Step 7: Apply ALEX channels 15-16**

```bash
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 15 "ALEX inear L"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 16 "ALEX inear R"
```

**Step 8: Verify stable members**

Run: `netaudio channel list --device-name "Y001-Yamaha-DANTE-ACCEL-0821C4" | head -20`
Expected: Channels 3-16 show PETKA, STEVO, MAREK, ZUZKA, TINA, MIREC, ALEX inear L/R

### Task 3: Rename Irregular Members (Channels 17-20)

**Step 1: Apply PATRIKA channels 17-18**

```bash
IEM_DEVICE="Y001-Yamaha-DANTE-ACCEL-0821C4"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 17 "PATRIKA inear L"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 18 "PATRIKA inear R"
```

**Step 2: Apply ANI channels 19-20**

```bash
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 19 "ANI inear L"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 20 "ANI inear R"
```

**Step 3: Verify irregular members**

Run: `netaudio channel list --device-name "Y001-Yamaha-DANTE-ACCEL-0821C4" | head -25`
Expected: Channels 17-20 show PATRIKA, ANI inear L/R

---

## Phase 3: Apply Tech Zone Names (Ch 33-35)

### Task 4: Rename Tech Channels

**Step 1: Apply ENGINEER channels 33-34**

```bash
IEM_DEVICE="Y001-Yamaha-DANTE-ACCEL-0821C4"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 33 "ENGINEER inear L"
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 34 "ENGINEER inear R"
```

**Step 2: Apply TRANSLATOR channel 35 (mono)**

```bash
netaudio config --device-name "$IEM_DEVICE" --set-channel-name 35 "TRANSLATOR inear"
```

**Step 3: Verify tech zone**

Run: `netaudio channel list --device-name "Y001-Yamaha-DANTE-ACCEL-0821C4" | sed -n '33,38p'`
Expected: Channels 33-35 show ENGINEER inear L/R, TRANSLATOR inear

---

## Phase 4: Update Configuration

### Task 5: Update band_members.yaml

**Files:**

- Modify: `config/band_members.yaml`

**Step 1: Update band_members.yaml with new channel assignments**

```yaml
# Band member configuration for IEM routing
# Each member gets a stereo output pair for their in-ears
# Zone: Channels 3-32 (Band), 33-48 (Tech)

band_members:
  # Stable members (always present)
  - id: 1
    name: "Petka"
    output_track_name: "PETKA inear"
    dante_output_L: 3
    dante_output_R: 4
    stable: true

  - id: 2
    name: "Stevo"
    output_track_name: "STEVO inear"
    dante_output_L: 5
    dante_output_R: 6
    stable: true

  - id: 3
    name: "Marek"
    output_track_name: "MAREK inear"
    dante_output_L: 7
    dante_output_R: 8
    stable: true

  - id: 4
    name: "Zuzka"
    output_track_name: "ZUZKA inear"
    dante_output_L: 9
    dante_output_R: 10
    stable: true

  - id: 5
    name: "Tina"
    output_track_name: "TINA inear"
    dante_output_L: 11
    dante_output_R: 12
    stable: true

  - id: 6
    name: "Mirec"
    output_track_name: "MIREC inear"
    dante_output_L: 13
    dante_output_R: 14
    stable: true

  - id: 7
    name: "Alex"
    output_track_name: "ALEX inear"
    dante_output_L: 15
    dante_output_R: 16
    stable: true

  # Irregular members (sometimes present)
  - id: 8
    name: "Patrika"
    output_track_name: "PATRIKA inear"
    dante_output_L: 17
    dante_output_R: 18
    stable: false

  - id: 9
    name: "Ani"
    output_track_name: "ANI inear"
    dante_output_L: 19
    dante_output_R: 20
    stable: false

# Tech roles (separate zone)
tech_roles:
  - id: 101
    name: "Engineer"
    output_track_name: "ENGINEER inear"
    dante_output_L: 33
    dante_output_R: 34
    mono: false

  - id: 102
    name: "Translator"
    output_track_name: "TRANSLATOR inear"
    dante_output_L: 35
    dante_output_R: null
    mono: true
```

**Step 2: Commit config update**

```bash
git add config/band_members.yaml
git commit -m "config: update band_members.yaml with new channel assignments

Zone-based allocation: Band (3-32), Tech (33-48)
Stable members first, then irregular, then tech roles.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Phase 5: Verification

### Task 6: Full Verification

**Step 1: List all TX channels and verify naming**

Run: `netaudio channel list --device-name "Y001-Yamaha-DANTE-ACCEL-0821C4" | grep -E "^[0-9]+:" | head -40`

Expected output (channels 3-20, 33-35):

```
3:PETKA inear L
4:PETKA inear R
5:STEVO inear L
6:STEVO inear R
7:MAREK inear L
8:MAREK inear R
9:ZUZKA inear L
10:ZUZKA inear R
11:TINA inear L
12:TINA inear R
13:MIREC inear L
14:MIREC inear R
15:ALEX inear L
16:ALEX inear R
17:PATRIKA inear L
18:PATRIKA inear R
19:ANI inear L
20:ANI inear R
...
33:ENGINEER inear L
34:ENGINEER inear R
35:TRANSLATOR inear
```

**Step 2: Verify L channels are on odd numbers**

All L channels should be on odd: 3, 5, 7, 9, 11, 13, 15, 17, 19, 33 (Ableton constraint met)

**Step 3: Final commit**

```bash
git add -A
git commit -m "feat: implement Dante IEM channel naming scheme

- Zone-based allocation (Band 3-32, Tech 33-48)
- NAME inear L/R naming convention
- Stable → Irregular → Tech ordering
- Ableton L=odd constraint compliance

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

## Summary

| Phase | Tasks | Description              |
| ----- | ----- | ------------------------ |
| Pre   | 0     | Verify device online     |
| 1     | 1     | Clear old names (3-23)   |
| 2     | 2-3   | Apply band names (3-20)  |
| 3     | 4     | Apply tech names (33-35) |
| 4     | 5     | Update config YAML       |
| 5     | 6     | Verification             |

**Total: 7 tasks, ~15-20 netaudio commands**
