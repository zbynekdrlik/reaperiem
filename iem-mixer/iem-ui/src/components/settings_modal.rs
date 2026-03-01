//! Settings modal for user preferences (persisted in localStorage)

use gloo_storage::{LocalStorage, Storage};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// User-configurable settings, persisted per member in localStorage
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserSettings {
    #[serde(default = "default_true")]
    pub double_tap_fader: bool,
    #[serde(default = "default_true")]
    pub double_tap_pan: bool,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            double_tap_fader: true,
            double_tap_pan: true,
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
    /// Signal for pan double-tap setting
    double_tap_pan: ReadSignal<bool>,
    set_double_tap_pan: WriteSignal<bool>,
    /// Member ID for localStorage persistence
    member_id: String,
) -> impl IntoView {
    // StoredValue is Copy + Send + Sync — closures inside view! can use it freely
    let member_id = StoredValue::new(member_id);

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

                        <div class="settings-row" on:click=move |_| {
                            let new_val = !double_tap_pan.get_untracked();
                            set_double_tap_pan.set(new_val);
                            let mid = member_id.get_value();
                            let mut settings = UserSettings::load(&mid);
                            settings.double_tap_pan = new_val;
                            settings.save(&mid);
                        }>
                            <div class="settings-label">
                                <div class="settings-name">"Pan double-tap"</div>
                                <div class="settings-desc">"Double-tap pan to animate to center"</div>
                            </div>
                            <div class=move || if double_tap_pan.get() { "toggle-switch on" } else { "toggle-switch" }>
                                <div class="toggle-knob"></div>
                            </div>
                        </div>
                    </div>

                    <div class="settings-section">
                        <div class="settings-section-title">"Security"</div>

                        <button class="settings-action-btn" on:click=move |_| {
                            on_close.run(());
                            on_open_pin_change.run(());
                        }>
                            "Change PIN"
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
