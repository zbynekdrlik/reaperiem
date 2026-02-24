//! VU Meter component

use leptos::prelude::*;

/// Vertical VU meter component
#[component]
pub fn Meter(
    /// Peak level (0.0 to 1.0+)
    level: Signal<f32>,
) -> impl IntoView {
    // Convert linear level to percentage (logarithmic scale)
    let height_pct = move || {
        let l = level.get();
        if l <= 0.0001 {
            return 0.0;
        }
        let db = 20.0 * l.log10();
        // Map -60dB..+6dB to 0..100%
        ((db + 60.0) / 66.0 * 100.0).clamp(0.0, 100.0)
    };

    // Determine color class based on level
    let color_class = move || {
        let l = level.get();
        if l > 1.0 {
            "clip"
        } else if l > 0.7 {
            "warn"
        } else {
            ""
        }
    };

    view! {
        <div class="meter-container">
            <div
                class=move || format!("meter-fill {}", color_class())
                style=move || format!("height: {}%", height_pct())
            />
        </div>
    }
}
