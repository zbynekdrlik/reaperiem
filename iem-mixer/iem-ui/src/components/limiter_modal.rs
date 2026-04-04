//! Limiter modal — ReaLimit controls for IEM output bus (#72)

use leptos::prelude::*;

/// Limiter modal for controlling ReaLimit on a member's output bus
#[component]
pub fn LimiterModal(
    /// Track name for the header
    track_name: String,
    /// Parameter values from server (display)
    threshold_db: ReadSignal<f32>,
    ceiling_db: ReadSignal<f32>,
    release_ms: ReadSignal<f32>,
    /// Normalized values for slider positioning (0-1)
    threshold_norm: ReadSignal<f32>,
    ceiling_norm: ReadSignal<f32>,
    release_norm: ReadSignal<f32>,
    /// Whether limiter FX is enabled (not bypassed)
    enabled: ReadSignal<bool>,
    /// Whether data is loading
    loading: ReadSignal<bool>,
    /// Callback when a parameter changes: (param_name, normalized_value)
    on_param_change: Callback<(String, f32)>,
    /// Callback when enabled state changes
    on_enabled_change: Callback<bool>,
    /// Callback to close the modal
    on_close: Callback<()>,
) -> impl IntoView {
    let on_close_overlay = on_close;
    let on_close_btn = on_close;
    let on_param_change_reset = on_param_change;

    view! {
        <div class="limiter-overlay" on:click=move |_| on_close_overlay.run(())>
            <div class="limiter-modal" on:click=move |ev| ev.stop_propagation()>
                <div class="limiter-header">
                    <span class="limiter-title">{track_name} " — Limiter"</span>
                    <button class="limiter-close-btn" on:click=move |_| on_close_btn.run(())>
                        "\u{00d7}"
                    </button>
                </div>
                {move || {
                    if loading.get() {
                        view! { <div class="limiter-loading">"Loading..."</div> }.into_any()
                    } else {
                        view! {
                            <div class="limiter-params">
                                <LimiterSlider
                                    label="THRESH"
                                    value_db=threshold_db
                                    value_norm=threshold_norm
                                    suffix="dB"
                                    param_name="threshold"
                                    on_change=on_param_change
                                />
                                <LimiterSlider
                                    label="CEIL"
                                    value_db=ceiling_db
                                    value_norm=ceiling_norm
                                    suffix="dB"
                                    param_name="ceiling"
                                    on_change=on_param_change
                                />
                                <LimiterSlider
                                    label="REL"
                                    value_db=release_ms
                                    value_norm=release_norm
                                    suffix="ms"
                                    param_name="release"
                                    on_change=on_param_change
                                />
                                <div class="limiter-toggle-row">
                                    <span>"Limiter"</span>
                                    <button
                                        class=move || {
                                            if enabled.get() {
                                                "limiter-toggle-btn on"
                                            } else {
                                                "limiter-toggle-btn off"
                                            }
                                        }
                                        on:click=move |_| {
                                            on_enabled_change.run(!enabled.get_untracked())
                                        }
                                    >
                                        {move || if enabled.get() { "ON" } else { "OFF" }}
                                    </button>
                                    {move || {
                                        if !enabled.get() {
                                            view! {
                                                <span class="limiter-warning">
                                                    "HEARING PROTECTION OFF"
                                                </span>
                                            }
                                                .into_any()
                                        } else {
                                            view! {}.into_any()
                                        }
                                    }}
                                </div>
                                <div class="limiter-toggle-row">
                                    <button
                                        class="limiter-reset-btn"
                                        on:click=move |_| {
                                            on_param_change_reset.run(("threshold".to_string(), 0.6));
                                            on_param_change_reset.run(("ceiling".to_string(), 0.7));
                                            on_param_change_reset.run(("release".to_string(), 0.1));
                                        }
                                    >
                                        "Reset to defaults"
                                    </button>
                                </div>
                            </div>
                        }
                            .into_any()
                    }
                }}
            </div>
        </div>
    }
}

/// Individual slider for a limiter parameter
#[component]
fn LimiterSlider(
    /// Label shown to the left (e.g. "THRESH")
    label: &'static str,
    /// Display value (dB or ms)
    value_db: ReadSignal<f32>,
    /// Normalized value 0-1 for slider position
    value_norm: ReadSignal<f32>,
    /// Unit suffix ("dB" or "ms")
    suffix: &'static str,
    /// Parameter name sent in callback
    param_name: &'static str,
    /// Called with (param_name, normalized_value)
    on_change: Callback<(String, f32)>,
) -> impl IntoView {
    let (local_norm, set_local_norm) = signal(value_norm.get_untracked());
    let (is_active, set_is_active) = signal(false);
    let track_ref = NodeRef::<leptos::html::Div>::new();

    // Sync from server when not dragging
    Effect::new(move |_| {
        let v = value_norm.get();
        if !is_active.get_untracked() {
            set_local_norm.set(v);
        }
    });

    let handle_pointerdown = move |ev: web_sys::PointerEvent| {
        ev.prevent_default();
        set_is_active.set(true);
        // Capture pointer for reliable tracking
        if let Some(el) = track_ref.get() {
            let target: &web_sys::Element = &el;
            let _ = target.set_pointer_capture(ev.pointer_id());
        }
        // Calculate and send value
        if let Some(el) = track_ref.get() {
            let rect = el.get_bounding_client_rect();
            let x = (ev.client_x() as f64 - rect.left()) / rect.width();
            let norm = x.clamp(0.0, 1.0) as f32;
            set_local_norm.set(norm);
            on_change.run((param_name.to_string(), norm));
        }
    };

    let handle_pointermove = move |ev: web_sys::PointerEvent| {
        if !is_active.get_untracked() {
            return;
        }
        ev.prevent_default();
        if let Some(el) = track_ref.get() {
            let rect = el.get_bounding_client_rect();
            let x = (ev.client_x() as f64 - rect.left()) / rect.width();
            let norm = x.clamp(0.0, 1.0) as f32;
            set_local_norm.set(norm);
            on_change.run((param_name.to_string(), norm));
        }
    };

    let handle_pointerup = move |ev: web_sys::PointerEvent| {
        if !is_active.get_untracked() {
            return;
        }
        set_is_active.set(false);
        if let Some(el) = track_ref.get() {
            let target: &web_sys::Element = &el;
            let _ = target.release_pointer_capture(ev.pointer_id());
        }
    };

    let pct = move || local_norm.get() * 100.0;

    view! {
        <div class="limiter-param-row">
            <span class="limiter-param-label">{label}</span>
            <div
                node_ref=track_ref
                class=move || {
                    if is_active.get() {
                        "limiter-slider-track active"
                    } else {
                        "limiter-slider-track"
                    }
                }
                on:pointerdown=handle_pointerdown
                on:pointermove=handle_pointermove
                on:pointerup=handle_pointerup
            >
                <div class="limiter-slider-fill" style=move || format!("width:{}%", pct()) />
                <div class="limiter-slider-thumb" style=move || format!("left:{}%", pct()) />
            </div>
            <span class="limiter-param-value">
                {move || format!("{:.1} {}", value_db.get(), suffix)}
            </span>
        </div>
    }
}
