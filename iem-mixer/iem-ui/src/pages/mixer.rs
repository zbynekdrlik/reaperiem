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
        }
    });

    let load_preset_member_id = member_id.clone();
    let on_load_preset = Callback::new(move |preset: PresetData| {
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

        // Send all changes to server
        let preset_clone = preset.clone();
        spawn_local(async move {
            for (track_index, state) in preset_clone.channels {
                let _ = set_send_level(&member, track_index, state.vol).await;
                let _ = set_send_mute(&member, track_index, state.mute).await;
                let _ = set_send_pan(&member, track_index, state.pan).await;
            }
        });
    });

    // Toolbar callbacks
    let on_presets = Callback::new(move |_: ()| {
        set_preset_modal_visible.set(true);
    });

    let reset_member_id = member_id.clone();
    let on_reset = Callback::new(move |_: ()| {
        if web_sys::window()
            .and_then(|w| w.confirm_with_message("Reset all channels to 0 dB?").ok())
            .unwrap_or(false)
        {
            let member = reset_member_id();
            spawn_local(async move {
                let _ = batch_control(&member, BatchOperation::Reset).await;
            });
        }
    });

    let more_me_member_id = member_id.clone();
    let on_more_me = Callback::new(move |_: ()| {
        let member = more_me_member_id();
        spawn_local(async move {
            let _ = batch_control(&member, BatchOperation::MoreMe).await;
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
                        />
                    </div>
                </div>
            </Show>

            <Toolbar
                on_presets=on_presets
                on_reset=on_reset
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

                        // Level change handler
                        let on_level_change = Callback::new(move |new_level: f32| {
                            let member = member_id.get();

                            // Mark as touched
                            set_fader_touched.update(|t| {
                                t.insert(track_idx, true);
                                if let Some(partner) = partner_idx {
                                    t.insert(partner, true);
                                }
                            });

                            // Update local state
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

                            // Send to server
                            spawn_local(async move {
                                let _ = set_send_level(&member, track_idx, new_level).await;
                                if let Some(partner) = partner_idx {
                                    let _ = set_send_level(&member, partner, new_level).await;
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

                        // Pan change handler
                        let on_pan_change = Callback::new(move |new_pan: f32| {
                            let member = member_id.get();

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
                                let _ = set_send_pan(&member, track_idx, new_pan).await;
                                if let Some(partner) = partner_idx {
                                    let _ = set_send_pan(&member, partner, 1.0 - new_pan).await;
                                }
                            });
                        });

                        // Mute toggle handler
                        let on_mute_click = move |_| {
                            let member = member_id.get();
                            let current_muted = channels.get()
                                .iter()
                                .find(|c| c.track_index == track_idx)
                                .map(|c| c.muted)
                                .unwrap_or(false);
                            let new_muted = !current_muted;

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
                                let _ = set_send_mute(&member, track_idx, new_muted).await;
                                if let Some(partner) = partner_idx {
                                    let _ = set_send_mute(&member, partner, new_muted).await;
                                }
                            });
                        };

                        view! {
                            <div class=move || {
                                let mut classes = vec!["channel"];
                                if muted { classes.push("muted"); }
                                if is_my { classes.push("more-me"); }
                                if is_stereo { classes.push("stereo-pair"); }
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

                                <button
                                    class=move || if muted { "mute-btn on" } else { "mute-btn off" }
                                    on:click=on_mute_click
                                >
                                    "M"
                                </button>
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
