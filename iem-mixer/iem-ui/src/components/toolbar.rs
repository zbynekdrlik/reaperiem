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
    /// Whether the current user is an engineer (shows Mute All + Listen buttons)
    #[prop(default = false)]
    is_engineer: bool,
    /// Called when Mute All button is clicked (engineer only)
    #[prop(optional)]
    on_mute_all: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <div class="toolbar">
            {is_engineer.then(|| {
                let on_mute_all = on_mute_all.clone();
                view! {
                    <button
                        class="toolbar-btn toolbar-btn-mute-all"
                        on:click=move |_| {
                            if let Some(ref cb) = on_mute_all {
                                cb.run(());
                            }
                        }
                    >
                        "Mute All"
                    </button>
                    <ListenButton />
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
