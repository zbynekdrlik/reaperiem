# Fix: Solo Leaves Everything Muted After PWA Crash (#155)

## Problem

When a band member activates solo and their PWA crashes/hangs/is killed, tracks remain muted in REAPER with no recovery path. The member must manually unmute tens of tracks.

**Root cause:** Solo state is managed client-side. The UI sends individual `SetMute` commands to mute/unmute tracks, while `SetSolo` only syncs the solo indicator. When the last WebSocket disconnects, `solo_states` is cleared from server memory, and on reconnect the client doesn't know solo was active — REAPER tracks stay muted.

## Design: Server-Managed Solo

Move solo muting entirely to the server. The client sends `SetSolo` only; the server handles all REAPER mute commands and persists pre-solo state for crash recovery.

### New Server State

Add to `MixerCache` in `lib.rs`:

```rust
/// Pre-solo mute states per member — saved when solo activates
/// (member_id -> track_index -> was_muted_before_solo)
pub pre_solo_mutes: HashMap<String, HashMap<usize, bool>>,
```

### SetSolo Handler Changes (`proxy.rs`)

When server receives `ClientMsg::SetSolo { soloed }`:

**Case 1: Entering solo** (no current solo, `soloed` non-empty)
1. Read current mute states from `member_states` cache → save to `pre_solo_mutes`
2. For each channel in `member_states`:
   - If `track_index` NOT in `soloed` set → send `SET/TRACK/{t}/SEND/{si}/MUTE/1` to REAPER, update cache `muted = true`
   - If `track_index` IN `soloed` set → send `SET/TRACK/{t}/SEND/{si}/MUTE/0` to REAPER, update cache `muted = false`
3. Store `solo_states[member] = soloed`
4. Broadcast `SoloUpdate` + `ChannelUpdate` for changed channels

**Case 2: Switching solo** (current solo non-empty, new `soloed` different)
1. Keep existing `pre_solo_mutes` (from original solo entry)
2. For each channel: apply same mute logic as Case 1 based on new soloed set
3. Update `solo_states[member]`
4. Broadcast updates

**Case 3: Exiting solo** (`soloed` empty, current solo non-empty)
1. Read `pre_solo_mutes[member]`
2. For each channel: restore mute state from saved pre_solo_mutes → send REAPER commands
3. Clear `solo_states[member]` and `pre_solo_mutes[member]`
4. Broadcast `SoloUpdate { soloed: [] }` + `ChannelUpdate` for changed channels

### Disconnect Behavior Change (`proxy.rs`)

**Do NOT clear `solo_states` or `pre_solo_mutes` on disconnect.** Remove only `member_states` and `active_members` as before. Solo persists until explicit unsolo.

```rust
// BEFORE (line 1446):
cache.solo_states.remove(&member_id);

// AFTER: remove this line — solo survives disconnects
```

### Reconnect Behavior (already works)

On connect, server already sends `SoloUpdate` from cached `solo_states` (lines 1036-1046). After this fix, the state will still be there after a disconnect, so the reconnecting client will receive the active solo state and render correctly.

### UI Changes (`mixer.rs`)

In the solo click handler (`on_solo_click`):
- **Remove** all `ws_send(ws, &iem_core::ClientMsg::SetMute { ... })` calls from solo logic
- **Keep** local `set_channels.update(...)` for immediate visual feedback (optimistic UI)
- **Keep** `ws_send(ws, &iem_core::ClientMsg::SetSolo { ... })` — this is the only WS message sent
- **Keep** `pre_solo_mutes` local signal for UI display (but it's now just for optimistic rendering; server is source of truth)

The server broadcasts `ChannelUpdate` for each changed channel, which the UI already handles for cross-tab sync.

### SoloUpdate Handler (UI)

On receiving `SoloUpdate` from server:
- **Keep** existing logic that updates `soloed` signal and channel muted display
- The server now also sends `ChannelUpdate` for each muted/unmuted track, so the UI gets explicit mute state from the server

### send_index_for

The `send_index_for` closure is defined inside `apply_command_to_cache` (line 1645), NOT in the SetSolo handler scope (line 1347). Two approaches:

**Chosen approach:** Move SetSolo handling INTO `apply_command_to_cache` instead of the early `continue` block. This gives access to `send_index_for`, `member_index`, `reaper_url`, `input_count`, and `mix_members`. Add `SetSolo` as a new match arm in the command dispatch at line 1658.

The early handler (line 1347) is removed. SetSolo flows through the same validation and dispatch path as SetMute/SetLevel.

### REAPER Command Helper

Extract a helper for sending mute commands for all channels during solo transitions:

```rust
async fn apply_solo_mutes(
    http_client: &reqwest::Client,
    reaper_url: &str,
    channels: &mut [Channel],
    soloed: &HashSet<usize>,
    member_index: usize,
    mix_members: &[(usize, Option<usize>)],
) -> Vec<(usize, bool)> {
    // For each channel, compute should_mute = !soloed.contains(track_index)
    // Send REAPER set_send_mute for changed channels only
    // Update channel.muted in cache
    // Returns vec of (track_index, new_muted) for ChannelUpdate broadcast
}
```

### What This Fixes

| Scenario | Before | After |
|----------|--------|-------|
| PWA crash during solo | Tracks muted forever, no recovery | Reconnect shows solo active, unsolo restores |
| Kill and reopen PWA | Solo state lost, mutes orphaned | Solo state persisted, displayed on reconnect |
| Multiple tabs, one crashes | Solo state inconsistent | Server manages state, all tabs sync |
| Normal solo on/off | Works (UI manages mutes) | Works (server manages mutes) |

### Files Changed

| File | Change |
|------|--------|
| `iem-mixer/crates/iem-server/src/lib.rs` | Add `pre_solo_mutes` to `MixerCache` |
| `iem-mixer/crates/iem-server/src/proxy.rs` | Server-side solo muting in SetSolo handler, remove solo cleanup from disconnect |
| `iem-mixer/iem-ui/src/pages/mixer.rs` | Remove SetMute calls from solo click handler |
| `iem-mixer/e2e/tests/live/mixer.spec.ts` | Update solo E2E tests to verify server-managed behavior |

### Testing

1. **Unit test**: `SetSolo` handler saves pre_solo_mutes, sends correct REAPER commands
2. **E2E test**: Solo on → verify tracks muted in REAPER → unsolo → verify tracks restored
3. **E2E test**: Solo on → disconnect WS → reconnect → verify solo still active → unsolo → verify restore
4. **Live verification**: Solo on → kill PWA → reopen → see solo active → click unsolo → all tracks back to normal
