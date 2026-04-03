# Alert UX Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the SOS alert persistent until cleared, replace annoying beep with embedded chime + vibration loop + system notification, and make it production-ready for live church environments.

**Architecture:** Server tracks active alerts in `MixerCache`. New `ClearAlert` WS message lets either side dismiss. Engineer gets looping vibration + system `Notification` + subtle embedded `.mp3`. Member button toggles between SOS/Active. Alerts survive reconnects via `ActiveAlerts` catch-up message on WS connect.

**Tech Stack:** Rust (Leptos 0.7 + Axum), WASM (wasm32), Playwright E2E, web-sys (Notification API, vibration)

**Spec:** `docs/superpowers/specs/2026-03-29-alert-ux-improvements-design.md`

---

## File Map

| File | Responsibility | Action |
|------|---------------|--------|
| `iem-mixer/crates/iem-core/src/ws.rs` | WS message types | Add `ClearAlert`, `AlertCleared`, `ActiveAlerts` |
| `iem-mixer/crates/iem-server/src/lib.rs` | Server state | Add `active_alerts` to `MixerCache` |
| `iem-mixer/crates/iem-server/src/proxy.rs` | WS handler | Handle `ClearAlert`, send `ActiveAlerts` on connect, update `CallEngineer` |
| `iem-mixer/iem-ui/src/components/alert_button.rs` | Member SOS button | Replace countdown with toggle (SOS/Active) |
| `iem-mixer/iem-ui/src/components/alert_toast.rs` | Engineer toast | Remove auto-dismiss, add vibration loop + notification + embedded sound |
| `iem-mixer/iem-ui/src/pages/mixer.rs` | Page integration | Handle new WS messages, pass `alert_active` to button |
| `iem-mixer/iem-ui/alert.mp3` | Alert sound file | NEW: subtle chime |
| `iem-mixer/iem-ui/index.html` | Trunk config | Add `copy-file` for alert.mp3 |
| `iem-mixer/iem-ui/style.css` | Styles | Add `.alert-btn.active` pulse animation |
| `iem-mixer/e2e/tests/alert.spec.ts` | E2E tests | Update for persistent alert behavior |

---

### Task 1: Add new WS message types

**Files:**
- Modify: `iem-mixer/crates/iem-core/src/ws.rs:77-78` (ClientMsg), `ws.rs:140-144` (ServerMsg), `ws.rs:646-667` (tests)

- [ ] **Step 1: Add `ClearAlert` to `ClientMsg` enum**

In `iem-mixer/crates/iem-core/src/ws.rs`, after `CallEngineer,` (line 78), add:

```rust
    /// Clear active alert (sent by engineer or member to dismiss)
    ClearAlert,
```

- [ ] **Step 2: Add `AlertCleared` and `ActiveAlerts` to `ServerMsg` enum**

After the `EngineerAlert` variant (line 144), add:

```rust
    /// Alert cleared by engineer or member
    AlertCleared { member_id: String },
    /// Active alerts sent to engineer on WS connect (catch-up)
    ActiveAlerts {
        alerts: Vec<AlertInfo>,
    },
```

Add the `AlertInfo` struct before the `ClientMsg` enum (after line 34):

```rust
/// Alert info for ActiveAlerts catch-up message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlertInfo {
    pub from_member: String,
    pub from_name: String,
}
```

- [ ] **Step 3: Add serialization tests**

After the existing `test_server_msg_engineer_alert_serialization` test (line 667), add:

```rust
    #[test]
    fn test_client_msg_clear_alert_serialization() {
        let msg = ClientMsg::ClearAlert;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"cmd\":\"ClearAlert\""));
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_alert_cleared_serialization() {
        let msg = ServerMsg::AlertCleared {
            member_id: "petka".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"AlertCleared\""));
        assert!(json.contains("\"member_id\":\"petka\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn test_server_msg_active_alerts_serialization() {
        let msg = ServerMsg::ActiveAlerts {
            alerts: vec![AlertInfo {
                from_member: "petka".to_string(),
                from_name: "Petka".to_string(),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"event\":\"ActiveAlerts\""));
        assert!(json.contains("\"from_member\":\"petka\""));
        let decoded: ServerMsg = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/crates/iem-core/src/ws.rs
git commit -m "feat: add ClearAlert, AlertCleared, ActiveAlerts WS messages"
```

---

### Task 2: Server-side persistent alert state

**Files:**
- Modify: `iem-mixer/crates/iem-server/src/lib.rs:133-134`
- Modify: `iem-mixer/crates/iem-server/src/proxy.rs:1036-1046` (on-connect), `proxy.rs:1125-1157` (CallEngineer handler), `proxy.rs:1500-1504` and `proxy.rs:1653-1654` (apply_command_to_cache)

- [ ] **Step 1: Replace `alert_cooldowns` with `active_alerts` in MixerCache**

In `iem-mixer/crates/iem-server/src/lib.rs`, replace line 133-134:

```rust
    /// Last alert timestamp per member for rate limiting (#125)
    pub alert_cooldowns: HashMap<String, std::time::Instant>,
```

with:

```rust
    /// Active SOS alerts (member_id -> alert state). Persists until cleared.
    pub active_alerts: HashMap<String, (String, String)>, // (from_member, from_name)
```

- [ ] **Step 2: Update `CallEngineer` handler to store persistent alert**

In `iem-mixer/crates/iem-server/src/proxy.rs`, replace the `CallEngineer` handler (lines 1125-1157) with:

```rust
                            // Handle band member alert to engineer (#125)
                            if let ClientMsg::CallEngineer = cmd {
                                // No-op if alert already active for this member
                                let cache = state.mixer_cache.read().await;
                                if cache.active_alerts.contains_key(&member_id) {
                                    drop(cache);
                                    continue;
                                }
                                drop(cache);

                                // Look up member display name
                                let discovered = state.discovered_members.read().await;
                                let display_name = discovered
                                    .iter()
                                    .find(|m| m.id() == member_id)
                                    .map(|m| m.name.clone())
                                    .unwrap_or_else(|| member_id.clone());
                                drop(discovered);

                                // Store active alert
                                let mut cache = state.mixer_cache.write().await;
                                cache.active_alerts.insert(
                                    member_id.clone(),
                                    (member_id.clone(), display_name.clone()),
                                );
                                drop(cache);

                                // Broadcast to all engineer devices
                                let _ = state.event_tx.send((
                                    "engineer".to_string(),
                                    ServerMsg::EngineerAlert {
                                        from_member: member_id.clone(),
                                        from_name: display_name,
                                    },
                                ));
                                // Also notify the member their alert is active
                                let _ = state.event_tx.send((
                                    member_id.clone(),
                                    ServerMsg::EngineerAlert {
                                        from_member: member_id.clone(),
                                        from_name: String::new(),
                                    },
                                ));
                                continue;
                            }
```

- [ ] **Step 3: Add `ClearAlert` handler**

Right after the `CallEngineer` handler, add:

```rust
                            // Handle alert clear (from engineer or member)
                            if let ClientMsg::ClearAlert = cmd {
                                let mut cache = state.mixer_cache.write().await;
                                // Engineer clears: member_id is "engineer", need to find which alert
                                // Member clears: member_id is the member who sent the alert
                                let cleared_member = if member_id == "engineer" {
                                    // Engineer can only have one toast visible at a time in current UI
                                    // Clear all alerts (simplest approach)
                                    let keys: Vec<String> = cache.active_alerts.keys().cloned().collect();
                                    cache.active_alerts.clear();
                                    keys
                                } else {
                                    cache.active_alerts.remove(&member_id);
                                    vec![member_id.clone()]
                                };
                                drop(cache);

                                for cleared in &cleared_member {
                                    // Notify engineer
                                    let _ = state.event_tx.send((
                                        "engineer".to_string(),
                                        ServerMsg::AlertCleared {
                                            member_id: cleared.clone(),
                                        },
                                    ));
                                    // Notify member
                                    let _ = state.event_tx.send((
                                        cleared.clone(),
                                        ServerMsg::AlertCleared {
                                            member_id: cleared.clone(),
                                        },
                                    ));
                                }
                                continue;
                            }
```

- [ ] **Step 4: Send `ActiveAlerts` on engineer WS connect**

In `proxy.rs`, after the solo state send (line 1046), add:

```rust
    // Send active alerts to engineer on connect (catch-up)
    if member_id == "engineer" {
        let cache = state.mixer_cache.read().await;
        if !cache.active_alerts.is_empty() {
            let alerts: Vec<iem_core::AlertInfo> = cache
                .active_alerts
                .values()
                .map(|(from_member, from_name)| iem_core::AlertInfo {
                    from_member: from_member.clone(),
                    from_name: from_name.clone(),
                })
                .collect();
            let msg = ServerMsg::ActiveAlerts { alerts };
            let json = serde_json::to_string(&msg).unwrap_or_default();
            let _ = socket.send(Message::Text(json.into())).await;
        }
    }
```

- [ ] **Step 5: Add `ClearAlert` to both `apply_command_to_cache` match arms**

In the first match (around line 1504), after the `CallEngineer` arm:

```rust
        iem_core::ClientMsg::ClearAlert => {
            return Err("ClearAlert should not reach apply_command_to_cache".to_string());
        }
```

In the second match (around line 1654), after the `CallEngineer` arm:

```rust
        iem_core::ClientMsg::ClearAlert => {
            unreachable!("ClearAlert handled before apply_command_to_cache")
        }
```

- [ ] **Step 6: Commit**

```bash
git add iem-mixer/crates/iem-server/src/lib.rs iem-mixer/crates/iem-server/src/proxy.rs
git commit -m "feat: persistent server-side alert state with ClearAlert support"
```

---

### Task 3: Generate and add subtle alert sound

**Files:**
- Create: `iem-mixer/iem-ui/alert.mp3`
- Modify: `iem-mixer/iem-ui/index.html:11`

- [ ] **Step 1: Generate a subtle chime sound**

Use `ffmpeg` to synthesize a gentle sine-wave chime (two soft tones, ~1s):

```bash
ffmpeg -f lavfi -i "sine=frequency=523:duration=0.3,volume=0.15" \
       -f lavfi -i "sine=frequency=659:duration=0.4" \
       -filter_complex "[0]aformat=sample_rates=44100[a];[1]aformat=sample_rates=44100,volume=0.12,adelay=400|400[b];[a][b]amix=inputs=2:duration=longest,afade=t=out:st=0.5:d=0.3" \
       -b:a 48k iem-mixer/iem-ui/alert.mp3
```

If ffmpeg is not available, download a free chime sound (~10KB) from a royalty-free source.

- [ ] **Step 2: Add Trunk copy-file directive**

In `iem-mixer/iem-ui/index.html`, after line 11 (`audio_player.js`), add:

```html
    <link data-trunk rel="copy-file" href="alert.mp3" />
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/iem-ui/alert.mp3 iem-mixer/iem-ui/index.html
git commit -m "feat: add subtle alert chime sound file"
```

---

### Task 4: Rewrite alert button as toggle (member side)

**Files:**
- Rewrite: `iem-mixer/iem-ui/src/components/alert_button.rs`
- Modify: `iem-mixer/iem-ui/src/pages/mixer.rs:100,281,420,470,537,991`

- [ ] **Step 1: Rewrite `AlertButton` component**

Replace the entire content of `iem-mixer/iem-ui/src/components/alert_button.rs`:

```rust
//! Band member alert button — calls engineer for help (#125)

use leptos::prelude::*;

/// Alert/SOS button for band members.
/// Toggles between "send alert" (idle) and "cancel alert" (active).
#[component]
pub fn AlertButton(
    ws: ReadSignal<Option<web_sys::WebSocket>>,
    /// Whether this member has an active alert
    active: ReadSignal<bool>,
) -> impl IntoView {
    let on_click = move |_| {
        if let Some(socket) = ws.get_untracked() {
            if socket.ready_state() == web_sys::WebSocket::OPEN {
                let cmd = if active.get_untracked() {
                    serde_json::to_string(&iem_core::ClientMsg::ClearAlert)
                } else {
                    serde_json::to_string(&iem_core::ClientMsg::CallEngineer)
                };
                if let Ok(json) = cmd {
                    let _ = socket.send_with_str(&json);
                }
            }
        }

        // Vibrate to confirm action
        if let Some(window) = web_sys::window() {
            let _ = window.navigator().vibrate_with_duration(100);
        }
    };

    view! {
        <button
            class="alert-btn"
            class:active=move || active.get()
            on:click=on_click
        >
            {move || {
                if active.get() {
                    "SOS Active".to_string()
                } else {
                    "SOS".to_string()
                }
            }}
        </button>
    }
}
```

- [ ] **Step 2: Add `alert_active` signal and update `AlertButton` usage in mixer.rs**

In `iem-mixer/iem-ui/src/pages/mixer.rs`:

a) Add `alert_active` signal next to the existing `alert_data` signal (around line 420):

```rust
    // Whether this member has an active SOS alert
    let (alert_active, set_alert_active) = signal(false);
```

b) Update `connect_websocket` signature (line 100) — add after `set_alert_data`:

```rust
    set_alert_active: WriteSignal<bool>,
```

c) In the WS `EngineerAlert` handler (line 277-282), add after `set_alert_data.set(...)`:

```rust
                    iem_core::ServerMsg::EngineerAlert {
                        from_member,
                        from_name,
                    } => {
                        set_alert_data.set(Some((from_member.clone(), from_name)));
                        // Member receiving their own alert confirmation
                        set_alert_active.set(true);
                    }
```

d) Add handlers for `AlertCleared` and `ActiveAlerts` after the `EngineerAlert` handler:

```rust
                    iem_core::ServerMsg::AlertCleared { member_id: cleared } => {
                        set_alert_active.set(false);
                        // Remove from engineer toast if this was the displayed alert
                        if let Some((ref m, _)) = alert_data_read.get_untracked() {
                            if *m == cleared {
                                set_alert_data.set(None);
                            }
                        }
                    }
                    iem_core::ServerMsg::ActiveAlerts { alerts } => {
                        // Engineer reconnect: show first pending alert
                        if let Some(first) = alerts.first() {
                            set_alert_data.set(Some((
                                first.from_member.clone(),
                                first.from_name.clone(),
                            )));
                        }
                    }
```

Note: `alert_data_read` needs to be a `ReadSignal` — use `alert_data` (the read half of the signal).

e) Pass `set_alert_active` to both `connect_websocket` call sites (lines 470 and 537):

```rust
            set_alert_active,
```

f) Update `<AlertButton>` in toolbar — pass `active` prop. In `iem-mixer/iem-ui/src/components/toolbar.rs`, update the AlertButton render:

```rust
            {(!is_engineer && ws.is_some()).then(|| {
                let ws_sig = ws.unwrap();
                view! {
                    <AlertButton ws=ws_sig active=alert_active />
                }
            })}
```

This requires `alert_active` to be passed to `Toolbar` as a new prop:

```rust
    #[prop(optional)]
    alert_active: Option<ReadSignal<bool>>,
```

And the render becomes:

```rust
            {(!is_engineer && ws.is_some() && alert_active.is_some()).then(|| {
                let ws_sig = ws.unwrap();
                let active = alert_active.unwrap();
                view! {
                    <AlertButton ws=ws_sig active=active />
                }
            })}
```

Pass from mixer.rs `<Toolbar>`:

```rust
            <Toolbar
                ...existing props...
                alert_active=alert_active
            />
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/iem-ui/src/components/alert_button.rs iem-mixer/iem-ui/src/pages/mixer.rs iem-mixer/iem-ui/src/components/toolbar.rs
git commit -m "feat: alert button toggles between SOS and Active state"
```

---

### Task 5: Rewrite alert toast — persistent with vibration loop, notification, embedded sound

**Files:**
- Rewrite: `iem-mixer/iem-ui/src/components/alert_toast.rs`

- [ ] **Step 1: Rewrite `AlertToast` component**

Replace the entire content of `iem-mixer/iem-ui/src/components/alert_toast.rs`:

```rust
//! Engineer alert toast — persistent until cleared (#125)

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

/// Persistent alert toast for engineer.
/// Vibrates every 3s, plays subtle chime every 10s, shows system notification.
/// Stays until engineer clicks dismiss (sends ClearAlert via WS).
#[component]
pub fn AlertToast(
    alert: ReadSignal<Option<(String, String)>>,
    ws: ReadSignal<Option<web_sys::WebSocket>>,
) -> impl IntoView {
    // Start/stop vibration loop and sound loop when alert changes
    Effect::new(move || {
        let current = alert.get();
        if let Some((_, ref name)) = current {
            // System notification (ask permission if needed)
            let name_clone = name.clone();
            wasm_bindgen_futures::spawn_local(async move {
                request_and_notify(&name_clone).await;
            });

            // Start vibration loop (every 3s)
            let vib_cb = Closure::wrap(Box::new(move || {
                if let Some(window) = web_sys::window() {
                    let _ = window.navigator().vibrate_with_duration(200);
                }
            }) as Box<dyn FnMut()>);
            if let Some(window) = web_sys::window() {
                let id = window
                    .set_interval_with_callback_and_timeout_and_arguments_0(
                        vib_cb.as_ref().unchecked_ref(),
                        3000,
                    )
                    .unwrap_or(0);
                // Store interval ID for cleanup
                let _ = js_sys::Reflect::set(
                    &window,
                    &JsValue::from_str("__iem_alert_vib"),
                    &JsValue::from(id),
                );
                // Initial vibrate
                let _ = window.navigator().vibrate_with_duration(200);
            }
            vib_cb.forget();

            // Start sound loop (play chime, repeat every 10s)
            play_chime();
            let sound_cb = Closure::wrap(Box::new(move || {
                play_chime();
            }) as Box<dyn FnMut()>);
            if let Some(window) = web_sys::window() {
                let id = window
                    .set_interval_with_callback_and_timeout_and_arguments_0(
                        sound_cb.as_ref().unchecked_ref(),
                        10_000,
                    )
                    .unwrap_or(0);
                let _ = js_sys::Reflect::set(
                    &window,
                    &JsValue::from_str("__iem_alert_snd"),
                    &JsValue::from(id),
                );
            }
            sound_cb.forget();
        } else {
            // Alert cleared — stop loops
            stop_loops();
        }
    });

    let on_dismiss = move |_| {
        // Send ClearAlert via WS
        if let Some(socket) = ws.get_untracked() {
            if socket.ready_state() == web_sys::WebSocket::OPEN {
                let cmd =
                    serde_json::to_string(&iem_core::ClientMsg::ClearAlert).unwrap_or_default();
                let _ = socket.send_with_str(&cmd);
            }
        }
    };

    view! {
        <Show when=move || alert.get().is_some()>
            <div class="alert-toast">
                <div class="alert-toast-content">
                    <span class="alert-toast-icon">"!"</span>
                    <span class="alert-toast-text">
                        {move || {
                            alert.get()
                                .map(|(_, name)| format!("{} needs help!", name))
                                .unwrap_or_default()
                        }}
                    </span>
                    <button class="alert-toast-dismiss" on:click=on_dismiss>"OK"</button>
                </div>
            </div>
        </Show>
    }
}

fn play_chime() {
    if let Some(window) = web_sys::window() {
        let audio = web_sys::HtmlAudioElement::new_with_src("/alert.mp3").ok();
        if let Some(audio) = audio {
            audio.set_volume(0.15);
            let _ = audio.play();
        }
    }
}

fn stop_loops() {
    if let Some(window) = web_sys::window() {
        // Clear vibration interval
        if let Ok(val) = js_sys::Reflect::get(&window, &JsValue::from_str("__iem_alert_vib")) {
            if let Some(id) = val.as_f64() {
                window.clear_interval_with_handle(id as i32);
            }
        }
        // Clear sound interval
        if let Ok(val) = js_sys::Reflect::get(&window, &JsValue::from_str("__iem_alert_snd")) {
            if let Some(id) = val.as_f64() {
                window.clear_interval_with_handle(id as i32);
            }
        }
        // Stop vibration
        let _ = window.navigator().vibrate_with_duration(0);
    }
}

async fn request_and_notify(name: &str) {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    // Request notification permission if not granted
    if let Ok(perm) = js_sys::Reflect::get(&window, &JsValue::from_str("Notification")) {
        if let Ok(permission) = js_sys::Reflect::get(&perm, &JsValue::from_str("permission")) {
            if permission.as_string().as_deref() != Some("granted") {
                // Request permission
                let promise = js_sys::Reflect::get(&perm, &JsValue::from_str("requestPermission"))
                    .ok()
                    .and_then(|f| f.dyn_ref::<js_sys::Function>().cloned());
                if let Some(func) = promise {
                    let result = func.call0(&perm).ok();
                    if let Some(p) = result.and_then(|v| v.dyn_into::<js_sys::Promise>().ok()) {
                        let _ = wasm_bindgen_futures::JsFuture::from(p).await;
                    }
                }
            }
        }
    }
    // Show notification
    let opts = web_sys::NotificationOptions::new();
    opts.set_body(&format!("{} needs help!", name));
    opts.set_require_interaction(true);
    let _ = web_sys::Notification::new_with_options(&format!("IEM Alert: {}", name), &opts);
}
```

- [ ] **Step 2: Add required web-sys features to Cargo.toml**

In `iem-mixer/iem-ui/Cargo.toml`, add to the `web-sys` features list:

```toml
    "HtmlAudioElement",
    "HtmlMediaElement",
    "Notification",
    "NotificationOptions",
    "NotificationPermission",
```

- [ ] **Step 3: Update mixer.rs AlertToast usage**

The `AlertToast` component now takes `ws` instead of `set_alert` prop. In `iem-mixer/iem-ui/src/pages/mixer.rs`, update the render (around line 991):

```rust
            <AlertToast alert=alert_data ws=ws />
```

- [ ] **Step 4: Commit**

```bash
git add iem-mixer/iem-ui/src/components/alert_toast.rs iem-mixer/iem-ui/Cargo.toml iem-mixer/iem-ui/src/pages/mixer.rs
git commit -m "feat: persistent alert toast with vibration loop, chime, and notification"
```

---

### Task 6: Update CSS for active alert button

**Files:**
- Modify: `iem-mixer/iem-ui/style.css`

- [ ] **Step 1: Replace `.alert-btn.cooldown` with `.alert-btn.active`**

In `iem-mixer/iem-ui/style.css`, replace the `.alert-btn.cooldown` block:

```css
.alert-btn.cooldown {
  background: #5d4037;
  border-color: #795548;
  color: #bcaaa4;
  cursor: not-allowed;
  opacity: 0.7;
}
```

with:

```css
.alert-btn.active {
  background: #d32f2f;
  border-color: #ff5252;
  color: #fff;
  animation: alert-pulse 1.5s ease-in-out infinite;
}

@keyframes alert-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}
```

- [ ] **Step 2: Commit**

```bash
git add iem-mixer/iem-ui/style.css
git commit -m "feat: pulsing active state for SOS button"
```

---

### Task 7: Update E2E tests for persistent alert behavior

**Files:**
- Rewrite: `iem-mixer/e2e/tests/alert.spec.ts`

- [ ] **Step 1: Replace test "alert button shows cooldown after click"**

Replace the cooldown test (lines 68-95) with a persistent-active test:

```typescript
  test("alert button shows active state after click (no countdown)", async ({
    page,
  }) => {
    await page.goto("/");
    const membersResp = await page.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) return;

    const member = members[0].id;
    await loginAs(page, member);
    await page.goto(`/${member}`);
    if (!(await waitForMixer(page))) return;

    const alertBtn = page.locator(".alert-btn");
    const btnVisible = await alertBtn
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(btnVisible, "alert button must be visible")) return;

    // Click SOS
    await alertBtn.click({ force: true });
    await page.waitForTimeout(500);

    // Button should show active state (not disabled, has "active" class)
    const hasActive = (await alertBtn.getAttribute("class"))?.includes("active");
    if (!assume(hasActive, "button must show active state (requires server)")) return;

    // Button should NOT be disabled (it's a toggle)
    const isEnabled = !(await alertBtn.isDisabled());
    expect(isEnabled).toBeTruthy();

    // Button text should indicate active
    const text = await alertBtn.textContent();
    expect(text).toContain("Active");
  });
```

- [ ] **Step 2: Update two-browser test for persistent dismiss**

Replace the engineer toast test (lines 97-164) with a test that verifies persistence + dismiss:

```typescript
  test("alert persists until engineer dismisses", async ({ browser }) => {
    const ctx1 = await browser.newContext();
    const ctx2 = await browser.newContext();
    const memberPage = await ctx1.newPage();
    const engineerPage = await ctx2.newPage();

    await memberPage.goto("/");
    const membersResp = await memberPage.request.get("/api/members");
    const members = await membersResp.json();
    if (!assume(members.length >= 1, "Need at least 1 member")) {
      await ctx1.close();
      await ctx2.close();
      return;
    }

    const member = members[0];
    await loginAs(memberPage, member.id);
    await memberPage.goto(`/${member.id}`);
    if (!(await waitForMixer(memberPage))) {
      await ctx1.close();
      await ctx2.close();
      return;
    }

    await engineerPage.goto("/");
    await loginAs(engineerPage, "engineer", "1177");
    await engineerPage.goto("/engineer");
    if (!(await waitForMixer(engineerPage))) {
      await ctx1.close();
      await ctx2.close();
      return;
    }

    await memberPage.waitForTimeout(1000);
    await engineerPage.waitForTimeout(1000);

    // Member clicks SOS
    const alertBtn = memberPage.locator(".alert-btn");
    const btnVisible = await alertBtn
      .waitFor({ state: "visible", timeout: 5000 })
      .catch(() => null);
    if (!assume(btnVisible, "alert button must be visible")) {
      await ctx1.close();
      await ctx2.close();
      return;
    }
    await alertBtn.click({ force: true });

    // Engineer sees toast
    const toast = engineerPage.locator(".alert-toast");
    await expect(toast).toBeVisible({ timeout: 5000 });

    // Wait 6 seconds — toast must STILL be visible (no auto-dismiss)
    await engineerPage.waitForTimeout(6000);
    await expect(toast).toBeVisible();

    // Engineer dismisses
    const dismissBtn = engineerPage.locator(".alert-toast-dismiss");
    await dismissBtn.click({ force: true });

    // Toast disappears
    await expect(toast).not.toBeVisible({ timeout: 3000 });

    // Member button returns to idle (not active)
    await memberPage.waitForTimeout(1000);
    const memberBtnClass = await alertBtn.getAttribute("class");
    expect(memberBtnClass).not.toContain("active");

    await ctx1.close();
    await ctx2.close();
  });
```

- [ ] **Step 3: Commit**

```bash
git add iem-mixer/e2e/tests/alert.spec.ts
git commit -m "test: update alert E2E for persistent behavior"
```

---

### Task 8: Lint, push, CI

- [ ] **Step 1: Run formatting and clippy locally (via CI push)**

Review all changes holistically. Ensure no dead code, no unused imports. Push to dev:

```bash
git push origin dev
```

- [ ] **Step 2: Monitor CI**

```bash
gh run list --branch dev --limit 2
# Wait for completion
gh run view <run-id> --json status,conclusion,jobs
```

All jobs must pass. If lint/clippy fails, fix in a single follow-up commit.

- [ ] **Step 3: Create PR**

```bash
gh pr create --title "feat: persistent alert with subtle sound + vibration" --body "..."
```

Monitor PR CI to completion. Report green PR URL.
