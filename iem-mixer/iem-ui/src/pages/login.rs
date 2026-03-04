//! Login page with PIN numpad

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_query_map};
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::auth::save_auth;

/// Login page with PIN numpad
#[component]
pub fn LoginPage() -> impl IntoView {
    let query = use_query_map();
    let navigate = use_navigate();
    let navigate_back = navigate.clone();

    // Get member and next URL from query params
    let member = move || {
        query
            .get()
            .get("member")
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    let next = move || {
        query
            .get()
            .get("next")
            .map(|s| s.to_string())
            .unwrap_or_else(|| "/".to_string())
    };

    // PIN state
    let (pin, set_pin) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    // Wrap the digit handler in Rc for cloning
    let handle_digit = Rc::new(move |digit: char| {
        set_error.set(None);
        let mut current_pin = pin.get();
        if current_pin.len() < 4 {
            current_pin.push(digit);
            set_pin.set(current_pin.clone());
        }

        // Auto-submit when 4 digits entered
        if current_pin.len() == 4 {
            let member_val = member();
            let next_val = next();
            let pin_val = current_pin;
            let nav = navigate.clone();

            set_loading.set(true);

            spawn_local(async move {
                match api::login(&member_val, &pin_val).await {
                    Ok(auth) => {
                        save_auth(&auth);
                        nav(&next_val, Default::default());
                    }
                    Err(e) => {
                        set_error.set(Some(e));
                        set_pin.set(String::new());
                        set_loading.set(false);
                    }
                }
            });
        }
    });

    // Create clones for each button
    let hd1 = handle_digit.clone();
    let hd2 = handle_digit.clone();
    let hd3 = handle_digit.clone();
    let hd4 = handle_digit.clone();
    let hd5 = handle_digit.clone();
    let hd6 = handle_digit.clone();
    let hd7 = handle_digit.clone();
    let hd8 = handle_digit.clone();
    let hd9 = handle_digit.clone();
    let hd0 = handle_digit.clone();

    view! {
        <div class="app">
            <header class="mixer-header">
                <button class="back-btn" on:click=move |_| { navigate_back("/", Default::default()); }>
                    "\u{2190}"
                </button>
                <h1>"NEWLEVEL IEM MIXER"</h1>
                <div style="width: 40px"></div>
            </header>
            <main class="main">
                <div class="login-container">
                    <div class="login-box">
                        <h2>"Enter PIN"</h2>
                        <p class="subtitle">{move || {
                            let m = member();
                            if m.is_empty() {
                                "Enter your PIN".to_string()
                            } else {
                                format!("PIN for {}", m)
                            }
                        }}</p>

                        // PIN dots
                        <div class="pin-display">
                            {move || {
                                let len = pin.get().len();
                                let has_error = error.get().is_some();
                                (0..4).map(|i| {
                                    let class = if has_error {
                                        "pin-dot error"
                                    } else if i < len {
                                        "pin-dot filled"
                                    } else {
                                        "pin-dot"
                                    };
                                    view! { <div class=class></div> }
                                }).collect::<Vec<_>>()
                            }}
                        </div>

                        // Numpad
                        <div class="numpad">
                            <button class="numpad-btn" on:click=move |_| hd1('1') disabled=loading>"1"</button>
                            <button class="numpad-btn" on:click=move |_| hd2('2') disabled=loading>"2"</button>
                            <button class="numpad-btn" on:click=move |_| hd3('3') disabled=loading>"3"</button>
                            <button class="numpad-btn" on:click=move |_| hd4('4') disabled=loading>"4"</button>
                            <button class="numpad-btn" on:click=move |_| hd5('5') disabled=loading>"5"</button>
                            <button class="numpad-btn" on:click=move |_| hd6('6') disabled=loading>"6"</button>
                            <button class="numpad-btn" on:click=move |_| hd7('7') disabled=loading>"7"</button>
                            <button class="numpad-btn" on:click=move |_| hd8('8') disabled=loading>"8"</button>
                            <button class="numpad-btn" on:click=move |_| hd9('9') disabled=loading>"9"</button>
                            <button
                                class="numpad-btn clear"
                                on:click=move |_| {
                                    set_error.set(None);
                                    set_pin.set(String::new());
                                }
                                disabled=loading
                            >"CLR"</button>
                            <button class="numpad-btn" on:click=move |_| hd0('0') disabled=loading>"0"</button>
                            <button
                                class="numpad-btn backspace"
                                on:click=move |_| {
                                    set_error.set(None);
                                    set_pin.update(|p| { p.pop(); });
                                }
                                disabled=loading
                            >"\u{232B}"</button>
                        </div>

                        // Error message
                        {move || error.get().map(|e| view! {
                            <p class="login-error">{e}</p>
                        })}
                    </div>
                </div>
            </main>
        </div>
    }
}
