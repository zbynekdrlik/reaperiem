//! Band member alert button — calls engineer for help (#125)

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

/// Alert/SOS button for band members to call the engineer.
/// Sends CallEngineer via WebSocket, enters 30s cooldown after click.
#[component]
pub fn AlertButton(ws: ReadSignal<Option<web_sys::WebSocket>>) -> impl IntoView {
    let (cooldown, set_cooldown) = signal(false);
    let (countdown, set_countdown) = signal(0u32);

    let on_click = move |_| {
        if cooldown.get_untracked() {
            return;
        }

        // Send CallEngineer via WebSocket
        if let Some(socket) = ws.get_untracked() {
            if socket.ready_state() == web_sys::WebSocket::OPEN {
                let cmd = serde_json::to_string(&iem_core::ClientMsg::CallEngineer)
                    .unwrap_or_default();
                let _ = socket.send_with_str(&cmd);
            }
        }

        // Vibrate to confirm
        if let Some(window) = web_sys::window() {
            let _ = window.navigator().vibrate_with_duration(100);
        }

        // Enter cooldown
        set_cooldown.set(true);
        set_countdown.set(30);

        // Countdown timer — ticks every 1s, clears itself when done
        let interval_cb = Closure::wrap(Box::new(move || {
            let current = countdown.get_untracked();
            if current <= 1 {
                set_cooldown.set(false);
                set_countdown.set(0);
            } else {
                set_countdown.set(current - 1);
            }
        }) as Box<dyn FnMut()>);

        if let Some(window) = web_sys::window() {
            let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(
                interval_cb.as_ref().unchecked_ref(),
                1000,
            );
        }
        // Keep closure alive for the duration of the interval
        interval_cb.forget();
    };

    view! {
        <button
            class="alert-btn"
            class:cooldown=move || cooldown.get()
            disabled=move || cooldown.get()
            on:click=on_click
        >
            {move || {
                if cooldown.get() {
                    format!("{}", countdown.get())
                } else {
                    "SOS".to_string()
                }
            }}
        </button>
    }
}
