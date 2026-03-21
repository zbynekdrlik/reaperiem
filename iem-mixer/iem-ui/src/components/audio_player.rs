//! Listen button component for engineer audio monitoring
//!
//! Opens a separate WebSocket to /ws/audio for binary Opus frame streaming.
//! Uses JS interop (audio_player.js) for WebCodecs decoding and Web Audio playback.

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::components::settings_modal::UserSettings;

// JS interop functions from audio_player.js
#[wasm_bindgen(module = "/audio_player.js")]
extern "C" {
    #[wasm_bindgen(js_name = "initAudioPlayer")]
    fn init_audio_player();

    #[wasm_bindgen(js_name = "feedOpusFrame")]
    fn feed_opus_frame(data: &[u8]);

    #[wasm_bindgen(js_name = "stopAudioPlayer")]
    fn stop_audio_player();

    #[wasm_bindgen(js_name = "isAudioSupported")]
    fn is_audio_supported() -> bool;

    #[wasm_bindgen(js_name = "setListenGain")]
    pub fn set_listen_gain(db: f32);
}

/// Audio listening states
#[derive(Debug, Clone, Copy, PartialEq)]
enum ListenState {
    Idle,
    Listening,
    NoSource,
    Unsupported,
}

/// Listen button for engineer audio monitoring
///
/// Opens a separate WebSocket connection to /ws/audio when activated.
/// Receives binary Opus frames and plays them via Web Audio API.
/// `member_id` specifies whose mix to listen to (e.g., "petka", "engineer").
#[component]
pub fn ListenButton(
    /// Which member's mix to listen to
    member_id: String,
) -> impl IntoView {
    let member_id = member_id.clone();
    let (state, set_state) = signal(ListenState::Idle);
    let (ws, set_ws) = signal(Option::<web_sys::WebSocket>::None);
    let (listen_target, set_listen_target) = signal(String::new());

    // Check browser support on mount
    Effect::new(move || {
        if !is_audio_supported() {
            set_state.set(ListenState::Unsupported);
        }
    });

    // Cleanup on unmount
    on_cleanup(move || {
        if let Some(ws) = ws.get_untracked() {
            let _ = ws.close();
        }
        stop_audio_player();
    });

    let member_id_toggle = member_id.clone();
    let toggle = move |_: web_sys::MouseEvent| {
        if state.get() == ListenState::Unsupported {
            return;
        }

        if state.get() == ListenState::Idle || state.get() == ListenState::NoSource {
            // Start listening
            start_listening(
                set_state,
                set_ws,
                set_listen_target,
                member_id_toggle.clone(),
            );
        } else {
            // Stop listening
            stop_listening(ws, set_state, set_ws);
            set_listen_target.set(String::new());
        }
    };

    let btn_class = move || match state.get() {
        ListenState::Idle => "toolbar-btn toolbar-btn-listen",
        ListenState::Listening => "toolbar-btn toolbar-btn-listen listening",
        ListenState::NoSource => "toolbar-btn toolbar-btn-listen no-source",
        ListenState::Unsupported => "toolbar-btn toolbar-btn-listen unsupported",
    };

    let btn_text = move || match state.get() {
        ListenState::Idle => "\u{1F50A} Listen".to_string(),
        ListenState::Listening => {
            let target = listen_target.get();
            if target.is_empty() || target == "engineer" {
                "\u{1F50A} Listening...".to_string()
            } else {
                let mut chars = target.chars();
                let cap: String = match chars.next() {
                    None => String::new(),
                    Some(c) => c.to_uppercase().chain(chars).collect(),
                };
                format!("\u{1F50A} Listening ({})...", cap)
            }
        }
        ListenState::NoSource => "\u{1F50A} No Source".to_string(),
        ListenState::Unsupported => "\u{1F507} Unsupported".to_string(),
    };

    view! {
        <button
            class=btn_class
            on:click=toggle
            disabled=move || state.get() == ListenState::Unsupported
        >
            {btn_text}
        </button>
    }
}

fn start_listening(
    set_state: WriteSignal<ListenState>,
    set_ws: WriteSignal<Option<web_sys::WebSocket>>,
    set_listen_target: WriteSignal<String>,
    member_id: String,
) {
    // Build WebSocket URL
    let auth = match crate::auth::get_auth() {
        Some(a) => a,
        None => return,
    };

    let location = web_sys::window().unwrap().location();
    let protocol = location.protocol().unwrap_or_default();
    let host = location.host().unwrap_or_default();
    let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };
    let url = format!("{}//{}/ws/audio?token={}", ws_protocol, host, auth.token);

    // Init audio player NOW — in the click handler's call stack.
    // Mobile browsers require AudioContext to be created from a user gesture.
    // If we wait for the async on_open callback, the context will be suspended
    // and resume() won't work (it's also not a user gesture).
    init_audio_player();

    // Apply saved listen boost from engineer settings
    let settings = UserSettings::load("engineer");
    set_listen_gain(settings.listen_boost_db);

    let socket = match web_sys::WebSocket::new(&url) {
        Ok(s) => s,
        Err(e) => {
            web_sys::console::error_1(&format!("Audio WS connect failed: {:?}", e).into());
            return;
        }
    };
    socket.set_binary_type(web_sys::BinaryType::Arraybuffer);

    // On open: send ListenStart with member_id (audio player already initialized above)
    let member_id_open = member_id.clone();
    let on_open = Closure::wrap(Box::new(move |_: web_sys::Event| {
        // Send ListenStart command with target member
        let cmd = serde_json::to_string(&iem_core::ClientMsg::ListenStart {
            member_id: member_id_open.clone(),
        })
        .unwrap_or_default();
        if let Some(w) = web_sys::window() {
            if let Ok(ws_val) = js_sys::Reflect::get(&w, &"__iem_audio_ws".into()) {
                if let Some(ws) = ws_val.dyn_ref::<web_sys::WebSocket>() {
                    let _ = ws.send_with_str(&cmd);
                }
            }
        }
    }) as Box<dyn FnMut(_)>);

    // On message: handle text (status) and binary (Opus frames)
    let set_state_msg = set_state;
    let frame_counter = std::cell::Cell::new(0u32);
    let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        // Binary message = Opus frame
        if let Ok(buf) = event.data().dyn_into::<js_sys::ArrayBuffer>() {
            let array = js_sys::Uint8Array::new(&buf);
            let raw_len = array.length();
            let data = array.to_vec();
            let count = frame_counter.get();
            if count < 3 || count % 200 == 0 {
                web_sys::console::log_1(
                    &format!(
                        "[audio-ws] #{} arraybuf={}B vec={}B",
                        count,
                        raw_len,
                        data.len()
                    )
                    .into(),
                );
            }
            frame_counter.set(count + 1);
            feed_opus_frame(&data);
            set_state_msg.set(ListenState::Listening);
            return;
        }

        // Text message = status update
        if let Some(text) = event.data().as_string() {
            if let Ok(msg) = serde_json::from_str::<iem_core::ServerMsg>(&text) {
                if let iem_core::ServerMsg::AudioStatus { status, target } = msg {
                    if let Some(t) = target {
                        set_listen_target.set(t);
                    }
                    match status.as_str() {
                        "listening" => set_state_msg.set(ListenState::Listening),
                        "no_source" => set_state_msg.set(ListenState::NoSource),
                        "stopped" => set_state_msg.set(ListenState::Idle),
                        _ => {}
                    }
                }
            }
        }
    }) as Box<dyn FnMut(_)>);

    // On close: reset state
    let set_state_close = set_state;
    let set_ws_close = set_ws;
    let on_close = Closure::wrap(Box::new(move |_: web_sys::Event| {
        stop_audio_player();
        set_state_close.set(ListenState::Idle);
        set_ws_close.set(None);
    }) as Box<dyn FnMut(_)>);

    // On error: log and reset state
    let set_state_error = set_state;
    let on_error = Closure::wrap(Box::new(move |_: web_sys::Event| {
        web_sys::console::error_1(&"[audio] WebSocket error".into());
        stop_audio_player();
        set_state_error.set(ListenState::Idle);
    }) as Box<dyn FnMut(_)>);

    // Store WS ref globally BEFORE registering on_open callback.
    // on_open reads window.__iem_audio_ws to send ListenStart.
    // If set_onopen fires synchronously before this assignment, ListenStart is never sent.
    if let Some(w) = web_sys::window() {
        let _ = js_sys::Reflect::set(&w, &"__iem_audio_ws".into(), &socket);
    }

    socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
    socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    on_open.forget();
    on_message.forget();
    on_close.forget();
    on_error.forget();

    set_ws.set(Some(socket.clone()));
    // Do NOT set ListenState::Listening here — wait for the first binary frame
    // to arrive in on_message (line 149). Setting it prematurely makes the UI
    // show "Listening..." even when the WS fails to connect or ListenStart
    // is never sent. The on_message handler already sets Listening on first frame.
}

fn stop_listening(
    ws: ReadSignal<Option<web_sys::WebSocket>>,
    set_state: WriteSignal<ListenState>,
    set_ws: WriteSignal<Option<web_sys::WebSocket>>,
) {
    if let Some(socket) = ws.get_untracked() {
        // Send ListenStop command before closing
        let cmd = serde_json::to_string(&iem_core::ClientMsg::ListenStop).unwrap_or_default();
        let _ = socket.send_with_str(&cmd);
        let _ = socket.close();
    }
    stop_audio_player();
    set_ws.set(None);
    set_state.set(ListenState::Idle);
}
