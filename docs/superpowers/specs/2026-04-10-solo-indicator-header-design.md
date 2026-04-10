# Solo Indicator and Clear Button in Header

## Problem

After v1.137.0 solo now persists across tabs and WebSocket disconnects. This introduces a new usability gap:

- When a user opens the PWA and a previous solo is still active, they see channels muted with no indication WHY
- Solo can be activated on one tab but there's no visual cue on other tabs (beyond the solo button itself, which is only visible on the relevant channel tab — Mics, Stems, etc.)
- To unsolo, the user must find the soloed channel (which may be on a different category tab) and click its solo button
- No quick way to "clear all solo" without hunting for the source channel

## Goal

Make solo state globally visible in the header (always on screen regardless of category tab) and give the user a one-click way to clear solo from any tab.

## Constraints (User feedback)

- **No new bar** — cannot add a banner that consumes vertical space above the mixer
- **Reuse header space** — shrink existing elements to make room
- Version/date stays visible normally, replaced by Clear Solo button only when solo is active

## Design

### Header Layout Changes

**Current header (before):**
```
[ ← ] [ MemberName ] [ v1.137.0 / 2026-04-10 ] [ ⚙ ] [LAN] [●]
```

**When solo is NOT active (after — compacted):**
```
[←][ MemberName ] [ v1.137.0 / 2026-04-10 ] [ ⚙ ] [L] [●]
                                                     [A]
                                                     [N]
```

**When solo IS active:**
```
[←][ MemberName ] [ 🟡 SOLO ✕ ] [ ⚙ ] [L] [●]
                                        [A]
                                        [N]
```

### Component Changes

1. **Back button (`.back-btn`)** — shrink padding
   - Smaller horizontal/vertical padding (`4px 6px` or similar)
   - Still tappable on touch devices (min 32px touch target)
   - Icon stays the same `←`

2. **Network indicator (`.network-indicator`)** — vertical text
   - CSS: `writing-mode: vertical-rl; transform: rotate(180deg);` (or similar)
   - Font size: ~9-10px
   - Letters stack vertically: L over A over N
   - Saves ~30-40px horizontal space

3. **Solo button (new, conditional)** — in place of `.header-version`
   - Conditional render: `when soloed.get().is_empty() → .header-version, else → .header-solo-btn`
   - Label: "SOLO ✕" (text + close icon)
   - Background: yellow or orange (`#f59e0b` or similar) — high visibility
   - Bold text, white foreground
   - Subtle pulse animation to draw attention (`@keyframes pulse-solo`)
   - Same height as version display (no layout shift)
   - Click handler sends `ClientMsg::SetSolo { soloed: vec![] }` via WebSocket
   - Also clears local `pre_solo_mutes` signal (already handled in SoloUpdate receiver)

### Data Flow

```
User clicks "SOLO ✕" in header
  ↓
ws_send(ClientMsg::SetSolo { soloed: vec![] })
  ↓
Server apply_command_to_cache processes SetSolo with empty soloed
  → had_solo && !wants_solo branch (EXITING SOLO)
  → reads pre_solo_mutes from cache
  → sends REAPER set_send_mute commands to restore original mute states
  → clears solo_states and pre_solo_mutes for member
  → broadcasts SoloUpdate { soloed: [] }
  → broadcasts ChannelUpdate for each changed channel
  ↓
All member's tabs receive:
  - SoloUpdate → set_soloed.set(empty) → header reverts from SOLO btn to version display
  - ChannelUpdate per track → set_channels updates mute states → channels visually unmute
```

### Why This Works

- Server-side SetSolo handler (v1.137.0) already handles empty `soloed` vec as "exit solo and restore"
- `soloed` signal is already shared across the mixer component
- Header is rendered at the top level (`mixer-header`), always visible regardless of category tab
- No new server-side state or messages needed

### Files Changed

| File | Change |
|------|--------|
| `iem-mixer/iem-ui/src/pages/mixer.rs` | Header conditional render: SOLO button when `!soloed.is_empty()`, else version display. Shrink back button markup. |
| `iem-mixer/iem-ui/styles/...` (or inline) | CSS: shrunk `.back-btn`, vertical `.network-indicator`, new `.header-solo-btn` with pulse animation |
| `iem-mixer/e2e/tests/live/mixer.spec.ts` | New E2E: solo on channel → verify header shows SOLO btn → click → verify header reverts + channels unmuted. Cross-tab: solo on tab1 → verify tab2 header shows SOLO btn. |

### CSS Details

```css
/* Compacted back button */
.back-btn {
  padding: 4px 8px;  /* was larger */
  font-size: 1.2rem; /* slightly smaller */
}

/* Vertical network indicator */
.network-indicator {
  writing-mode: vertical-rl;
  transform: rotate(180deg);
  font-size: 9px;
  font-weight: bold;
  padding: 2px 4px;
  line-height: 1.1;
}

/* Solo button in header (replaces version when active) */
.header-solo-btn {
  background: #f59e0b;  /* amber */
  color: white;
  border: none;
  padding: 6px 12px;
  border-radius: 4px;
  font-weight: bold;
  font-size: 0.9rem;
  cursor: pointer;
  animation: pulse-solo 1.5s ease-in-out infinite;
}

.header-solo-btn:hover {
  background: #d97706;
}

@keyframes pulse-solo {
  0%, 100% { box-shadow: 0 0 0 0 rgba(245, 158, 11, 0.6); }
  50% { box-shadow: 0 0 0 6px rgba(245, 158, 11, 0); }
}
```

### Testing

1. **E2E: single-tab clear solo from header**
   - Login, solo a channel
   - Verify header shows "SOLO ✕" button (yellow)
   - Click header SOLO button
   - Verify header reverts to version display
   - Verify soloed channel's solo button returns to "off"
   - Verify other channels are no longer muted

2. **E2E: cross-tab clear from non-source tab**
   - Open two tabs (ctx1, ctx2) for same member
   - On tab1: solo a channel
   - On tab2: verify header shows SOLO button (received via SoloUpdate broadcast)
   - On tab2: click header SOLO button
   - Verify both tabs' headers revert to version
   - Verify both tabs' channels are unmuted

3. **E2E: clear from different category tab**
   - Solo a channel on Mics tab
   - Switch to Stems (or another) tab
   - Verify SOLO button still visible in header
   - Click it → verify unsolo works

### Accessibility

- SOLO button has `aria-label="Clear solo"` for screen readers
- Pulse animation respects `prefers-reduced-motion: reduce` — disables pulse

### What Stays the Same

- Server-side solo state management (v1.137.0 logic unchanged)
- Channel-level solo buttons (still functional)
- `SetSolo` / `SoloUpdate` / `ChannelUpdate` WebSocket messages
- Pre-solo mute restore behavior
