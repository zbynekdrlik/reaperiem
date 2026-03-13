//! Landing page with member grid

use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use crate::api::{MemberInfo, get_members_with_timeout};
use crate::auth::{get_auth, is_token_expired};

/// Load state for the landing page
#[derive(Clone)]
enum LoadState {
    Loading,
    Loaded(Vec<MemberInfo>),
    Error(String),
}

/// Landing page showing all band members
#[component]
pub fn LandingPage() -> impl IntoView {
    let navigate = use_navigate();

    // Auto-redirect: remember last member, get to mixer ASAP (#84).
    // - Valid token → straight to mixer
    // - Expired token → PIN login for remembered member (skip member grid)
    // - No auth → show member grid (first-time user)
    // sessionStorage flag prevents re-redirect when user navigates back.
    Effect::new(move |_| {
        if let Some(auth) = get_auth() {
            let window = web_sys::window().unwrap();
            if let Ok(Some(storage)) = window.session_storage() {
                if storage.get_item("iem_redirected").ok().flatten().is_none() {
                    let _ = storage.set_item("iem_redirected", "1");
                    if !is_token_expired() {
                        let target = format!("/{}", auth.member);
                        navigate(&target, Default::default());
                    } else {
                        let target =
                            format!("/login?member={m}&next=/{m}", m = auth.member);
                        navigate(&target, Default::default());
                    }
                }
            }
        }
    });

    // State signal for loading/loaded/error
    let (state, set_state) = signal(LoadState::Loading);
    // Retry trigger - incrementing this refetches
    let (retry_trigger, set_retry_trigger) = signal(0u32);

    // Effect that fetches members on mount and on retry
    Effect::new(move |_| {
        // Read retry trigger to track it
        let _ = retry_trigger.get();

        // Reset to loading state
        set_state.set(LoadState::Loading);

        // Spawn async fetch
        wasm_bindgen_futures::spawn_local(async move {
            match get_members_with_timeout().await {
                Ok(members) => set_state.set(LoadState::Loaded(members)),
                Err(e) => set_state.set(LoadState::Error(e)),
            }
        });
    });

    let retry = move |_| {
        set_retry_trigger.update(|n| *n += 1);
    };

    view! {
        <div class="app">
            <header class="header">
                <h1>"NEWLEVEL IEM MIXER"</h1>
                <div class="header-version">
                    <span class="header-version-number">{iem_core::version_label()}</span>
                    <span class="header-version-date">{iem_core::build_datetime()}</span>
                </div>
            </header>
            <main class="main">
                {move || {
                    match state.get() {
                        LoadState::Loading => view! {
                            <div class="loading">
                                <div class="spinner"></div>
                            </div>
                        }.into_any(),
                        LoadState::Loaded(members) => view! {
                            <MemberGrid members=members />
                        }.into_any(),
                        LoadState::Error(e) => view! {
                            <NetworkError error=e on_retry=retry />
                        }.into_any(),
                    }
                }}
            </main>
        </div>
    }
}

/// Network error component with retry button
#[component]
fn NetworkError(error: String, on_retry: impl Fn(()) + 'static) -> impl IntoView {
    let is_timeout = error == "NETWORK_TIMEOUT";

    view! {
        <div class="network-error">
            <div class="network-error-icon">"📡"</div>
            <h2>"Connection Failed"</h2>
            {if is_timeout {
                view! {
                    <p class="network-error-message">
                        "Unable to connect to the mixer server."
                        <br />
                        "Make sure your phone is on the "<strong>"band WiFi network"</strong>"."
                    </p>
                }.into_any()
            } else {
                view! {
                    <p class="network-error-message">
                        "Network error: " {error}
                    </p>
                }.into_any()
            }}
            <button class="retry-btn" on:click=move |_| on_retry(())>
                "Try Again"
            </button>
        </div>
    }
}

/// Grid of member cards
#[component]
fn MemberGrid(members: Vec<MemberInfo>) -> impl IntoView {
    if members.is_empty() {
        view! {
            <div class="empty-state">
                <div class="empty-icon">"🎧"</div>
                <h2>"No Members Configured"</h2>
                <p>"Add band members in config.yaml to get started."</p>
                <p class="hint">"Config location: %APPDATA%\\iem-mixer\\config.yaml"</p>
            </div>
        }
        .into_any()
    } else {
        view! {
            <div class="member-grid">
                {members.into_iter().map(|member| {
                    let initial = member.name.chars().next().unwrap_or('?').to_uppercase().to_string();
                    let href = format!("/{}", member.id);
                    view! {
                        <a href=href class="member-card">
                            <div class="avatar">{initial}</div>
                            <div class="name">{member.name}</div>
                        </a>
                    }
                }).collect::<Vec<_>>()}
            </div>
        }
        .into_any()
    }
}
