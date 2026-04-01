# Fast Startup & Token Longevity Design

## Problem

Two issues degrade the band member experience during live events:

1. **Slow startup (~10s)**: Opening the PWA shows a blank/spinning page while WASM downloads and compiles. During a service or rehearsal, band members need instant access to their mix.
2. **Repeated PIN entry**: 24-hour token expiry forces re-authentication daily. Band members shouldn't need to fumble with PINs during a live event.

## Design

### Fix 1: Token Expiry 24h → 7 Days

**Server** (`iem-server/src/auth.rs`):
- Change `MEMBER_TOKEN_EXPIRY_SECS` from `24 * 60 * 60` to `7 * 24 * 60 * 60` (7 days)
- Change `ENGINEER_TOKEN_EXPIRY_SECS` to match (7 days)
- Update tests that assert specific expiry values

**Client** (`iem-ui/src/auth.rs`):
- No change needed — client reads `exp` from the JWT payload directly. Longer server expiry automatically means longer client validity.

### Fix 2: Pre-WASM App Shell

Add a minimal loading screen directly in `index.html` that renders instantly before WASM downloads. A small inline JS snippet reads `localStorage("iem_token")` and shows the member's name if available.

**File**: `iem-ui/index.html`

Behavior:
- On page load (before WASM): Show a styled loading screen with spinner
- If `iem_token` exists in localStorage: Parse the JWT payload to get the member name, show "Loading {name}..." — gives the user confidence the app knows who they are
- If no token: Show "NEWLEVEL IEM MIXER" with spinner
- When WASM mounts: Leptos removes this element (the app renders into `<body>`, replacing the shell)

The shell is pure HTML + inline CSS + ~20 lines of JS. No dependencies.

### Fix 3: Service Worker Precaching (Hashed Assets Only)

Cache WASM and JS files that have content hashes in their filenames using a cache-first strategy. This makes repeat loads instant (~200ms vs ~5s).

**File**: `iem-ui/sw.js`

Rules:
- **Cache-first** for files matching the pattern `*-[hex16]*.{js,wasm}` (Trunk output with content hash)
- **Network-only** for everything else (index.html, sw.js, manifest.json, API calls)
- **Never cache** index.html in the SW — this caused blank pages on 2026-03-19
- On SW activate: delete all caches that don't match the current cache version
- Cache name includes a version identifier to enable cleanup

Why this is safe (unlike the March 19 incident):
- Hashed files are immutable by definition — their content never changes for a given hash
- index.html always comes from network, so it always references current hashes
- If a cached hash is no longer referenced, it's just orphaned storage (cleaned up on next SW activate)
- If a new hash is needed, SW falls back to network fetch and caches the result

### Files to Modify

| File | Change |
|------|--------|
| `iem-mixer/crates/iem-server/src/auth.rs` | Token expiry 24h → 7 days + update tests |
| `iem-mixer/iem-ui/index.html` | Add pre-WASM app shell (inline HTML/CSS/JS) |
| `iem-mixer/iem-ui/sw.js` | Add cache-first for hashed assets, cleanup on activate |

### Testing

- **Unit tests**: Update `test_member_token_expiry_24h` → `test_member_token_expiry_7d` (and engineer equivalent)
- **E2E**: Verify login → close app → reopen → no PIN required (token still valid)
- **E2E**: Verify app shell visible before WASM loads (Playwright screenshot of initial load)
- **Deploy verification**: After deploy, verify SW caches WASM files and serves them on second load
