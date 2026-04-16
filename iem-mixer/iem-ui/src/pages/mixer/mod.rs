//! Mixer page with faders, meters, categories, and presets
//!
//! Uses WebSocket for real-time bidirectional communication with REAPER.

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

use crate::components::alert_toast::AlertToast;
use crate::components::category_tabs::{Category, CategoryTabs};
use crate::components::eq_modal::EQModal;
use crate::components::limiter_modal::LimiterModal;
use crate::components::pin_change_modal::PinChangeModal;
use crate::components::preset_modal::{ChannelState, PresetData, PresetModal};
use crate::components::settings_modal::SettingsModal;
use crate::components::snapshot_modal::SnapshotModal;
use crate::components::toolbar::Toolbar;

mod components;
mod connection;
mod helpers;
mod push;
mod state;

use components::{ChannelList, GlobalVolumeFader, StemsVolumeFader};
use connection::ConnectionManager;
use helpers::*;
use push::subscribe_to_push;
use state::MixerState;

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

    let state = MixerState::new(&member_id());

    // Destructure all signals into local variables for use throughout MixerPage body
    let (channels, set_channels) = state.channels;
    let (meters, set_meters) = state.meters;
    let (connected, set_connected) = state.connected;
    let (active_category, set_active_category) = state.active_category;
    let (preset_modal_visible, set_preset_modal_visible) = state.preset_modal_visible;
    let (pin_modal_visible, set_pin_modal_visible) = state.pin_modal_visible;
    let (settings_modal_visible, set_settings_modal_visible) = state.settings_modal_visible;
    let (snapshot_modal_visible, set_snapshot_modal_visible) = state.snapshot_modal_visible;
    let (has_photo, set_has_photo) = state.has_photo;
    let (double_tap_fader, set_double_tap_fader) = state.double_tap_fader;
    let (fader_touched, set_fader_touched) = state.fader_touched;
    let (loading, set_loading) = state.loading;
    let (soloed, set_soloed) = state.soloed;
    let (pre_solo_mutes, set_pre_solo_mutes) = state.pre_solo_mutes;
    let (data_pulse, set_data_pulse) = state.data_pulse;
    let (global_level, set_global_level) = state.global_level;
    let (global_muted, set_global_muted) = state.global_muted;
    let (global_touched, set_global_touched) = state.global_touched;
    let (stems_level, set_stems_level) = state.stems_level;
    let (stems_muted, set_stems_muted) = state.stems_muted;
    let (stems_touched, set_stems_touched) = state.stems_touched;
    let (stems_bus_idx, set_stems_bus_idx) = state.stems_bus_idx;
    let (eq_open, set_eq_open) = state.eq_open;
    let (eq_bands, set_eq_bands) = state.eq_bands;
    let (eq_loading, set_eq_loading) = state.eq_loading;
    let (limiter_open, set_limiter_open) = state.limiter_open;
    let (limiter_limit_db, set_limiter_limit_db) = state.limiter_limit_db;
    let (limiter_limit_norm, set_limiter_limit_norm) = state.limiter_limit_norm;
    let (limiter_enabled, set_limiter_enabled) = state.limiter_enabled;
    let (limiter_loading, set_limiter_loading) = state.limiter_loading;
    let (limiter_active_seconds, set_limiter_active_seconds) = state.limiter_active_seconds;
    let (pinned_channels, set_pinned_channels) = state.pinned_channels;
    let (hidden_channels, set_hidden_channels) = state.hidden_channels;
    let (network_mode, set_network_mode) = state.network_mode;
    let (output_track_idx, set_output_track_idx) = state.output_track_idx;
    let (alert_data, set_alert_data) = state.alert_data;
    let (alert_active, set_alert_active) = state.alert_active;
    let (talk_state, set_talk_state) = state.talk_state;
    let (engineer_talking, set_engineer_talking) = state.engineer_talking;
    let (ws, set_ws) = state.ws;

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

    // ConnectionManager owns all background tasks (WS connect, reconnect,
    // watchdog, token-expiry). Dropping it clears all JS intervals and sets
    // the disposal_guard so forgotten closures no-op. The _connection binding
    // keeps it alive for the component's lifetime.
    let _connection = ConnectionManager::new(state, member_id.clone());

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
        let _ = set_channels.try_update(|chs| {
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
        let _ = set_preset_modal_visible.try_set(true);
    });

    let on_history = Callback::new(move |_: ()| {
        let _ = set_snapshot_modal_visible.try_set(true);
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
        let _ = set_preset_modal_visible.try_set(false);
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
                            let _ = set_channels.try_update(|chs| {
                                for c in chs.iter_mut() {
                                    let should_be_muted = saved.get(&c.track_index).copied().unwrap_or(false);
                                    c.muted = should_be_muted;
                                }
                            });
                            let _ = set_pre_solo_mutes.try_set(HashMap::new());
                            let _ = set_soloed.try_set(std::collections::HashSet::new());
                            // Send empty SetSolo — server restores REAPER mutes and broadcasts
                            ws_send(ws, &iem_core::ClientMsg::SetSolo { soloed: vec![] });
                        }
                    >
                        "SOLO"
                        <span class="solo-close">"\u{2715}"</span>
                    </button>
                </Show>
                <button class="settings-btn" on:click=move |_| { let _ = set_settings_modal_visible.try_set(true); }>
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
                on_select=move |cat| { let _ = set_active_category.try_set(cat); }
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
                on_close=Callback::new(move |_: ()| { let _ = set_settings_modal_visible.try_set(false); })
                on_open_pin_change=Callback::new(move |_: ()| { let _ = set_pin_modal_visible.try_set(true); })
                double_tap_fader=double_tap_fader
                set_double_tap_fader=set_double_tap_fader
                member_id=member_id()
                is_engineer=is_engineer_own_mixer
                has_photo=has_photo.into()
                set_has_photo=set_has_photo
            />

            <PinChangeModal
                visible=pin_modal_visible.into()
                on_close=Callback::new(move |_: ()| { let _ = set_pin_modal_visible.try_set(false); })
                member_id=member_id()
            />

            <SnapshotModal
                visible=snapshot_modal_visible.into()
                member_id=member_id()
                on_close=Callback::new(move |_: ()| { let _ = set_snapshot_modal_visible.try_set(false); })
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
                                    let _ = set_eq_open.try_set(None);
                                    let _ = set_eq_bands.try_set(Vec::new());
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
                            active_seconds=limiter_active_seconds
                            on_reset=Callback::new(move |_: ()| {
                                if let Some((ti, _)) = limiter_open.get_untracked() {
                                    ws_send(ws_for_lim, &iem_core::ClientMsg::ResetLimiterActivity {
                                        track_index: ti,
                                    });
                                    // Optimistic local update so the user sees "never" immediately.
                                    let _ = set_limiter_active_seconds.try_set(0.0);
                                }
                            })
                            on_param_change=Callback::new(move |(param, value): (String, f32)| {
                                if let Some((ti, _)) = limiter_open.get_untracked() {
                                    // Optimistic local update (no server echo)
                                    let _ = set_limiter_limit_norm.try_set(value);
                                    let _ = set_limiter_limit_db.try_set(value * 6.0 - 6.0);
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
                                    let _ = set_limiter_enabled.try_set(en);
                                }
                            })
                            on_close=Callback::new(move |_: ()| {
                                let cb = Closure::once_into_js(move || {
                                    let _ = set_limiter_open.try_set(None);
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
