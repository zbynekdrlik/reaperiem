//! Preset modal component
//!
//! Presets are stored server-side and synced across devices.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;

use crate::auth::get_token;

/// Preset data used by the mixer to apply channel states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetData {
    /// Track index -> channel state
    pub channels: std::collections::HashMap<usize, ChannelState>,
    /// Timestamp when preset was created (seconds since epoch)
    #[serde(default)]
    pub created_at: Option<i64>,
    /// Timestamp when preset was last updated (seconds since epoch)
    #[serde(default)]
    pub updated_at: Option<i64>,
    /// Stems bus volume in dB (None if no stems bus)
    #[serde(default)]
    pub stems_level_db: Option<f32>,
}

/// Channel state in a preset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelState {
    pub vol: f32,
    pub mute: bool,
    pub pan: f32,
}

/// Preset info from the server API
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetInfo {
    name: String,
    channel_count: usize,
    created_at: i64,
    updated_at: i64,
}

/// Full preset entry from the server API
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetEntry {
    name: String,
    channels: std::collections::HashMap<usize, ChannelState>,
    created_at: i64,
    updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stems_level_db: Option<f32>,
}

/// Request to save a preset
#[derive(Serialize)]
struct SavePresetRequest {
    name: String,
    channels: std::collections::HashMap<usize, ChannelState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stems_level_db: Option<f32>,
}

/// Format timestamp for display in Slovak format (DD.MM. HH:MM)
fn format_timestamp(ts: i64) -> String {
    // Server timestamps are in seconds, JS Date expects milliseconds
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64((ts * 1000) as f64));
    let month = date.get_month() + 1;
    let day = date.get_date();
    let hours = date.get_hours();
    let mins = date.get_minutes();
    format!("{}.{}. {:02}:{:02}", day, month, hours, mins)
}

/// Fetch all presets from server
async fn fetch_presets(member_id: &str) -> Result<Vec<PresetInfo>, String> {
    let token = get_token().ok_or("Not authenticated")?;
    let url = format!("/api/presets/{}", member_id);

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

/// Get a specific preset from server
async fn fetch_preset(member_id: &str, name: &str) -> Result<PresetEntry, String> {
    let token = get_token().ok_or("Not authenticated")?;
    let url = format!("/api/presets/{}/{}", member_id, encode_name(name));

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

/// Save a preset to server
async fn save_preset_api(
    member_id: &str,
    name: &str,
    channels: std::collections::HashMap<usize, ChannelState>,
    stems_level_db: Option<f32>,
) -> Result<(), String> {
    let token = get_token().ok_or("Not authenticated")?;
    let url = format!("/api/presets/{}", member_id);

    let req = SavePresetRequest {
        name: name.to_string(),
        channels,
        stems_level_db,
    };

    let resp = gloo_net::http::Request::post(&url)
        .header("Authorization", &format!("Bearer {}", token))
        .json(&req)
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

/// Update a preset on server
async fn update_preset_api(
    member_id: &str,
    name: &str,
    channels: std::collections::HashMap<usize, ChannelState>,
    stems_level_db: Option<f32>,
) -> Result<(), String> {
    let token = get_token().ok_or("Not authenticated")?;
    let url = format!("/api/presets/{}/{}", member_id, encode_name(name));

    let req = SavePresetRequest {
        name: name.to_string(),
        channels,
        stems_level_db,
    };

    let resp = gloo_net::http::Request::put(&url)
        .header("Authorization", &format!("Bearer {}", token))
        .json(&req)
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

/// Delete a preset from server
async fn delete_preset_api(member_id: &str, name: &str) -> Result<(), String> {
    let token = get_token().ok_or("Not authenticated")?;
    let url = format!("/api/presets/{}/{}", member_id, encode_name(name));

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

/// URL-encode preset name for use in path segments
fn encode_name(name: &str) -> String {
    js_sys::encode_uri_component(name)
        .as_string()
        .unwrap_or_else(|| name.to_string())
}

/// Preset modal component
#[component]
pub fn PresetModal(
    /// Whether modal is visible
    visible: ReadSignal<bool>,
    /// Member ID for preset storage
    member_id: String,
    /// Called to close modal
    on_close: Callback<()>,
    /// Called when a preset is loaded
    on_load: Callback<PresetData>,
    /// Called to get current channel states for saving
    get_current_state: Callback<(), PresetData>,
) -> impl IntoView {
    let (presets, set_presets) = signal(Vec::<PresetInfo>::new());
    let (new_name, set_new_name) = signal(String::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let member_id_stored = StoredValue::new(member_id);

    // Refresh presets when modal opens
    Effect::new(move |_| {
        if visible.get() {
            let member_id = member_id_stored.get_value();
            set_loading.set(true);
            set_error.set(None);

            wasm_bindgen_futures::spawn_local(async move {
                match fetch_presets(&member_id).await {
                    Ok(list) => {
                        set_presets.set(list);
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

    let handle_save = move |_| {
        let name = new_name.get().trim().to_string();
        if name.is_empty() {
            return;
        }

        let state = get_current_state.run(());
        let member_id = member_id_stored.get_value();
        set_loading.set(true);

        wasm_bindgen_futures::spawn_local(async move {
            match save_preset_api(&member_id, &name, state.channels, state.stems_level_db).await {
                Ok(()) => {
                    // Refresh list
                    if let Ok(list) = fetch_presets(&member_id).await {
                        set_presets.set(list);
                    }
                    set_new_name.set(String::new());
                }
                Err(e) => set_error.set(Some(e)),
            }
            set_loading.set(false);
        });
    };

    let handle_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let input = target.dyn_into::<web_sys::HtmlInputElement>().unwrap();
        set_new_name.set(input.value());
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
            <div class="modal">
                <button class="modal-close" on:click=move |_| on_close.run(())>
                    "\u{00D7}"
                </button>
                <h2>"Presets"</h2>

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

                <div class="preset-list">
                    {move || {
                        let current_presets = presets.get();
                        if current_presets.is_empty() && !loading.get() {
                            view! {
                                <div class="no-presets">"No saved presets yet"</div>
                            }.into_any()
                        } else {
                            view! {
                                <>
                                    {current_presets.into_iter().map(|info| {
                                        let name_load = info.name.clone();
                                        let name_update = info.name.clone();
                                        let name_delete = info.name.clone();
                                        let updated_at = info.updated_at;

                                        view! {
                                            <div class="preset-item">
                                                <div
                                                    class="preset-info"
                                                    on:click=move |_| {
                                                        let member_id = member_id_stored.get_value();
                                                        let name = name_load.clone();
                                                        wasm_bindgen_futures::spawn_local(async move {
                                                            match fetch_preset(&member_id, &name).await {
                                                                Ok(entry) => {
                                                                    let data = PresetData {
                                                                        channels: entry.channels.into_iter().map(|(k, v)| {
                                                                            (k, ChannelState { vol: v.vol, mute: v.mute, pan: v.pan })
                                                                        }).collect(),
                                                                        created_at: Some(entry.created_at),
                                                                        updated_at: Some(entry.updated_at),
                                                                    };
                                                                    on_load.run(data);
                                                                    on_close.run(());
                                                                }
                                                                Err(e) => {
                                                                    web_sys::console::error_1(&format!("Failed to load preset: {}", e).into());
                                                                }
                                                            }
                                                        });
                                                    }
                                                >
                                                    <span class="name">{info.name.clone()}</span>
                                                    <span class="preset-timestamp">{format_timestamp(updated_at)}</span>
                                                </div>
                                                <div class="preset-actions">
                                                    <button
                                                        class="update-preset"
                                                        on:click=move |_| {
                                                            let state = get_current_state.run(());
                                                            let member_id = member_id_stored.get_value();
                                                            let name = name_update.clone();
                                                            set_loading.set(true);

                                                            wasm_bindgen_futures::spawn_local(async move {
                                                                match update_preset_api(&member_id, &name, state.channels, state.stems_level_db).await {
                                                                    Ok(()) => {
                                                                        if let Ok(list) = fetch_presets(&member_id).await {
                                                                            set_presets.set(list);
                                                                        }
                                                                    }
                                                                    Err(e) => set_error.set(Some(e)),
                                                                }
                                                                set_loading.set(false);
                                                            });
                                                        }
                                                    >
                                                        "Upd"
                                                    </button>
                                                    <button
                                                        class="delete-preset"
                                                        on:click=move |_| {
                                                            let member_id = member_id_stored.get_value();
                                                            let name = name_delete.clone();
                                                            set_loading.set(true);

                                                            wasm_bindgen_futures::spawn_local(async move {
                                                                match delete_preset_api(&member_id, &name).await {
                                                                    Ok(()) => {
                                                                        if let Ok(list) = fetch_presets(&member_id).await {
                                                                            set_presets.set(list);
                                                                        }
                                                                    }
                                                                    Err(e) => set_error.set(Some(e)),
                                                                }
                                                                set_loading.set(false);
                                                            });
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

                <div class="preset-input-row">
                    <input
                        type="text"
                        class="preset-input"
                        placeholder="Preset name..."
                        maxlength="30"
                        prop:value=move || new_name.get()
                        on:input=handle_input
                    />
                    <button class="preset-save-btn" on:click=handle_save disabled=move || loading.get()>
                        "Save"
                    </button>
                </div>
            </div>
        </div>
    }
}
