//! Pan knob component

use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Horizontal pan slider component
#[component]
pub fn PanKnob(
    /// Current pan value (0.0 = left, 0.5 = center, 1.0 = right) - reactive signal
    value: Signal<f32>,
    /// Called when pan changes
    on_change: Callback<f32>,
) -> impl IntoView {
    let (local_value, set_local_value) = signal(value.get_untracked());

    // Update local value when signal changes (now properly tracks!)
    Effect::new(move |_| {
        set_local_value.set(value.get());
    });

    let handle_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let input = target.dyn_into::<web_sys::HtmlInputElement>().unwrap();
        let new_value: f32 = input.value().parse().unwrap_or(50.0) / 100.0;
        set_local_value.set(new_value);
        on_change.run(new_value);
    };

    let handle_dblclick = move |_| {
        set_local_value.set(0.5);
        on_change.run(0.5);
    };

    view! {
        <div class="pan-container">
            <input
                type="range"
                class="pan-slider"
                min="0"
                max="100"
                value=move || (local_value.get() * 100.0) as i32
                on:input=handle_input
                on:dblclick=handle_dblclick
                title="Pan (double-click to center)"
            />
        </div>
    }
}
