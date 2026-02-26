//! Mixer page with faders, meters, categories, and presets
//!
//! Uses WebSocket for real-time bidirectional communication with REAPER.

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api::{BatchOperation, Channel, batch_control};
use crate::auth::can_access_member;
use crate::components::category_tabs::{Category, CategoryTabs};
use crate::components::fader::Fader;
use crate::components::meter::Meter;
use crate::components::pan::PanKnob;
use crate::components::preset_modal::{ChannelState, PresetData, PresetModal};
use crate::components::toolbar::Toolbar;

/// Processed channel for display (handles stereo pairs)
#[derive(Debug, Clone)]
struct DisplayChannel {
    track_index: usize,
    display_name: String,
    level_db: f32,
    pan: f32,
    muted: bool,
    #[allow(dead_code)]
    category: String,
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

/// Convert iem_core::Channel to crate::api::Channel
fn convert_channel(ch: iem_core::Channel) -> Channel {
    Channel {
        track_index: ch.track_index,
        name: ch.name,
        level_db: ch.level_db,
        pan: ch.pan,
        muted: ch.muted,
        category: ch.category,
        stereo_pair: ch.stereo_pair,
        stereo_side: ch.stereo_side,
    }
}

/// Create and connect a WebSocket, wiring up message handlers to signals
fn connect_websocket(
    member: &str,
    set_ws: WriteSignal<Option<web_sys::WebSocket>>,
    set_channels: WriteSignal<Vec<Channel>>,
    set_meters: WriteSignal<HashMap<usize, f32>>,
    set_connected: WriteSignal<bool>,
    set_loading: WriteSignal<bool>,
    fader_touched: ReadSignal<HashMap<usize, bool>>,
) {
    let location = web_sys::window().unwrap().location();
    let host = location.host().unwrap_or_default();
    let protocol = if location.protocol().unwrap_or_default() == "https:" {
        "wss"
    } else {
        "ws"
    };
    let ws_url = format!("{}://{}/ws/{}", protocol, host, member);

    let ws = match web_sys::WebSocket::new(&ws_url) {
        Ok(ws) => ws,
        Err(e) => {
            web_sys::console::error_1(&format!("WS connect error: {:?}", e).into());
            return;
        }
    };

    set_ws.set(Some(ws.clone()));

    // Handle incoming messages
    let onmessage = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
        if let Some(text) = e.data().as_string() {
            if let Ok(msg) = serde_json::from_str::<iem_core::ServerMsg>(&text) {
                let touched = fader_touched.get_untracked();
                match msg {
                    iem_core::ServerMsg::State {
                        channels: new_chs,
                        connected: conn,
                    } => {
                        set_channels.update(|chs| {
                            if chs.is_empty() {
                                // First state — populate
                                *chs = new_chs.into_iter().map(convert_channel).collect();
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
                        set_connected.set(conn);
                        set_loading.set(false);
                    }
                    iem_core::ServerMsg::Meters { meters: m } => {
                        // Debug: log meter summary occasionally
                        static mut LOG_COUNT: u32 = 0;
                        unsafe {
                            LOG_COUNT += 1;
                            if LOG_COUNT % 66 == 1 {
                                let non_zero = m.iter().filter(|(_, v)| **v > 0.001).count();
                                web_sys::console::log_1(
                                    &format!(
                                        "Meters: {} entries, {} with signal",
                                        m.len(),
                                        non_zero
                                    )
                                    .into(),
                                );
                            }
                        }
                        set_meters.set(m);
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
                    iem_core::ServerMsg::ConnectionChanged { connected: conn } => {
                        set_connected.set(conn);
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
    let navigate = use_navigate();
    let navigate_back = navigate.clone();

    // Get member ID from route params
    let member_id = move || {
        params
            .get()
            .get("member")
            .map(|s| s.to_string())
            .unwrap_or_default()
    };

    // Check auth on mount
    Effect::new(move |_| {
        let member = member_id();
        if !can_access_member(&member) {
            let nav = navigate.clone();
            nav(
                &format!("/login?member={}&next=/{}", member, member),
                Default::default(),
            );
        }
    });

    // Reactive state
    let (channels, set_channels) = signal(Vec::<Channel>::new());
    let (meters, set_meters) = signal(HashMap::<usize, f32>::new());
    let (connected, set_connected) = signal(false);
    let (active_category, set_active_category) = signal(Category::All);
    let (preset_modal_visible, set_preset_modal_visible) = signal(false);
    let (fader_touched, set_fader_touched) = signal(HashMap::<usize, bool>::new());
    let (loading, set_loading) = signal(true);
    let (soloed, set_soloed) = signal(std::collections::HashSet::<usize>::new());
    let (pre_solo_mutes, set_pre_solo_mutes) = signal(HashMap::<usize, bool>::new());

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
            set_ws,
            set_channels,
            set_meters,
            set_connected,
            set_loading,
            fader_touched,
        );
    });

    // Auto-reconnect: check every 2s if WebSocket is closed
    let reconnect_member_id = member_id.clone();
    let reconnect_handle = gloo_timers::callback::Interval::new(2000, move || {
        let needs_reconnect = match ws.get_untracked() {
            Some(ref w) => w.ready_state() == web_sys::WebSocket::CLOSED,
            None => false,
        };
        if needs_reconnect {
            let member = reconnect_member_id();
            if !member.is_empty() {
                connect_websocket(
                    &member,
                    set_ws,
                    set_channels,
                    set_meters,
                    set_connected,
                    set_loading,
                    fader_touched,
                );
            }
        }
    });
    std::mem::forget(reconnect_handle);

    // Handle back button
    let on_back = move |_| {
        navigate_back("/", Default::default());
    };

    // Process channels for display (handle stereo pairs)
    let display_channels = move || {
        let chs = channels.get();
        let member = member_id();
        let my_input = format!("{} mic", member.to_uppercase());
        let active_cat = active_category.get();

        let mut result = Vec::new();
        let mut seen_pairs: std::collections::HashSet<String> = std::collections::HashSet::new();

        for ch in &chs {
            if ch.stereo_side.as_deref() == Some("R") {
                continue;
            }
            if !active_cat.matches(&ch.category) {
                continue;
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
            let is_my_input = ch.name.to_uppercase() == my_input;

            result.push(DisplayChannel {
                track_index: ch.track_index,
                display_name,
                level_db: ch.level_db,
                pan: ch.pan,
                muted: ch.muted,
                category: ch.category.clone(),
                is_stereo,
                partner_index,
                is_my_input,
            });
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
    let on_presets = Callback::new(move |_: ()| {
        set_preset_modal_visible.set(true);
    });

    let more_me_member_id = member_id.clone();
    let on_more_me = Callback::new(move |_: ()| {
        if !connected.get() {
            web_sys::console::warn_1(&"+Me blocked: not connected to REAPER".into());
            return;
        }
        let member = more_me_member_id();
        spawn_local(async move {
            if let Err(e) = batch_control(&member, BatchOperation::MoreMe).await {
                web_sys::console::error_1(&format!("+Me API error: {:?}", e).into());
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
                <div class=move || {
                    if connected.get() {
                        "status-dot connected"
                    } else {
                        "status-dot error"
                    }
                } />
            </header>

            <CategoryTabs
                active=active_category.into()
                on_select=move |cat| set_active_category.set(cat)
            />

            <Show
                when=move || !connected.get() && !loading.get()
                fallback=|| ()
            >
                <div class="disconnected-warning">
                    "DISCONNECTED - Controls disabled (REAPER not reachable)"
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
                        />
                    </div>
                </div>
            </Show>

            <Toolbar
                on_presets=on_presets
                on_more_me=on_more_me
            />

            <PresetModal
                visible=preset_modal_visible.into()
                member_id=member_id()
                on_close=on_close_modal
                on_load=on_load_preset
                get_current_state=get_current_state
            />
        </div>
    }
}

/// Channel list component to handle individual channel rendering
#[component]
fn ChannelList(
    display_channels: Signal<Vec<DisplayChannel>>,
    meters: ReadSignal<HashMap<usize, f32>>,
    channels: ReadSignal<Vec<Channel>>,
    set_channels: WriteSignal<Vec<Channel>>,
    set_fader_touched: WriteSignal<HashMap<usize, bool>>,
    soloed: ReadSignal<std::collections::HashSet<usize>>,
    set_soloed: WriteSignal<std::collections::HashSet<usize>>,
    pre_solo_mutes: ReadSignal<HashMap<usize, bool>>,
    set_pre_solo_mutes: WriteSignal<HashMap<usize, bool>>,
    connected: ReadSignal<bool>,
    ws: ReadSignal<Option<web_sys::WebSocket>>,
) -> impl IntoView {
    move || {
        let chs = display_channels.get();
        if chs.is_empty() {
            view! {
                <div class="no-channels">"No channels in this category"</div>
            }
            .into_any()
        } else {
            view! {
                <>
                    {chs.iter().map(|ch| {
                        let track_idx = ch.track_index;
                        let partner_idx = ch.partner_index;
                        let name = ch.display_name.clone();
                        let is_my = ch.is_my_input;
                        let is_stereo = ch.is_stereo;

                        // Derived signals that properly track channel updates from WebSocket
                        let level_signal = Signal::derive(move || {
                            channels.get()
                                .iter()
                                .find(|c| c.track_index == track_idx)
                                .map(|c| c.level_db)
                                .unwrap_or(-60.0)
                        });

                        let muted_signal = Signal::derive(move || {
                            channels.get()
                                .iter()
                                .find(|c| c.track_index == track_idx)
                                .map(|c| c.muted)
                                .unwrap_or(false)
                        });

                        let pan_signal = Signal::derive(move || {
                            channels.get()
                                .iter()
                                .find(|c| c.track_index == track_idx)
                                .map(|c| c.pan)
                                .unwrap_or(0.5)
                        });

                        let meter_level = Signal::derive(move || {
                            meters.get().get(&track_idx).copied().unwrap_or(0.0)
                        });

                        // Level change handler
                        let on_level_change = Callback::new(move |new_level: f32| {
                            if !connected.get() {
                                return;
                            }

                            // Mark as touched
                            set_fader_touched.update(|t| {
                                t.insert(track_idx, true);
                                if let Some(partner) = partner_idx {
                                    t.insert(partner, true);
                                }
                            });

                            // Optimistic update
                            set_channels.update(|chs| {
                                if let Some(ch) = chs.iter_mut().find(|c| c.track_index == track_idx) {
                                    ch.level_db = new_level;
                                }
                                if let Some(partner) = partner_idx {
                                    if let Some(ch) = chs.iter_mut().find(|c| c.track_index == partner) {
                                        ch.level_db = new_level;
                                    }
                                }
                            });

                            // Send via WebSocket (instant, no async)
                            ws_send(ws, &iem_core::ClientMsg::SetLevel {
                                track_index: track_idx,
                                level_db: new_level,
                            });
                            if let Some(partner) = partner_idx {
                                ws_send(ws, &iem_core::ClientMsg::SetLevel {
                                    track_index: partner,
                                    level_db: new_level,
                                });
                            }

                            // Clear touched flag after delay
                            gloo_timers::callback::Timeout::new(200, move || {
                                set_fader_touched.update(|t| {
                                    t.remove(&track_idx);
                                    if let Some(p) = partner_idx {
                                        t.remove(&p);
                                    }
                                });
                            }).forget();
                        });

                        // Pan change handler
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

                            set_channels.update(|chs| {
                                if let Some(ch) = chs.iter_mut().find(|c| c.track_index == track_idx) {
                                    ch.pan = new_pan;
                                }
                                if let Some(partner) = partner_idx {
                                    if let Some(ch) = chs.iter_mut().find(|c| c.track_index == partner) {
                                        ch.pan = 1.0 - new_pan;
                                    }
                                }
                            });

                            ws_send(ws, &iem_core::ClientMsg::SetPan {
                                track_index: track_idx,
                                pan: new_pan,
                            });
                            if let Some(partner) = partner_idx {
                                ws_send(ws, &iem_core::ClientMsg::SetPan {
                                    track_index: partner,
                                    pan: 1.0 - new_pan,
                                });
                            }

                            gloo_timers::callback::Timeout::new(1000, move || {
                                set_fader_touched.update(|t| {
                                    t.remove(&track_idx);
                                    if let Some(p) = partner_idx {
                                        t.remove(&p);
                                    }
                                });
                            }).forget();
                        });

                        // Mute toggle handler
                        let on_mute_click = move |_| {
                            if !connected.get() {
                                return;
                            }

                            let current_muted = channels.get()
                                .iter()
                                .find(|c| c.track_index == track_idx)
                                .map(|c| c.muted)
                                .unwrap_or(false);
                            let new_muted = !current_muted;

                            set_fader_touched.update(|t| {
                                t.insert(track_idx, true);
                                if let Some(partner) = partner_idx {
                                    t.insert(partner, true);
                                }
                            });

                            set_channels.update(|chs| {
                                if let Some(ch) = chs.iter_mut().find(|c| c.track_index == track_idx) {
                                    ch.muted = new_muted;
                                }
                                if let Some(partner) = partner_idx {
                                    if let Some(ch) = chs.iter_mut().find(|c| c.track_index == partner) {
                                        ch.muted = new_muted;
                                    }
                                }
                            });

                            ws_send(ws, &iem_core::ClientMsg::SetMute {
                                track_index: track_idx,
                                muted: new_muted,
                            });
                            if let Some(partner) = partner_idx {
                                ws_send(ws, &iem_core::ClientMsg::SetMute {
                                    track_index: partner,
                                    muted: new_muted,
                                });
                            }

                            gloo_timers::callback::Timeout::new(1000, move || {
                                set_fader_touched.update(|t| {
                                    t.remove(&track_idx);
                                    if let Some(p) = partner_idx {
                                        t.remove(&p);
                                    }
                                });
                            }).forget();
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

                        let is_soloed = move || soloed.get().contains(&track_idx);
                        let is_connected = move || connected.get();

                        view! {
                            <div class=move || {
                                let mut classes = vec!["channel"];
                                if muted_signal.get() { classes.push("muted"); }
                                if is_my { classes.push("more-me"); }
                                if is_stereo { classes.push("stereo-pair"); }
                                if !is_connected() { classes.push("disconnected"); }
                                classes.join(" ")
                            }>
                                <div class="ch-label">
                                    <div class="ch-name">{parse_track_name(&name).0}</div>
                                    <div class="ch-type">
                                        {parse_track_name(&name).1}
                                        {if is_stereo { " (st)" } else { "" }}
                                    </div>
                                </div>

                                <Meter level=meter_level />

                                <div class="fader-area">
                                    <Fader
                                        value=level_signal
                                        min=-60.0
                                        max=12.0
                                        on_change=move |v| on_level_change.run(v)
                                    />
                                    <div class="db-display">{move || format_db(level_signal.get())}</div>
                                </div>

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
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </>
            }.into_any()
        }
    }
}

/// Parse track name into main and type parts
fn parse_track_name(name: &str) -> (String, String) {
    let parts: Vec<&str> = name.split_whitespace().collect();
    if parts.len() >= 2 {
        let main = if parts[0].len() > 7 {
            parts[0][..6].to_string()
        } else {
            parts[0].to_string()
        };
        (main, parts[1..].join(" "))
    } else {
        (name.to_string(), String::new())
    }
}

/// Format dB value for display
fn format_db(db: f32) -> String {
    if db <= -60.0 {
        "-\u{221E}".to_string()
    } else if db >= 0.0 {
        format!("+{:.1}", db)
    } else {
        format!("{:.1}", db)
    }
}
