//! Backup & Restore section for the Settings modal (engineer-only)

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Backup restore section shown in engineer Settings modal.
/// Lists available backups, shows preview on click, allows restore.
#[component]
pub fn BackupSection() -> impl IntoView {
    let (backups, set_backups) = signal(Vec::<iem_core::BackupInfo>::new());
    let (selected, set_selected) = signal(Option::<String>::None);
    let (preview, set_preview) = signal(Option::<iem_core::RestorePreview>::None);
    let (restoring, set_restoring) = signal(false);
    let (result, set_result) = signal(Option::<iem_core::RestoreResult>::None);
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    // Load backups on mount
    {
        spawn_local(async move {
            let token = match crate::auth::get_auth() {
                Some(a) => a.token,
                None => return,
            };
            match crate::api::list_backups(&token).await {
                Ok(list) => set_backups.set(list),
                Err(e) => set_error.set(Some(e)),
            }
        });
    }

    view! {
        <div class="settings-section" data-testid="backup-section">
            <div class="settings-section-title">"Backups"</div>

            // Error display
            {move || error.get().map(|e| view! {
                <div class="backup-error" style="color: #ff6b6b; padding: 8px; font-size: 0.85em;">{e}</div>
            })}

            // Result display
            {move || result.get().map(|r| view! {
                <div class="backup-result" style="color: #51cf66; padding: 8px; font-size: 0.85em;">
                    {format!("Restored {} values", r.restored_count)}
                    {if !r.skipped.is_empty() {
                        format!(", {} skipped", r.skipped.len())
                    } else {
                        String::new()
                    }}
                    {if r.project_saved { " — project saved" } else { " — project NOT saved!" }}
                </div>
            })}

            // Backup list
            <div class="backup-list" style="max-height: 200px; overflow-y: auto; margin: 8px 0;">
                {move || {
                    let list = backups.get();
                    if list.is_empty() {
                        return view! { <div style="color: #888; font-size: 0.85em; padding: 8px;">"No backups available"</div> }.into_any();
                    }
                    list.into_iter().map(|b| {
                        let filename = b.filename.clone();
                        let display_time = b.timestamp.get(..16).unwrap_or(&b.timestamp).to_string();
                        let meta = format!("{} sends", b.send_count);
                        view! {
                            <div
                                class="settings-row"
                                style=move || format!(
                                    "cursor: pointer; padding: 6px 8px; border-radius: 4px; {}",
                                    if selected.get().as_deref() == Some(&filename) { "background: rgba(255,255,255,0.1);" } else { "" }
                                )
                                on:click={
                                    let filename = filename.clone();
                                    move |_| {
                                        let fname = filename.clone();
                                        set_selected.set(Some(fname.clone()));
                                        set_preview.set(None);
                                        set_result.set(None);
                                        set_error.set(None);
                                        set_loading.set(true);
                                        spawn_local(async move {
                                            let token = match crate::auth::get_auth() {
                                                Some(a) => a.token,
                                                None => return,
                                            };
                                            match crate::api::preview_restore(&token, &fname).await {
                                                Ok(p) => set_preview.set(Some(p)),
                                                Err(e) => set_error.set(Some(e)),
                                            }
                                            set_loading.set(false);
                                        });
                                    }
                                }
                            >
                                <div class="settings-label">
                                    <div class="settings-name" style="font-size: 0.9em;">{display_time}</div>
                                    <div class="settings-desc">{meta}</div>
                                </div>
                            </div>
                        }
                    }).collect_view().into_any()
                }}
            </div>

            // Loading indicator
            {move || loading.get().then(|| view! {
                <div style="padding: 8px; color: #888; font-size: 0.85em;">"Loading preview..."</div>
            })}

            // Preview + Restore button
            {move || preview.get().map(|p| {
                let change_count = p.changes.len();
                let skipped_count = p.skipped.len();
                view! {
                    <div class="backup-preview" style="padding: 8px; border-top: 1px solid rgba(255,255,255,0.1); margin-top: 8px;">
                        <div style="font-size: 0.85em; margin-bottom: 8px;">
                            <div style="color: #51cf66;">
                                {format!("{} values to restore", change_count)}
                            </div>
                            <div style="color: #888;">
                                {format!("{} unchanged", p.unchanged_count)}
                            </div>
                            {(skipped_count > 0).then(|| view! {
                                <div style="color: #fcc419;">
                                    {format!("{} skipped (not found)", skipped_count)}
                                </div>
                            })}
                        </div>
                        <button
                            class="settings-action-btn"
                            disabled=move || restoring.get() || change_count == 0
                            on:click=move |_| {
                                if let Some(fname) = selected.get_untracked() {
                                    set_restoring.set(true);
                                    set_error.set(None);
                                    spawn_local(async move {
                                        let token = match crate::auth::get_auth() {
                                            Some(a) => a.token,
                                            None => return,
                                        };
                                        match crate::api::apply_restore(&token, &fname).await {
                                            Ok(r) => {
                                                set_result.set(Some(r));
                                                set_preview.set(None);
                                            }
                                            Err(e) => set_error.set(Some(e)),
                                        }
                                        set_restoring.set(false);
                                    });
                                }
                            }
                        >
                            {move || if restoring.get() { "Restoring..." } else { "Restore" }}
                        </button>
                    </div>
                }
            })}
        </div>
    }
}
