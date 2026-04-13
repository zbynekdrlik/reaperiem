# Talkback Quality Fix Design (#154)

**Issue:** #154 — "talk sound from phone mic to members inears is very low quality and hanging, not sound fluently"

**Target version:** 1.147.0 → 1.148.0

**Branch:** dev → PR to main

---

## Problem Statement

When the engineer (or any member, since v1.56+) presses the phone-mic Talk button, the audio that reaches the band members' inears has three perceptible defects:

- **Low quality** — artifacts, robotic/compressed voice
- **Hanging** — continuous-silent stretches mid-sentence
- **Not fluent** — choppy/stuttering

All three have been observed simultaneously. No single root cause has been proven — the pipeline has 6 identified fragility points, and we have no runtime measurements to pick the one that fires in production. This PR ships **all six fixes plus diagnostics plus a permanent E2E quality gate** in one go, so we do not have to chase the bug in a second round.

---

## Current Pipeline (v1.147.0, verified live)

```
BROWSER (phone/engineer):
  getUserMedia(48kHz, mono, EC=true, NS=true, AGC=true)
    → AudioContext → ScriptProcessor(1024 samples, ~21.3ms)
    → WebCodecs AudioEncoder (Opus, 64 kbps, mono, 48kHz)
    → WebSocket binary frames → /ws/talkback?token=<JWT>

SERVER (iem-mixer on iem.lan):
  proxy.rs:3049-3076 handle_talkback_ws — PURE RELAY (no buffer)
    → OIEM UDP packet ("OIEM"+seq+size+raw_opus) → VST's heartbeat addr

REAPER (iem.lan):
  OIEM Receive VST3 on ENGINEER mic track (OIEMReceiveProcessor.cpp):
    → UDP thread → lock-free FIFO
    → audio thread: Opus decode (20ms frames, 960 samples @ 48kHz)
    → LagrangeInterpolator resample 48k → host SR (96kHz)
    → dual-mono → mixed (+=) into track buffer (coexists with Dante passthrough)
    → track sends → members' inear buses
```

## Fragility Points (file:line evidence)

1. **No jitter buffer anywhere** — `proxy.rs:3049-3076` relays WS→UDP with zero buffering. Any transient WS gap reaches the VST directly; VST's accumulation buffer silently passes through Dante-only audio when empty (`OIEMReceiveProcessor.cpp:108`) → **perceived "hanging"**.
2. **Chunk-size mismatch** — ScriptProcessor emits 1024-sample (21.3ms) chunks; Opus native frame is 960 samples (20ms). Encoder re-frames on every chunk, producing variable encoder latency and fragmented frames → **contributes to choppy perception**.
3. **No backpressure in browser** — `talkback.js:60-66` sends raw Opus without checking `ws.readyState`. If the WS is half-open (PWA backgrounded, cellular handoff), the browser keeps encoding into a dead pipe; nothing reaches REAPER; the mic LED says "transmitting" → **perceived "hanging"**.
4. **No sequence tracking on talkback UDP path** — the listen direction has seq/gap detection at `audio_stream.rs:179-191`; the talkback UDP write in `proxy.rs:3066` has no seq, no gap counter, no diagnostics. **We cannot measure packet loss.**
5. **Opus bitrate 64 kbps** — on the low end for 48 kHz mono voice. Makes packet loss more audible (fewer redundancy bits per frame) and exacerbates #1–#3.
6. **Zero audio-quality E2E coverage** — `e2e/tests/live/talkback.spec.ts` checks button visibility and zero console errors only. It does not verify any audio reaches REAPER. **The bug is invisible to CI today.**

---

## Architecture (after fix)

```
BROWSER                  SERVER                      REAPER (VST — unchanged)
┌──────────────┐        ┌──────────────────────┐    ┌──────────────────┐
│getUserMedia  │        │/ws/talkback          │    │OIEM Receive VST  │
│48k mono      │ ─WS──▶ │  ├─ frame validator  │    │  ├─ UDP thread   │
│AudioWorklet  │  Opus  │  ├─ jitter buffer    │    │  ├─ FIFO         │
│(960-sample)  │ 96kbps │  │    (60ms, drop-   │ ─▶ │  ├─ Opus decode  │
│Opus 96kbps   │        │  │     oldest on     │    │  ├─ Lagrange 48→ │
│readyState-   │        │  │     overflow)     │    │  │   host SR     │
│aware send    │        │  ├─ seq tracker      │    │  └─ mix into buf │
│              │        │  ├─ metrics recorder │    └──────────────────┘
└──────────────┘        │  └─ UDP sender       │
                        └──────────────────────┘
                                  │
                                  ▼
                        /api/talkback/diagnostics  (GET, engineer-only)
                        {recv_vst_addr, pkts_in,
                         seq_gaps, buf_fill_ms,
                         last_pkt_age_ms, underruns,
                         bitrate_kbps}
```

**Out of scope:** VST changes (`OIEMReceiveProcessor.cpp`). JUCE VST3 rebuild adds ~20 minutes to every CI run, and the server jitter buffer is expected to eliminate the VST's accumulator underrun condition. If post-deploy diagnostics reveal the VST is still the bottleneck, that is a follow-up issue.

---

## Components

### 1. Browser — `iem-mixer/iem-ui/talkback.js`

**Change 1a: AudioWorklet replaces ScriptProcessor.**

Move from `ScriptProcessor(1024, 1, 1)` to an `AudioWorkletProcessor` that accumulates exactly 960 input samples (20 ms @ 48 kHz) before posting a frame to the main thread. This gives the Opus encoder clean 20 ms frame boundaries with no re-framing.

Worklet runs on the Web Audio render thread (not the main thread), so it is not starved by DOM/GC pauses. This is the modern replacement for ScriptProcessor, which is deprecated.

**Change 1b: Bitrate 64 kbps → 96 kbps.**

`AudioEncoder` config: `bitrate: 96000`. 96 kbps mono Opus is the sweet spot for transparent voice (audibly identical to PCM at normal listening levels). Bandwidth impact: +4 kB/s during active talk — negligible even on 3G.

**Change 1c: readyState-aware send with drop-on-closed.**

Before every `ws.send(frame)`:

```js
if (ws.readyState !== WebSocket.OPEN) {
  _dropped_frames++;
  return; // drop silently, don't queue
}
ws.send(frame);
```

No queueing. If the socket is CLOSING/CLOSED, the mic is logically "off" until reopened. This prevents silent-hang backlog.

---

### 2. Server — `iem-mixer/crates/iem-server/src/proxy.rs` (handle_talkback_ws)

**Change 2a: Jitter buffer, 60 ms ring, drop-oldest on overflow.**

New struct `JitterBuffer` (in a new module `talkback_buffer.rs` to keep proxy.rs focused). Interface:

```rust
struct JitterBuffer {
    buf: VecDeque<(u16, Vec<u8>)>, // (seq, opus_frame)
    target_ms: u32,                 // 60 ms
    frame_ms: u32,                  // 20 ms (Opus)
    next_seq: u16,
}

impl JitterBuffer {
    fn push(&mut self, frame: Vec<u8>);     // drop-oldest on overflow
    fn pop(&mut self) -> Option<Vec<u8>>;   // called on 20 ms interval
    fn fill_ms(&self) -> u32;
    fn depth_frames(&self) -> usize;
}
```

Drain loop: a tokio task pulls one frame every 20 ms from the buffer, wraps in OIEM header with monotonically incremented sequence, sends UDP. If buffer is empty, emits a zero-payload OIEM keepalive (VST ignores zero-payload packets — already safe).

Worst case added latency: 60 ms. Typical: 20–40 ms.

**Change 2b: Sequence number in OIEM header.**

The UDP packet format already has a 2-byte seq field. The pure-relay code at `proxy.rs:3066` currently does not populate it meaningfully. The jitter buffer's drain loop now assigns monotonic sequences per talkback session.

**Change 2c: Per-connection metrics.**

`TalkbackMetrics` struct (atomic counters):

```rust
pub struct TalkbackMetrics {
    pub recv_vst_addr: ArcSwap<Option<SocketAddr>>,
    pub packets_in: AtomicU64,
    pub packets_out: AtomicU64,
    pub seq_gaps: AtomicU64,             // gaps in INCOMING WS sequence if we track it later
    pub buffer_fill_ms: AtomicU32,       // current fill
    pub buffer_overflows: AtomicU64,     // drop-oldest events
    pub last_packet_age_ms: AtomicU64,   // ms since last frame received on WS
    pub bitrate_kbps: AtomicU32,         // negotiated from client handshake
    pub underruns: AtomicU64,            // drain loop found empty buffer
}
```

Owned by `AppState`. Single global — talkback is single-talker-at-a-time.

---

### 3. Diagnostics API — `iem-mixer/crates/iem-server/src/routes.rs`

The stub at `routes.rs:468` (`talkback_diagnostics_handler`) becomes a real handler. Engineer-only (middleware checks role).

```
GET /api/talkback/diagnostics
Response 200:
{
  "recv_vst_addr": "127.0.0.1:54321",
  "packets_in": 1247,
  "packets_out": 1247,
  "seq_gaps": 0,
  "buffer_fill_ms": 42,
  "buffer_overflows": 0,
  "last_packet_age_ms": 18,
  "underruns": 2,
  "bitrate_kbps": 96
}

Response 403: non-engineer
```

No WebSocket push — engineer can poll from devtools or we add a small stats panel in a follow-up.

---

### 4. E2E Test — `iem-mixer/e2e/tests/live/talkback-quality.spec.ts` (NEW)

Playwright config override for this spec only:

```ts
launchOptions: {
  args: [
    '--use-fake-ui-for-media-stream',
    '--use-fake-device-for-media-stream',
    '--use-file-for-fake-audio-capture=tests/fixtures/talkback-1k-tone.wav',
  ],
}
```

Fixture `tests/fixtures/talkback-1k-tone.wav` — 5 s of 1 kHz sine, -12 dBFS, mono 48 kHz. Committed to repo (~480 kB).

**Test flow:**

1. Log in as engineer (PIN 1177), navigate to own mixer page.
2. Hold Talk button (`pointerdown` on `.talk-button`) for 5000 ms.
3. While held: poll `http://iem.lan:8080/_/NTRACK;TRACK` every 100 ms (50 samples), parse ENGINEER mic track row, extract field 6 (last_meter_peak, dB×10). Record 50 meter samples.
4. After release: continue polling for 500 ms, assert meter drops to ≤ −60 dB (-600) within 200 ms of release.
5. Fetch `/api/talkback/diagnostics`, assert `packets_in > 200`, `packets_out > 200`, `seq_gaps == 0`, `recv_vst_addr` is present.

**Assertions:**

- **Signal present (A1):** ≥ 40 of 50 meter samples above −60 dB (80% coverage during 5 s of active tone)
- **No hangs (A2):** No consecutive 500 ms window (5 samples) during talk where all samples ≤ −60 dB
- **Clean release (A3):** Meter drops to ≤ −60 dB within 200 ms after `pointerup`
- **Diagnostics sane (A4):** `packets_in ≥ 200`, `packets_out ≥ 200`, `seq_gaps == 0`, `buffer_overflows == 0` (on loopback; non-zero would indicate the server drain loop is not keeping up)
- **Console clean (A5):** No errors, no warnings (standard airuleset requirement)

---

## Phase-1 RED Gate (mandatory)

Before any fix code ships, `talkback-quality.spec.ts` must **FAIL against v1.147.0 production**, with output captured to `/tmp/talkback154-phase1-red.txt`. Expected failures on v1.147.0:

- A4 fails: `/api/talkback/diagnostics` returns 404 or stub (endpoint doesn't exist) — or the response is missing fields.
- A2 might or might not fail depending on live network quality — on clean LAN, the meter may show signal throughout the 5 s.

A4 is the deterministic RED: the new diagnostics field set does not exist on v1.147.0, so the assertion fails with "property packets_in not found". That is the proof the test is wired to the real system.

If A2 also fails on v1.147.0 (meter shows a 500 ms gap under the fake-audio loopback with zero network noise), that is the smoking-gun evidence of the hang we are fixing.

RED output is captured and referenced in the PR description per airuleset.

---

## Data Flow — Frame Lifetime

```
t=0ms     Browser Worklet pushes 960-sample PCM frame
t=~3ms    AudioEncoder emits Opus frame, ws.send() (if OPEN)
t=~4ms    Server /ws/talkback receives binary WS frame
t=~5ms    talkback_buffer.push(opus) → buffer fills
t=20,40,...   drain loop pops one frame every 20ms
          → assigns seq, wraps in OIEM header, sendto(UDP)
t=~25ms   VST UDP thread receives, pushes to FIFO
t=~26ms   VST audio thread pops, Opus-decodes, resamples, mixes
t=~46ms   Sound reaches REAPER's track output → inear sends → member phone
```

Total one-way latency: ~45 ms + 60 ms worst-case jitter buffer = ~105 ms. Imperceptible for push-to-talk (threshold ~200 ms).

---

## Error Handling

| Condition | Behavior |
|---|---|
| Browser WS closes mid-talk | Drop frames silently, attempt reconnect via existing WS logic |
| Server jitter buffer overflow | Drop-oldest, increment `buffer_overflows` counter, no error propagated |
| Server drain loop runs with empty buffer | Emit zero-payload keepalive (ignored by VST), increment `underruns` |
| VST not registered (no heartbeat yet) | Server skips UDP send; `recv_vst_addr` stays None; diagnostics shows null |
| Opus encoder error (browser) | Existing error-reporting path logs to server, talkback session ends |
| Non-engineer hits `/api/talkback/diagnostics` | 403 Forbidden |

---

## Testing Strategy

### E2E (primary)
- `talkback-quality.spec.ts` — described above. Runs in the post-deploy `live/` suite on the self-hosted runner with real REAPER.

### Unit (secondary)
- `talkback_buffer.rs` — unit tests for push/pop/drop-oldest/fill_ms/overflow counter. Landed in CI in the fix commit (no local Rust test execution per airuleset).
- `routes.rs` — diagnostics handler unit test (mock AppState, verify JSON shape and 403 for non-engineer).

### Existing talkback.spec.ts
- Unchanged. Still verifies button visibility.

### Mutation testing
- Handled automatically by `cargo-mutants --in-diff` in CI. New `talkback_buffer.rs` logic must pass mutation gates.

---

## Version Bump (first commit, per airuleset)

1.147.0 → 1.148.0 across:

- `iem-mixer/crates/iem-core/Cargo.toml`
- `iem-mixer/Cargo.toml`
- `iem-mixer/crates/iem-server/Cargo.toml`
- `iem-mixer/iem-ui/Cargo.toml`
- `iem-mixer/src-tauri/Cargo.toml`
- `iem-mixer/src-tauri/tauri.conf.json`

---

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| AudioWorklet not supported on old iOS Safari | WebCodecs support already restricts to modern browsers — Worklet has been supported on iOS 14.5+ (2021). Existing PWA already requires iOS ≥ 16 for other reasons. |
| 60 ms jitter buffer feels laggy | Worst case 105 ms one-way, well under 200 ms push-to-talk threshold. If user complains post-deploy, configurable via `config.yaml` (out of scope for this PR — hardcoded 60 ms). |
| Fake-audio WAV fixture too large to commit | 5 s × 48 kHz × 2 bytes = 480 kB. Acceptable. Alternative: generate on test startup from a tone generator (~30 lines of code) — chosen only if repo size becomes an issue. |
| Drain loop cannot keep up (high CPU) | tokio task at 20 ms interval on an async runtime — trivially fast on the iem.lan hardware (Ryzen, 32 GB). `underruns` counter will show it if wrong. |
| VST was the real bottleneck all along | Diagnostics will tell us — if `buffer_overflows == 0`, `underruns == 0`, but live testing still reports hangs post-deploy, follow-up issue will target the VST. |
| Browser breaks AudioWorklet in a future version | The AudioWorklet API has been stable since 2018. Risk is low; unit tests of the worklet logic itself are impractical, but the E2E covers the integration. |

---

## Out of Scope

- VST rebuild (`OIEMReceiveProcessor.cpp` changes)
- WebRTC as a WebSocket replacement (full redesign, separate issue)
- Concurrent-talker lock / queuing (current behavior: last-pressed wins; separate concern)
- Persistent config for jitter buffer size (hardcoded 60 ms for v1.148.0)
- Diagnostics panel in the UI (poll-only API this round; panel in follow-up if useful)

---

## Success Criteria

- [ ] Version 1.148.0 lands on `main` after green CI (all 10 jobs including deploy + post-deploy E2E)
- [ ] `talkback-quality.spec.ts` passes on the post-deploy runner
- [ ] `/api/talkback/diagnostics` returns the new schema on production
- [ ] Engineer confirms talkback audio sounds fluent in a live test (subjective sign-off)
- [ ] README changelog entry for v1.148.0

---

## File Map (new and modified)

**New:**

- `iem-mixer/crates/iem-server/src/talkback_buffer.rs` — jitter buffer + unit tests
- `iem-mixer/iem-ui/talkback-worklet.js` — AudioWorklet processor (loads into AudioContext)
- `iem-mixer/e2e/tests/live/talkback-quality.spec.ts` — quality gate
- `iem-mixer/e2e/tests/fixtures/talkback-1k-tone.wav` — 5 s 1 kHz -12 dBFS fake-audio fixture

**Modified:**

- `iem-mixer/iem-ui/talkback.js` — Worklet wiring, bitrate 96k, readyState gate
- `iem-mixer/crates/iem-server/src/proxy.rs` — handle_talkback_ws uses JitterBuffer + TalkbackMetrics
- `iem-mixer/crates/iem-server/src/routes.rs:468` — diagnostics handler implementation
- `iem-mixer/crates/iem-server/src/lib.rs` — register TalkbackMetrics in AppState
- 5 × `Cargo.toml` + `tauri.conf.json` — version bump
- `README.md` — v1.148.0 changelog entry
