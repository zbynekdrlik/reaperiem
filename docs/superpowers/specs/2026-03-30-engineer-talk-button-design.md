# Engineer Talk Button — Design Spec

**Issue:** #123 — Engineer wants a talk button to communicate with band members from phone/headset
**Date:** 2026-03-30

## Overview

Push-to-talk button that captures the engineer's phone/headset microphone, streams audio through the server to REAPER, where it arrives on a dedicated TALKBACK track. Band members hear the engineer through their existing IEM sends and control the talkback volume independently.

## Architecture

### End-to-End Audio Flow

```
Browser (engineer phone/laptop)
  getUserMedia → AudioWorklet (PCM→f32) → Opus Encoder (WebCodecs)
  → binary Opus frames via WebSocket /ws/talkback
      ↓
Server (iem-mixer on iem.lan)
  Receives Opus frames on /ws/talkback (engineer-only auth)
  Pure relay — no decode, no re-encode, no audio processing
  Sends as OIEM UDP packets to receive VST's registered address
      ↓
REAPER
  OIEM Receive VST3 on TALKBACK track
  Opus decode → resample 48kHz→96kHz → track audio output
  Track sends → each member's inear (volume per member)
```

### Port Strategy

**Single port 7980** for all OIEM traffic (replaces VBAN-range port 6980):

- Server binds `127.0.0.1:7980`
- Sender VST (existing, listen mode) sends TO 7980 from ephemeral port — server receives REAPER audio
- Receive VST (new, talkback) sends heartbeat packets TO 7980 — server learns its return address and sends talkback frames back
- No conflict with VB-Matrix or other VBAN software (standard VBAN range is 6980-6999)

**Migration:** Existing OIEM sender VST changes port constant from 6980 → 7980. Server `audio_stream.rs` bind address changes to 7980.

### Strict Separation from Listen Mode

The Talk feature is completely independent from Listen:

| Aspect | Listen (existing, untouched) | Talk (new) |
|--------|------------------------------|------------|
| WebSocket | `/ws/audio` | `/ws/talkback` |
| UDP direction | server recv() | server sendto() |
| State struct | `ListenTarget` | `TalkbackState` |
| Frame counters | existing diagnostics | separate (or none) |
| Locks | `engineer_listen_target` | `talkback_state` (separate) |
| Audio processing | frame dropper, rate tracker | pure relay |

**Listen mode code (`proxy.rs` audio handler, `audio_stream.rs`) is NOT modified.** Talk is purely additive — new WebSocket endpoint, new state, new UDP send path.

## OIEM Receive VST3

Mirror of the existing OIEM Send VST3, inverted:

### Input Side (UDP)
- Binds ephemeral local port
- Sends periodic heartbeat (every 5s) to `127.0.0.1:7980`: 8-byte packet with OIEM magic `"OIEM"` + `0x00 0x01` (stream_id=1 = heartbeat) + `0x00 0x00` (zero payload). Server distinguishes heartbeat from audio by stream_id byte at offset 4
- Receives OIEM packets from server on same socket

### Processing Pipeline
```
OIEM packet → parse header → extract Opus frame
  → Opus Decoder (48kHz stereo)
  → LagrangeInterpolator resample (48kHz → REAPER sample rate, e.g. 96kHz)
  → deinterleave L/R → VST3 output buffers
```

### Thread Architecture (mirrors sender)
- **UDP receive thread** (consumer): receives OIEM packets, parses, pushes Opus frames to FIFO
- **Audio thread** (producer of PCM): pops Opus frames from FIFO, decodes, resamples, outputs
- **Lock-free SPSC FIFO** between threads (same `AbstractFifo` pattern as sender)

### Build
- Same CMake/JUCE/libopus toolchain as sender VST
- Added to existing CI build pipeline
- Deployed to `C:\Program Files\Common Files\VST3\OIEM Receive.vst3`
- Inserted on TALKBACK track via ReaScript (dynamic registration)

## REAPER Track Setup

**New track: TALKBACK** (separate from ENGINEER mic which has a real Dante input):

- No hardware input (audio comes from OIEM Receive VST3)
- Sends to each member's inear track (same pattern as ENGINEER mic sends)
- Each member controls talkback volume via their send fader
- Created via ReaScript `setup_talkback.lua`

## One-at-a-Time Lock

Only one engineer can talk at a time. Server maintains exclusive lock:

```rust
struct TalkbackState {
    active_talker: Option<String>,       // member_id holding the lock
    recv_vst_addr: Option<SocketAddr>,   // receive VST's UDP address (from heartbeat)
}
```

- `TalkStart` → if `active_talker` is `None`, grant lock → `TalkAcquired`
- `TalkStart` → if someone else holds lock → `TalkBusy { who }`
- `TalkStop` or WS disconnect → release lock → broadcast `TalkReleased`
- Orphan safety: auto-release on disconnect (same pattern as listen mute restore)

## WebSocket Protocol

### New ClientMsg variants

```rust
TalkStart    // Engineer pressed talk button
TalkStop     // Engineer released talk button
```

### New ServerMsg variants

```rust
TalkAcquired                  // Lock granted — start sending audio
TalkBusy { who: String }      // Another engineer is talking
TalkReleased                  // Lock released — button returns to idle
```

### Talkback audio WebSocket

**Endpoint:** `/ws/talkback?token=<JWT>`

- Engineer-only auth (same as `/ws/audio`)
- Binary frames: Opus-encoded mic audio from browser
- Server relays to OIEM Receive VST via UDP
- Text frames: status messages (future use)

### Flow

1. Engineer presses Talk → `TalkStart` on mixer WS (`/ws/{member_id}`)
2. Server grants lock → `TalkAcquired`
3. Browser requests mic permission (`getUserMedia`), opens `/ws/talkback`
4. Browser encodes mic audio (Opus via WebCodecs), sends binary frames
5. Server wraps in OIEM header, sends UDP to receive VST
6. Engineer releases → `TalkStop` → browser stops mic, closes `/ws/talkback`
7. Server releases lock → broadcasts `TalkReleased`

## UI Design

### Toolbar Layout (engineer only)

```
┌──────┬─────────────┬─────────────┐
│ 🔇   │  ▶ Listen   │  🎙 Talk    │
│ All  │             │             │
└──────┴─────────────┴─────────────┘
```

- **Mute All**: compact icon-only button (48px wide)
- **Listen**: existing button, unchanged
- **Talk**: new push-to-talk button, same flex size as Listen

### Button States

| State | Appearance | Trigger |
|-------|-----------|---------|
| Idle | Blue border, "🎙 Talk" | Default |
| Live | Red background, "🔴 LIVE", pulsing glow | Button held down |
| In Use | Grey, disabled, "🎙 In Use" | Another engineer talking |

### Interaction

- **Desktop:** `mousedown` starts, `mouseup` stops
- **Mobile:** `touchstart` starts, `touchend` stops
- **Both:** `pointerdown`/`pointerup` for unified handling
- **Safety:** if pointer leaves button area or page loses focus → auto-stop

### Mic Permission

- Browser requests `getUserMedia({ audio: true })` on first `TalkAcquired`
- Permission persists for the session
- If denied → show error state on button, "Mic blocked"

## Component Structure

### New Files

| File | Purpose |
|------|---------|
| `iem-ui/src/components/talk_button.rs` | Push-to-talk UI component |
| `iem-server/src/talkback.rs` | Talkback state + UDP sender |
| `vban-vst/src-receive/` | OIEM Receive VST3 source |
| `scripts/reascripts/setup_talkback.lua` | Create TALKBACK track + sends |

### Modified Files

| File | Change |
|------|--------|
| `iem-core/src/ws.rs` | Add TalkStart/TalkStop/TalkAcquired/TalkBusy/TalkReleased |
| `iem-server/src/proxy.rs` | Handle Talk messages, new `/ws/talkback` endpoint |
| `iem-server/src/lib.rs` | Add `TalkbackState` to `AppState` |
| `iem-server/src/audio_stream.rs` | Change bind port 6980 → 7980 (ONLY change) |
| `iem-ui/src/components/toolbar.rs` | Add Talk button, shrink Mute All |
| `iem-ui/style.css` | Talk button styles |
| `vban-vst/src/PluginProcessor.h` | Change sender port 6980 → 7980 |
| `e2e/tests/` | Talk button E2E tests |

## Testing

### Unit Tests
- `ws.rs`: Serialization roundtrip for new message variants
- `talkback.rs`: Lock acquire/release/busy logic, orphan cleanup on disconnect
- OIEM Receive VST: Opus decode + resample correctness

### E2E Tests (Playwright)
- Engineer sees Talk button, band member does not
- Push-to-talk: hold → LIVE state, release → idle
- Lock exclusivity: second engineer sees "In Use"
- Lock release on disconnect (navigate away)
- Mic permission denied → error state
- Console zero errors/warnings

### Integration Tests (against real system)
- Talkback audio arrives on TALKBACK track in REAPER
- Band member can adjust talkback volume via send fader
- Listen and Talk work simultaneously without interference
