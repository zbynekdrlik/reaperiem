//! Landing page with member grid

use leptos::prelude::*;

use crate::api::{MemberInfo, get_members};

/// Landing page showing all band members
#[component]
pub fn LandingPage() -> impl IntoView {
    // Fetch members on mount
    let members = LocalResource::new(|| async { get_members().await });

    view! {
        <div class="app">
            <header class="header">
                <h1>"IEM Mixer"</h1>
            </header>
            <main class="main">
                <Suspense fallback=move || view! {
                    <div class="loading">
                        <div class="spinner"></div>
                    </div>
                }>
                    {move || {
                        members.get().map(|result| {
                            match result.as_ref() {
                                Ok(members) => view! {
                                    <MemberGrid members=members.clone() />
                                }.into_any(),
                                Err(e) => view! {
                                    <div class="error">
                                        <p>"Failed to load members: " {e.clone()}</p>
                                    </div>
                                }.into_any(),
                            }
                        })
                    }}
                </Suspense>
            </main>
        </div>
    }
}

/// Grid of member cards
#[component]
fn MemberGrid(members: Vec<MemberInfo>) -> impl IntoView {
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
}
