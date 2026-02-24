//! Bottom toolbar component

use leptos::prelude::*;

/// Bottom toolbar with +Me, Reset, and Presets buttons
#[component]
pub fn Toolbar(
    /// Called when Presets button is clicked
    on_presets: Callback<()>,
    /// Called when Reset button is clicked
    on_reset: Callback<()>,
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
                class="toolbar-btn"
                on:click=move |_| on_reset.run(())
            >
                "Reset"
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
