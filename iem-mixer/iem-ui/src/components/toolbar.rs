//! Bottom toolbar component

use leptos::prelude::*;

use super::audio_player::ListenButton;

/// Bottom toolbar with Presets, History, and optional Mute All / Listen buttons
/// Note: Reset button removed - 0 dB = unity gain = dangerously loud for IEMs
/// Note: +Me button removed - was broken and unwanted by user
#[component]
pub fn Toolbar(
    /// Called when Presets button is clicked
    on_presets: Callback<()>,
    /// Called when History button is clicked
    on_history: Callback<()>,
    /// Whether the current user is an engineer (shows Listen button)
    #[prop(default = false)]
    is_engineer: bool,
    /// Whether engineer is on their own mixer page (shows Mute All)
    #[prop(default = false)]
    is_engineer_own_mixer: bool,
    /// Called when Mute All button is clicked (engineer on own mixer only)
    #[prop(optional)]
    on_mute_all: Option<Callback<()>>,
    /// Member ID for listen target (whose mix to listen to)
    #[prop(default = String::new())]
    member_id: String,
) -> impl IntoView {
    view! {
        <div class="toolbar">
            {(is_engineer_own_mixer && on_mute_all.is_some()).then(|| {
                let cb = on_mute_all.clone().unwrap();
                view! {
                    <button
                        class="toolbar-btn toolbar-btn-mute-all"
                        on:click=move |_| cb.run(())
                    >
                        "Mute All"
                    </button>
                }
            })}
            {is_engineer.then(|| {
                let member_id = member_id.clone();
                view! {
                    <ListenButton member_id=member_id.clone() />
                }
            })}
            {(!is_engineer).then(|| {
                view! {
                    <button
                        class="toolbar-btn"
                        on:click=move |_| on_presets.run(())
                    >
                        "Presets"
                    </button>
                    <button
                        class="toolbar-btn"
                        on:click=move |_| on_history.run(())
                    >
                        "History"
                    </button>
                }
            })}
        </div>
    }
}
