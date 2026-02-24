//! Fader component

use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Vertical fader component
#[component]
pub fn Fader(
    /// Current value in dB
    value: f32,
    /// Minimum value (default -60)
    #[prop(default = -60.0)]
    min: f32,
    /// Maximum value (default 12)
    #[prop(default = 12.0)]
    max: f32,
    /// Called when value changes
    on_change: impl Fn(f32) + 'static,
) -> impl IntoView {
    let (local_value, set_local_value) = signal(value);

    // Update local value when prop changes
    Effect::new(move |_| {
        set_local_value.set(value);
    });

    let handle_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let input = target.dyn_into::<web_sys::HtmlInputElement>().unwrap();
        let new_value: f32 = input.value().parse().unwrap_or(0.0);
        set_local_value.set(new_value);
        on_change(new_value);
    };

    // Format dB value for display
    let format_db = move || {
        let v = local_value.get();
        if v <= -60.0 {
            "-\u{221E}".to_string()
        } else if v >= 0.0 {
            format!("+{:.0}", v)
        } else {
            format!("{:.0}", v)
        }
    };

    view! {
        <div class="fader-container">
            <input
                type="range"
                class="fader"
                min=min
                max=max
                step="0.5"
                value=move || local_value.get()
                on:input=handle_input
            />
            <span class="fader-value">{format_db}</span>
        </div>
    }
}
