# Fast Startup & 7-Day Token Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the ~10s blank page on PWA open and reduce PIN entry from daily to weekly.

**Architecture:** Three independent changes — (1) extend JWT expiry from 24h to 7 days, (2) add an HTML/CSS/JS app shell in `index.html` that renders before WASM loads, (3) add service worker cache-first strategy for content-hashed WASM/JS assets. Each change is independently useful and testable.

**Tech Stack:** Rust (Axum auth), HTML/CSS/JS (app shell), Service Worker Cache API

---

## File Structure

| File | Role | Change |
|------|------|--------|
| `iem-mixer/crates/iem-server/src/auth.rs` | JWT token creation + auth middleware | Expiry constants 24h → 7d, update tests |
| `iem-mixer/iem-ui/index.html` | HTML entry point loaded before WASM | Add app shell div with inline CSS/JS |
| `iem-mixer/iem-ui/src/lib.rs` | WASM entry point | Remove app shell after Leptos mounts |
| `iem-mixer/iem-ui/sw.js` | Service worker | Add fetch handler for cache-first hashed assets |
| `iem-mixer/crates/iem-core/Cargo.toml` | Core crate version | Bump 1.127.0 → 1.128.0 |
| `iem-mixer/Cargo.toml` | Workspace version | Bump 1.127.0 → 1.128.0 |
| `iem-mixer/crates/iem-server/Cargo.toml` | Server version | Bump 1.127.0 → 1.128.0 |
| `iem-mixer/iem-ui/Cargo.toml` | UI version | Bump 1.127.0 → 1.128.0 |
| `iem-mixer/src-tauri/Cargo.toml` | Tauri version | Bump 1.127.0 → 1.128.0 |
| `iem-mixer/src-tauri/tauri.conf.json` | NSIS installer version | Bump 1.127.0 → 1.128.0 |

---

### Task 1: Version Bump

**Files:**
- Modify: `iem-mixer/crates/iem-core/Cargo.toml:3`
- Modify: `iem-mixer/Cargo.toml`
- Modify: `iem-mixer/crates/iem-server/Cargo.toml:3`
- Modify: `iem-mixer/iem-ui/Cargo.toml`
- Modify: `iem-mixer/src-tauri/Cargo.toml`
- Modify: `iem-mixer/src-tauri/tauri.conf.json:4`

- [ ] **Step 1: Bump all version files from 1.127.0 to 1.128.0**

```bash
sed -i 's/version = "1.127.0"/version = "1.128.0"/' \
  iem-mixer/crates/iem-core/Cargo.toml \
  iem-mixer/Cargo.toml \
  iem-mixer/crates/iem-server/Cargo.toml \
  iem-mixer/iem-ui/Cargo.toml \
  iem-mixer/src-tauri/Cargo.toml
sed -i 's/"version": "1.127.0"/"version": "1.128.0"/' iem-mixer/src-tauri/tauri.conf.json
```

- [ ] **Step 2: Verify all files were bumped**

```bash
grep -r '"1.127.0"\|"1\.127\.0"' iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
```

Expected: No output (all instances replaced).

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/crates/iem-core/Cargo.toml iem-mixer/Cargo.toml iem-mixer/crates/iem-server/Cargo.toml iem-mixer/iem-ui/Cargo.toml iem-mixer/src-tauri/Cargo.toml iem-mixer/src-tauri/tauri.conf.json
git commit -m "chore: bump version to 1.128.0"
```

---

### Task 2: Extend Token Expiry from 24h to 7 Days

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/auth.rs:44-47` (constants)
- Modify: `iem-mixer/crates/iem-server/src/auth.rs:396-455` (tests)

- [ ] **Step 1: Update the expiry constants**

In `iem-mixer/crates/iem-server/src/auth.rs`, change lines 44-47:

Old:
```rust
/// Token expiration for members (24 hours)
const MEMBER_TOKEN_EXPIRY_SECS: u64 = 24 * 60 * 60;
/// Token expiration for engineers (24 hours — same as members)
const ENGINEER_TOKEN_EXPIRY_SECS: u64 = 24 * 60 * 60;
```

New:
```rust
/// Token expiration for members (7 days)
const MEMBER_TOKEN_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;
/// Token expiration for engineers (7 days — same as members)
const ENGINEER_TOKEN_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;
```

- [ ] **Step 2: Update `test_member_token_expiry_24h` → rename and fix assertions**

Old test (lines 396-419):
```rust
    #[test]
    fn test_member_token_expiry_24h() {
        let config = test_config();
        let result = issue_token(&config, "petka", false);

        assert!(result.is_ok());
        let response = result.unwrap().0;

        // Member tokens should have 24h expiry
        assert!(!response.engineer);
        assert_eq!(response.expires_in, 24 * 60 * 60);

        // Verify the token claims
        let claims = extract_claims(&response.token, &config.jwt_secret).unwrap();
        assert_eq!(claims.sub, "petka");
        assert!(!claims.engineer);

        // Verify expiry is approximately 24h from now (within 5 sec tolerance)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expected_exp = now + 24 * 60 * 60;
        assert!((claims.exp as i64 - expected_exp as i64).abs() < 5);
    }
```

New test:
```rust
    #[test]
    fn test_member_token_expiry_7d() {
        let config = test_config();
        let result = issue_token(&config, "petka", false);

        assert!(result.is_ok());
        let response = result.unwrap().0;

        // Member tokens should have 7-day expiry
        assert!(!response.engineer);
        assert_eq!(response.expires_in, 7 * 24 * 60 * 60);

        // Verify the token claims
        let claims = extract_claims(&response.token, &config.jwt_secret).unwrap();
        assert_eq!(claims.sub, "petka");
        assert!(!claims.engineer);

        // Verify expiry is approximately 7 days from now (within 5 sec tolerance)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expected_exp = now + 7 * 24 * 60 * 60;
        assert!((claims.exp as i64 - expected_exp as i64).abs() < 5);
    }
```

- [ ] **Step 3: Update `test_engineer_token_expiry_24h` → rename and fix assertions**

Old test (lines 421-444):
```rust
    #[test]
    fn test_engineer_token_expiry_24h() {
        let config = test_config();
        let result = issue_token(&config, "engineer", true);

        assert!(result.is_ok());
        let response = result.unwrap().0;

        // Engineer tokens should have 24h expiry
        assert!(response.engineer);
        assert_eq!(response.expires_in, 24 * 60 * 60);

        // Verify the token claims
        let claims = extract_claims(&response.token, &config.jwt_secret).unwrap();
        assert_eq!(claims.sub, "engineer");
        assert!(claims.engineer);

        // Verify expiry is approximately 24h from now (within 5 sec tolerance)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expected_exp = now + 24 * 60 * 60;
        assert!((claims.exp as i64 - expected_exp as i64).abs() < 5);
    }
```

New test:
```rust
    #[test]
    fn test_engineer_token_expiry_7d() {
        let config = test_config();
        let result = issue_token(&config, "engineer", true);

        assert!(result.is_ok());
        let response = result.unwrap().0;

        // Engineer tokens should have 7-day expiry
        assert!(response.engineer);
        assert_eq!(response.expires_in, 7 * 24 * 60 * 60);

        // Verify the token claims
        let claims = extract_claims(&response.token, &config.jwt_secret).unwrap();
        assert_eq!(claims.sub, "engineer");
        assert!(claims.engineer);

        // Verify expiry is approximately 7 days from now (within 5 sec tolerance)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expected_exp = now + 7 * 24 * 60 * 60;
        assert!((claims.exp as i64 - expected_exp as i64).abs() < 5);
    }
```

- [ ] **Step 4: Update `test_token_expiry_constants`**

Old test (lines 446-454):
```rust
    #[test]
    fn test_token_expiry_constants() {
        // Verify expiry constants are correct — both 24 hours
        assert_eq!(MEMBER_TOKEN_EXPIRY_SECS, 24 * 60 * 60);
        assert_eq!(ENGINEER_TOKEN_EXPIRY_SECS, 24 * 60 * 60);

        // Both roles should have the same expiry
        assert_eq!(ENGINEER_TOKEN_EXPIRY_SECS, MEMBER_TOKEN_EXPIRY_SECS);
    }
```

New test:
```rust
    #[test]
    fn test_token_expiry_constants() {
        // Verify expiry constants are correct — both 7 days
        assert_eq!(MEMBER_TOKEN_EXPIRY_SECS, 7 * 24 * 60 * 60);
        assert_eq!(ENGINEER_TOKEN_EXPIRY_SECS, 7 * 24 * 60 * 60);

        // Both roles should have the same expiry
        assert_eq!(ENGINEER_TOKEN_EXPIRY_SECS, MEMBER_TOKEN_EXPIRY_SECS);
    }
```

- [ ] **Step 5: Run cargo fmt check**

```bash
cd iem-mixer && cargo fmt --all --check
```

Expected: No formatting issues.

- [ ] **Step 6: Commit**

```bash
git add iem-mixer/crates/iem-server/src/auth.rs
git commit -m "feat: extend token expiry from 24h to 7 days"
```

---

### Task 3: Pre-WASM App Shell

**Files:**
- Modify: `iem-mixer/iem-ui/index.html:24-34` (body content)
- Modify: `iem-mixer/iem-ui/src/lib.rs:14-21` (WASM entry point)

- [ ] **Step 1: Add app shell to index.html**

Replace the entire `<body>` section of `iem-mixer/iem-ui/index.html` (lines 24-35).

Old:
```html
<body>
    <noscript>This app requires JavaScript and WebAssembly to run.</noscript>
    <script>
    if ('serviceWorker' in navigator) {
        navigator.serviceWorker.register('/sw.js');
        // Request notification permission for engineer alerts
        if ('Notification' in window && Notification.permission === 'default') {
            Notification.requestPermission();
        }
    }
    </script>
</body>
```

New:
```html
<body>
    <noscript>This app requires JavaScript and WebAssembly to run.</noscript>
    <div id="app-shell">
        <style>
            #app-shell {
                position: fixed; inset: 0;
                background: #1a1a2e; color: #e0e0e0;
                display: flex; flex-direction: column;
                align-items: center; justify-content: center;
                font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
                z-index: 9999;
            }
            #app-shell h1 {
                font-size: 1.2rem; letter-spacing: 0.15em;
                margin-bottom: 2rem; text-transform: uppercase;
            }
            .shell-spinner {
                width: 40px; height: 40px;
                border: 3px solid rgba(255,255,255,0.15);
                border-top-color: #4fc3f7;
                border-radius: 50%;
                animation: shell-spin 0.8s linear infinite;
            }
            .shell-member {
                margin-top: 1rem; font-size: 0.9rem; opacity: 0.7;
            }
            @keyframes shell-spin { to { transform: rotate(360deg); } }
        </style>
        <h1>Newlevel IEM Mixer</h1>
        <div class="shell-spinner"></div>
        <div class="shell-member" id="shell-member-name"></div>
    </div>
    <script>
    if ('serviceWorker' in navigator) {
        navigator.serviceWorker.register('/sw.js');
        if ('Notification' in window && Notification.permission === 'default') {
            Notification.requestPermission();
        }
    }
    // Show cached member name before WASM loads
    try {
        var raw = localStorage.getItem('iem_token');
        if (raw) {
            var auth = JSON.parse(raw);
            if (auth && auth.member) {
                document.getElementById('shell-member-name').textContent =
                    'Loading ' + auth.member + '\u2019s mixer\u2026';
            }
        }
    } catch(e) {}
    </script>
</body>
```

- [ ] **Step 2: Remove app shell when WASM mounts**

Leptos `mount_to_body` **appends** to `<body>` — it does not replace existing children. So the app shell div will remain visible unless explicitly removed.

In `iem-mixer/iem-ui/src/lib.rs`, add removal after mount:

Old:
```rust
/// Main entry point for WASM
#[wasm_bindgen(start)]
pub fn main() {
    // Better panic messages in console
    console_error_panic_hook::set_once();

    // Mount the app
    leptos::mount::mount_to_body(router::App);
}
```

New:
```rust
/// Main entry point for WASM
#[wasm_bindgen(start)]
pub fn main() {
    // Better panic messages in console
    console_error_panic_hook::set_once();

    // Mount the app
    leptos::mount::mount_to_body(router::App);

    // Remove pre-WASM loading shell now that Leptos has mounted
    if let Some(shell) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("app-shell"))
    {
        shell.remove();
    }
}
```

- [ ] **Step 3: Run cargo fmt check**

```bash
cd iem-mixer && cargo fmt --all --check
```

Expected: No formatting issues.

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/iem-ui/index.html iem-mixer/iem-ui/src/lib.rs
git commit -m "feat: add pre-WASM app shell for instant loading screen"
```

---

### Task 4: Service Worker Precaching for Hashed Assets

**Files:**
- Modify: `iem-mixer/iem-ui/sw.js` (entire file)

- [ ] **Step 1: Rewrite sw.js with cache-first for hashed assets**

Replace the entire content of `iem-mixer/iem-ui/sw.js`:

```javascript
// IEM Mixer Service Worker — PWA shell + hashed asset caching.
// Only content-hashed files (WASM/JS from Trunk) are cached.
// index.html and unhashed files are NEVER cached in SW (caused blank pages 2026-03-19).

const CACHE_NAME = 'iem-assets-v1';
// Trunk outputs files like: iem-ui-c72f48fccb666eb9.js, iem-ui-c72f48fccb666eb9_bg.wasm
const HASH_RE = /[a-f0-9]{16,}\.(js|wasm)$/;

self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  // Delete all caches except current version
  event.waitUntil(
    caches
      .keys()
      .then((names) =>
        Promise.all(
          names
            .filter((name) => name !== CACHE_NAME)
            .map((name) => caches.delete(name))
        )
      )
      .then(() => self.clients.claim()),
  );
});

// Cache-first for content-hashed assets (immutable by definition).
// All other requests (index.html, API, unhashed files) go straight to network.
self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);

  // Only cache same-origin GET requests for hashed files
  if (url.origin !== self.location.origin) return;
  if (event.request.method !== "GET") return;
  if (!HASH_RE.test(url.pathname)) return;

  event.respondWith(
    caches.open(CACHE_NAME).then((cache) =>
      cache.match(event.request).then((cached) => {
        if (cached) return cached;
        return fetch(event.request).then((response) => {
          if (response.ok) {
            cache.put(event.request, response.clone());
          }
          return response;
        });
      })
    )
  );
});

// Handle alert notifications from WASM (works when app is in background)
self.addEventListener("message", (event) => {
  if (event.data && event.data.type === "ALERT") {
    const name = event.data.name || "Member";
    self.registration.showNotification(`IEM Alert: ${name}`, {
      body: `${name} needs help!`,
      requireInteraction: true,
      tag: "iem-alert", // Replace previous alert notification
      vibrate: [500, 200, 500, 200, 500],
    });
  }
});

// Clicking notification brings app to foreground
self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  event.waitUntil(
    self.clients.matchAll({ type: "window" }).then((clients) => {
      if (clients.length > 0) {
        return clients[0].focus();
      }
      return self.clients.openWindow("/engineer");
    }),
  );
});

// Handle Web Push notifications (works even when app is fully closed) (#133)
self.addEventListener("push", (event) => {
  let data = {};
  try {
    data = event.data?.json() ?? {};
  } catch {
    data = {};
  }

  if (data.type === "SOS") {
    event.waitUntil(
      self.registration.showNotification(`IEM Alert: ${data.name || "Member"}`, {
        body: `${data.name || "Someone"} needs help!`,
        requireInteraction: true,
        tag: "iem-alert",
        vibrate: [500, 200, 500, 200, 500],
      }),
    );
  } else {
    // Generic fallback for unknown push types
    event.waitUntil(
      self.registration.showNotification("IEM Mixer", {
        body: "New alert — tap to open",
        requireInteraction: true,
        tag: "iem-generic",
      }),
    );
  }
});
```

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/iem-ui/sw.js
git commit -m "feat: add SW cache-first for hashed WASM/JS assets"
```

---

## Verification

After pushing to `dev` and CI deploys:

1. **Token expiry** — CI unit tests verify 7-day expiry values (automated)
2. **App shell** — Open `https://iem.newlevel.media/` in Playwright, take screenshot within 1 second of navigation (before WASM fully loads). Should show "Newlevel IEM Mixer" with spinner, not blank page.
3. **SW caching** — After first load, open DevTools → Application → Cache Storage → `iem-assets-v1`. Should contain `.wasm` and `.js` files with content hashes.
4. **Repeat load speed** — Second load should render in <1s (WASM served from SW cache).
5. **Token persistence** — Log in, close app, reopen after 1 day → should go straight to mixer (no PIN).
6. **No blank pages on deploy** — Push a new version, verify app loads correctly (new WASM hash fetched from network, old one cleaned up).
