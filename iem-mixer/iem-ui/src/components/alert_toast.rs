//! Engineer alert toast — shows when a band member calls for help (#125)

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

/// Alert toast displayed on engineer devices when a band member calls for help.
/// Shows member name, plays alert sound, vibrates, auto-dismisses after 10s.
#[component]
pub fn AlertToast(
    alert: ReadSignal<Option<(String, String)>>,
    set_alert: WriteSignal<Option<(String, String)>>,
) -> impl IntoView {
    // Play alert sound and vibrate
    let play_alert = move || {
        // Vibrate SOS pattern
        if let Some(window) = web_sys::window() {
            let navigator = window.navigator();
            let pattern = js_sys::Array::new();
            pattern.push(&JsValue::from(200));
            pattern.push(&JsValue::from(100));
            pattern.push(&JsValue::from(200));
            pattern.push(&JsValue::from(100));
            pattern.push(&JsValue::from(200));
            let _ = navigator.vibrate_with_u32_sequence(&pattern);
        }

        // Play alert beeps via Web Audio API
        if let Ok(ctx) = web_sys::AudioContext::new() {
            for i in 0..3u32 {
                let start = i as f64 * 0.3;
                if let Ok(osc) = ctx.create_oscillator() {
                    osc.set_type(web_sys::OscillatorType::Square);
                    let _ = osc.frequency().set_value(880.0);
                    if let Ok(gain) = ctx.create_gain() {
                        let _ = gain.gain().set_value(0.3);
                        let _ = osc.connect_with_audio_node(&gain);
                        let _ = gain.connect_with_audio_node(&ctx.destination());
                        let _ = osc.start_with_when(ctx.current_time() + start);
                        let _ = osc.stop_with_when(ctx.current_time() + start + 0.2);
                    }
                }
            }
        }

        // Auto-dismiss after 10s
        let dismiss_cb = Closure::once(move || {
            set_alert.set(None);
        });
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                dismiss_cb.as_ref().unchecked_ref(),
                10_000,
            );
        }
        dismiss_cb.forget();
    };

    // Watch for new alerts — play sound when alert data changes to Some
    let (prev_member, set_prev_member) = signal(String::new());

    Effect::new(move || {
        let current = alert.get();
        if let Some((ref member_id, _)) = current {
            let prev = prev_member.get_untracked();
            if prev != *member_id || prev.is_empty() {
                set_prev_member.set(member_id.clone());
                play_alert();
            }
        } else {
            set_prev_member.set(String::new());
        }
    });

    let on_dismiss = move |_| {
        set_alert.set(None);
    };

    view! {
        <Show when=move || alert.get().is_some()>
            <div class="alert-toast">
                <div class="alert-toast-content">
                    <span class="alert-toast-icon">"!"</span>
                    <span class="alert-toast-text">
                        {move || {
                            alert.get()
                                .map(|(_, name)| format!("{} needs help!", name))
                                .unwrap_or_default()
                        }}
                    </span>
                    <button class="alert-toast-dismiss" on:click=on_dismiss>"X"</button>
                </div>
            </div>
        </Show>
    }
}
