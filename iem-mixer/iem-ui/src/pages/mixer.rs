//! Mixer page with faders

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use wasm_bindgen_futures::spawn_local;

use crate::api::{get_mixer_state, set_send_level, set_send_mute, Channel};
use crate::auth::can_access_member;
use crate::components::fader::Fader;

/// Mixer page for a specific member
#[component]
pub fn MixerPage() -> impl IntoView {
    let params = use_params_map();
    let navigate = use_navigate();
    let navigate_back = navigate.clone();

    // Get member ID from route params
    let member_id = move || {
        params.get().get("member").map(|s| s.to_string()).unwrap_or_default()
    };

    // Check auth on mount
    Effect::new(move |_| {
        let member = member_id();
        if !can_access_member(&member) {
            let nav = navigate.clone();
            nav(&format!("/login?member={}&next=/{}", member, member), Default::default());
        }
    });

    // Fetch mixer state
    let mixer_state = LocalResource::new(move || {
        let member = member_id();
        async move { get_mixer_state(&member).await }
    });

    // Reactive channel state
    let (channels, set_channels) = signal(Vec::<Channel>::new());

    // Update channels when mixer state loads
    Effect::new(move |_| {
        if let Some(result) = mixer_state.get() {
            if let Ok(state) = result.as_ref() {
                set_channels.set(state.channels.clone());
            }
        }
    });

    // Handle back button
    let on_back = move |_| {
        navigate_back("/", Default::default());
    };

    // Handle level change
    let on_level_change = move |track_index: usize, level_db: f32| {
        let member = member_id();

        // Update local state immediately
        set_channels.update(|chs| {
            if let Some(ch) = chs.iter_mut().find(|c| c.track_index == track_index) {
                ch.level_db = level_db;
            }
        });

        // Send to server
        spawn_local(async move {
            let _ = set_send_level(&member, track_index, level_db).await;
        });
    };

    // Handle mute toggle
    let on_mute_toggle = move |track_index: usize| {
        let member = member_id();
        let current_muted = channels.get().iter()
            .find(|c| c.track_index == track_index)
            .map(|c| c.muted)
            .unwrap_or(false);
        let new_muted = !current_muted;

        // Update local state
        set_channels.update(|chs| {
            if let Some(ch) = chs.iter_mut().find(|c| c.track_index == track_index) {
                ch.muted = new_muted;
            }
        });

        // Send to server
        spawn_local(async move {
            let _ = set_send_mute(&member, track_index, new_muted).await;
        });
    };

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
            </header>

            <Suspense fallback=move || view! {
                <div class="loading">
                    <div class="spinner"></div>
                </div>
            }>
                {move || {
                    let chs = channels.get();
                    if chs.is_empty() {
                        view! {
                            <div class="loading">
                                <div class="spinner"></div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="channels">
                                {chs.iter().map(|ch| {
                                    let track_idx = ch.track_index;
                                    let name = ch.name.clone();
                                    let level = ch.level_db;
                                    let muted = ch.muted;

                                    view! {
                                        <div class="channel">
                                            <div class="name">{name}</div>
                                            <Fader
                                                value=level
                                                min=-60.0
                                                max=12.0
                                                on_change=move |v| on_level_change(track_idx, v)
                                            />
                                            <button
                                                class=move || if muted { "mute-btn muted" } else { "mute-btn" }
                                                on:click=move |_| on_mute_toggle(track_idx)
                                            >
                                                "M"
                                            </button>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </Suspense>
        </div>
    }
}
