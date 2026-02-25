//! Bottom toolbar component

use leptos::prelude::*;

/// Bottom toolbar with +Me and Presets buttons
/// Note: Reset button removed - 0 dB = unity gain = dangerously loud for IEMs
#[component]
pub fn Toolbar(
    /// Called when Presets button is clicked
    on_presets: Callback<()>,
    /// Called when +Me button is clicked
    on_more_me: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="toolbar">
            <button
                class="toolbar-btn"
                on:click=move |_| on_presets.run(())
            >
                "Presets"
            </button>
            <button
                class="toolbar-btn more-me-btn"
                on:click=move |_| on_more_me.run(())
            >
                "+Me"
            </button>
        </div>
    }
}
