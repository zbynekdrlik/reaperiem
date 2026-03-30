//! Engineer push-to-talk button for talkback to band members (#123)

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

// JS interop functions from talkback.js
#[wasm_bindgen(module = "/talkback.js")]
extern "C" {
    #[wasm_bindgen(js_name = "isTalkbackSupported")]
    fn is_talkback_supported() -> bool;

    #[wasm_bindgen(catch, js_name = "startTalkback")]
    async fn start_talkback(ws_url: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = "stopTalkback")]
    fn stop_talkback();
}

/// Talkback state machine
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TalkState {
    /// Ready to talk
    Idle,
    /// Actively sending audio
    Live,
    /// Another engineer holds the lock
    InUse,
    /// Microphone permission denied
    MicBlocked,
    /// Browser doesn't support WebCodecs/getUserMedia
    Unsupported,
}

/// Push-to-talk button for engineer talkback.
///
/// Hold to talk, release to stop. Uses pointer events for touch+mouse.
/// Sends TalkStart/TalkStop via the mixer WebSocket; server responds with
/// TalkAcquired/TalkBusy/TalkReleased to coordinate the lock.
#[component]
pub fn TalkButton(
    /// Mixer WebSocket connection
    ws: ReadSignal<Option<web_sys::WebSocket>>,
    /// Current talkback state
    state: ReadSignal<TalkState>,
    /// State setter (updated by mixer WS handler)
    set_state: WriteSignal<TalkState>,
) -> impl IntoView {
    // Check browser support on mount
    Effect::new(move || {
        if !is_talkback_supported() {
            set_state.set(TalkState::Unsupported);
        }
    });

    // Build the talkback WS URL (separate from mixer WS)
    let build_talkback_ws_url = move || -> Option<String> {
        let location = web_sys::window()?.location();
        let protocol = location.protocol().ok()?;
        let host = location.host().ok()?;
        let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };
        let token = crate::auth::get_token()?;
        Some(format!(
            "{}//{}/ws/talkback?token={}",
            ws_protocol, host, token
        ))
    };

    // Pointer down: request talk lock via mixer WS
    let ws_url_builder = build_talkback_ws_url.clone();
    let on_pointer_down = move |e: web_sys::PointerEvent| {
        e.prevent_default();
        // Capture pointer — prevents pointerleave from firing on slight finger movement
        if let Some(target) = e.target() {
            if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                let _ = el.set_pointer_capture(e.pointer_id());
            }
        }
        let current = state.get_untracked();
        if current == TalkState::Unsupported
            || current == TalkState::MicBlocked
            || current == TalkState::InUse
        {
            return;
        }

        // Send TalkStart via mixer WS to acquire lock
        if let Some(socket) = ws.get_untracked() {
            if socket.ready_state() == web_sys::WebSocket::OPEN {
                if let Ok(json) = serde_json::to_string(&iem_core::ClientMsg::TalkStart) {
                    let _ = socket.send_with_str(&json);
                }
            }
        }

        // Start mic capture (the WS handler will set state to Live on TalkAcquired,
        // but we start capturing proactively to reduce latency)
        if let Some(url) = ws_url_builder() {
            let set_state = set_state;
            wasm_bindgen_futures::spawn_local(async move {
                match start_talkback(&url).await {
                    Ok(()) => {
                        // Mic is capturing; state will be set to Live by TalkAcquired handler
                    }
                    Err(e) => {
                        let msg = format!("{:?}", e);
                        if msg.contains("NotAllowedError") || msg.contains("Permission") {
                            set_state.set(TalkState::MicBlocked);
                        } else {
                            web_sys::console::error_1(
                                &format!("[talk] start failed: {}", msg).into(),
                            );
                        }
                    }
                }
            });
        }
    };

    // Release talk lock helper (shared by pointerup and pointerleave)
    let release_talk = move || {
        let current = state.get_untracked();
        if current != TalkState::Live && current != TalkState::Idle {
            return;
        }

        // Stop mic capture
        stop_talkback();

        // Send TalkStop via mixer WS
        if let Some(socket) = ws.get_untracked() {
            if socket.ready_state() == web_sys::WebSocket::OPEN {
                if let Ok(json) = serde_json::to_string(&iem_core::ClientMsg::TalkStop) {
                    let _ = socket.send_with_str(&json);
                }
            }
        }

        set_state.set(TalkState::Idle);
    };

    let on_pointer_up = move |e: web_sys::PointerEvent| {
        // Release pointer capture
        if let Some(target) = e.target() {
            if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                let _ = el.release_pointer_capture(e.pointer_id());
            }
        }
        release_talk();
    };

    let btn_class = move || match state.get() {
        TalkState::Idle => "toolbar-btn-talk",
        TalkState::Live => "toolbar-btn-talk live",
        TalkState::InUse => "toolbar-btn-talk in-use",
        TalkState::MicBlocked => "toolbar-btn-talk mic-blocked",
        TalkState::Unsupported => "toolbar-btn-talk unsupported",
    };

    let btn_text = move || match state.get() {
        TalkState::Idle => "\u{1F3A4} Talk".to_string(),
        TalkState::Live => "\u{1F3A4} LIVE".to_string(),
        TalkState::InUse => "\u{1F3A4} In Use".to_string(),
        TalkState::MicBlocked => "\u{1F3A4} Mic Blocked".to_string(),
        TalkState::Unsupported => "\u{1F3A4} N/A".to_string(),
    };

    let is_disabled = move || {
        matches!(
            state.get(),
            TalkState::InUse | TalkState::MicBlocked | TalkState::Unsupported
        )
    };

    // Cleanup on unmount
    on_cleanup(move || {
        stop_talkback();
    });

    view! {
        <button
            class=btn_class
            on:pointerdown=on_pointer_down
            on:pointerup=on_pointer_up
            disabled=is_disabled
        >
            {btn_text}
        </button>
    }
}
