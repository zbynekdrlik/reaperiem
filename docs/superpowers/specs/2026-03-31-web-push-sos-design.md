# Web Push Notifications for SOS Alert (#133)

## Goal

Enable the server to send push notifications to the engineer's phone when a band member presses SOS, even when the mixer app is fully closed or the screen is off.

## Context

The SOS alert system (v1.122.0) uses WebSocket broadcasting: `CallEngineer` triggers `EngineerAlert` which shows a persistent toast with vibration, chime, and red overlay. This only works when the app is open with an active WebSocket connection. If the engineer's phone screen is off or the browser tab is closed, the alert is silently missed.

Web Push (VAPID) solves this by letting the server send a push message to the browser's push service (FCM/Mozilla), which wakes the service worker independently of the app state.

## Architecture

```
Band Member taps SOS
  → Server receives CallEngineer via WS
  → Server broadcasts EngineerAlert via WS (existing, unchanged)
  → Server sends Web Push to all engineer push subscriptions (NEW)
  → Browser push service delivers to service worker (NEW)
  → SW shows system notification (even if app is closed)
  → Notification click opens /engineer page (existing handler)
```

## Scope

- **In scope**: Engineer SOS push notifications only
- **Out of scope**: Band member push notifications, EngineerTalking push, general notification framework

## Components

### 1. VAPID Key Management

**File**: `iem-core/src/config.rs`

Add `vapid_private_key` and `vapid_public_key` fields to `Config`. Auto-generate a P-256 ECDSA key pair on first startup if not present (same pattern as `jwt_secret` auto-generation). Store as base64url-encoded strings in `config.yaml`.

The VAPID public key is served to the frontend for `pushManager.subscribe()`. The private key signs push messages per RFC 8292.

### 2. Push Subscription Store

**File**: `iem-server/src/push.rs` (new)

`PushStore` manages engineer push subscriptions:
- Storage: `Arc<RwLock<Vec<PushSubscription>>>` (engineer-only, no per-member keying needed)
- Persistence: `{config_dir}/push_subscriptions.json` with atomic writes (same pattern as `PinStore`, `PresetStore`)
- `PushSubscription` fields: `endpoint` (URL), `p256dh` (key), `auth` (secret)
- Deduplication: by endpoint URL — re-subscribing from the same browser updates the existing entry
- Cleanup: remove subscriptions that return HTTP 404 or 410 (expired/unsubscribed)

### 3. API Endpoints

**File**: `iem-server/src/routes.rs`

| Endpoint | Method | Auth | Purpose |
|----------|--------|------|---------|
| `/api/push/vapid-key` | GET | None | Return VAPID public key (base64url) for browser `subscribe()` |
| `/api/push/subscribe` | POST | Engineer JWT | Store push subscription |

**Subscribe request body**:
```json
{
  "endpoint": "https://fcm.googleapis.com/fcm/send/...",
  "keys": {
    "p256dh": "base64url...",
    "auth": "base64url..."
  }
}
```

### 4. Push Delivery

**File**: `iem-server/src/proxy.rs` (in `CallEngineer` handler, ~line 1171)

After the existing `EngineerAlert` WebSocket broadcast, spawn a `tokio::spawn` task to send Web Push to all engineer subscriptions. The push payload is JSON:

```json
{
  "type": "SOS",
  "name": "Petka",
  "member": "petka"
}
```

Push delivery is fire-and-forget (spawned task). On 404/410 response from the push service, remove the stale subscription. Other errors are logged but do not affect the WS broadcast.

### 5. Frontend: Push Subscription

**File**: `iem-ui/src/pages/mixer.rs` (engineer session init)

After the engineer WebSocket connects successfully:
1. Fetch VAPID public key from `GET /api/push/vapid-key`
2. Call `navigator.serviceWorker.ready` → `pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: vapidPublicKey })`
3. Send the resulting `PushSubscription` to `POST /api/push/subscribe`

Only runs for engineer sessions (checked via `is_engineer` flag). If the browser already has an active subscription, re-send it anyway (server deduplicates by endpoint).

### 6. Service Worker: Push Event

**File**: `iem-ui/sw.js`

Add a `push` event listener (separate from the existing `message` handler):

```javascript
self.addEventListener('push', (event) => {
  const data = event.data?.json() ?? {};
  if (data.type === 'SOS') {
    event.waitUntil(
      self.registration.showNotification(`IEM Alert: ${data.name}`, {
        body: `${data.name} needs help!`,
        requireInteraction: true,
        vibrate: [500, 200, 500, 200, 500],
        tag: 'iem-alert',
      })
    );
  }
});
```

The existing `notificationclick` handler already opens `/engineer` — it works for both `message`-based and `push`-based notifications.

## Dependencies

**New crate**: `web-push` — handles VAPID signing, RFC 8291 encryption, and push API delivery. One dependency replaces ~200 lines of manual crypto.

## What Does NOT Change

- Existing WebSocket-based alert toast (vibration, chime, red overlay) — untouched
- Existing SW `message` handler for in-app background notifications — untouched
- Existing `notificationclick` handler — reused as-is
- Alert state management (`active_alerts` HashMap, catch-up on connect) — untouched
- `ClearAlert` flow — untouched (push notifications are fire-and-forget; clearing is handled by the app when open)

## Testing

- **Unit test**: VAPID key auto-generation (generate → serialize → deserialize → keys match)
- **Unit test**: PushStore CRUD (add subscription, dedup by endpoint, remove stale)
- **Integration test**: `/api/push/vapid-key` returns valid base64url key
- **Integration test**: `/api/push/subscribe` stores subscription, rejects non-engineer tokens
- **E2E test**: Full flow — engineer logs in, subscription is sent, band member presses SOS, push notification appears (requires Playwright + service worker interception)
