//! Mixer page with faders, meters, categories, and presets

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::collections::HashMap;
use wasm_bindgen_futures::spawn_local;

use crate::api::{
    BatchOperation, Channel, batch_control, poll_mixer_state, set_send_level, set_send_mute,
    set_send_pan,
};
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
    // Solo state: track indices that are currently soloed
    let (soloed, set_soloed) = signal(std::collections::HashSet::<usize>::new());
    // Pre-solo mute states: saved when first solo is engaged
    let (pre_solo_mutes, set_pre_solo_mutes) = signal(HashMap::<usize, bool>::new());

    // Poll for updates
    let poll_member_id = member_id.clone();
    Effect::new(move |_| {
        let member = poll_member_id();
        if member.is_empty() {
            return;
        }

        // Initial fetch
        let member_clone = member.clone();
        spawn_local(async move {
            if let Ok(resp) = poll_mixer_state(&member_clone).await {
                set_channels.set(resp.channels);
                set_meters.set(resp.meters);
                set_connected.set(resp.connected);
                set_loading.set(false);
            } else {
                set_loading.set(false);
            }
        });

        // Set up polling interval
        let member_for_interval = member.clone();
        let interval_handle = gloo_timers::callback::Interval::new(500, move || {
            let member = member_for_interval.clone();
            let touched = fader_touched.get_untracked();

            spawn_local(async move {
                if let Ok(resp) = poll_mixer_state(&member).await {
                    // Only update channels that aren't being touched
                    set_channels.update(|chs| {
                        for new_ch in &resp.channels {
                            if !touched.get(&new_ch.track_index).copied().unwrap_or(false) {
                                if let Some(ch) =
                                    chs.iter_mut().find(|c| c.track_index == new_ch.track_index)
                                {
                                    ch.level_db = new_ch.level_db;
                                    ch.muted = new_ch.muted;
                                    ch.pan = new_ch.pan;
                                }
                            }
                        }
                    });
                    set_meters.set(resp.meters);
                    set_connected.set(resp.connected);
                }
            });
        });

        // Keep interval alive
        std::mem::forget(interval_handle);
    });

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
            // Skip R side of stereo pairs
            if ch.stereo_side.as_deref() == Some("R") {
                continue;
            }

            // Skip if category doesn't match
            if !active_cat.matches(&ch.category) {
                continue;
            }

            // Check if this is L side of a stereo pair
            let (is_stereo, partner_index) = if ch.stereo_side.as_deref() == Some("L") {
                if let Some(ref pair_name) = ch.stereo_pair {
                    if seen_pairs.contains(pair_name) {
                        continue;
                    }
                    seen_pairs.insert(pair_name.clone());

                    // Find R partner
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

            // Create display name (strip " L" suffix for stereo)
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

    // Preset handlers - use Callback
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

    let load_preset_member_id = member_id.clone();
    let on_load_preset = Callback::new(move |preset: PresetData| {
        // CRITICAL SAFETY: Block preset loading when disconnected
        if !connected.get() {
            web_sys::console::warn_1(&"Preset loading blocked: not connected to REAPER".into());
            return;
        }

        let member = load_preset_member_id();

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

        // Send all changes to server with error handling
        let preset_clone = preset.clone();
        spawn_local(async move {
            let mut errors = 0;
            for (track_index, state) in preset_clone.channels {
                if let Err(e) = set_send_level(&member, track_index, state.vol).await {
                    web_sys::console::error_1(&format!("Preset level error: {:?}", e).into());
                    errors += 1;
                }
                if let Err(e) = set_send_mute(&member, track_index, state.mute).await {
                    web_sys::console::error_1(&format!("Preset mute error: {:?}", e).into());
                    errors += 1;
                }
                if let Err(e) = set_send_pan(&member, track_index, state.pan).await {
                    web_sys::console::error_1(&format!("Preset pan error: {:?}", e).into());
                    errors += 1;
                }
            }
            if errors > 0 {
                web_sys::console::warn_1(&format!("Preset loaded with {} API errors", errors).into());
            }
        });
    });

    // Toolbar callbacks
    let on_presets = Callback::new(move |_: ()| {
        set_preset_modal_visible.set(true);
    });

    let more_me_member_id = member_id.clone();
    let on_more_me = Callback::new(move |_: ()| {
        // CRITICAL SAFETY: Block when disconnected
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
                    // Capitalize member name
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

            // SAFETY: Show warning when disconnected from REAPER
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
                            member_id=Signal::derive(member_id.clone())
                            channels=channels
                            set_channels=set_channels
                            _fader_touched=fader_touched
                            set_fader_touched=set_fader_touched
                            soloed=soloed
                            set_soloed=set_soloed
                            pre_solo_mutes=pre_solo_mutes
                            set_pre_solo_mutes=set_pre_solo_mutes
                            connected=connected
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
    member_id: Signal<String>,
    channels: ReadSignal<Vec<Channel>>,
    set_channels: WriteSignal<Vec<Channel>>,
    _fader_touched: ReadSignal<HashMap<usize, bool>>,
    set_fader_touched: WriteSignal<HashMap<usize, bool>>,
    soloed: ReadSignal<std::collections::HashSet<usize>>,
    set_soloed: WriteSignal<std::collections::HashSet<usize>>,
    pre_solo_mutes: ReadSignal<HashMap<usize, bool>>,
    set_pre_solo_mutes: WriteSignal<HashMap<usize, bool>>,
    /// Connection state - controls are disabled when not connected (SAFETY)
    connected: ReadSignal<bool>,
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
                        let level = ch.level_db;
                        let muted = ch.muted;
                        let pan = ch.pan;
                        let is_my = ch.is_my_input;
                        let is_stereo = ch.is_stereo;

                        // Get meter level for this track
                        let meter_level = Signal::derive(move || {
                            meters.get().get(&track_idx).copied().unwrap_or(0.0)
                        });

                        // Level change handler - SAFETY: Only allow when connected to REAPER
                        let on_level_change = Callback::new(move |new_level: f32| {
                            // CRITICAL SAFETY: Block changes when disconnected from REAPER
                            if !connected.get() {
                                web_sys::console::warn_1(&"Level change blocked: not connected to REAPER".into());
                                return;
                            }

                            let member = member_id.get();

                            // Mark as touched
                            set_fader_touched.update(|t| {
                                t.insert(track_idx, true);
                                if let Some(partner) = partner_idx {
                                    t.insert(partner, true);
                                }
                            });

                            // Store old level for rollback on error
                            let old_level = channels.get()
                                .iter()
                                .find(|c| c.track_index == track_idx)
                                .map(|c| c.level_db)
                                .unwrap_or(new_level);

                            // Update local state optimistically
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

                            // Send to server with error handling
                            spawn_local(async move {
                                if let Err(e) = set_send_level(&member, track_idx, new_level).await {
                                    web_sys::console::error_1(&format!("Level API error: {:?}", e).into());
                                    // Rollback on error
                                    set_channels.update(|chs| {
                                        if let Some(ch) = chs.iter_mut().find(|c| c.track_index == track_idx) {
                                            ch.level_db = old_level;
                                        }
                                    });
                                }
                                if let Some(partner) = partner_idx {
                                    if let Err(e) = set_send_level(&member, partner, new_level).await {
                                        web_sys::console::error_1(&format!("Level API error (partner): {:?}", e).into());
                                    }
                                }
                            });

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

                        // Pan change handler - SAFETY: Only allow when connected to REAPER
                        let on_pan_change = Callback::new(move |new_pan: f32| {
                            // CRITICAL SAFETY: Block changes when disconnected from REAPER
                            if !connected.get() {
                                web_sys::console::warn_1(&"Pan change blocked: not connected to REAPER".into());
                                return;
                            }

                            let member = member_id.get();

                            // Store old pan for rollback
                            let old_pan = channels.get()
                                .iter()
                                .find(|c| c.track_index == track_idx)
                                .map(|c| c.pan)
                                .unwrap_or(new_pan);

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

                            spawn_local(async move {
                                if let Err(e) = set_send_pan(&member, track_idx, new_pan).await {
                                    web_sys::console::error_1(&format!("Pan API error: {:?}", e).into());
                                    // Rollback on error
                                    set_channels.update(|chs| {
                                        if let Some(ch) = chs.iter_mut().find(|c| c.track_index == track_idx) {
                                            ch.pan = old_pan;
                                        }
                                    });
                                }
                                if let Some(partner) = partner_idx {
                                    if let Err(e) = set_send_pan(&member, partner, 1.0 - new_pan).await {
                                        web_sys::console::error_1(&format!("Pan API error (partner): {:?}", e).into());
                                    }
                                }
                            });
                        });

                        // Mute toggle handler - SAFETY: Only allow when connected to REAPER
                        let on_mute_click = move |_| {
                            // CRITICAL SAFETY: Block changes when disconnected from REAPER
                            if !connected.get() {
                                web_sys::console::warn_1(&"Mute change blocked: not connected to REAPER".into());
                                return;
                            }

                            let member = member_id.get();
                            let current_muted = channels.get()
                                .iter()
                                .find(|c| c.track_index == track_idx)
                                .map(|c| c.muted)
                                .unwrap_or(false);
                            let new_muted = !current_muted;

                            // Optimistically update local state
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

                            spawn_local(async move {
                                if let Err(e) = set_send_mute(&member, track_idx, new_muted).await {
                                    web_sys::console::error_1(&format!("Mute API error: {:?}", e).into());
                                    // Rollback on error
                                    set_channels.update(|chs| {
                                        if let Some(ch) = chs.iter_mut().find(|c| c.track_index == track_idx) {
                                            ch.muted = current_muted;
                                        }
                                    });
                                }
                                if let Some(partner) = partner_idx {
                                    if let Err(e) = set_send_mute(&member, partner, new_muted).await {
                                        web_sys::console::error_1(&format!("Mute API error (partner): {:?}", e).into());
                                    }
                                }
                            });
                        };

                        // Solo toggle handler - SAFETY: Only allow when connected to REAPER
                        let on_solo_click = move |_| {
                            // CRITICAL SAFETY: Block changes when disconnected from REAPER
                            if !connected.get() {
                                web_sys::console::warn_1(&"Solo change blocked: not connected to REAPER".into());
                                return;
                            }

                            let member = member_id.get();
                            let all_channels = channels.get();
                            let current_soloed = soloed.get();
                            let is_currently_soloed = current_soloed.contains(&track_idx);

                            if is_currently_soloed {
                                // Removing solo from this channel
                                let mut new_soloed = current_soloed.clone();
                                new_soloed.remove(&track_idx);
                                if let Some(partner) = partner_idx {
                                    new_soloed.remove(&partner);
                                }

                                if new_soloed.is_empty() {
                                    // All solos cleared - restore pre-solo mute states
                                    let saved = pre_solo_mutes.get();
                                    for ch in &all_channels {
                                        let should_be_muted = saved.get(&ch.track_index).copied().unwrap_or(false);
                                        let idx = ch.track_index;
                                        let member_clone = member.clone();
                                        set_channels.update(|chs| {
                                            if let Some(c) = chs.iter_mut().find(|c| c.track_index == idx) {
                                                c.muted = should_be_muted;
                                            }
                                        });
                                        spawn_local(async move {
                                            let _ = set_send_mute(&member_clone, idx, should_be_muted).await;
                                        });
                                    }
                                    set_pre_solo_mutes.set(HashMap::new());
                                } else {
                                    // Other channels still soloed - mute this one
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
                                    let member_clone = member.clone();
                                    spawn_local(async move {
                                        let _ = set_send_mute(&member_clone, track_idx, true).await;
                                        if let Some(partner) = partner_idx {
                                            let _ = set_send_mute(&member_clone, partner, true).await;
                                        }
                                    });
                                }
                                set_soloed.set(new_soloed);
                            } else {
                                // Adding solo to this channel
                                let was_empty = current_soloed.is_empty();

                                if was_empty {
                                    // First solo - save current mute states
                                    let mut saved_mutes = HashMap::new();
                                    for ch in &all_channels {
                                        saved_mutes.insert(ch.track_index, ch.muted);
                                    }
                                    set_pre_solo_mutes.set(saved_mutes);

                                    // Mute all except this one
                                    for ch in &all_channels {
                                        let should_mute = ch.track_index != track_idx && partner_idx != Some(ch.track_index);
                                        let idx = ch.track_index;
                                        let member_clone = member.clone();
                                        set_channels.update(|chs| {
                                            if let Some(c) = chs.iter_mut().find(|c| c.track_index == idx) {
                                                c.muted = should_mute;
                                            }
                                        });
                                        spawn_local(async move {
                                            let _ = set_send_mute(&member_clone, idx, should_mute).await;
                                        });
                                    }
                                } else {
                                    // Additional solo - unmute this channel
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
                                    let member_clone = member.clone();
                                    spawn_local(async move {
                                        let _ = set_send_mute(&member_clone, track_idx, false).await;
                                        if let Some(partner) = partner_idx {
                                            let _ = set_send_mute(&member_clone, partner, false).await;
                                        }
                                    });
                                }

                                let mut new_soloed = current_soloed.clone();
                                new_soloed.insert(track_idx);
                                if let Some(partner) = partner_idx {
                                    new_soloed.insert(partner);
                                }
                                set_soloed.set(new_soloed);
                            }
                        };

                        // Check if this track is soloed
                        let is_soloed = move || soloed.get().contains(&track_idx);

                        // Check if connected for visual state
                        let is_connected = move || connected.get();

                        view! {
                            <div class=move || {
                                let mut classes = vec!["channel"];
                                if muted { classes.push("muted"); }
                                if is_my { classes.push("more-me"); }
                                if is_stereo { classes.push("stereo-pair"); }
                                // SAFETY: Add disconnected class when not connected
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
                                        value=level
                                        min=-60.0
                                        max=12.0
                                        on_change=move |v| on_level_change.run(v)
                                    />
                                    <div class="db-display">{format_db(level)}</div>
                                </div>

                                <PanKnob
                                    value=pan
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
                                        class=move || if muted { "mute-btn on" } else { "mute-btn off" }
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
