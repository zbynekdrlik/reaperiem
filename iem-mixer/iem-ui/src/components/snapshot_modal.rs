//! Snapshot history modal component
//!
//! Displays mix snapshot history with restore, pin, and delete functionality.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;

use crate::auth::get_token;

/// Snapshot info from the API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub timestamp: i64,
    pub label: String,
    pub pinned: bool,
    pub channel_count: usize,
}

/// Format timestamp for display in Slovak format (DD.MM. HH:MM)
fn format_timestamp(ts: i64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64((ts * 1000) as f64));
    let month = date.get_month() + 1;
    let day = date.get_date();
    let hours = date.get_hours();
    let mins = date.get_minutes();
    // Slovak uses 24-hour format: DD.MM. HH:MM
    format!("{}.{}. {:02}:{:02}", day, month, hours, mins)
}

/// Format relative time (e.g., "2 hours ago", "yesterday")
fn format_relative(ts: i64) -> String {
    let now = (js_sys::Date::now() / 1000.0) as i64;
    let diff = now - ts;

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        let mins = diff / 60;
        if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", mins)
        }
    } else if diff < 86400 {
        let hours = diff / 3600;
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else if diff < 172800 {
        "yesterday".to_string()
    } else {
        let days = diff / 86400;
        format!("{} days ago", days)
    }
}

/// Fetch snapshots from API
async fn fetch_snapshots(member_id: &str) -> Result<Vec<SnapshotInfo>, String> {
    let token = get_token().ok_or("Not authenticated")?;
    let url = format!("/api/snapshots/{}", member_id);

    let resp = gloo_net::http::Request::get(&url)
        .header("Authorization", &format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.ok() {
        resp.json().await.map_err(|e| format!("Parse error: {}", e))
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Create a manual snapshot
async fn create_snapshot(member_id: &str, label: Option<String>) -> Result<(), String> {
    let token = get_token().ok_or("Not authenticated")?;
    let url = format!("/api/snapshots/{}", member_id);

    #[derive(Serialize)]
    struct CreateReq {
        label: Option<String>,
    }

    let resp = gloo_net::http::Request::post(&url)
        .header("Authorization", &format!("Bearer {}", token))
        .json(&CreateReq { label })
        .map_err(|e| format!("Request error: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.ok() {
        Ok(())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Delete a snapshot
async fn delete_snapshot(member_id: &str, timestamp: i64) -> Result<(), String> {
    let token = get_token().ok_or("Not authenticated")?;
    let url = format!("/api/snapshots/{}/{}", member_id, timestamp);

    let resp = gloo_net::http::Request::delete(&url)
        .header("Authorization", &format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.ok() {
        Ok(())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Restore a snapshot
async fn restore_snapshot(member_id: &str, timestamp: i64) -> Result<(), String> {
    let token = get_token().ok_or("Not authenticated")?;
    let url = format!("/api/snapshots/{}/{}/restore", member_id, timestamp);

    let resp = gloo_net::http::Request::post(&url)
        .header("Authorization", &format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.ok() {
        Ok(())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Snapshot history modal component
#[component]
pub fn SnapshotModal(
    /// Whether modal is visible
    visible: ReadSignal<bool>,
    /// Member ID for snapshot storage
    member_id: String,
    /// Called to close modal
    on_close: Callback<()>,
) -> impl IntoView {
    let (snapshots, set_snapshots) = signal(Vec::<SnapshotInfo>::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let member_id_stored = StoredValue::new(member_id);

    // Refresh snapshots when modal opens
    Effect::new(move |_| {
        if visible.get() {
            let member_id = member_id_stored.get_value();
            set_loading.set(true);
            set_error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                match fetch_snapshots(&member_id).await {
                    Ok(list) => {
                        set_snapshots.set(list);
                        set_loading.set(false);
                    }
                    Err(e) => {
                        set_error.set(Some(e));
                        set_loading.set(false);
                    }
                }
            });
        }
    });

    let handle_save_now = move |_| {
        let member_id = member_id_stored.get_value();
        set_loading.set(true);

        wasm_bindgen_futures::spawn_local(async move {
            match create_snapshot(&member_id, Some("manual".to_string())).await {
                Ok(()) => {
                    // Refresh list
                    if let Ok(list) = fetch_snapshots(&member_id).await {
                        set_snapshots.set(list);
                    }
                }
                Err(e) => set_error.set(Some(e)),
            }
            set_loading.set(false);
        });
    };

    let handle_overlay_click = move |ev: web_sys::MouseEvent| {
        let target = ev.target().unwrap();
        if let Ok(elem) = target.dyn_into::<web_sys::HtmlElement>() {
            if elem.class_list().contains("modal-overlay") {
                on_close.run(());
            }
        }
    };

    view! {
        <div
            class=move || if visible.get() { "modal-overlay visible" } else { "modal-overlay" }
            on:click=handle_overlay_click
        >
            <div class="modal snapshot-modal">
                <button class="modal-close" on:click=move |_| on_close.run(())>
                    "\u{00D7}"
                </button>
                <h2>"Mix History"</h2>

                <Show when=move || loading.get() fallback=|| ()>
                    <div class="snapshot-loading">
                        <div class="spinner"></div>
                    </div>
                </Show>

                <Show when=move || error.get().is_some() fallback=|| ()>
                    <div class="snapshot-error">
                        {move || error.get().unwrap_or_default()}
                    </div>
                </Show>

                <div class="snapshot-list">
                    {move || {
                        let current = snapshots.get();
                        if current.is_empty() && !loading.get() {
                            view! {
                                <div class="no-presets">"No snapshots yet. Changes are auto-saved daily."</div>
                            }.into_any()
                        } else {
                            view! {
                                <>
                                    {current.into_iter().map(|snap| {
                                        let timestamp = snap.timestamp;
                                        let is_pinned = snap.pinned;
                                        let label = snap.label.clone();
                                        let channel_count = snap.channel_count;

                                        view! {
                                            <div class=move || if is_pinned { "snapshot-item pinned" } else { "snapshot-item" }>
                                                <div class="snapshot-info">
                                                    <div class="snapshot-label">
                                                        {if is_pinned { format!("{}", label) } else { label.clone() }}
                                                        <span class="snapshot-channels">{format!("({} ch)", channel_count)}</span>
                                                    </div>
                                                    <div class="snapshot-time">
                                                        <span class="relative">{format_relative(timestamp)}</span>
                                                        <span class="absolute">{format_timestamp(timestamp)}</span>
                                                    </div>
                                                </div>
                                                <div class="snapshot-actions">
                                                    <button
                                                        class="restore-btn"
                                                        on:click={
                                                            let member_id = member_id_stored.get_value();
                                                            move |_| {
                                                                let member_id = member_id.clone();
                                                                set_loading.set(true);
                                                                wasm_bindgen_futures::spawn_local(async move {
                                                                    if let Err(e) = restore_snapshot(&member_id, timestamp).await {
                                                                        set_error.set(Some(e));
                                                                    }
                                                                    set_loading.set(false);
                                                                    on_close.run(());
                                                                });
                                                            }
                                                        }
                                                    >
                                                        "Restore"
                                                    </button>
                                                    <button
                                                        class="delete-btn"
                                                        on:click={
                                                            let member_id = member_id_stored.get_value();
                                                            move |_| {
                                                                let member_id = member_id.clone();
                                                                set_loading.set(true);
                                                                wasm_bindgen_futures::spawn_local(async move {
                                                                    if let Err(e) = delete_snapshot(&member_id, timestamp).await {
                                                                        set_error.set(Some(e));
                                                                    } else if let Ok(list) = fetch_snapshots(&member_id).await {
                                                                        set_snapshots.set(list);
                                                                    }
                                                                    set_loading.set(false);
                                                                });
                                                            }
                                                        }
                                                    >
                                                        "Del"
                                                    </button>
                                                </div>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </>
                            }.into_any()
                        }
                    }}
                </div>

                <button class="snapshot-save-btn" on:click=handle_save_now disabled=move || loading.get()>
                    "Save Snapshot Now"
                </button>
            </div>
        </div>
    }
}
