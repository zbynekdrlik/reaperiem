# Vibration Reliability Fix — Design Spec (#162)

## Problem

The engineer's phone vibration during SOS alerts is unreliable — sometimes vibrates, sometimes doesn't. The root cause is that `alert_toast.rs` uses `setInterval(1500ms)` + individual `navigator.vibrate(500)` calls. Mobile browsers (especially Android Chrome) aggressively throttle `setInterval` in background/minimized PWAs, and `navigator.vibrate()` is cancelled when the page becomes hidden per the Visibility API spec. When the user returns to the app, vibration never restarts because no `visibilitychange` listener re-triggers it.

## Changes

### 1. Replace interval-based vibration with pattern-based vibration

**File:** `iem-mixer/iem-ui/src/components/alert_toast.rs`

Replace the current approach:
```
setInterval(1500ms) → vibrate(500) on each tick
```

With:
```
navigator.vibrate([500, 1000, 500, 1000, ...]) — pattern repeated 30 times (~45s)
setInterval(30000ms) → re-fire the full pattern (safety net for pattern expiry)
```

The browser handles pattern timing natively, which is resilient to JS timer throttling. The 30s refresh interval has ~20x fewer callbacks than the current 1.5s interval, making it far less susceptible to background throttling.

### 2. Add visibilitychange listener for foreground recovery

When the page becomes visible again (`document.visibilityState === "visible"`), immediately re-fire the vibration pattern if the alert is still active. This handles the "engineer switched apps and came back" case that currently leaves vibration dead.

The listener must be added when the alert starts and removed when it clears, to avoid keeping a permanent global listener.

### 3. Clean up on alert clear

When the alert is dismissed:
- Clear the 30s refresh interval
- Call `navigator.vibrate(0)` to stop any in-progress pattern
- Remove the `visibilitychange` listener

### What stays the same

- Service worker notification with vibrate pattern (`[500, 200, 500, 200, 500]`) — already reliable for backgrounded apps
- Sound loop (`play_chime()` every 10s) — not affected by this change
- Alert button haptic feedback (100ms on member's click) — not affected
- Red page pulse overlay — not affected
- WebSocket message flow — not affected

## Files touched

| File | Change |
|------|--------|
| `iem-mixer/iem-ui/src/components/alert_toast.rs` | Replace interval vibration with pattern + visibilitychange listener |

## Out of scope

- Sound loop reliability (not reported as broken)
- Member-side haptic feedback (100ms on button click — works fine)
- Service worker notification vibrate pattern (already reliable)
