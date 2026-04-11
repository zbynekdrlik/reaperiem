//! Mixer page with faders, meters, categories, and presets
//!
//! Uses WebSocket for real-time bidirectional communication with REAPER.

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

use crate::api::Channel;
use crate::components::alert_toast::AlertToast;
use crate::components::category_tabs::{Category, CategoryTabs};
use crate::components::eq_modal::{EQModal, EqBandState};
use crate::components::fader::Fader;
use crate::components::limiter_modal::LimiterModal;
use crate::components::meter::Meter;
use crate::components::pan::PanKnob;
use crate::components::pin_change_modal::PinChangeModal;
use crate::components::preset_modal::{ChannelState, PresetData, PresetModal};
use crate::components::settings_modal::{SettingsModal, UserSettings};
use crate::components::snapshot_modal::SnapshotModal;
use crate::components::talk_button::TalkState;
use crate::components::toolbar::Toolbar;

/// Post-release guard duration in milliseconds.
/// With server-side echo suppression, this only needs to cover WebSocket round-trip (~10-20ms).
const POST_RELEASE_GUARD_MS: i32 = 100;

/// Minimum interval between WebSocket sends per track (ms).
/// Limits to ~20 commands/sec to avoid overwhelming the server.
const THROTTLE_INTERVAL_MS: f64 = 50.0;

/// Processed channel for display (handles stereo pairs)
/// Note: level_db, pan, muted are read via derived signals from channels
#[derive(Debug, Clone, PartialEq)]
struct DisplayChannel {
    track_index: usize,
    display_name: String,
    is_stereo: bool,
    partner_index: Option<usize>,
    is_my_input: bool,
}

/// Send a command via WebSocket (synchronous, non-blocking)
fn ws_send(ws: ReadSignal<Option<web_sys::WebSocket>>, cmd: &iem_core::ClientMsg) {
    if let Some(ws) = ws.get_untracked() {
        if ws.ready_state() == web_sys::WebSocket::OPEN {
            if let Ok(json) = serde_json::to_string(cmd) {
                let _ = ws.send_with_str(&json);
            }
        }
    }
}

/// Storage for WebSocket closures to prevent memory leaks on reconnect.
/// Dropping a Closure that was passed to JS via `as_ref().unchecked_ref()` properly
/// releases the WASM-side allocation. Without this, `Closure::forget()` leaks on every reconnect.
/// Uses Rc<RefCell<>> because wasm_bindgen::Closure is !Send (WASM is single-threaded).
type WsClosures = (
    Closure<dyn FnMut(web_sys::MessageEvent)>,
    Closure<dyn FnMut(web_sys::CloseEvent)>,
);
type WsClosureStore = std::rc::Rc<std::cell::RefCell<Option<WsClosures>>>;

/// Counter for consecutive WebSocket failures without receiving data.
/// Shared across connect_websocket calls via Rc<Cell<>>.
type WsFailCounter = std::rc::Rc<std::cell::Cell<u32>>;

/// Max consecutive WS failures before redirecting to login
const MAX_WS_FAILURES: u32 = 3;

/// Create and connect a WebSocket, wiring up message handlers to signals
#[allow(clippy::too_many_arguments)]
fn connect_websocket(
    member: &str,
    last_frame_at: std::rc::Rc<std::cell::Cell<f64>>,
    reconnect_attempt: std::rc::Rc<std::cell::Cell<u32>>,
    ws: ReadSignal<Option<web_sys::WebSocket>>,
    set_ws: WriteSignal<Option<web_sys::WebSocket>>,
    set_channels: WriteSignal<Vec<Channel>>,
    set_meters: WriteSignal<HashMap<usize, [f32; 2]>>,
    set_connected: WriteSignal<bool>,
    set_loading: WriteSignal<bool>,
    fader_touched: ReadSignal<HashMap<usize, bool>>,
    set_global_level: WriteSignal<f32>,
    set_global_muted: WriteSignal<bool>,
    global_touched: ReadSignal<bool>,
    set_data_pulse: WriteSignal<bool>,
    set_pinned_channels: WriteSignal<Vec<usize>>,
    set_hidden_channels: WriteSignal<Vec<usize>>,
    set_network_mode: WriteSignal<String>,
    set_output_track_idx: WriteSignal<Option<usize>>,
    set_soloed: WriteSignal<std::collections::HashSet<usize>>,
    set_pre_solo_mutes: WriteSignal<HashMap<usize, bool>>,
    channels: ReadSignal<Vec<Channel>>,
    soloed: ReadSignal<std::collections::HashSet<usize>>,
    ws_closures: WsClosureStore,
    ws_fail_count: WsFailCounter,
    set_stems_level: WriteSignal<f32>,
    set_stems_muted: WriteSignal<bool>,
    stems_touched: ReadSignal<bool>,
    set_stems_bus_idx: WriteSignal<Option<usize>>,
    set_eq_bands: WriteSignal<Vec<EqBandState>>,
    set_eq_loading: WriteSignal<bool>,
    // Limiter signals (#72) — single "max level" control
    set_limiter_limit_db: WriteSignal<f32>,
    set_limiter_limit_norm: WriteSignal<f32>,
    set_limiter_enabled: WriteSignal<bool>,
    set_limiter_loading: WriteSignal<bool>,
    set_alert_data: WriteSignal<Option<(String, String)>>,
    alert_data: ReadSignal<Option<(String, String)>>,
    set_alert_active: WriteSignal<bool>,
    set_talk_state: WriteSignal<TalkState>,
    set_engineer_talking: WriteSignal<bool>,
    page_visible: std::rc::Rc<std::cell::Cell<bool>>,
) {
    // Close previous WebSocket if exists (prevents closure leak on reconnect)
    if let Some(Some(old_ws)) = ws.try_get_untracked() {
        old_ws.set_onmessage(None);
        old_ws.set_onclose(None);
        old_ws.set_onerror(None);
        let _ = old_ws.close();
    }

    // Initialize last_frame_at to now — the onmessage handler will refresh it
    // on every received frame, and the watchdog (in MixerPage) force-closes the
    // socket if it falls more than 30s behind. #153
    last_frame_at.set(js_sys::Date::now());

    let location = web_sys::window().unwrap().location();
    let host = location.host().unwrap_or_default();
    let protocol = if location.protocol().unwrap_or_default() == "https:" {
        "wss"
    } else {
        "ws"
    };
    // Include JWT token in WebSocket URL for authentication
    let token = crate::auth::get_token().unwrap_or_default();
    let ws_url = format!("{}://{}/ws/{}?token={}", protocol, host, member, token);

    let ws = match web_sys::WebSocket::new(&ws_url) {
        Ok(ws) => ws,
        Err(e) => {
            web_sys::console::error_1(&format!("WS connect error: {:?}", e).into());
            return;
        }
    };

    set_ws.set(Some(ws.clone()));

    // Expose WS to window for E2E test meter injection
    let _ = js_sys::Reflect::set(
        &web_sys::window().unwrap(),
        &wasm_bindgen::JsValue::from_str("__iem_ws"),
        &ws,
    );

    // Meter update throttle: skip updates arriving faster than 50ms apart.
    // Server sends every 150ms but network jitter can bunch messages.
    // This caps reactive signal storms at ~20/sec instead of unbounded.
    let last_meter_time = std::cell::Cell::new(0.0_f64);

    // Clone fail counter for use in closures
    let fail_count_msg = ws_fail_count.clone();
    let fail_count_close = ws_fail_count;

    let last_frame_at_msg = last_frame_at.clone();
    let reconnect_attempt_msg = reconnect_attempt.clone();

    // Handle incoming messages
    let onmessage = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        // Any received frame counts as "alive" — refresh watchdog and reset
        // the reconnect backoff counter. This runs on every frame (not just
        // the first); the Cell write is a no-op after the first reset, so
        // the extra cost is negligible and the code stays branch-free.
        last_frame_at_msg.set(js_sys::Date::now());
        reconnect_attempt_msg.set(0);
        if let Some(text) = e.data().as_string() {
            if let Ok(msg) = serde_json::from_str::<iem_core::ServerMsg>(&text) {
                let touched = fader_touched.get_untracked();
                match msg {
                    iem_core::ServerMsg::State {
                        channels: new_chs,
                        connected: conn,
                        global_level_db,
                        global_muted,
                        output_track_index,
                        stems_level_db,
                        stems_muted,
                        stems_bus_index,
                    } => {
                        // Successfully received data — reset failure counter
                        fail_count_msg.set(0);
                        let _ = set_channels.try_update(|chs| {
                            let touched_snapshot: std::collections::HashMap<usize, bool> =
                                touched.iter().map(|(k, v)| (*k, *v)).collect();
                            iem_core::merge_or_replace_channels(chs, new_chs, &touched_snapshot);
                        });
                        // Update global volume from initial state
                        if let Some(lvl) = global_level_db {
                            let _ = set_global_level.try_set(lvl);
                        }
                        if let Some(muted) = global_muted {
                            let _ = set_global_muted.try_set(muted);
                        }
                        if let Some(idx) = output_track_index {
                            let _ = set_output_track_idx.try_set(Some(idx));
                        }
                        if let Some(lvl) = stems_level_db {
                            let _ = set_stems_level.try_set(lvl);
                        }
                        if let Some(muted) = stems_muted {
                            let _ = set_stems_muted.try_set(muted);
                        }
                        if let Some(idx) = stems_bus_index {
                            let _ = set_stems_bus_idx.try_set(Some(idx));
                        }
                        let _ = set_connected.try_set(conn);
                        let _ = set_loading.try_set(false);
                    }
                    iem_core::ServerMsg::Meters { meters: m } => {
                        if !page_visible.get() {
                            return; // Skip meter updates when backgrounded
                        }
                        // Throttle: skip if less than 50ms since last meter update
                        let now = js_sys::Date::now();
                        if now - last_meter_time.get() >= 50.0 {
                            last_meter_time.set(now);
                            // Merge delta meters into existing map (server sends only changed values)
                            let _ = set_meters.try_update(|existing| {
                                for (k, v) in m {
                                    existing.insert(k, v);
                                }
                            });
                            let _ = set_data_pulse.try_update(|v| *v = !*v);
                        }
                    }
                    iem_core::ServerMsg::ChannelUpdate {
                        track_index,
                        level_db,
                        muted,
                        pan,
                    } => {
                        if !touched.get(&track_index).copied().unwrap_or(false) {
                            let _ = set_channels.try_update(|chs| {
                                if let Some(ch) =
                                    chs.iter_mut().find(|c| c.track_index == track_index)
                                {
                                    ch.level_db = level_db;
                                    ch.muted = muted;
                                    ch.pan = pan;
                                }
                            });
                        }
                    }
                    iem_core::ServerMsg::GlobalVolumeUpdate { level_db, muted } => {
                        if !global_touched.get_untracked() {
                            let _ = set_global_level.try_set(level_db);
                            let _ = set_global_muted.try_set(muted);
                        }
                    }
                    iem_core::ServerMsg::StemsVolumeUpdate { level_db, muted } => {
                        if !stems_touched.get_untracked() {
                            let _ = set_stems_level.try_set(level_db);
                            let _ = set_stems_muted.try_set(muted);
                        }
                    }
                    iem_core::ServerMsg::ConnectionChanged { connected: conn } => {
                        let _ = set_connected.try_set(conn);
                    }
                    iem_core::ServerMsg::CustomizationUpdate { pinned, hidden } => {
                        let _ = set_pinned_channels.try_set(pinned);
                        let _ = set_hidden_channels.try_set(hidden);
                    }
                    iem_core::ServerMsg::NetworkMode { mode } => {
                        let _ = set_network_mode.try_set(mode);
                    }
                    iem_core::ServerMsg::SoloUpdate { soloed: new_solo } => {
                        let new_soloed: std::collections::HashSet<usize> =
                            new_solo.into_iter().collect();
                        let current = soloed.get_untracked();
                        // Skip echo from our own command
                        if new_soloed != current {
                            if new_soloed.is_empty() && !current.is_empty() {
                                // Remote un-soloed all: clear pre-solo mutes
                                let _ = set_pre_solo_mutes.try_set(HashMap::new());
                            } else if !new_soloed.is_empty() && current.is_empty() {
                                // Remote entered solo: save current mute states for restore
                                let chs = channels.get_untracked();
                                let mut saved = HashMap::new();
                                for ch in &chs {
                                    saved.insert(ch.track_index, ch.muted);
                                }
                                let _ = set_pre_solo_mutes.try_set(saved);
                            } else if !new_soloed.is_empty() && !current.is_empty() {
                                // Remote exclusive switch: update local mute display (#131)
                                let _ = set_channels.try_update(|chs| {
                                    for c in chs.iter_mut() {
                                        c.muted = !new_soloed.contains(&c.track_index);
                                    }
                                });
                            }
                            let _ = set_soloed.try_set(new_soloed);
                        }
                    }
                    iem_core::ServerMsg::AudioStatus { .. } => {
                        // Audio status handled by ListenButton's own audio WebSocket
                    }
                    iem_core::ServerMsg::EngineerAlert {
                        from_member,
                        from_name,
                    } => {
                        let _ = set_alert_data.try_set(Some((from_member.clone(), from_name)));
                        let _ = set_alert_active.try_set(true);
                    }
                    iem_core::ServerMsg::AlertCleared { member_id: cleared } => {
                        let _ = set_alert_active.try_set(false);
                        if let Some((ref m, _)) = alert_data.get_untracked() {
                            if *m == cleared {
                                let _ = set_alert_data.try_set(None);
                            }
                        }
                    }
                    iem_core::ServerMsg::ActiveAlerts { alerts } => {
                        if let Some(first) = alerts.first() {
                            let _ = set_alert_data.try_set(Some((
                                first.from_member.clone(),
                                first.from_name.clone(),
                            )));
                        }
                    }
                    iem_core::ServerMsg::TalkAcquired => {
                        let _ = set_talk_state.try_set(TalkState::Live);
                    }
                    iem_core::ServerMsg::TalkBusy { .. } => {
                        let _ = set_talk_state.try_set(TalkState::InUse);
                    }
                    iem_core::ServerMsg::TalkReleased => {
                        let _ = set_talk_state.try_set(TalkState::Idle);
                    }
                    iem_core::ServerMsg::EngineerTalking { active } => {
                        // Red page overlay on band member devices (no vibration)
                        if let Some(window) = web_sys::window() {
                            if let Some(doc) = window.document() {
                                if let Some(body) = doc.body() {
                                    if active {
                                        let _ = body.class_list().add_1("talk-live-overlay");
                                    } else {
                                        let _ = body.class_list().remove_1("talk-live-overlay");
                                    }
                                }
                            }
                        }
                        let _ = set_engineer_talking.try_set(active);
                    }
                    iem_core::ServerMsg::EqParams {
                        track_index: _,
                        track_name: _,
                        bands,
                    } => {
                        let _ = set_eq_bands.try_set(
                            bands
                                .into_iter()
                                .map(|b| EqBandState {
                                    band_type: b.band_type,
                                    freq_hz: b.freq_hz,
                                    gain_db: b.gain_db,
                                    bw: b.bw,
                                    freq_norm: b.freq_norm,
                                    gain_norm: b.gain_norm,
                                    bw_norm: b.bw_norm,
                                    enabled: b.enabled,
                                })
                                .collect(),
                        );
                        let _ = set_eq_loading.try_set(false);
                    }
                    iem_core::ServerMsg::EqParamsMulti { .. } => {
                        // Handled by preset modal (future integration)
                    }
                    iem_core::ServerMsg::LimiterParams {
                        track_index: _,
                        track_name: _,
                        limit_db,
                        limit_norm,
                        enabled,
                    } => {
                        let _ = set_limiter_limit_db.try_set(limit_db);
                        let _ = set_limiter_limit_norm.try_set(limit_norm);
                        let _ = set_limiter_enabled.try_set(enabled);
                        let _ = set_limiter_loading.try_set(false);
                    }
                }
            }
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

    let reconnect_attempt_close = reconnect_attempt.clone();

    // Handle close — mark disconnected and increment failure counter
    let onclose = Closure::wrap(Box::new(move |_: web_sys::CloseEvent| {
        let _ = set_connected.try_set(false);
        fail_count_close.set(fail_count_close.get() + 1);
        reconnect_attempt_close.set(reconnect_attempt_close.get() + 1);
    }) as Box<dyn FnMut(web_sys::CloseEvent)>);
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));

    // Store closures so they stay alive (preventing JS callback invalidation)
    // and get dropped on next reconnect (preventing memory leak from Closure::forget)
    *ws_closures.borrow_mut() = Some((onmessage, onclose));
}

/// Subscribe to Web Push for engineer SOS alerts (#133).
/// Fetches VAPID key, subscribes via Push API, sends subscription to server.
fn subscribe_to_push() {
    wasm_bindgen_futures::spawn_local(async move {
        // 1. Fetch VAPID public key from server
        let token = match crate::auth::get_token() {
            Some(t) => t,
            None => {
                web_sys::console::log_1(&"[push] no auth token, skipping".into());
                return;
            }
        };

        let resp = match gloo_net::http::Request::get("/api/push/vapid-key")
            .send()
            .await
        {
            Ok(r) if r.ok() => r,
            Ok(r) => {
                web_sys::console::warn_1(
                    &format!("[push] vapid-key request failed: {}", r.status()).into(),
                );
                return;
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] vapid-key fetch error: {:?}", e).into());
                return;
            }
        };
        let json: serde_json::Value = match resp.json().await {
            Ok(j) => j,
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] vapid-key parse error: {:?}", e).into());
                return;
            }
        };
        let vapid_key = match json.get("key").and_then(|k| k.as_str()) {
            Some(k) => k.to_string(),
            None => {
                web_sys::console::log_1(&"[push] VAPID not configured, skipping".into());
                return;
            }
        };
        web_sys::console::log_1(&format!("[push] got VAPID key: {}...", &vapid_key[..20]).into());

        // 2. Get ServiceWorkerRegistration
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let navigator = window.navigator();
        let sw_container: web_sys::ServiceWorkerContainer = match js_sys::Reflect::get(
            &navigator,
            &wasm_bindgen::JsValue::from_str("serviceWorker"),
        )
        .ok()
        .and_then(|v| v.dyn_into().ok())
        {
            Some(c) => c,
            None => {
                web_sys::console::log_1(&"[push] serviceWorker not available, skipping".into());
                return;
            }
        };

        let ready_promise = match sw_container.ready() {
            Ok(p) => p,
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] sw.ready() failed: {:?}", e).into());
                return;
            }
        };
        web_sys::console::log_1(&"[push] waiting for SW ready...".into());
        let registration: web_sys::ServiceWorkerRegistration =
            match wasm_bindgen_futures::JsFuture::from(ready_promise).await {
                Ok(r) => match r.dyn_into() {
                    Ok(reg) => reg,
                    Err(e) => {
                        web_sys::console::warn_1(
                            &format!("[push] SW registration cast failed: {:?}", e).into(),
                        );
                        return;
                    }
                },
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("[push] SW ready await failed: {:?}", e).into(),
                    );
                    return;
                }
            };
        web_sys::console::log_1(&"[push] SW ready, getting push manager...".into());

        // 3. Subscribe to push
        let push_manager = match registration.push_manager() {
            Ok(pm) => pm,
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] push_manager() failed: {:?}", e).into());
                return;
            }
        };

        // Unsubscribe any existing push subscription first (required when VAPID key changes,
        // otherwise Chrome rejects subscribe() with a different applicationServerKey)
        if let Ok(existing_promise) = push_manager.get_subscription() {
            if let Ok(existing_val) = wasm_bindgen_futures::JsFuture::from(existing_promise).await {
                if !existing_val.is_null() && !existing_val.is_undefined() {
                    if let Ok(existing_sub) = existing_val.dyn_into::<web_sys::PushSubscription>() {
                        let _ = wasm_bindgen_futures::JsFuture::from(
                            existing_sub.unsubscribe().unwrap_or_else(|_| {
                                js_sys::Promise::resolve(&wasm_bindgen::JsValue::TRUE)
                            }),
                        )
                        .await;
                        web_sys::console::log_1(
                            &"[push] unsubscribed old push subscription".into(),
                        );
                    }
                }
            }
        }

        // Decode base64url VAPID key to Uint8Array
        let key_bytes = match base64url_decode(&vapid_key) {
            Some(b) => b,
            None => return,
        };
        let key_array = js_sys::Uint8Array::new_with_length(key_bytes.len() as u32);
        key_array.copy_from(&key_bytes);

        let opts = web_sys::PushSubscriptionOptionsInit::new();
        opts.set_user_visible_only(true);
        opts.set_application_server_key(&key_array.into());

        let sub_promise = match push_manager.subscribe_with_options(&opts) {
            Ok(p) => p,
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] subscribe failed: {:?}", e).into());
                return;
            }
        };
        web_sys::console::log_1(&"[push] subscribing to push...".into());
        let sub: web_sys::PushSubscription = match wasm_bindgen_futures::JsFuture::from(sub_promise)
            .await
        {
            Ok(v) => match v.dyn_into() {
                Ok(s) => s,
                Err(e) => {
                    web_sys::console::warn_1(
                        &format!("[push] subscription cast failed: {:?}", e).into(),
                    );
                    return;
                }
            },
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] subscribe await failed: {:?}", e).into());
                return;
            }
        };

        // 4. Send subscription JSON to server
        let sub_json = match sub.to_json() {
            Ok(j) => j,
            Err(_) => return,
        };
        let json_str = match js_sys::JSON::stringify(&sub_json)
            .ok()
            .and_then(|s| s.as_string())
        {
            Some(s) => s,
            None => return,
        };

        // Parse the JSON string to a serde_json::Value for gloo_net
        let body: serde_json::Value = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(_) => return,
        };

        let req = match gloo_net::http::Request::post("/api/push/subscribe")
            .header("Authorization", &format!("Bearer {}", token))
            .json(&body)
        {
            Ok(r) => r,
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] serialize error: {:?}", e).into());
                return;
            }
        };
        match req.send().await {
            Ok(r) if r.ok() => {
                web_sys::console::log_1(&"[push] engineer subscribed to Web Push".into());
            }
            Ok(r) => {
                web_sys::console::warn_1(
                    &format!("[push] subscribe POST failed: {}", r.status()).into(),
                );
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("[push] subscribe POST error: {:?}", e).into());
            }
        }
    });
}

/// Decode base64url (no padding) to bytes.
/// Note: atob() returns a Latin-1 string (each char = one byte 0-255).
/// Rust's `.bytes()` gives UTF-8 which mangles values > 127. Use `.chars() as u8` instead.
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut s = input.replace('-', "+").replace('_', "/");
    while s.len() % 4 != 0 {
        s.push('=');
    }
    web_sys::window()?
        .atob(&s)
        .ok()
        .map(|decoded| decoded.chars().map(|c| c as u8).collect())
}

/// Mixer page for a specific member
#[component]
pub fn MixerPage() -> impl IntoView {
    let params = use_params_map();
    let navigate_back = use_navigate();
    let navigate_to_login = use_navigate();

    // Auth guard: check token expiry AND cross-member access
    Effect::new(move |_| {
        let member = params
            .get()
            .get("member")
            .map(|s| s.to_string())
            .unwrap_or_default();

        if crate::auth::is_token_expired() {
            // Token expired or missing - redirect to login
            if !member.is_empty() {
                let login_url = format!("/login?member={}&next=/{}", member, member);
                navigate_to_login(&login_url, Default::default());
            } else {
                navigate_to_login("/", Default::default());
            }
            return;
        }

        // Cross-member access check: only allow access to own mixer (or engineer)
        if let Some(auth) = crate::auth::get_auth() {
            if !auth.engineer && auth.member != member && !member.is_empty() {
                // Clear stale auth and redirect to login for the target member
                crate::auth::clear_auth();
                let login_url = format!("/login?member={}&next=/{}", member, member);
                navigate_to_login(&login_url, Default::default());
            }
        }
    });

    // Get member ID from route params
    let member_id = move || {
        params
            .get()
            .get("member")
            .map(|s| s.to_string())
            .unwrap_or_default()
    };

    // Reactive state
    let (channels, set_channels) = signal(Vec::<Channel>::new());
    let (meters, set_meters) = signal(HashMap::<usize, [f32; 2]>::new());
    let (connected, set_connected) = signal(false);
    let (active_category, set_active_category) = signal(Category::Main);
    let (preset_modal_visible, set_preset_modal_visible) = signal(false);
    let (pin_modal_visible, set_pin_modal_visible) = signal(false);
    let (settings_modal_visible, set_settings_modal_visible) = signal(false);
    let (snapshot_modal_visible, set_snapshot_modal_visible) = signal(false);
    let (has_photo, set_has_photo) = signal(false);

    // Check if member has photo on mount (#16)
    // Use try_update to guard against disposal race — if the user navigates
    // away while `get_members` is still in flight, the await resumes on a
    // disposed signal and Leptos panics. See #153.
    {
        let mid = member_id();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(members) = crate::api::get_members().await {
                if let Some(m) = members.iter().find(|m| m.id == mid) {
                    let _ = set_has_photo.try_set(m.has_photo);
                }
            }
        });
    }

    // Load user settings from localStorage
    let user_settings = UserSettings::load(&member_id());
    let (double_tap_fader, set_double_tap_fader) = signal(user_settings.double_tap_fader);
    let (fader_touched, set_fader_touched) = signal(HashMap::<usize, bool>::new());
    let (loading, set_loading) = signal(true);
    let (soloed, set_soloed) = signal(std::collections::HashSet::<usize>::new());
    let (pre_solo_mutes, set_pre_solo_mutes) = signal(HashMap::<usize, bool>::new());

    // Status dot pulse — toggles on each Meters message to restart CSS animation
    let (data_pulse, set_data_pulse) = signal(false);

    // Global IEM output volume state
    let (global_level, set_global_level) = signal(0.0_f32);
    let (global_muted, set_global_muted) = signal(false);
    let (global_touched, set_global_touched) = signal(false);

    // Stems group bus volume state
    let (stems_level, set_stems_level) = signal(0.0_f32);
    let (stems_muted, set_stems_muted) = signal(false);
    let (stems_touched, set_stems_touched) = signal(false);
    let (stems_bus_idx, set_stems_bus_idx) = signal(Option::<usize>::None);

    // EQ modal state
    let (eq_open, set_eq_open) = signal(Option::<(usize, String)>::None);
    let (eq_bands, set_eq_bands) = signal(Vec::<EqBandState>::new());
    let (eq_loading, set_eq_loading) = signal(false);

    // Limiter modal state (#72) — single "max level" control
    let (limiter_open, set_limiter_open) = signal(Option::<(usize, String)>::None);
    let (limiter_limit_db, set_limiter_limit_db) = signal(-6.0_f32);
    let (limiter_limit_norm, set_limiter_limit_norm) = signal(0.0_f32);
    let (limiter_enabled, set_limiter_enabled) = signal(true);
    let (limiter_loading, set_limiter_loading) = signal(false);

    // Channel customization (pin/hide) — loaded from server via WS
    let (pinned_channels, set_pinned_channels) = signal(Vec::<usize>::new());
    let (hidden_channels, set_hidden_channels) = signal(Vec::<usize>::new());

    // Network mode indicator (local LAN vs remote internet)
    let (network_mode, set_network_mode) = signal(String::new());

    // Output track index for global volume metering (set from ServerMsg::State)
    let (output_track_idx, set_output_track_idx) = signal(Option::<usize>::None);

    // Alert data for engineer toast (member_id, display_name) (#125)
    let (alert_data, set_alert_data) = signal(Option::<(String, String)>::None);
    let (alert_active, set_alert_active) = signal(false);

    // Talkback state for engineer push-to-talk (#123)
    let (talk_state, set_talk_state) = signal(TalkState::Idle);
    // Engineer speaking indicator for band members (#123)
    let (engineer_talking, set_engineer_talking) = signal(false);

    // WebSocket connection
    let (ws, set_ws) = signal(Option::<web_sys::WebSocket>::None);

    // Closure storage: keeps WS callbacks alive without Closure::forget() leak
    let ws_closures: WsClosureStore = std::rc::Rc::new(std::cell::RefCell::new(None));

    // WS failure counter: tracks consecutive failures without receiving data
    let ws_fail_count: WsFailCounter = std::rc::Rc::new(std::cell::Cell::new(0));

    // Track page visibility — skip meter updates when backgrounded.
    // Created once in component body (NOT in connect_websocket) to avoid stacking listeners.
    let page_visible = std::rc::Rc::new(std::cell::Cell::new(true));
    {
        let pv = page_visible.clone();
        let vis_closure = Closure::wrap(Box::new(move || {
            if let Some(w) = web_sys::window() {
                if let Some(doc) = w.document() {
                    pv.set(!doc.hidden());
                }
            }
        }) as Box<dyn FnMut()>);
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let _ = doc.add_event_listener_with_callback(
                "visibilitychange",
                vis_closure.as_ref().unchecked_ref(),
            );
        }
        vis_closure.forget(); // Lives for component lifetime — one-time registration
    }

    // Watchdog + backoff state (#153). Lives in MixerPage so it survives across
    // reconnects — connect_websocket is called repeatedly by the reconnect loop
    // below, so these cells must NOT live inside that helper.
    // - last_frame_at: updated in onmessage, checked by the 5s watchdog
    // - reconnect_attempt: incremented in onclose, reset in onmessage, drives backoff
    // - last_reconnect_attempt_at: timestamp of last reconnect action, gates the
    //   backoff schedule inside the reconnect closure below
    let last_frame_at = std::rc::Rc::new(std::cell::Cell::new(js_sys::Date::now()));
    let reconnect_attempt = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let last_reconnect_attempt_at = std::rc::Rc::new(std::cell::Cell::new(0.0_f64));

    // Connect WebSocket when member is known
    let ws_member_id = member_id.clone();
    let ws_closures_effect = ws_closures.clone();
    let ws_fail_count_effect = ws_fail_count.clone();
    let page_visible_effect = page_visible.clone();
    let last_frame_at_effect = last_frame_at.clone();
    let reconnect_attempt_effect = reconnect_attempt.clone();
    Effect::new(move |_| {
        let member = ws_member_id();
        if member.is_empty() {
            return;
        }

        connect_websocket(
            &member,
            last_frame_at_effect.clone(),
            reconnect_attempt_effect.clone(),
            ws,
            set_ws,
            set_channels,
            set_meters,
            set_connected,
            set_loading,
            fader_touched,
            set_global_level,
            set_global_muted,
            global_touched,
            set_data_pulse,
            set_pinned_channels,
            set_hidden_channels,
            set_network_mode,
            set_output_track_idx,
            set_soloed,
            set_pre_solo_mutes,
            channels,
            soloed,
            ws_closures_effect.clone(),
            ws_fail_count_effect.clone(),
            set_stems_level,
            set_stems_muted,
            stems_touched,
            set_stems_bus_idx,
            set_eq_bands,
            set_eq_loading,
            set_limiter_limit_db,
            set_limiter_limit_norm,
            set_limiter_enabled,
            set_limiter_loading,
            set_alert_data,
            alert_data,
            set_alert_active,
            set_talk_state,
            set_engineer_talking,
            page_visible_effect.clone(),
        );
    });

    // Network mode (LAN/WAN) is now sent via WebSocket on every connect/reconnect,
    // so it automatically updates when switching between WiFi and mobile data.

    // Auto-reconnect: check every 2s if WebSocket is closed.
    // Uses raw JS setInterval to get an i32 handle (Send+Sync) for on_cleanup,
    // since gloo_timers::Interval contains non-Send closures.
    let reconnect_member_id = member_id.clone();
    let navigate_auth_fail = use_navigate();
    let reconnect_attempt_tick = reconnect_attempt.clone();
    let last_reconnect_attempt_at_tick = last_reconnect_attempt_at.clone();
    let last_frame_at_tick = last_frame_at.clone();
    let reconnect_closure = Closure::wrap(Box::new(move || {
        // Exponential backoff gate: skip this tick if the scheduled delay
        // hasn't elapsed since the last reconnect attempt. #153
        let now_ms = js_sys::Date::now();
        let attempt = reconnect_attempt_tick.get();
        let delay_ms = crate::lifecycle::backoff_delay_ms(attempt) as f64;
        let last_attempt = last_reconnect_attempt_at_tick.get();
        if last_attempt > 0.0 && (now_ms - last_attempt) < delay_ms {
            return;
        }

        let needs_reconnect = match ws.get_untracked() {
            Some(ref w) => w.ready_state() == web_sys::WebSocket::CLOSED,
            None => false,
        };
        if needs_reconnect {
            let member = reconnect_member_id();
            if member.is_empty() {
                return;
            }

            // After MAX_WS_FAILURES consecutive failures, check if token is invalid
            if ws_fail_count.get() >= MAX_WS_FAILURES {
                let nav = navigate_auth_fail.clone();
                let m = member.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if !crate::api::verify_token_valid(&m).await {
                        // Token rejected by server — clear auth and redirect to login
                        crate::auth::clear_auth();
                        let url = format!("/login?member={}&next=/{}", m, m);
                        nav(&url, Default::default());
                    }
                });
                return;
            }

            last_reconnect_attempt_at_tick.set(now_ms);
            connect_websocket(
                &member,
                last_frame_at_tick.clone(),
                reconnect_attempt_tick.clone(),
                ws,
                set_ws,
                set_channels,
                set_meters,
                set_connected,
                set_loading,
                fader_touched,
                set_global_level,
                set_global_muted,
                global_touched,
                set_data_pulse,
                set_pinned_channels,
                set_hidden_channels,
                set_network_mode,
                set_output_track_idx,
                set_soloed,
                set_pre_solo_mutes,
                channels,
                soloed,
                ws_closures.clone(),
                ws_fail_count.clone(),
                set_stems_level,
                set_stems_muted,
                stems_touched,
                set_stems_bus_idx,
                set_eq_bands,
                set_eq_loading,
                set_limiter_limit_db,
                set_limiter_limit_norm,
                set_limiter_enabled,
                set_limiter_loading,
                set_alert_data,
                alert_data,
                set_alert_active,
                set_talk_state,
                set_engineer_talking,
                page_visible.clone(),
            );
        }
    }) as Box<dyn FnMut()>);
    let interval_id = web_sys::window()
        .unwrap()
        .set_interval_with_callback_and_timeout_and_arguments_0(
            reconnect_closure.as_ref().unchecked_ref(),
            2000,
        )
        .unwrap();
    reconnect_closure.forget();

    // Watchdog (#153): every 5s, check whether the socket has received any
    // frame in the last 30s. If not, force-close it — onclose fires,
    // connected=false, the existing .disconnected-banner appears, and the
    // reconnect loop opens a new socket. Catches zombie sockets where
    // ready_state == OPEN but no data flows.
    let last_frame_at_watch = last_frame_at.clone();
    let ws_watch = ws;
    let watchdog_closure = Closure::wrap(Box::new(move || {
        let Some(socket) = ws_watch.get_untracked() else {
            return;
        };
        if socket.ready_state() != web_sys::WebSocket::OPEN {
            return;
        }
        let now = js_sys::Date::now();
        if crate::lifecycle::is_stale(
            last_frame_at_watch.get(),
            now,
            crate::lifecycle::WS_STALENESS_THRESHOLD_MS,
        ) {
            web_sys::console::warn_1(
                &"WS watchdog: no frames for >30s, force-closing socket".into(),
            );
            let _ = socket.close();
        }
    }) as Box<dyn FnMut()>);

    let watchdog_interval_id = web_sys::window()
        .unwrap()
        .set_interval_with_callback_and_timeout_and_arguments_0(
            watchdog_closure.as_ref().unchecked_ref(),
            crate::lifecycle::WS_WATCHDOG_INTERVAL_MS as i32,
        )
        .unwrap();
    watchdog_closure.forget();

    // Clean up reconnect interval on component unmount.
    // The i32 interval_id is Send+Sync so it can be captured in on_cleanup.
    // The WebSocket signal and its closures are dropped with the component.
    on_cleanup(move || {
        if let Some(w) = web_sys::window() {
            w.clear_interval_with_handle(interval_id);
            w.clear_interval_with_handle(watchdog_interval_id);
        }
    });

    // Periodic token expiry check (every 60 seconds).
    // Catches expired tokens while the page is open (engineer 4h, member 24h).
    let navigate_expired = use_navigate();
    let member_for_expiry = member_id.clone();
    let expiry_closure = Closure::wrap(Box::new(move || {
        if crate::auth::is_token_expired() {
            crate::auth::clear_auth();
            let m = member_for_expiry();
            if !m.is_empty() {
                let url = format!("/login?member={}&next=/{}", m, m);
                navigate_expired(&url, Default::default());
            } else {
                navigate_expired("/", Default::default());
            }
        }
    }) as Box<dyn FnMut()>);
    let expiry_interval_id = web_sys::window()
        .unwrap()
        .set_interval_with_callback_and_timeout_and_arguments_0(
            expiry_closure.as_ref().unchecked_ref(),
            60_000, // Check every 60 seconds
        )
        .unwrap();
    expiry_closure.forget();
    on_cleanup(move || {
        if let Some(w) = web_sys::window() {
            w.clear_interval_with_handle(expiry_interval_id);
        }
    });

    // Handle back button
    let on_back = move |_| {
        navigate_back("/", Default::default());
    };

    // Process channels for display (handle stereo pairs, pin/hide)
    // Memoized to avoid recomputation on every meter update
    let display_channels = Memo::new(move |_| {
        let chs = channels.get();
        let member = member_id();
        let my_input = format!("{} MIC", member.to_uppercase());
        let active_cat = active_category.get();
        let pinned = pinned_channels.get();
        let hidden = hidden_channels.get();

        let mut result = Vec::new();
        let mut seen_pairs: std::collections::HashSet<String> = std::collections::HashSet::new();

        for ch in &chs {
            if ch.stereo_side.as_deref() == Some("R") {
                continue;
            }

            let is_my_input = ch.name.to_uppercase() == my_input;
            let is_pinned = pinned.contains(&ch.track_index);
            let is_hidden = hidden.contains(&ch.track_index);

            // Hidden tab: only show hidden channels
            if active_cat == Category::Hidden {
                if !is_hidden {
                    continue;
                }
            } else if active_cat == Category::Main {
                // Main tab: show "me" channel + pinned channels
                // Hidden does NOT remove pinned channels from Main — only from category tabs
                if !is_my_input && !is_pinned {
                    continue;
                }
            } else {
                // Category tabs: filter by category, skip hidden
                if !active_cat.matches(&ch.category) {
                    continue;
                }
                if is_hidden {
                    continue;
                }
            }

            let (is_stereo, partner_index) = if ch.stereo_side.as_deref() == Some("L") {
                if let Some(ref pair_name) = ch.stereo_pair {
                    if seen_pairs.contains(pair_name) {
                        continue;
                    }
                    seen_pairs.insert(pair_name.clone());
                    let partner = chs
                        .iter()
                        .find(|c| {
                            c.stereo_pair.as_ref() == Some(pair_name)
                                && c.stereo_side.as_deref() == Some("R")
                        })
                        .map(|c| c.track_index);
                    (true, partner)
                } else {
                    (false, None)
                }
            } else {
                (false, None)
            };

            let display_name = if is_stereo {
                ch.name.trim_end_matches(" L").to_string()
            } else {
                ch.name.clone()
            };

            result.push(DisplayChannel {
                track_index: ch.track_index,
                display_name,
                is_stereo,
                partner_index,
                is_my_input,
            });
        }

        // Sort Stems: Click first, Guide second, then rest
        if active_cat == Category::Stems {
            result.sort_by_key(|ch| {
                let name_upper = ch.display_name.to_uppercase();
                if name_upper == "CLICK" {
                    0
                } else if name_upper == "GUIDE" {
                    1
                } else {
                    2
                }
            });
        }

        // Sort Main: own channel first, pinned channels after
        if active_cat == Category::Main {
            result.sort_by_key(|ch| if ch.is_my_input { 0 } else { 1 });
        }

        result
    });

    // Preset handlers
    let get_current_state = Callback::new(move |_: ()| {
        let chs = channels.get();
        let mut channel_states = HashMap::new();
        for ch in &chs {
            channel_states.insert(
                ch.track_index,
                ChannelState {
                    vol: ch.level_db,
                    mute: ch.muted,
                    pan: ch.pan,
                },
            );
        }
        let current_stems_level = if stems_bus_idx.get_untracked().is_some() {
            Some(stems_level.get_untracked())
        } else {
            None
        };
        // Include cached EQ data if the user has loaded EQ for a track
        let eq_data = {
            let bands = eq_bands.get_untracked();
            let open = eq_open.get_untracked();
            if !bands.is_empty() {
                if let Some((track_idx, _)) = open {
                    let mut eq_map = HashMap::new();
                    eq_map.insert(
                        track_idx,
                        bands
                            .iter()
                            .map(|b| crate::components::preset_modal::EqBandPreset {
                                band_type: b.band_type.clone(),
                                freq_hz: b.freq_hz,
                                gain_db: b.gain_db,
                                bw: b.bw,
                                freq_norm: b.freq_norm,
                                gain_norm: b.gain_norm,
                                bw_norm: b.bw_norm,
                            })
                            .collect(),
                    );
                    Some(eq_map)
                } else {
                    None
                }
            } else {
                None
            }
        };
        PresetData {
            channels: channel_states,
            created_at: None,
            updated_at: None,
            stems_level_db: current_stems_level,
            eq_bands: eq_data,
        }
    });

    let on_load_preset = Callback::new(move |preset: PresetData| {
        if !connected.get() {
            web_sys::console::warn_1(&"Preset loading blocked: not connected to REAPER".into());
            return;
        }

        // Update local state
        set_channels.update(|chs| {
            for ch in chs.iter_mut() {
                if let Some(state) = preset.channels.get(&ch.track_index) {
                    ch.level_db = state.vol;
                    ch.muted = state.mute;
                    ch.pan = state.pan;
                }
            }
        });

        // Send all changes via WebSocket
        for (track_index, state) in &preset.channels {
            ws_send(
                ws,
                &iem_core::ClientMsg::SetLevel {
                    track_index: *track_index,
                    level_db: state.vol,
                },
            );
            ws_send(
                ws,
                &iem_core::ClientMsg::SetMute {
                    track_index: *track_index,
                    muted: state.mute,
                },
            );
            ws_send(
                ws,
                &iem_core::ClientMsg::SetPan {
                    track_index: *track_index,
                    pan: state.pan,
                },
            );
        }
    });

    // Toolbar callbacks
    // Show engineer toolbar (Mute All + Listen) on any page when logged in as engineer
    let is_engineer = crate::auth::get_auth().map(|a| a.engineer).unwrap_or(false);
    // Mute All only on /engineer (engineer's own mixer)
    let is_engineer_own_mixer = is_engineer && member_id() == "engineer";

    // Subscribe to Web Push for SOS alerts (engineer only, one-time) (#133)
    if is_engineer {
        subscribe_to_push();
    }

    let on_presets = Callback::new(move |_: ()| {
        set_preset_modal_visible.set(true);
    });

    let on_history = Callback::new(move |_: ()| {
        set_snapshot_modal_visible.set(true);
    });

    let mute_all_member = member_id.clone();
    let on_mute_all = Callback::new(move |_: ()| {
        let member = mute_all_member();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = crate::api::batch_mute_all(&member).await {
                web_sys::console::error_1(&format!("Mute all failed: {}", e).into());
            }
        });
    });

    let on_close_modal = Callback::new(move |_: ()| {
        set_preset_modal_visible.set(false);
    });

    view! {
        <div class="app mixer">
            <header class="mixer-header">
                <button class="back-btn" on:click=on_back>
                    "\u{2190}"
                </button>
                <h1>{move || {
                    let m = member_id();
                    let mut chars = m.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(c) => c.to_uppercase().chain(chars).collect(),
                    }
                }}</h1>
                <Show
                    when=move || !soloed.get().is_empty()
                    fallback=|| view! {
                        <div class="header-version">
                            <span class="header-version-number">{iem_core::version_label()}</span>
                            <span class="header-version-date">{iem_core::build_datetime()}</span>
                        </div>
                    }
                >
                    <button
                        class="header-solo-btn"
                        aria-label="Clear solo"
                        on:click=move |_| {
                            if !connected.get() {
                                return;
                            }
                            // Optimistic UI: restore pre-solo mutes locally
                            let saved = pre_solo_mutes.get();
                            set_channels.update(|chs| {
                                for c in chs.iter_mut() {
                                    let should_be_muted = saved.get(&c.track_index).copied().unwrap_or(false);
                                    c.muted = should_be_muted;
                                }
                            });
                            set_pre_solo_mutes.set(HashMap::new());
                            set_soloed.set(std::collections::HashSet::new());
                            // Send empty SetSolo — server restores REAPER mutes and broadcasts
                            ws_send(ws, &iem_core::ClientMsg::SetSolo { soloed: vec![] });
                        }
                    >
                        "SOLO"
                        <span class="solo-close">"\u{2715}"</span>
                    </button>
                </Show>
                <button class="settings-btn" on:click=move |_| set_settings_modal_visible.set(true)>
                    "\u{2699}"
                </button>
                <Show
                    when=move || !network_mode.get().is_empty()
                    fallback=|| ()
                >
                    <div class=move || {
                        if network_mode.get() == "local" {
                            "network-indicator local"
                        } else {
                            "network-indicator remote"
                        }
                    }>
                        {move || if network_mode.get() == "local" { "LAN" } else { "WAN" }}
                    </div>
                </Show>
                <div class=move || {
                    let base = if connected.get() { "status-dot connected" } else { "status-dot disconnected" };
                    if connected.get() && data_pulse.get() {
                        format!("{} pulse-a", base)
                    } else if connected.get() {
                        format!("{} pulse-b", base)
                    } else {
                        base.to_string()
                    }
                }/>
            </header>

            <CategoryTabs
                active=active_category.into()
                on_select=move |cat| set_active_category.set(cat)
                show_hidden=Signal::derive(move || !hidden_channels.get().is_empty())
                show_mixes=Signal::derive(move || channels.get().iter().any(|ch| ch.category == "mixes"))
            />

            <Show
                when=move || !connected.get() && !loading.get()
                fallback=|| ()
            >
                <div class="disconnected-banner">
                    "Reconnecting to REAPER..."
                </div>
            </Show>

            <Show
                when=move || !loading.get()
                fallback=|| view! {
                    <div class="loading">
                        <div class="spinner"></div>
                    </div>
                }
            >
                <div class="channels-scroll">
                    <div class="channels-grid">
                        <Show
                            when=move || active_category.get() == Category::Main
                            fallback=|| ()
                        >
                            <GlobalVolumeFader
                                level=global_level
                                set_level=set_global_level
                                muted=global_muted
                                set_muted=set_global_muted
                                set_global_touched=set_global_touched
                                connected=connected
                                ws=ws
                                meters=meters.into()
                                output_track_idx=output_track_idx
                                set_eq_open=set_eq_open
                                set_eq_bands=set_eq_bands
                                set_eq_loading=set_eq_loading
                                is_engineer=is_engineer
                                set_limiter_open=set_limiter_open
                                set_limiter_loading=set_limiter_loading
                            />
                        </Show>
                        <Show
                            when=move || active_category.get() == Category::Stems
                            fallback=|| ()
                        >
                            <StemsVolumeFader
                                level=stems_level
                                set_level=set_stems_level
                                muted=stems_muted
                                set_muted=set_stems_muted
                                set_stems_touched=set_stems_touched
                                connected=connected
                                ws=ws
                                meters=meters.into()
                                stems_bus_idx=stems_bus_idx
                                set_eq_open=set_eq_open
                                set_eq_bands=set_eq_bands
                                set_eq_loading=set_eq_loading
                            />
                        </Show>
                        <ChannelList
                            display_channels=display_channels.into()
                            meters=meters.into()
                            channels=channels
                            set_channels=set_channels
                            set_fader_touched=set_fader_touched
                            soloed=soloed
                            set_soloed=set_soloed
                            pre_solo_mutes=pre_solo_mutes
                            set_pre_solo_mutes=set_pre_solo_mutes
                            connected=connected
                            ws=ws
                            double_tap_fader=double_tap_fader
                            pinned_channels=pinned_channels
                            set_pinned_channels=set_pinned_channels
                            hidden_channels=hidden_channels
                            set_hidden_channels=set_hidden_channels
                            active_category=active_category
                            set_eq_open=set_eq_open
                            set_eq_bands=set_eq_bands
                            set_eq_loading=set_eq_loading
                            member_id=member_id()
                            is_engineer=is_engineer
                        />
                        <Show
                            when=move || active_category.get() == Category::Main
                            fallback=|| ()
                        >
                            <StemsVolumeFader
                                level=stems_level
                                set_level=set_stems_level
                                muted=stems_muted
                                set_muted=set_stems_muted
                                set_stems_touched=set_stems_touched
                                connected=connected
                                ws=ws
                                meters=meters.into()
                                stems_bus_idx=stems_bus_idx
                                set_eq_open=set_eq_open
                                set_eq_bands=set_eq_bands
                                set_eq_loading=set_eq_loading
                            />
                        </Show>
                    </div>
                </div>
            </Show>

            <Toolbar
                on_presets=on_presets
                on_history=on_history
                is_engineer=is_engineer
                on_mute_all=on_mute_all
                is_engineer_own_mixer=is_engineer_own_mixer
                member_id=member_id()
                ws=ws
                alert_active=alert_active
                talk_state=talk_state
                set_talk_state=set_talk_state
            />

            {is_engineer.then(|| view! {
                <AlertToast alert=alert_data ws=ws />
            })}

            // "ENGINEER SPEAKING" banner for band members (#123)
            <Show when=move || engineer_talking.get()>
                <div class="engineer-speaking-banner">"ENGINEER SPEAKING"</div>
            </Show>

            <PresetModal
                visible=preset_modal_visible.into()
                member_id=member_id()
                on_close=on_close_modal
                on_load=on_load_preset
                get_current_state=get_current_state
            />

            <SettingsModal
                visible=settings_modal_visible.into()
                on_close=Callback::new(move |_: ()| set_settings_modal_visible.set(false))
                on_open_pin_change=Callback::new(move |_: ()| set_pin_modal_visible.set(true))
                double_tap_fader=double_tap_fader
                set_double_tap_fader=set_double_tap_fader
                member_id=member_id()
                is_engineer=is_engineer_own_mixer
                has_photo=has_photo.into()
                set_has_photo=set_has_photo
            />

            <PinChangeModal
                visible=pin_modal_visible.into()
                on_close=Callback::new(move |_: ()| set_pin_modal_visible.set(false))
                member_id=member_id()
            />

            <SnapshotModal
                visible=snapshot_modal_visible.into()
                member_id=member_id()
                on_close=Callback::new(move |_: ()| set_snapshot_modal_visible.set(false))
            />

            // EQ Modal (full-screen, shown when eq_open is Some)
            <Show when=move || eq_open.get().is_some() fallback=|| ()>
                {move || {
                    let (track_idx, track_name) = eq_open.get().unwrap();
                    let ws_for_eq = ws;
                    view! {
                        <EQModal
                            track_index=track_idx
                            track_name=track_name
                            bands=eq_bands
                            loading=eq_loading
                            on_param_change=Callback::new(move |(band, param, value): (u8, String, f32)| {
                                if let Some((ti, _)) = eq_open.get_untracked() {
                                    ws_send(ws_for_eq, &iem_core::ClientMsg::SetEqBand {
                                        track_index: ti,
                                        band,
                                        param,
                                        value,
                                    });
                                }
                            })
                            on_close=Callback::new(move |_: ()| {
                                // Defer to next macrotask via setTimeout(0) — setting eq_open=None
                                // destroys the EQ modal DOM. This must happen AFTER the current
                                // event handler stack fully unwinds (including microtasks), or
                                // Leptos reactive graph teardown hits dropped closures.
                                let cb = Closure::once_into_js(move || {
                                    set_eq_open.set(None);
                                    set_eq_bands.set(Vec::new());
                                });
                                web_sys::window()
                                    .unwrap()
                                    .set_timeout_with_callback(cb.as_ref().unchecked_ref())
                                    .unwrap();
                            })
                        />
                    }
                }}
            </Show>

            // Limiter Modal (#72)
            <Show when=move || limiter_open.get().is_some() fallback=|| ()>
                {move || {
                    let (_track_idx, track_name) = limiter_open.get().unwrap();
                    let ws_for_lim = ws;
                    view! {
                        <LimiterModal
                            track_name=track_name
                            limit_db=limiter_limit_db
                            limit_norm=limiter_limit_norm
                            enabled=limiter_enabled
                            loading=limiter_loading
                            on_param_change=Callback::new(move |(param, value): (String, f32)| {
                                if let Some((ti, _)) = limiter_open.get_untracked() {
                                    // Optimistic local update (no server echo)
                                    set_limiter_limit_norm.set(value);
                                    set_limiter_limit_db.set(value * 6.0 - 6.0);
                                    ws_send(ws_for_lim, &iem_core::ClientMsg::SetLimiterParam {
                                        track_index: ti,
                                        param,
                                        value,
                                    });
                                }
                            })
                            on_enabled_change=Callback::new(move |en: bool| {
                                if let Some((ti, _)) = limiter_open.get_untracked() {
                                    ws_send(ws_for_lim, &iem_core::ClientMsg::SetLimiterEnabled {
                                        track_index: ti,
                                        enabled: en,
                                    });
                                    set_limiter_enabled.set(en);
                                }
                            })
                            on_close=Callback::new(move |_: ()| {
                                let cb = Closure::once_into_js(move || {
                                    set_limiter_open.set(None);
                                });
                                web_sys::window()
                                    .unwrap()
                                    .set_timeout_with_callback(cb.as_ref().unchecked_ref())
                                    .unwrap();
                            })
                        />
                    }
                }}
            </Show>
        </div>
    }
}

/// Global IEM volume fader rendered on the Main tab
#[component]
fn GlobalVolumeFader(
    level: ReadSignal<f32>,
    set_level: WriteSignal<f32>,
    muted: ReadSignal<bool>,
    set_muted: WriteSignal<bool>,
    set_global_touched: WriteSignal<bool>,
    connected: ReadSignal<bool>,
    ws: ReadSignal<Option<web_sys::WebSocket>>,
    meters: ReadSignal<HashMap<usize, [f32; 2]>>,
    output_track_idx: ReadSignal<Option<usize>>,
    set_eq_open: WriteSignal<Option<(usize, String)>>,
    set_eq_bands: WriteSignal<Vec<EqBandState>>,
    set_eq_loading: WriteSignal<bool>,
    is_engineer: bool,
    set_limiter_open: WriteSignal<Option<(usize, String)>>,
    set_limiter_loading: WriteSignal<bool>,
) -> impl IntoView {
    let (is_fader_active, set_is_fader_active) = signal(false);

    // Guard timeout for post-release protection
    let (guard_id, set_guard_id) = signal(Option::<i32>::None);

    // Throttle state
    let (last_send_time, set_last_send_time) = signal(0.0_f64);
    let (pending_value, set_pending_value) = signal(Option::<f32>::None);
    let (pending_timeout, set_pending_timeout) = signal(Option::<i32>::None);

    let cancel_guard = move || {
        if let Some(id) = guard_id.get_untracked() {
            if let Some(w) = web_sys::window() {
                w.clear_timeout_with_handle(id);
            }
            set_guard_id.set(None);
        }
    };

    let set_guard = move || {
        cancel_guard();
        let cb = Closure::once_into_js(move || {
            set_guard_id.set(None);
            set_global_touched.set(false);
        });
        if let Some(w) = web_sys::window() {
            if let Ok(id) = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.unchecked_ref(),
                POST_RELEASE_GUARD_MS,
            ) {
                set_guard_id.set(Some(id));
            }
        }
    };

    let cancel_pending = move || {
        if let Some(id) = pending_timeout.get_untracked() {
            if let Some(w) = web_sys::window() {
                w.clear_timeout_with_handle(id);
            }
            set_pending_timeout.set(None);
        }
    };

    let on_level_change = Callback::new(move |new_level: f32| {
        set_level.set(new_level); // Optimistic update — prevents snap-back
        if !connected.get() {
            return;
        }

        // Throttled WebSocket send
        let now = js_sys::Date::now();
        let last = last_send_time.get_untracked();

        if now - last >= THROTTLE_INTERVAL_MS {
            set_last_send_time.set(now);
            set_pending_value.set(None);
            cancel_pending();
            ws_send(
                ws,
                &iem_core::ClientMsg::SetGlobalLevel {
                    level_db: new_level,
                },
            );
        } else {
            set_pending_value.set(Some(new_level));
            cancel_pending();
            let cb = Closure::once_into_js(move || {
                let pending = pending_value.get_untracked();
                if let Some(val) = pending {
                    set_last_send_time.set(js_sys::Date::now());
                    set_pending_value.set(None);
                    set_pending_timeout.set(None);
                    ws_send(ws, &iem_core::ClientMsg::SetGlobalLevel { level_db: val });
                }
            });
            if let Some(w) = web_sys::window() {
                if let Ok(id) = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                    cb.unchecked_ref(),
                    THROTTLE_INTERVAL_MS as i32,
                ) {
                    set_pending_timeout.set(Some(id));
                }
            }
        }
    });

    let on_touch_state = Callback::new(move |touching: bool| {
        if touching {
            cancel_guard();
            set_global_touched.set(true);
        } else {
            // Flush pending
            let pending = pending_value.get_untracked();
            if let Some(val) = pending {
                set_last_send_time.set(js_sys::Date::now());
                set_pending_value.set(None);
                cancel_pending();
                ws_send(ws, &iem_core::ClientMsg::SetGlobalLevel { level_db: val });
            }
            set_guard();
        }
    });

    let on_mute_click = move |_| {
        if !connected.get() {
            return;
        }
        let new_muted = !muted.get();
        set_muted.set(new_muted); // Optimistic update — immediate UI feedback
        set_global_touched.set(true);
        ws_send(ws, &iem_core::ClientMsg::SetGlobalMute { muted: new_muted });
        // Post-release guard for mute
        let cb = Closure::once_into_js(move || {
            set_global_touched.set(false);
        });
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.unchecked_ref(),
                POST_RELEASE_GUARD_MS,
            );
        }
    };

    let level_signal = Signal::derive(move || level.get());

    // Derive meter levels from the output track's meter data
    let meter_l = Signal::derive(move || {
        output_track_idx
            .get()
            .and_then(|idx| meters.with(|m| m.get(&idx).map(|v| v[0])))
            .unwrap_or(0.0)
    });
    let meter_r = Signal::derive(move || {
        output_track_idx
            .get()
            .and_then(|idx| meters.with(|m| m.get(&idx).map(|v| v[1])))
            .unwrap_or(0.0)
    });

    view! {
        <div
            class=move || {
                let mut classes = vec!["channel", "global-volume"];
                if muted.get() { classes.push("muted"); }
                if !connected.get() { classes.push("disconnected"); }
                if is_fader_active.get() { classes.push("fader-active"); }
                classes.join(" ")
            }
            data-testid="global-volume-fader"
        >
            <div class="ch-label">
                <div class="ch-name">"IEM VOL"</div>
                <div class="ch-type">"master"</div>
            </div>

            <div style="grid-area: menu"></div>

            <Meter level_l=meter_l level_r=meter_r />

            <div class="fader-area">
                <Fader
                    value=level_signal
                    min=-60.0
                    max=12.0
                    on_change=on_level_change
                    on_activate=Callback::new(move |active| set_is_fader_active.set(active))
                    on_touch_state=on_touch_state
                />
            </div>

            <div class="pan-container"></div>

            <div class="channel-btns global-vol-btns">
                <div class="db-display" data-value=move || level.get()>{move || format_db(level.get())}</div>
                <button
                    class="eq-btn-small"
                    on:click=move |_| {
                        if let Some(idx) = output_track_idx.get() {
                            set_eq_bands.set(Vec::new());
                            set_eq_loading.set(true);
                            set_eq_open.set(Some((idx, "IEM VOL".to_string())));
                            ws_send(
                                ws,
                                &iem_core::ClientMsg::GetEqParams { track_index: idx },
                            );
                        }
                    }
                >
                    "EQ"
                </button>
                {is_engineer.then(|| view! {
                    <button
                        class="limiter-btn-small"
                        on:click=move |_| {
                            if let Some(idx) = output_track_idx.get() {
                                set_limiter_loading.set(true);
                                set_limiter_open.set(Some((idx, "IEM VOL".to_string())));
                                ws_send(
                                    ws,
                                    &iem_core::ClientMsg::GetLimiterParams { track_index: idx },
                                );
                            }
                        }
                    >
                        "LIM"
                    </button>
                })}
                <button
                    class=move || if muted.get() { "mute-btn on" } else { "mute-btn off" }
                    on:click=on_mute_click
                >
                    "M"
                </button>
            </div>
        </div>
    }
}

/// Stems group bus volume fader rendered on Main and Stems tabs
#[component]
fn StemsVolumeFader(
    level: ReadSignal<f32>,
    set_level: WriteSignal<f32>,
    muted: ReadSignal<bool>,
    set_muted: WriteSignal<bool>,
    set_stems_touched: WriteSignal<bool>,
    connected: ReadSignal<bool>,
    ws: ReadSignal<Option<web_sys::WebSocket>>,
    meters: ReadSignal<HashMap<usize, [f32; 2]>>,
    stems_bus_idx: ReadSignal<Option<usize>>,
    set_eq_open: WriteSignal<Option<(usize, String)>>,
    set_eq_bands: WriteSignal<Vec<EqBandState>>,
    set_eq_loading: WriteSignal<bool>,
) -> impl IntoView {
    let (is_fader_active, set_is_fader_active) = signal(false);

    // Guard timeout for post-release protection
    let (guard_id, set_guard_id) = signal(Option::<i32>::None);

    // Throttle state
    let (last_send_time, set_last_send_time) = signal(0.0_f64);
    let (pending_value, set_pending_value) = signal(Option::<f32>::None);
    let (pending_timeout, set_pending_timeout) = signal(Option::<i32>::None);

    let cancel_guard = move || {
        if let Some(id) = guard_id.get_untracked() {
            if let Some(w) = web_sys::window() {
                w.clear_timeout_with_handle(id);
            }
            set_guard_id.set(None);
        }
    };

    let set_guard = move || {
        cancel_guard();
        let cb = Closure::once_into_js(move || {
            set_guard_id.set(None);
            set_stems_touched.set(false);
        });
        if let Some(w) = web_sys::window() {
            if let Ok(id) = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.unchecked_ref(),
                POST_RELEASE_GUARD_MS,
            ) {
                set_guard_id.set(Some(id));
            }
        }
    };

    let cancel_pending = move || {
        if let Some(id) = pending_timeout.get_untracked() {
            if let Some(w) = web_sys::window() {
                w.clear_timeout_with_handle(id);
            }
            set_pending_timeout.set(None);
        }
    };

    let on_level_change = Callback::new(move |new_level: f32| {
        set_level.set(new_level);
        if !connected.get() {
            return;
        }

        let now = js_sys::Date::now();
        let last = last_send_time.get_untracked();

        if now - last >= THROTTLE_INTERVAL_MS {
            set_last_send_time.set(now);
            set_pending_value.set(None);
            cancel_pending();
            ws_send(
                ws,
                &iem_core::ClientMsg::SetStemsLevel {
                    level_db: new_level,
                },
            );
        } else {
            set_pending_value.set(Some(new_level));
            cancel_pending();
            let cb = Closure::once_into_js(move || {
                let pending = pending_value.get_untracked();
                if let Some(val) = pending {
                    set_last_send_time.set(js_sys::Date::now());
                    set_pending_value.set(None);
                    set_pending_timeout.set(None);
                    ws_send(ws, &iem_core::ClientMsg::SetStemsLevel { level_db: val });
                }
            });
            if let Some(w) = web_sys::window() {
                if let Ok(id) = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                    cb.unchecked_ref(),
                    THROTTLE_INTERVAL_MS as i32,
                ) {
                    set_pending_timeout.set(Some(id));
                }
            }
        }
    });

    let on_touch_state = Callback::new(move |touching: bool| {
        if touching {
            cancel_guard();
            set_stems_touched.set(true);
        } else {
            let pending = pending_value.get_untracked();
            if let Some(val) = pending {
                set_last_send_time.set(js_sys::Date::now());
                set_pending_value.set(None);
                cancel_pending();
                ws_send(ws, &iem_core::ClientMsg::SetStemsLevel { level_db: val });
            }
            set_guard();
        }
    });

    let on_mute_click = move |_| {
        if !connected.get() {
            return;
        }
        let new_muted = !muted.get();
        set_muted.set(new_muted);
        set_stems_touched.set(true);
        ws_send(ws, &iem_core::ClientMsg::SetStemsMute { muted: new_muted });
        let cb = Closure::once_into_js(move || {
            set_stems_touched.set(false);
        });
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.unchecked_ref(),
                POST_RELEASE_GUARD_MS,
            );
        }
    };

    let level_signal = Signal::derive(move || level.get());

    let meter_l = Signal::derive(move || {
        stems_bus_idx
            .get()
            .and_then(|idx| meters.with(|m| m.get(&idx).map(|v| v[0])))
            .unwrap_or(0.0)
    });
    let meter_r = Signal::derive(move || {
        stems_bus_idx
            .get()
            .and_then(|idx| meters.with(|m| m.get(&idx).map(|v| v[1])))
            .unwrap_or(0.0)
    });

    // Only render if stems bus exists
    let has_stems_bus = Signal::derive(move || stems_bus_idx.get().is_some());

    view! {
        <Show when=move || has_stems_bus.get() fallback=|| ()>
            <div
                class=move || {
                    let mut classes = vec!["channel", "stems-volume"];
                    if muted.get() { classes.push("muted"); }
                    if !connected.get() { classes.push("disconnected"); }
                    if is_fader_active.get() { classes.push("fader-active"); }
                    classes.join(" ")
                }
                data-testid="stems-volume-fader"
            >
                <div class="ch-label">
                    <div class="ch-name">"STEMS"</div>
                    <div class="ch-type">"group"</div>
                </div>

                <div style="grid-area: menu"></div>

                <Meter level_l=meter_l level_r=meter_r />

                <div class="fader-area">
                    <Fader
                        value=level_signal
                        min=-60.0
                        max=12.0
                        on_change=on_level_change
                        on_activate=Callback::new(move |active| set_is_fader_active.set(active))
                        on_touch_state=on_touch_state
                    />
                </div>

                <div class="db-display" data-value=move || level.get()>{move || format_db(level.get())}</div>

                <div class="pan-container"></div>

                <div class="channel-btns">
                    <button
                        class="eq-btn-small"
                        on:click=move |_| {
                            if let Some(idx) = stems_bus_idx.get() {
                                set_eq_bands.set(Vec::new());
                                set_eq_loading.set(true);
                                set_eq_open.set(Some((idx, "STEMS".to_string())));
                                ws_send(
                                    ws,
                                    &iem_core::ClientMsg::GetEqParams { track_index: idx },
                                );
                            }
                        }
                    >
                        "EQ"
                    </button>
                    <button
                        class=move || if muted.get() { "mute-btn on" } else { "mute-btn off" }
                        on:click=on_mute_click
                    >
                        "M"
                    </button>
                </div>
            </div>
        </Show>
    }
}

/// Channel list component to handle individual channel rendering
#[component]
fn ChannelList(
    display_channels: Signal<Vec<DisplayChannel>>,
    meters: ReadSignal<HashMap<usize, [f32; 2]>>,
    channels: ReadSignal<Vec<Channel>>,
    set_channels: WriteSignal<Vec<Channel>>,
    set_fader_touched: WriteSignal<HashMap<usize, bool>>,
    soloed: ReadSignal<std::collections::HashSet<usize>>,
    set_soloed: WriteSignal<std::collections::HashSet<usize>>,
    pre_solo_mutes: ReadSignal<HashMap<usize, bool>>,
    set_pre_solo_mutes: WriteSignal<HashMap<usize, bool>>,
    connected: ReadSignal<bool>,
    ws: ReadSignal<Option<web_sys::WebSocket>>,
    double_tap_fader: ReadSignal<bool>,
    pinned_channels: ReadSignal<Vec<usize>>,
    set_pinned_channels: WriteSignal<Vec<usize>>,
    hidden_channels: ReadSignal<Vec<usize>>,
    set_hidden_channels: WriteSignal<Vec<usize>>,
    active_category: ReadSignal<Category>,
    set_eq_open: WriteSignal<Option<(usize, String)>>,
    set_eq_bands: WriteSignal<Vec<EqBandState>>,
    set_eq_loading: WriteSignal<bool>,
    /// Member ID for EQ access control (e.g., "petka")
    #[prop(into)]
    member_id: String,
    /// Whether the current user is an engineer (engineers can access all EQ)
    #[prop(default = false)]
    is_engineer: bool,
) -> impl IntoView {
    // Guard timeout IDs as raw JS setTimeout handles (i32 = Copy + Send + Sync).
    // Key scheme: track_idx for fader, track_idx+10000 for pan, track_idx+20000 for mute.
    let (_guard_ids, set_guard_ids) = signal(HashMap::<usize, i32>::new());

    // Throttle state signals — all Copy + Send + Sync for use in Callback::new closures.
    let (last_send_times, set_last_send_times) = signal(HashMap::<usize, f64>::new());
    let (pending_values, set_pending_values) = signal(HashMap::<usize, f32>::new());
    let (_pending_timeouts, set_pending_timeouts) = signal(HashMap::<usize, i32>::new());

    // Shared signal: which channel's kebab menu is open (None = all closed)
    let (open_menu, set_open_menu) = signal(Option::<usize>::None);

    // EQ access control: store member_id as StoredValue for use in closures
    let eq_member_id = StoredValue::new(member_id.to_uppercase());
    let eq_is_engineer = is_engineer;

    // CRITICAL: Use <For> with stable key to preserve Fader component identity
    // across re-renders. Without this, optimistic updates cause all Faders to
    // remount, losing their is_activated state (the "glow disappears" bug).
    view! {
        <Show
            when=move || !display_channels.get().is_empty()
            fallback=|| view! { <div class="no-channels">"No channels in this category"</div> }
        >
            <For
                each=move || display_channels.get()
                key=|ch| (ch.display_name.clone(), ch.track_index)
                children=move |ch| {
                    let track_idx = ch.track_index;
                    let partner_idx = ch.partner_index;
                    let name = ch.display_name.clone();
                    let eq_name = StoredValue::new(name.clone()); // For EQ button closure (Copy)
                    // EQ access: engineer can EQ any track; members only their own
                    let show_eq = eq_is_engineer || {
                        let mid = eq_member_id.get_value();
                        let upper_name = name.to_uppercase();
                        upper_name.starts_with(&mid)
                    };
                    let is_my = ch.is_my_input;
                    let is_stereo = ch.is_stereo;
                    let ch_is_pinned =
                        move || pinned_channels.get().contains(&track_idx);

                    // Derived signals using .with() to avoid cloning entire collections
                    let level_signal = Signal::derive(move || {
                        channels.with(|chs| {
                            chs.iter()
                                .find(|c| c.track_index == track_idx)
                                .map(|c| c.level_db)
                                .unwrap_or(-60.0)
                        })
                    });

                    let muted_signal = Signal::derive(move || {
                        channels.with(|chs| {
                            chs.iter()
                                .find(|c| c.track_index == track_idx)
                                .map(|c| c.muted)
                                .unwrap_or(false)
                        })
                    });

                    let pan_signal = Signal::derive(move || {
                        channels.with(|chs| {
                            chs.iter()
                                .find(|c| c.track_index == track_idx)
                                .map(|c| c.pan)
                                .unwrap_or(0.5)
                        })
                    });

                    // Meters show raw input level — NOT scaled by send fader, pan, or mute.
                    // This matches REAPER's own meter display: the meter shows what's
                    // coming IN on the track, independent of where/how it's being sent.
                    let meter_l = Signal::derive(move || {
                        meters.with(|m| m.get(&track_idx).map(|v| v[0]).unwrap_or(0.0))
                    });
                    let meter_r = Signal::derive(move || {
                        meters.with(|m| m.get(&track_idx).map(|v| v[1]).unwrap_or(0.0))
                    });

                    // Fader activation state for channel glow
                    let (is_fader_active, set_is_fader_active) = signal(false);

                    // Helper: cancel a guard timeout by key.
                    // All captures are Copy + Send + Sync, so this closure is too.
                    let cancel_guard = move |key: usize| {
                        set_guard_ids.update(|ids| {
                            if let Some(id) = ids.remove(&key) {
                                if let Some(w) = web_sys::window() {
                                    w.clear_timeout_with_handle(id);
                                }
                            }
                        });
                    };

                    // Helper: set a post-release guard timeout that clears
                    // fader_touched after POST_RELEASE_GUARD_MS.
                    let set_guard = move |key: usize| {
                        cancel_guard(key);
                        let cb = Closure::once_into_js(move || {
                            set_guard_ids.update(|ids| {
                                ids.remove(&key);
                            });
                            set_fader_touched.update(|t| {
                                t.remove(&track_idx);
                                if let Some(p) = partner_idx {
                                    t.remove(&p);
                                }
                            });
                        });
                        if let Some(w) = web_sys::window() {
                            if let Ok(id) =
                                w.set_timeout_with_callback_and_timeout_and_arguments_0(
                                    cb.unchecked_ref(),
                                    POST_RELEASE_GUARD_MS,
                                )
                            {
                                set_guard_ids.update(|ids| {
                                    ids.insert(key, id);
                                });
                            }
                        }
                    };

                    // Helper: cancel a pending throttle timeout for a track
                    let cancel_pending_timeout = move |tidx: usize| {
                        set_pending_timeouts.update(|m| {
                            if let Some(id) = m.remove(&tidx) {
                                if let Some(w) = web_sys::window() {
                                    w.clear_timeout_with_handle(id);
                                }
                            }
                        });
                    };

                    // Level change handler with throttling.
                    // Optimistic UI updates happen at full rate; WebSocket sends are
                    // throttled to max ~20/sec per track to avoid server queue buildup.
                    let on_level_change = Callback::new(move |new_level: f32| {
                        if !connected.get() {
                            return;
                        }

                        // Optimistic update at full rate
                        set_channels.update(|chs| {
                            if let Some(ch) =
                                chs.iter_mut().find(|c| c.track_index == track_idx)
                            {
                                ch.level_db = new_level;
                            }
                            if let Some(partner) = partner_idx {
                                if let Some(ch) =
                                    chs.iter_mut().find(|c| c.track_index == partner)
                                {
                                    ch.level_db = new_level;
                                }
                            }
                        });

                        // Throttled WebSocket send
                        let now = js_sys::Date::now();
                        let last_time =
                            last_send_times.with(|m| m.get(&track_idx).copied().unwrap_or(0.0));

                        if now - last_time >= THROTTLE_INTERVAL_MS {
                            // Enough time has passed — send immediately
                            set_last_send_times.update(|m| {
                                m.insert(track_idx, now);
                            });
                            set_pending_values.update(|m| {
                                m.remove(&track_idx);
                            });
                            cancel_pending_timeout(track_idx);

                            ws_send(
                                ws,
                                &iem_core::ClientMsg::SetLevel {
                                    track_index: track_idx,
                                    level_db: new_level,
                                },
                            );
                            if let Some(partner) = partner_idx {
                                ws_send(
                                    ws,
                                    &iem_core::ClientMsg::SetLevel {
                                        track_index: partner,
                                        level_db: new_level,
                                    },
                                );
                            }
                        } else {
                            // Too soon — store as pending, schedule deferred send
                            set_pending_values.update(|m| {
                                m.insert(track_idx, new_level);
                            });
                            cancel_pending_timeout(track_idx);

                            let cb = Closure::once_into_js(move || {
                                let pending =
                                    pending_values.with(|m| m.get(&track_idx).copied());
                                if let Some(val) = pending {
                                    set_last_send_times.update(|m| {
                                        m.insert(track_idx, js_sys::Date::now());
                                    });
                                    set_pending_values.update(|m| {
                                        m.remove(&track_idx);
                                    });
                                    set_pending_timeouts.update(|m| {
                                        m.remove(&track_idx);
                                    });
                                    ws_send(
                                        ws,
                                        &iem_core::ClientMsg::SetLevel {
                                            track_index: track_idx,
                                            level_db: val,
                                        },
                                    );
                                    if let Some(partner) = partner_idx {
                                        ws_send(
                                            ws,
                                            &iem_core::ClientMsg::SetLevel {
                                                track_index: partner,
                                                level_db: val,
                                            },
                                        );
                                    }
                                }
                            });
                            if let Some(w) = web_sys::window() {
                                if let Ok(id) =
                                    w.set_timeout_with_callback_and_timeout_and_arguments_0(
                                        cb.unchecked_ref(),
                                        THROTTLE_INTERVAL_MS as i32,
                                    )
                                {
                                    set_pending_timeouts.update(|m| {
                                        m.insert(track_idx, id);
                                    });
                                }
                            }
                        }
                    });

                    // Pan change handler with throttling + cancellable guard
                    // Uses pan_key = track_idx + 10000 to avoid collision with level keys
                    let on_pan_change = Callback::new(move |new_pan: f32| {
                        if !connected.get() {
                            return;
                        }

                        set_fader_touched.update(|t| {
                            t.insert(track_idx, true);
                            if let Some(partner) = partner_idx {
                                t.insert(partner, true);
                            }
                        });

                        // Optimistic UI update at full rate
                        set_channels.update(|chs| {
                            if let Some(ch) =
                                chs.iter_mut().find(|c| c.track_index == track_idx)
                            {
                                ch.pan = new_pan;
                            }
                            if let Some(partner) = partner_idx {
                                if let Some(ch) =
                                    chs.iter_mut().find(|c| c.track_index == partner)
                                {
                                    ch.pan = 1.0 - new_pan;
                                }
                            }
                        });

                        // Throttled WebSocket send (same pattern as level)
                        let pan_key = track_idx + 10000;
                        let now = js_sys::Date::now();
                        let last_time =
                            last_send_times.with(|m| m.get(&pan_key).copied().unwrap_or(0.0));

                        if now - last_time >= THROTTLE_INTERVAL_MS {
                            set_last_send_times.update(|m| {
                                m.insert(pan_key, now);
                            });
                            set_pending_values.update(|m| {
                                m.remove(&pan_key);
                            });
                            cancel_pending_timeout(pan_key);

                            ws_send(
                                ws,
                                &iem_core::ClientMsg::SetPan {
                                    track_index: track_idx,
                                    pan: new_pan,
                                },
                            );
                            if let Some(partner) = partner_idx {
                                ws_send(
                                    ws,
                                    &iem_core::ClientMsg::SetPan {
                                        track_index: partner,
                                        pan: 1.0 - new_pan,
                                    },
                                );
                            }
                        } else {
                            set_pending_values.update(|m| {
                                m.insert(pan_key, new_pan);
                            });
                            cancel_pending_timeout(pan_key);

                            let cb = Closure::once_into_js(move || {
                                let pending =
                                    pending_values.with(|m| m.get(&pan_key).copied());
                                if let Some(val) = pending {
                                    set_last_send_times.update(|m| {
                                        m.insert(pan_key, js_sys::Date::now());
                                    });
                                    set_pending_values.update(|m| {
                                        m.remove(&pan_key);
                                    });
                                    set_pending_timeouts.update(|m| {
                                        m.remove(&pan_key);
                                    });
                                    ws_send(
                                        ws,
                                        &iem_core::ClientMsg::SetPan {
                                            track_index: track_idx,
                                            pan: val,
                                        },
                                    );
                                    if let Some(partner) = partner_idx {
                                        ws_send(
                                            ws,
                                            &iem_core::ClientMsg::SetPan {
                                                track_index: partner,
                                                pan: 1.0 - val,
                                            },
                                        );
                                    }
                                }
                            });
                            if let Some(w) = web_sys::window() {
                                if let Ok(id) =
                                    w.set_timeout_with_callback_and_timeout_and_arguments_0(
                                        cb.unchecked_ref(),
                                        THROTTLE_INTERVAL_MS as i32,
                                    )
                                {
                                    set_pending_timeouts.update(|m| {
                                        m.insert(pan_key, id);
                                    });
                                }
                            }
                        }

                        // Cancellable post-release guard
                        set_guard(pan_key);
                    });

                    // Mute toggle handler with cancellable guard
                    let on_mute_click = move |_| {
                        if !connected.get() {
                            return;
                        }

                        let current_muted = channels.with(|chs| {
                            chs.iter()
                                .find(|c| c.track_index == track_idx)
                                .map(|c| c.muted)
                                .unwrap_or(false)
                        });
                        let new_muted = !current_muted;

                        set_fader_touched.update(|t| {
                            t.insert(track_idx, true);
                            if let Some(partner) = partner_idx {
                                t.insert(partner, true);
                            }
                        });

                        set_channels.update(|chs| {
                            if let Some(ch) =
                                chs.iter_mut().find(|c| c.track_index == track_idx)
                            {
                                ch.muted = new_muted;
                            }
                            if let Some(partner) = partner_idx {
                                if let Some(ch) =
                                    chs.iter_mut().find(|c| c.track_index == partner)
                                {
                                    ch.muted = new_muted;
                                }
                            }
                        });

                        ws_send(
                            ws,
                            &iem_core::ClientMsg::SetMute {
                                track_index: track_idx,
                                muted: new_muted,
                            },
                        );
                        if let Some(partner) = partner_idx {
                            ws_send(
                                ws,
                                &iem_core::ClientMsg::SetMute {
                                    track_index: partner,
                                    muted: new_muted,
                                },
                            );
                        }

                        // Cancellable post-release guard (mute key = track_idx + 20000)
                        set_guard(track_idx + 20000);
                    };

                    // Solo toggle handler
                    let on_solo_click = move |_| {
                        if !connected.get() {
                            return;
                        }

                        let all_channels = channels.get();
                        let current_soloed = soloed.get();
                        let is_currently_soloed = current_soloed.contains(&track_idx);

                        if is_currently_soloed {
                            // UN-SOLO this track
                            let mut new_soloed = current_soloed.clone();
                            new_soloed.remove(&track_idx);
                            if let Some(partner) = partner_idx {
                                new_soloed.remove(&partner);
                            }

                            if new_soloed.is_empty() {
                                // Restore pre-solo mutes (optimistic UI)
                                let saved = pre_solo_mutes.get();
                                set_channels.update(|chs| {
                                    for c in chs.iter_mut() {
                                        let should_be_muted = saved.get(&c.track_index).copied().unwrap_or(false);
                                        c.muted = should_be_muted;
                                    }
                                });
                                set_pre_solo_mutes.set(HashMap::new());
                            } else {
                                // Partial unsolo — mute the desoloed track(s)
                                set_channels.update(|chs| {
                                    if let Some(ch) = chs.iter_mut().find(|c| c.track_index == track_idx) {
                                        ch.muted = true;
                                    }
                                    if let Some(partner) = partner_idx {
                                        if let Some(ch) = chs.iter_mut().find(|c| c.track_index == partner) {
                                            ch.muted = true;
                                        }
                                    }
                                });
                            }

                            let soloed_vec: Vec<usize> = new_soloed.iter().copied().collect();
                            set_soloed.set(new_soloed);
                            ws_send(ws, &iem_core::ClientMsg::SetSolo { soloed: soloed_vec });
                        } else {
                            // SOLO this track
                            let was_empty = current_soloed.is_empty();

                            if was_empty {
                                // Save pre-solo mutes for optimistic UI restore
                                let mut saved_mutes = HashMap::new();
                                for ch in &all_channels {
                                    saved_mutes.insert(ch.track_index, ch.muted);
                                }
                                set_pre_solo_mutes.set(saved_mutes);
                            }

                            // Optimistic UI: mute everything except solo target
                            set_channels.update(|chs| {
                                for c in chs.iter_mut() {
                                    c.muted = c.track_index != track_idx
                                        && partner_idx != Some(c.track_index);
                                }
                            });

                            // Build soloed set — exclusive (only new track + partner)
                            let mut new_soloed = std::collections::HashSet::new();
                            new_soloed.insert(track_idx);
                            if let Some(partner) = partner_idx {
                                new_soloed.insert(partner);
                            }
                            let soloed_vec: Vec<usize> = new_soloed.iter().copied().collect();
                            set_soloed.set(new_soloed);
                            ws_send(ws, &iem_core::ClientMsg::SetSolo { soloed: soloed_vec });
                        }
                    };

                    // Touch state handler: manages fader_touched guards and flushes
                    // pending throttled values on release.
                    let on_touch_state = Callback::new(move |touching: bool| {
                        if touching {
                            // Cancel any pending release guard
                            cancel_guard(track_idx);
                            set_fader_touched.update(|t| {
                                t.insert(track_idx, true);
                                if let Some(partner) = partner_idx {
                                    t.insert(partner, true);
                                }
                            });
                        } else {
                            // Flush any pending throttled value immediately on release
                            let pending =
                                pending_values.with(|m| m.get(&track_idx).copied());
                            if let Some(val) = pending {
                                set_last_send_times.update(|m| {
                                    m.insert(track_idx, js_sys::Date::now());
                                });
                                set_pending_values.update(|m| {
                                    m.remove(&track_idx);
                                });
                                cancel_pending_timeout(track_idx);
                                ws_send(
                                    ws,
                                    &iem_core::ClientMsg::SetLevel {
                                        track_index: track_idx,
                                        level_db: val,
                                    },
                                );
                                if let Some(partner) = partner_idx {
                                    ws_send(
                                        ws,
                                        &iem_core::ClientMsg::SetLevel {
                                            track_index: partner,
                                            level_db: val,
                                        },
                                    );
                                }
                            }

                            // Cancellable post-release guard
                            set_guard(track_idx);
                        }
                    });

                    let is_soloed = move || soloed.get().contains(&track_idx);
                    let is_connected = move || connected.get();
                    let is_hidden_tab = move || active_category.get() == Category::Hidden;

                    // Pin toggle: add/remove track from pinned list
                    let on_pin_click = move |_| {
                        let mut pinned = pinned_channels.get();
                        if pinned.contains(&track_idx) {
                            pinned.retain(|&x| x != track_idx);
                        } else {
                            pinned.push(track_idx);
                        }
                        set_pinned_channels.set(pinned.clone());
                        // Save to server via WS
                        let hidden = hidden_channels.get();
                        ws_send(ws, &iem_core::ClientMsg::UpdateCustomization {
                            pinned,
                            hidden,
                        });
                    };

                    // Hide/unhide toggle: add/remove track from hidden list
                    let on_hide_click = move |_| {
                        let mut hidden = hidden_channels.get();
                        if hidden.contains(&track_idx) {
                            hidden.retain(|&x| x != track_idx);
                        } else {
                            hidden.push(track_idx);
                        }
                        set_hidden_channels.set(hidden.clone());
                        // Save to server via WS
                        let pinned = pinned_channels.get();
                        ws_send(ws, &iem_core::ClientMsg::UpdateCustomization {
                            pinned,
                            hidden,
                        });
                    };

                    view! {
                        <div
                            class=move || {
                                let mut classes = vec!["channel"];
                                if muted_signal.get() { classes.push("muted"); }
                                if is_my { classes.push("more-me"); }
                                if is_stereo { classes.push("stereo-pair"); }
                                if !is_connected() { classes.push("disconnected"); }
                                if is_fader_active.get() { classes.push("fader-active"); }
                                if open_menu.get() == Some(track_idx) { classes.push("menu-open"); }
                                classes.join(" ")
                            }
                            on:click=move |_| set_open_menu.set(None)
                        >
                            <div class="ch-label">
                                <div class="ch-name">{parse_track_name(&name).0}</div>
                                <div class="ch-type">
                                    {parse_track_name(&name).1}
                                    {if is_stereo { " (st)" } else { "" }}
                                </div>
                            </div>

                            <Meter level_l=meter_l level_r=meter_r />

                            <div class="fader-area">
                                <Fader
                                    value=level_signal
                                    min=-60.0
                                    max=12.0
                                    on_change=on_level_change
                                    on_activate=Callback::new(move |active| set_is_fader_active.set(active))
                                    on_touch_state=on_touch_state
                                    double_tap_enabled=double_tap_fader.into()
                                />
                            </div>

                            <div class="db-display">{move || format_db(level_signal.get())}</div>

                            <PanKnob
                                value=pan_signal
                                on_change=on_pan_change
                            />

                            <div class="channel-btns">
                                <button
                                    class=move || if is_soloed() { "solo-btn on" } else { "solo-btn off" }
                                    on:click=on_solo_click
                                >
                                    "S"
                                </button>
                                <button
                                    class=move || if muted_signal.get() { "mute-btn on" } else { "mute-btn off" }
                                    on:click=on_mute_click
                                >
                                    "M"
                                </button>
                            </div>
                            // Kebab menu button (⋮)
                            <button
                                class=move || if open_menu.get() == Some(track_idx) { "ch-menu-btn open" } else { "ch-menu-btn" }
                                on:click=move |ev: web_sys::MouseEvent| {
                                    ev.stop_propagation();
                                    set_open_menu.update(|v| {
                                        *v = if *v == Some(track_idx) { None } else { Some(track_idx) };
                                    });
                                }
                            >
                                "\u{22EE}"
                            </button>

                            // Kebab menu popup (only when this channel's menu is open)
                            <Show when=move || open_menu.get() == Some(track_idx) fallback=|| ()>
                                <div class="ch-menu-popup" on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()>
                                    <button
                                        class=move || if ch_is_pinned() { "ch-menu-item pinned" } else { "ch-menu-item" }
                                        on:click=move |ev: web_sys::MouseEvent| { ev.stop_propagation(); on_pin_click(ev); set_open_menu.set(None); }
                                    >
                                        <span class="menu-icon">{move || if ch_is_pinned() { "\u{2605}" } else { "\u{2606}" }}</span>
                                        {move || if ch_is_pinned() { "Unpin" } else { "Pin to Main" }}
                                    </button>
                                    <button
                                        class="ch-menu-item"
                                        on:click=move |ev: web_sys::MouseEvent| { ev.stop_propagation(); on_hide_click(ev); set_open_menu.set(None); }
                                    >
                                        <span class="menu-icon">{move || if is_hidden_tab() { "\u{25C9}" } else { "\u{2715}" }}</span>
                                        {move || if is_hidden_tab() { "Unhide" } else { "Hide" }}
                                    </button>
                                    {if show_eq { Some(view! {
                                        <button
                                            class="ch-menu-item"
                                            on:click=move |ev: web_sys::MouseEvent| {
                                                ev.stop_propagation();
                                                set_open_menu.set(None);
                                                set_eq_bands.set(Vec::new());
                                                set_eq_loading.set(true);
                                                set_eq_open.set(Some((track_idx, eq_name.get_value())));
                                                // Request EQ params from REAPER
                                                ws_send(ws, &iem_core::ClientMsg::GetEqParams { track_index: track_idx });
                                            }
                                        >
                                            <span class="menu-icon">"\u{2261}"</span>
                                            "EQ"
                                        </button>
                                    }) } else { None }}
                                </div>
                            </Show>
                        </div>
                    }
                }
            />
            </Show>
            // Backdrop to close kebab menu on outside tap
            <Show when=move || open_menu.get().is_some() fallback=|| ()>
                <div class="ch-menu-backdrop" on:click=move |_| set_open_menu.set(None)></div>
            </Show>
    }
}

/// Parse track name into main and type parts
fn parse_track_name(name: &str) -> (String, String) {
    let parts: Vec<&str> = name.split_whitespace().collect();
    if parts.len() >= 2 {
        (parts[0].to_string(), parts[1..].join(" "))
    } else {
        (name.to_string(), String::new())
    }
}

/// Format dB value for display with unit suffix
fn format_db(db: f32) -> String {
    if db <= -60.0 {
        "-\u{221E}dB".to_string()
    } else if db >= 0.0 {
        format!("+{:.1}dB", db)
    } else {
        format!("{:.1}dB", db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_db_uses_proper_notation() {
        assert!(format_db(0.0).ends_with("dB"), "Must use 'dB' not 'db'");
        assert!(format_db(-6.0).ends_with("dB"));
        assert!(format_db(-60.0).ends_with("dB")); // -inf case
    }

    #[test]
    fn test_format_db_max_length() {
        let cases = [0.0, 6.0, 12.0, -6.0, -12.5, -59.9, -60.0, -100.0];
        for db in cases {
            let s = format_db(db);
            assert!(
                s.chars().count() <= 7,
                "format_db({db}) = \"{s}\" exceeds 7 chars"
            );
        }
    }
}
