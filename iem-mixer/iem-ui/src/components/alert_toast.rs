//! Engineer alert toast — persistent until cleared (#125)

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

/// Persistent alert toast for engineer.
/// Vibrates every 3s, plays subtle chime every 10s, shows system notification.
/// Stays until engineer clicks dismiss (sends ClearAlert via WS).
#[component]
pub fn AlertToast(
    alert: ReadSignal<Option<(String, String)>>,
    ws: ReadSignal<Option<web_sys::WebSocket>>,
) -> impl IntoView {
    // Start/stop vibration loop and sound loop when alert changes
    Effect::new(move || {
        let current = alert.get();
        if let Some((_, ref name)) = current {
            // System notification (ask permission if needed)
            let name_clone = name.clone();
            wasm_bindgen_futures::spawn_local(async move {
                request_and_notify(&name_clone).await;
            });

            // Start vibration loop (every 3s)
            let vib_cb = Closure::wrap(Box::new(move || {
                if let Some(window) = web_sys::window() {
                    let _ = window.navigator().vibrate_with_duration(200);
                }
            }) as Box<dyn FnMut()>);
            if let Some(window) = web_sys::window() {
                let id = window
                    .set_interval_with_callback_and_timeout_and_arguments_0(
                        vib_cb.as_ref().unchecked_ref(),
                        3000,
                    )
                    .unwrap_or(0);
                let _ = js_sys::Reflect::set(
                    &window,
                    &JsValue::from_str("__iem_alert_vib"),
                    &JsValue::from(id),
                );
                let _ = window.navigator().vibrate_with_duration(200);
            }
            vib_cb.forget();

            // Start sound loop (play chime, repeat every 10s)
            play_chime();
            let sound_cb = Closure::wrap(Box::new(move || {
                play_chime();
            }) as Box<dyn FnMut()>);
            if let Some(window) = web_sys::window() {
                let id = window
                    .set_interval_with_callback_and_timeout_and_arguments_0(
                        sound_cb.as_ref().unchecked_ref(),
                        10_000,
                    )
                    .unwrap_or(0);
                let _ = js_sys::Reflect::set(
                    &window,
                    &JsValue::from_str("__iem_alert_snd"),
                    &JsValue::from(id),
                );
            }
            sound_cb.forget();
        } else {
            stop_loops();
        }
    });

    let on_dismiss = move |_| {
        if let Some(socket) = ws.get_untracked() {
            if socket.ready_state() == web_sys::WebSocket::OPEN {
                let cmd =
                    serde_json::to_string(&iem_core::ClientMsg::ClearAlert).unwrap_or_default();
                let _ = socket.send_with_str(&cmd);
            }
        }
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
                    <button class="alert-toast-dismiss" on:click=on_dismiss>"OK"</button>
                </div>
            </div>
        </Show>
    }
}

fn play_chime() {
    let audio = web_sys::HtmlAudioElement::new_with_src("/alert.mp3").ok();
    if let Some(audio) = audio {
        audio.set_volume(0.5);
        let _ = audio.play();
    }
}

fn stop_loops() {
    if let Some(window) = web_sys::window() {
        let mut had_loops = false;
        if let Ok(val) = js_sys::Reflect::get(&window, &JsValue::from_str("__iem_alert_vib")) {
            if let Some(id) = val.as_f64() {
                window.clear_interval_with_handle(id as i32);
                let _ = js_sys::Reflect::delete_property(
                    &window,
                    &JsValue::from_str("__iem_alert_vib"),
                );
                had_loops = true;
            }
        }
        if let Ok(val) = js_sys::Reflect::get(&window, &JsValue::from_str("__iem_alert_snd")) {
            if let Some(id) = val.as_f64() {
                window.clear_interval_with_handle(id as i32);
                let _ = js_sys::Reflect::delete_property(
                    &window,
                    &JsValue::from_str("__iem_alert_snd"),
                );
                had_loops = true;
            }
        }
        // Only cancel vibration if we actually had loops running
        if had_loops {
            let _ = window.navigator().vibrate_with_duration(0);
        }
    }
}

async fn request_and_notify(name: &str) {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    if let Ok(perm) = js_sys::Reflect::get(&window, &JsValue::from_str("Notification")) {
        if let Ok(permission) = js_sys::Reflect::get(&perm, &JsValue::from_str("permission")) {
            if permission.as_string().as_deref() != Some("granted") {
                let promise = js_sys::Reflect::get(&perm, &JsValue::from_str("requestPermission"))
                    .ok()
                    .and_then(|f| f.dyn_ref::<js_sys::Function>().cloned());
                if let Some(func) = promise {
                    let result = func.call0(&perm).ok();
                    if let Some(p) = result.and_then(|v| v.dyn_into::<js_sys::Promise>().ok()) {
                        let _ = wasm_bindgen_futures::JsFuture::from(p).await;
                    }
                }
            }
        }
    }
    let opts = web_sys::NotificationOptions::new();
    opts.set_body(&format!("{} needs help!", name));
    opts.set_require_interaction(true);
    let _ = web_sys::Notification::new_with_options(&format!("IEM Alert: {}", name), &opts);
}
