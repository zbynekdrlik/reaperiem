//! Preset modal component

use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Preset data stored in localStorage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PresetData {
    /// Track index -> channel state
    pub channels: std::collections::HashMap<usize, ChannelState>,
    /// Timestamp when preset was created (milliseconds since epoch)
    #[serde(default)]
    pub created_at: Option<i64>,
    /// Timestamp when preset was last updated (milliseconds since epoch)
    #[serde(default)]
    pub updated_at: Option<i64>,
}

/// Channel state in a preset
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChannelState {
    pub vol: f32,
    pub mute: bool,
    pub pan: f32,
}

/// Get current timestamp in milliseconds
fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

/// Format timestamp for display
fn format_timestamp(ts: i64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ts as f64));
    let month = date.get_month() + 1;
    let day = date.get_date();
    let hours = date.get_hours();
    let mins = date.get_minutes();
    let ampm = if hours >= 12 { "PM" } else { "AM" };
    let hours12 = if hours == 0 {
        12
    } else if hours > 12 {
        hours - 12
    } else {
        hours
    };
    format!("{}/{} {}:{:02} {}", month, day, hours12, mins, ampm)
}

/// Get localStorage key for presets
fn presets_key(member_id: &str) -> String {
    format!("iem_presets_{}", member_id.to_lowercase())
}

/// Load presets from localStorage
pub fn load_presets(member_id: &str) -> std::collections::HashMap<String, PresetData> {
    let key = presets_key(member_id);
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(Some(data)) = storage.get_item(&key) {
                if let Ok(presets) = serde_json::from_str(&data) {
                    return presets;
                }
            }
        }
    }
    std::collections::HashMap::new()
}

/// Save presets to localStorage
pub fn save_presets(member_id: &str, presets: &std::collections::HashMap<String, PresetData>) {
    let key = presets_key(member_id);
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(data) = serde_json::to_string(presets) {
                let _ = storage.set_item(&key, &data);
            }
        }
    }
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
    let (presets, set_presets) = signal(load_presets(&member_id));
    let (new_name, set_new_name) = signal(String::new());
    let member_id_clone = member_id.clone();

    // Refresh presets when modal opens
    let member_id_effect = member_id.clone();
    Effect::new(move |_| {
        if visible.get() {
            set_presets.set(load_presets(&member_id_effect));
        }
    });

    let handle_save = {
        let member_id = member_id.clone();
        move |_| {
            let name = new_name.get().trim().to_string();
            if name.is_empty() {
                return;
            }

            let mut state = get_current_state.run(());
            let ts = now_ms();
            state.created_at = Some(ts);
            state.updated_at = Some(ts);

            let mut current_presets = presets.get();
            current_presets.insert(name, state);
            save_presets(&member_id, &current_presets);
            set_presets.set(current_presets);
            set_new_name.set(String::new());
        }
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

                <div class="preset-list">
                    {move || {
                        let current_presets = presets.get();
                        if current_presets.is_empty() {
                            view! {
                                <div class="no-presets">"No saved presets yet"</div>
                            }.into_any()
                        } else {
                            view! {
                                <>
                                    {
                                    let mut sorted: Vec<_> = current_presets.iter().collect();
                                    sorted.sort_by(|a, b| {
                                        let ts_a = a.1.created_at.unwrap_or(0);
                                        let ts_b = b.1.created_at.unwrap_or(0);
                                        ts_b.cmp(&ts_a)
                                    });
                                    sorted.iter().map(|(name, data)| {
                                        let name_load = name.clone();
                                        let name_update = name.clone();
                                        let name_delete = name.clone();
                                        let member_id_update = member_id_clone.clone();
                                        let member_id_delete = member_id_clone.clone();
                                        let timestamp = data.updated_at.or(data.created_at);

                                        view! {
                                            <div class="preset-item">
                                                <div
                                                    class="preset-info"
                                                    on:click=move |_| {
                                                        let p = presets.get();
                                                        if let Some(data) = p.get(&name_load) {
                                                            on_load.run(data.clone());
                                                            on_close.run(());
                                                        }
                                                    }
                                                >
                                                    <span class="name">{name.clone()}</span>
                                                    {timestamp.map(|ts| view! {
                                                        <span class="preset-timestamp">{format_timestamp(ts)}</span>
                                                    })}
                                                </div>
                                                <div class="preset-actions">
                                                    <button
                                                        class="update-preset"
                                                        on:click=move |_| {
                                                            let mut state = get_current_state.run(());
                                                            let mut current = presets.get();
                                                            // Preserve created_at, update updated_at
                                                            if let Some(existing) = current.get(&name_update) {
                                                                state.created_at = existing.created_at;
                                                            }
                                                            state.updated_at = Some(now_ms());
                                                            current.insert(name_update.clone(), state);
                                                            save_presets(&member_id_update, &current);
                                                            set_presets.set(current);
                                                        }
                                                    >
                                                        "Upd"
                                                    </button>
                                                    <button
                                                        class="delete-preset"
                                                        on:click=move |_| {
                                                            let mut current = presets.get();
                                                            current.remove(&name_delete);
                                                            save_presets(&member_id_delete, &current);
                                                            set_presets.set(current);
                                                        }
                                                    >
                                                        "Del"
                                                    </button>
                                                </div>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()
                                    }
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
                    <button class="preset-save-btn" on:click=handle_save>
                        "Save"
                    </button>
                </div>
            </div>
        </div>
    }
}
