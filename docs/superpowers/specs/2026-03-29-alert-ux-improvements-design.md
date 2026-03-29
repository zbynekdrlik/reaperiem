# Alert UX Improvements — Persistent, Subtle, Production-Ready

## Problem

The initial alert implementation (v1.122.0) has two issues for production use:

1. **Annoying sound**: The 880Hz square wave beep is terrible in a quiet live environment. A sound engineer would find it unacceptable.
2. **Auto-dismiss**: The alert disappears after 10s with a countdown. If the engineer misses it (phone in pocket, looking at mixer), the alert is gone. The member has no way to know if the engineer saw it.

## Design

### Persistent Alert Lifecycle

Alert stays active until explicitly cleared by either the engineer or the band member.

**Server state**: `MixerCache` gains `active_alerts: HashMap<String, AlertState>` where key = member_id who called.

```rust
struct AlertState {
    from_member: String,
    from_name: String,
    created_at: std::time::Instant,
}
```

**New WS messages**:
- `ClientMsg::ClearAlert` — sent by engineer or member to dismiss their alert
- `ServerMsg::AlertCleared { member_id: String }` — broadcast to both member and engineer when cleared
- `ServerMsg::ActiveAlerts { alerts: Vec<{from_member, from_name}> }` — sent to engineer on WS connect (catch-up for alerts sent while offline)

**Flow**:
1. Member clicks SOS -> server stores `AlertState`, broadcasts `EngineerAlert` to engineer
2. Alert persists on both sides until cleared
3. Engineer clicks dismiss on toast -> sends `ClearAlert`, server removes alert, broadcasts `AlertCleared` to both
4. OR member clicks SOS again (cancel) -> sends `ClearAlert`, same flow
5. On engineer WS connect -> server sends `ActiveAlerts` with any pending alerts

**Rate limiting**: Keep 30s cooldown on `CallEngineer`, but only when no active alert exists for that member. If alert is already active, `CallEngineer` is a no-op (already alerting).

### Engineer-Side Alert UX

**Toast**: Fixed-position banner, no auto-dismiss. Shows "{Name} needs help!" with a dismiss button.

**Vibration loop**: `setInterval` every 3 seconds, 200ms vibrate pulse. Interval cleared when alert is dismissed.

**System notification**: `new Notification("{Name} needs help!", { requireInteraction: true })`. Stays in notification tray until clicked or alert cleared. Permission requested on first alert attempt (or in Settings modal).

**Subtle sound**: Embedded `.mp3` file (soft chime or water drop, ~10KB). Played once on initial alert, then repeated every 10s at low volume (gain 0.15) while alert is active. Stops on dismiss.

### Member-Side Alert UX

**Button states**:
- Idle: Red "SOS" button
- Active: Pulsing/highlighted "SOS Active" button (indicates alert is live)
- Clicking while active: Sends `ClearAlert` (cancel the alert)

No countdown. No auto-re-enable. The button toggles between "send alert" and "cancel alert".

**Vibration**: Single 100ms buzz on send (confirmation). No looping on member side.

### Sound File

- Format: `.mp3` (universal browser support)
- Content: Soft chime, water drop, or gentle bird chirp — something a sound engineer would recognize as intentional but that wouldn't be noticed by the audience
- Size: ~5-20KB
- Location: `iem-mixer/iem-ui/alert.mp3`
- Referenced in Trunk build via `<link data-trunk rel="copy-file" href="alert.mp3" />`
- Played via `new Audio("/alert.mp3")` with gain control

### Files to Change

| File | Change |
|------|--------|
| `iem-mixer/crates/iem-core/src/ws.rs` | Add `ClearAlert`, `AlertCleared`, `ActiveAlerts` messages |
| `iem-mixer/crates/iem-server/src/lib.rs` | Add `active_alerts: HashMap<String, AlertState>` to MixerCache |
| `iem-mixer/crates/iem-server/src/proxy.rs` | Handle ClearAlert, send ActiveAlerts on connect, update CallEngineer logic |
| `iem-mixer/iem-ui/src/components/alert_button.rs` | Remove countdown, add active/toggle state |
| `iem-mixer/iem-ui/src/components/alert_toast.rs` | Remove auto-dismiss, add vibration loop, notification API, embedded sound |
| `iem-mixer/iem-ui/alert.mp3` | NEW: subtle alert sound file |
| `iem-mixer/iem-ui/index.html` | Add Trunk copy-file for alert.mp3 |
| `iem-mixer/iem-ui/style.css` | Update alert-btn active state, pulse animation |
| `iem-mixer/e2e/tests/alert.spec.ts` | Update tests for persistent behavior |

### Verification

1. E2E: Member sends alert -> engineer toast stays until dismissed (no auto-hide)
2. E2E: Engineer dismisses -> both sides clear
3. E2E: Member cancels -> both sides clear
4. E2E: Engineer reconnects -> sees pending alert
5. Manual: Vibration loops on engineer phone every 3s
6. Manual: System notification appears when app is in background
7. Manual: Sound is subtle and appropriate for live environment
