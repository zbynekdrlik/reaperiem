# Limiter Activation Counter — Design

**Issue:** [#145](https://github.com/zbynekdrlik/reaperiem/issues/145) — "limiter activation log"

**Goal:** Surface, inside the existing per-track Limiter modal, how long that track's output limiter has been actively reducing gain since the last reset. Both engineer and the band member themselves can see and reset their own counter.

## Problem

Engineer needs to know which band members' inears have had the safety limiter pushing the signal down, so they can either lower the engineer-side fader or coach the member to turn down. Today there is no observation of limiter gain reduction anywhere in the app — the limiter does its job silently.

## Scope (explicit YAGNI list)

In scope:
- One extra line + one button inside the existing `LimiterModal`
- Per-inear-track cumulative active milliseconds (reduction-time counter)
- Per-track Reset button
- Counter is visible to anyone who can open that track's modal: the member themselves (for their own track) and the engineer (for any track)

Out of scope (do not build):
- Peak GR readout
- Live "currently active" indicator on channel strips or in the modal
- Cross-member list / leaderboard / engineer-toolbar view
- Per-event timeline or timestamps
- Persistence across app restarts
- Notifications, alerts, automatic actions

## Architecture

```
REAPER (iem.lan)
  └─ Each inear track: JS:loser/MGA_JSLimiterST (modified)
       └─ slider5 = ext_gr_meter   (added; read-only readout, dB)
       
  └─ scripts/reascripts/meter_bridge.lua (existing defer loop, ~tick rate)
       └─ For each inear track with the limiter:
            read slider5 via TrackFX_GetParam
            if value < -1.0 dB → activity_ms[track] += dt
       └─ Write all totals to EXTSTATE: REAPERIEM_LIMITER_ACTIVITY/totals
            Format: "23:1230;24:0;25:8470;..."   (track_index:active_ms)

iem-mixer-app (iem.lan)
  └─ poller.rs (existing 150 ms loop)
       └─ Read REAPERIEM_LIMITER_ACTIVITY/totals each cycle
       └─ Store in AppState.limiter_activity: HashMap<usize, u64>
            (key = inear track index, value = active_ms cumulative)
  └─ proxy.rs WebSocket handler (existing per-member WS)
       └─ ServerMsg::LimiterParams extended: + active_seconds: f64
            Sent in response to GetLimiterParams (when modal opens)
       └─ ClientMsg::ResetLimiterActivity { track_index }
            Server zeros the HashMap entry for that one track
            ALSO writes EXTSTATE REAPERIEM_LIMITER_ACTIVITY/reset = "<track_index>"
            so the ReaScript counter zeroes too (otherwise next poller cycle
            would re-overwrite the server's zero)

iem-ui (Leptos WASM)
  └─ components/limiter_modal.rs
       └─ Two new elements above the close button:
            "21.3 sec limited"  (or "not limited yet" when zero,
                                    or "1 min 23 sec limited" at ≥60 s)
            [Reset] button → sends ClientMsg::ResetLimiterActivity
       └─ active_seconds signal updated when LimiterParams arrives
```

## Detection mechanism

`MGA_JSLimiterST` already computes a continuous gain-reduction value internally
(`gr_meter`) and writes it to REAPER's `ext_gr_meter` extension for the FX UI's
own meter display. We add one read-only slider that mirrors the same value, so
ReaScript can poll it via `TrackFX_GetParam`:

```jsfx
// added to MGA_JSLimiterST after slider4
slider5:0<-30,0,0.1>GR (dB read-only)

// added to @block (after the existing ext_gr_meter assignment)
slider5 = ext_gr_meter;
sliderchange(slider5);
```

`sliderchange()` is the JSFX call that pushes the new slider value into REAPER's
parameter automation system, making it readable from `TrackFX_GetParam(track, fx, 4)`
(parameter index 4 = slider5, zero-based).

**Activation threshold:** GR less than −1.0 dB counts as "limiter active". One dB
is the conventional threshold for audible gain reduction; sub-1 dB reduction is
transparent and shouldn't inflate the counter.

**Tick rate:** meter_bridge already runs once per `defer()` cycle (~30 ms when REAPER
audio is running). At each tick we add the elapsed wall time since the previous
tick to `active_ms[track]` for every track currently above threshold. We measure
elapsed time with `reaper.time_precise()` so a brief defer hiccup doesn't get
counted as activation time.

## Storage model

In-memory only — `Arc<Mutex<HashMap<usize, u64>>>` on `AppState`, keyed by inear
track index, value in milliseconds. Resets on:
- App restart (HashMap is empty at startup)
- Engineer or member explicitly clicks the per-track Reset button

No persistence across app restarts. No auto-reset by time. The counter answers
"since this app started, or since I last hit Reset, how long has the limiter
been doing work on this track?"

## Reset semantics

`ClientMsg::ResetLimiterActivity { track_index }` does two things atomically:

1. Server zeros `AppState.limiter_activity[track_index]`
2. Server writes `EXTSTATE REAPERIEM_LIMITER_ACTIVITY/reset = "<track_index>"`

meter_bridge.lua reads the reset key each tick. When set, it zeros the matching
local accumulator and clears the EXTSTATE key. Without this round-trip, the next
poller cycle would re-overwrite the server's zero with the ReaScript's still-large
total.

## Authorization

The existing `LimiterModal` access rules already govern who can open the dialog:
- Engineer can open any member's limiter modal (track ownership check `owns_limiter_track` in proxy.rs returns true for engineer)
- A band member can open only their own track's modal (#156)

The counter and Reset button inherit those rules unchanged. We do not add a
separate engineer-only gate — if you're allowed to see the limiter modal for
that track, you're allowed to see and reset its counter.

## Wire format

Extending the existing `ServerMsg::LimiterParams` variant in `iem-core/src/ws.rs`:

```rust
LimiterParams {
    track_index: usize,
    limit_db: f32,
    limit_norm: f32,
    enabled: bool,
    active_seconds: f64,   // NEW — cumulative active seconds since reset
}
```

New `ClientMsg` variant in `iem-core/src/ws.rs`:

```rust
ResetLimiterActivity { track_index: usize },
```

## UI

Inside `LimiterModal` between the existing toggle row and the close area, one new
row with two elements:

```
┌─────────────────────────────────────────┐
│ PETRONELA — Limiter                  ✕  │
├─────────────────────────────────────────┤
│  ▮ MAX LEVEL  ──●─────────  -6.0 dB     │
│                                         │
│  Limiter   [ ON ]                       │
│                                         │
│  23.4 sec limited      [ Reset ]        │  ← NEW row
└─────────────────────────────────────────┘
```

Format rules:
- 0 s → "not limited yet"
- < 60 s → "X.X sec limited" (one decimal for resolution at short durations)
- ≥ 60 s → "M min S sec limited" (no decimal; no hours expected in a session)

The phrasing deliberately avoids an opaque "Active: M:SS" display — a user
opening the modal for the first time must understand the number without
external context.

The counter does not auto-refresh while the modal is open in v1 (it's set once
when LimiterParams arrives in response to GetLimiterParams on modal open). If
the user wants a fresh value, they close and reopen the modal. (Live polling
inside the modal is a v2 feature once we know whether the static reading is
already useful.)

## Testing

Unit tests (Rust):
1. `limiter_activity::accumulator_only_counts_below_threshold` — feed synthetic
   GR samples (-0.5 dB, -1.5 dB, -3.0 dB, +0.0 dB), assert active_ms increments
   only on the -1.5 and -3.0 samples.
2. `proxy::reset_limiter_activity_authorization` — engineer can reset any track,
   member can reset only their own (uses same `owns_limiter_track` helper).
3. `proxy::reset_limiter_activity_zeros_state_and_writes_extstate` — verify both
   the HashMap entry zero and the EXTSTATE write happen together.
4. `serde::limiter_params_includes_active_seconds` — ServerMsg roundtrip
   includes the new field.

E2E live (deploy job, runs on iem.lan with real REAPER):
1. `limiter-activity.spec.ts`:
   - Login as engineer, navigate to a member's mixer
   - Use `tone_generator` ReaScript to send a known signal hot enough to engage
     the limiter (the existing tone generator already used by audio-pipeline tests)
   - Hold for 5 seconds, stop tone
   - Open that member's limiter modal
   - Read the activity row text, parse the "X.X sec limited" or "M min S sec limited" form, assert total seconds ≥ 5
   - Click Reset
   - Close modal, reopen
   - Assert "not limited yet" (or sub-second residual)

## Failure modes

- **MGA_JSLimiterST modification breaks the limiter audio path:** Mitigated by
  setup_output_limiter.lua's idempotent migration. We rebuild the JSFX file once
  and ship via CI; existing inserted instances will pick up the new slider5 on
  next REAPER reload of the FX. Audio path unchanged (we only added a read-only
  slider — `gain` and `gainO` math is untouched).
- **slider5 read returns zero (plugin not yet reloaded):** Counter stays at zero
  for that track until reload. Acceptable degradation. We document the one-time
  reload requirement in the v1.149.0 changelog.
- **REAPER restart loses HashMap:** Intentional — counter is session-only.
- **Server restart while ReaScript counter is non-zero:** Next poller cycle reads
  REAPER's still-non-zero EXTSTATE total and seeds the server HashMap with it.
  This is the right behavior (the counter is tracking what REAPER actually did,
  not what the server happened to remember).
- **`active_ms` overflow:** u64 ms is safe for ~580 million years. Not a
  concern.

## Versioning

- Plugin file change → ships in v1.149.0
- New `ServerMsg.active_seconds` field is additive (older WASM clients ignore it)
- New `ClientMsg::ResetLimiterActivity` variant is additive (older servers
  reject unknown variants — but the server ships first, so this is always safe
  in our deploy order: server before frontend, both bundled in the same Tauri
  installer)

## File touch list

- `scripts/reascripts/setup_output_limiter.lua` — bump comment header version
- New: `scripts/reascripts/jsfx/MGA_JSLimiterST` — local fork with slider5 added
  (deployed to `%APPDATA%\REAPER\Effects\loser\` by deploy step, replacing the
  upstream copy)
- `scripts/reascripts/meter_bridge.lua` — add limiter activity polling block,
  EXTSTATE write, reset-key handling
- `iem-mixer/crates/iem-core/src/ws.rs` — extend LimiterParams, add
  ResetLimiterActivity ClientMsg
- `iem-mixer/crates/iem-server/src/lib.rs` — add `limiter_activity` field on AppState
- `iem-mixer/crates/iem-server/src/poller.rs` — read EXTSTATE, populate HashMap
- `iem-mixer/crates/iem-server/src/proxy.rs` — handle ResetLimiterActivity command;
  include active_seconds when responding to GetLimiterParams
- `iem-mixer/iem-ui/src/components/limiter_modal.rs` — add Active row + Reset
  button + new prop `active_seconds: ReadSignal<f64>` + reset callback
- `iem-mixer/iem-ui/src/pages/mixer.rs` — wire active_seconds signal end-to-end
- `iem-mixer/iem-ui/style.css` — minor styling for new row
- New: `iem-mixer/e2e/tests/live/limiter-activity.spec.ts` — full E2E
- `README.md` — v1.149.0 changelog entry
- 5× `Cargo.toml` + `tauri.conf.json` — version bump 1.148.0 → 1.149.0

## Open questions

None at this point — three confirmations were collected during brainstorming:
- Scope: cumulative totals only, no live indicator, no peak GR
- Placement: inside existing LimiterModal (not toolbar)
- Visibility: members and engineer alike (per existing modal access rules)

And three defaults were taken, documented for spec-review override:
- Activation threshold: GR > 1 dB
- Reset model: per-track Reset button + auto-reset on app restart
- Modal value: snapshot at open, no live polling inside modal (v2 if needed)
