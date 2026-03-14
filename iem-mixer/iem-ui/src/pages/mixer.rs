//! Mixer page with faders, meters, categories, and presets
//!
//! Uses WebSocket for real-time bidirectional communication with REAPER.

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

use crate::api::Channel;
use crate::components::category_tabs::{Category, CategoryTabs};
use crate::components::fader::Fader;
use crate::components::meter::Meter;
use crate::components::pan::PanKnob;
use crate::components::pin_change_modal::PinChangeModal;
use crate::components::preset_modal::{ChannelState, PresetData, PresetModal};
use crate::components::settings_modal::{SettingsModal, UserSettings};
use crate::components::snapshot_modal::SnapshotModal;
use crate::components::toolbar::Toolbar;

/// Post-release guard duration in milliseconds.
/// With server-side echo suppression, this only needs to cover WebSocket round-trip (~10-20ms).
const POST_RELEASE_GUARD_MS: i32 = 100;

/// Minimum interval between WebSocket sends per track (ms).
/// Limits to ~20 commands/sec to avoid overwhelming the server.
const THROTTLE_INTERVAL_MS: f64 = 50.0;

/// Processed channel for display (handles stereo pairs)
/// Note: level_db, pan, muted are read via derived signals from channels
#[derive(Debug, Clone)]
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

/// Create and connect a WebSocket, wiring up message handlers to signals
fn connect_websocket(
    member: &str,
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
) {
    // Close previous WebSocket if exists (prevents closure leak on reconnect)
    if let Some(Some(old_ws)) = ws.try_get_untracked() {
        old_ws.set_onmessage(None);
        old_ws.set_onclose(None);
        old_ws.set_onerror(None);
        let _ = old_ws.close();
    }

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

    // Handle incoming messages
    let onmessage = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        if let Some(text) = e.data().as_string() {
            if let Ok(msg) = serde_json::from_str::<iem_core::ServerMsg>(&text) {
                let touched = fader_touched.get_untracked();
                match msg {
                    iem_core::ServerMsg::State {
                        channels: new_chs,
                        connected: conn,
                        global_level_db,
                        global_muted,
                    } => {
                        set_channels.update(|chs| {
                            if chs.is_empty() {
                                // First state — populate
                                *chs = new_chs;
                            } else {
                                // Update non-touched channels
                                for new_ch in &new_chs {
                                    if !touched.get(&new_ch.track_index).copied().unwrap_or(false) {
                                        if let Some(ch) = chs
                                            .iter_mut()
                                            .find(|c| c.track_index == new_ch.track_index)
                                        {
                                            ch.level_db = new_ch.level_db;
                                            ch.muted = new_ch.muted;
                                            ch.pan = new_ch.pan;
                                        }
                                    }
                                }
                            }
                        });
                        // Update global volume from initial state
                        if let Some(lvl) = global_level_db {
                            set_global_level.set(lvl);
                        }
                        if let Some(muted) = global_muted {
                            set_global_muted.set(muted);
                        }
                        set_connected.set(conn);
                        set_loading.set(false);
                    }
                    iem_core::ServerMsg::Meters { meters: m } => {
                        set_meters.set(m);
                        set_data_pulse.update(|v| *v = !*v);
                    }
                    iem_core::ServerMsg::ChannelUpdate {
                        track_index,
                        level_db,
                        muted,
                        pan,
                    } => {
                        if !touched.get(&track_index).copied().unwrap_or(false) {
                            set_channels.update(|chs| {
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
                            set_global_level.set(level_db);
                            set_global_muted.set(muted);
                        }
                    }
                    iem_core::ServerMsg::ConnectionChanged { connected: conn } => {
                        set_connected.set(conn);
                    }
                    iem_core::ServerMsg::CustomizationUpdate { pinned, hidden } => {
                        set_pinned_channels.set(pinned);
                        set_hidden_channels.set(hidden);
                    }
                    iem_core::ServerMsg::NetworkMode { mode } => {
                        set_network_mode.set(mode);
                    }
                }
            }
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    // Handle close — mark disconnected (reconnect interval will handle retry)
    let onclose = Closure::wrap(Box::new(move |_: web_sys::CloseEvent| {
        set_connected.set(false);
    }) as Box<dyn FnMut(web_sys::CloseEvent)>);
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();
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

    // Channel customization (pin/hide) — loaded from server via WS
    let (pinned_channels, set_pinned_channels) = signal(Vec::<usize>::new());
    let (hidden_channels, set_hidden_channels) = signal(Vec::<usize>::new());

    // Network mode indicator (local LAN vs remote internet)
    let (network_mode, set_network_mode) = signal(String::new());

    // WebSocket connection
    let (ws, set_ws) = signal(Option::<web_sys::WebSocket>::None);

    // Connect WebSocket when member is known
    let ws_member_id = member_id.clone();
    Effect::new(move |_| {
        let member = ws_member_id();
        if member.is_empty() {
            return;
        }

        connect_websocket(
            &member,
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
        );
    });

    // Network mode (LAN/WAN) is now sent via WebSocket on every connect/reconnect,
    // so it automatically updates when switching between WiFi and mobile data.

    // Auto-reconnect: check every 2s if WebSocket is closed.
    // Uses raw JS setInterval to get an i32 handle (Send+Sync) for on_cleanup,
    // since gloo_timers::Interval contains non-Send closures.
    let reconnect_member_id = member_id.clone();
    let reconnect_closure = Closure::wrap(Box::new(move || {
        let needs_reconnect = match ws.get_untracked() {
            Some(ref w) => w.ready_state() == web_sys::WebSocket::CLOSED,
            None => false,
        };
        if needs_reconnect {
            let member = reconnect_member_id();
            if !member.is_empty() {
                connect_websocket(
                    &member,
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
                );
            }
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

    // Clean up reconnect interval on component unmount.
    // The i32 interval_id is Send+Sync so it can be captured in on_cleanup.
    // The WebSocket signal and its closures are dropped with the component.
    on_cleanup(move || {
        if let Some(w) = web_sys::window() {
            w.clear_interval_with_handle(interval_id);
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
    let display_channels = move || {
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
    };

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
        PresetData {
            channels: channel_states,
            created_at: None,
            updated_at: None,
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
    let is_engineer = crate::auth::get_auth().map(|a| a.engineer).unwrap_or(false);

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
                <div class="header-version">
                    <span class="header-version-number">{iem_core::version_label()}</span>
                    <span class="header-version-date">{iem_core::build_datetime()}</span>
                </div>
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
                            />
                        </Show>
                        <ChannelList
                            display_channels=Signal::derive(display_channels)
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
                        />
                    </div>
                </div>
            </Show>

            <Toolbar
                on_presets=on_presets
                on_history=on_history
                is_engineer=is_engineer
                on_mute_all=on_mute_all
            />

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

            <div class="meter-stereo">
                <div class="meter-bar"><div class="meter-fill" style="width:0%"></div></div>
                <div class="meter-bar"><div class="meter-fill" style="width:0%"></div></div>
            </div>

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
                <div class="solo-btn off" style="visibility: hidden">"S"</div>
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
                key=|ch| ch.track_index
                children=move |ch| {
                    let track_idx = ch.track_index;
                    let partner_idx = ch.partner_index;
                    let name = ch.display_name.clone();
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
                            let mut new_soloed = current_soloed.clone();
                            new_soloed.remove(&track_idx);
                            if let Some(partner) = partner_idx {
                                new_soloed.remove(&partner);
                            }

                            if new_soloed.is_empty() {
                                let saved = pre_solo_mutes.get();
                                for ch in &all_channels {
                                    let should_be_muted = saved.get(&ch.track_index).copied().unwrap_or(false);
                                    let idx = ch.track_index;
                                    set_channels.update(|chs| {
                                        if let Some(c) = chs.iter_mut().find(|c| c.track_index == idx) {
                                            c.muted = should_be_muted;
                                        }
                                    });
                                    ws_send(ws, &iem_core::ClientMsg::SetMute {
                                        track_index: idx,
                                        muted: should_be_muted,
                                    });
                                }
                                set_pre_solo_mutes.set(HashMap::new());
                            } else {
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
                                ws_send(ws, &iem_core::ClientMsg::SetMute {
                                    track_index: track_idx,
                                    muted: true,
                                });
                                if let Some(partner) = partner_idx {
                                    ws_send(ws, &iem_core::ClientMsg::SetMute {
                                        track_index: partner,
                                        muted: true,
                                    });
                                }
                            }
                            set_soloed.set(new_soloed);
                        } else {
                            let was_empty = current_soloed.is_empty();

                            if was_empty {
                                let mut saved_mutes = HashMap::new();
                                for ch in &all_channels {
                                    saved_mutes.insert(ch.track_index, ch.muted);
                                }
                                set_pre_solo_mutes.set(saved_mutes);

                                for ch in &all_channels {
                                    let should_mute = ch.track_index != track_idx && partner_idx != Some(ch.track_index);
                                    let idx = ch.track_index;
                                    set_channels.update(|chs| {
                                        if let Some(c) = chs.iter_mut().find(|c| c.track_index == idx) {
                                            c.muted = should_mute;
                                        }
                                    });
                                    ws_send(ws, &iem_core::ClientMsg::SetMute {
                                        track_index: idx,
                                        muted: should_mute,
                                    });
                                }
                            } else {
                                set_channels.update(|chs| {
                                    if let Some(ch) = chs.iter_mut().find(|c| c.track_index == track_idx) {
                                        ch.muted = false;
                                    }
                                    if let Some(partner) = partner_idx {
                                        if let Some(ch) = chs.iter_mut().find(|c| c.track_index == partner) {
                                            ch.muted = false;
                                        }
                                    }
                                });
                                ws_send(ws, &iem_core::ClientMsg::SetMute {
                                    track_index: track_idx,
                                    muted: false,
                                });
                                if let Some(partner) = partner_idx {
                                    ws_send(ws, &iem_core::ClientMsg::SetMute {
                                        track_index: partner,
                                        muted: false,
                                    });
                                }
                            }

                            let mut new_soloed = current_soloed.clone();
                            new_soloed.insert(track_idx);
                            if let Some(partner) = partner_idx {
                                new_soloed.insert(partner);
                            }
                            set_soloed.set(new_soloed);
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
                                        on:click=move |ev: web_sys::MouseEvent| { on_pin_click(ev); set_open_menu.set(None); }
                                    >
                                        <span class="menu-icon">{move || if ch_is_pinned() { "\u{2605}" } else { "\u{2606}" }}</span>
                                        {move || if ch_is_pinned() { "Unpin" } else { "Pin to Main" }}
                                    </button>
                                    <button
                                        class="ch-menu-item"
                                        on:click=move |ev: web_sys::MouseEvent| { on_hide_click(ev); set_open_menu.set(None); }
                                    >
                                        <span class="menu-icon">{move || if is_hidden_tab() { "\u{25C9}" } else { "\u{2715}" }}</span>
                                        {move || if is_hidden_tab() { "Unhide" } else { "Hide" }}
                                    </button>
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
