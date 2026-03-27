//! Settings modal for user preferences (persisted in localStorage)

use gloo_storage::{LocalStorage, Storage};
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// User-configurable settings, persisted per member in localStorage
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserSettings {
    #[serde(default = "default_true")]
    pub double_tap_fader: bool,
    /// Listen boost in dB (0-24), engineer-only. Applied immediately via GainNode.
    #[serde(default)]
    pub listen_boost_db: f32,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            double_tap_fader: true,
            listen_boost_db: 0.0,
        }
    }
}

impl UserSettings {
    fn storage_key(member_id: &str) -> String {
        format!("iem_settings_{}", member_id)
    }

    pub fn load(member_id: &str) -> Self {
        LocalStorage::get(&Self::storage_key(member_id)).unwrap_or_default()
    }

    pub fn save(&self, member_id: &str) {
        let _ = LocalStorage::set(&Self::storage_key(member_id), self);
    }
}

/// Settings modal with toggle switches for preferences
#[component]
pub fn SettingsModal(
    /// Whether the modal is visible
    visible: ReadSignal<bool>,
    /// Callback when the modal should close
    on_close: Callback<()>,
    /// Callback to open PIN change modal
    on_open_pin_change: Callback<()>,
    /// Signal for fader double-tap setting
    double_tap_fader: ReadSignal<bool>,
    set_double_tap_fader: WriteSignal<bool>,
    /// Member ID for localStorage persistence
    member_id: String,
    /// Whether the current user is an engineer (shows Audio section)
    #[prop(default = false)]
    is_engineer: bool,
    /// Whether the current user is engineer viewing another member's page
    /// (shows Elevated toggle for granting mix channel access)
    #[prop(default = false)]
    is_engineer_viewing_member: bool,
) -> impl IntoView {
    // StoredValue is Copy + Send + Sync — closures inside view! can use it freely
    let member_id = StoredValue::new(member_id);

    // Listen boost signal (engineer-only, loaded from settings)
    let initial_boost = if is_engineer {
        UserSettings::load(&member_id.get_value()).listen_boost_db
    } else {
        0.0
    };
    let (listen_boost, set_listen_boost) = signal(initial_boost);

    // Get navigate function at component level (Leptos hooks rule)
    let navigate = use_navigate();

    view! {
        <Show when=move || visible.get() fallback=|| ()>
            <div class="pin-modal-overlay" on:click=move |_| on_close.run(())>
                <div class="pin-modal settings-modal" on:click=move |e| e.stop_propagation()>
                    <button class="modal-close" on:click=move |_| on_close.run(())>
                        "\u{00D7}"
                    </button>
                    <h2>"Settings"</h2>

                    <div class="settings-section">
                        <div class="settings-section-title">"Preferences"</div>

                        <div class="settings-row" on:click=move |_| {
                            let new_val = !double_tap_fader.get_untracked();
                            set_double_tap_fader.set(new_val);
                            let mid = member_id.get_value();
                            let mut settings = UserSettings::load(&mid);
                            settings.double_tap_fader = new_val;
                            settings.save(&mid);
                        }>
                            <div class="settings-label">
                                <div class="settings-name">"Fader double-tap"</div>
                                <div class="settings-desc">"Double-tap fader to animate to 0 dB"</div>
                            </div>
                            <div class=move || if double_tap_fader.get() { "toggle-switch on" } else { "toggle-switch" }>
                                <div class="toggle-knob"></div>
                            </div>
                        </div>
                    </div>

                    {if is_engineer {
                        Some(view! {
                            <div class="settings-section" data-testid="listen-boost-section">
                                <div class="settings-section-title">"Audio"</div>

                                <div class="settings-row listen-boost-row">
                                    <div class="settings-label">
                                        <div class="settings-name">"Listen boost"</div>
                                        <div class="settings-desc">"Applied immediately"</div>
                                    </div>
                                    <div class="listen-boost-stepper">
                                        <button
                                            class="boost-btn"
                                            data-testid="boost-minus"
                                            on:click=move |_| {
                                                let new_val = (listen_boost.get_untracked() - 3.0).max(0.0);
                                                set_listen_boost.set(new_val);
                                                let mid = member_id.get_value();
                                                let mut settings = UserSettings::load(&mid);
                                                settings.listen_boost_db = new_val;
                                                settings.save(&mid);
                                                crate::components::audio_player::set_listen_gain(new_val);
                                            }
                                        >
                                            "\u{2212}"
                                        </button>
                                        <span class="boost-value" data-testid="boost-value">
                                            {move || {
                                                let db = listen_boost.get() as i32;
                                                if db == 0 { "0 dB".to_string() } else { format!("+{} dB", db) }
                                            }}
                                        </span>
                                        <button
                                            class="boost-btn"
                                            data-testid="boost-plus"
                                            on:click=move |_| {
                                                let new_val = (listen_boost.get_untracked() + 3.0).min(24.0);
                                                set_listen_boost.set(new_val);
                                                let mid = member_id.get_value();
                                                let mut settings = UserSettings::load(&mid);
                                                settings.listen_boost_db = new_val;
                                                settings.save(&mid);
                                                crate::components::audio_player::set_listen_gain(new_val);
                                            }
                                        >
                                            "+"
                                        </button>
                                    </div>
                                </div>
                            </div>
                        })
                    } else {
                        None
                    }}

                    {if is_engineer_viewing_member {
                        let (elevated, set_elevated) = signal(false);
                        // Fetch current elevated status on mount
                        let mid_for_fetch = member_id.get_value();
                        leptos::task::spawn_local({
                            let mid = mid_for_fetch.clone();
                            async move {
                                if let Some(auth) = crate::auth::get_auth() {
                                    let url = format!("/api/members/{}/elevated", mid);
                                    let opts = web_sys::RequestInit::new();
                                    opts.set_method("GET");
                                    let headers = web_sys::Headers::new().unwrap();
                                    headers.set("Authorization", &format!("Bearer {}", auth.token)).ok();
                                    opts.set_headers(&headers);
                                    if let Ok(resp) = wasm_bindgen_futures::JsFuture::from(
                                        web_sys::window().unwrap().fetch_with_str_and_init(&url, &opts),
                                    ).await {
                                        let resp: web_sys::Response = resp.into();
                                        if resp.ok() {
                                            if let Ok(json) = wasm_bindgen_futures::JsFuture::from(resp.json().unwrap()).await {
                                                if let Some(val) = js_sys::Reflect::get(&json, &"elevated".into()).ok() {
                                                    set_elevated.set(val.as_bool().unwrap_or(false));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        });
                        Some(view! {
                            <div class="settings-section">
                                <div class="settings-section-title">"Access"</div>
                                <div class="settings-row" on:click=move |_| {
                                    let new_val = !elevated.get_untracked();
                                    set_elevated.set(new_val);
                                    let mid = member_id.get_value();
                                    leptos::task::spawn_local(async move {
                                        if let Some(auth) = crate::auth::get_auth() {
                                            let url = format!("/api/members/{}/elevated", mid);
                                            let opts = web_sys::RequestInit::new();
                                            opts.set_method("PUT");
                                            let headers = web_sys::Headers::new().unwrap();
                                            headers.set("Authorization", &format!("Bearer {}", auth.token)).ok();
                                            headers.set("Content-Type", "application/json").ok();
                                            opts.set_headers(&headers);
                                            let body = format!(r#"{{"elevated":{}}}"#, new_val);
                                            opts.set_body(&wasm_bindgen::JsValue::from_str(&body));
                                            let _ = wasm_bindgen_futures::JsFuture::from(
                                                web_sys::window().unwrap().fetch_with_str_and_init(&url, &opts),
                                            ).await;
                                        }
                                    });
                                }>
                                    <div class="settings-label">
                                        <div class="settings-name">"Elevated access"</div>
                                        <div class="settings-desc">"Allow this member to view and control other members' mixes"</div>
                                    </div>
                                    <div class=move || if elevated.get() { "toggle-switch on" } else { "toggle-switch" }>
                                        <div class="toggle-knob"></div>
                                    </div>
                                </div>
                            </div>
                        })
                    } else {
                        None
                    }}

                    <div class="settings-section">
                        <div class="settings-section-title">"Security"</div>

                        <button class="settings-action-btn" on:click=move |_| {
                            on_close.run(());
                            on_open_pin_change.run(());
                        }>
                            "Change PIN"
                        </button>
                    </div>

                    <div class="settings-section">
                        <div class="settings-section-title">"Session"</div>

                        <button class="settings-action-btn logout-btn" on:click={
                            let navigate = navigate.clone();
                            move |_| {
                                // Clear auth state
                                crate::auth::clear_auth();
                                // Navigate to landing page
                                navigate("/", Default::default());
                            }
                        }>
                            "Logout"
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
