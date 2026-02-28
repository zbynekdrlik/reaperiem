//! Bottom toolbar component

use leptos::prelude::*;

/// Bottom toolbar with Presets button
/// Note: Reset button removed - 0 dB = unity gain = dangerously loud for IEMs
/// Note: +Me button removed - was broken and unwanted by user
#[component]
pub fn Toolbar(
    /// Called when Presets button is clicked
    on_presets: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="toolbar">
            <button
                class="toolbar-btn"
                on:click=move |_| on_presets.run(())
            >
                "Presets"
            </button>
        </div>
    }
}
