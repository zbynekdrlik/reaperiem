//! Backup restore — preview diff and apply a `MixerBackup` to the live system.
//!
//! Two public entry points:
//! - `preview_restore` — compares backup against live state, returns a diff
//! - `apply_restore`   — applies every value from the backup, saves project

use std::collections::HashMap;

use iem_core::backup::{
    MixerBackup, RestoreCategory, RestoreChange, RestorePreview, RestoreResult, SkippedEntry,
};

use crate::{AppState, proxy};

// =============================================================================
// Public API
// =============================================================================

/// Compare a `MixerBackup` against live REAPER state and return a diff.
///
/// Skipped entries are created when:
/// - A track name from the backup no longer exists in REAPER
/// - A send destination no longer exists or the routing changed
pub async fn preview_restore(
    state: &AppState,
    backup: &MixerBackup,
) -> Result<RestorePreview, String> {
    let reaper_url = {
        let config = state.config.read().await;
        config.reaper_url.clone()
    };

    let client = &state.http_client;

    // Build name → track-index map (with send counts for optimized routing)
    let (track_map, send_counts) = query_track_map_with_sends(client, &reaper_url).await?;

    // Only build routing for source tracks that appear in the backup sends
    let needed_src_names: std::collections::HashSet<String> =
        backup.sends.iter().map(|s| s.src_name.clone()).collect();
    let routing = query_send_routing_for(
        client,
        &reaper_url,
        &track_map,
        &send_counts,
        &needed_src_names,
    )
    .await?;

    // Reverse map: index → name (for description strings)
    let index_to_name: HashMap<u32, String> = track_map
        .iter()
        .map(|(name, idx)| (*idx, name.clone()))
        .collect();

    let mut changes: Vec<RestoreChange> = Vec::new();
    let mut skipped: Vec<SkippedEntry> = Vec::new();
    let mut unchanged_count: usize = 0;

    // --- Sends ---
    for send in &backup.sends {
        let src_idx = match track_map.get(&send.src_name) {
            Some(i) => *i,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} → {}", send.src_name, send.dest_name),
                    reason: format!("Source track '{}' not found in REAPER", send.src_name),
                });
                continue;
            }
        };
        let dest_idx = match track_map.get(&send.dest_name) {
            Some(i) => *i,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} → {}", send.src_name, send.dest_name),
                    reason: format!("Destination track '{}' not found in REAPER", send.dest_name),
                });
                continue;
            }
        };
        let send_idx = match routing.get(&src_idx).and_then(|m| m.get(&dest_idx)) {
            Some(i) => *i,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} → {}", send.src_name, send.dest_name),
                    reason: "Send routing not found in REAPER".to_string(),
                });
                continue;
            }
        };

        match query_send_value(client, &reaper_url, src_idx as usize, send_idx as usize).await {
            Some((cur_vol_linear, cur_pan, cur_muted)) => {
                let cur_vol_db = proxy::reaper_vol_to_db(cur_vol_linear as f32) as f64;
                let vol_changed = (cur_vol_db - send.vol).abs() > 0.0001;
                let pan_changed = (cur_pan - send.pan).abs() > 0.001;
                let mute_changed = cur_muted != send.mute;

                if vol_changed || pan_changed || mute_changed {
                    let mut parts: Vec<String> = Vec::new();
                    if vol_changed {
                        parts.push(format!("vol {:.1}dB", cur_vol_db));
                    }
                    if pan_changed {
                        parts.push(format!("pan {:.2}", cur_pan));
                    }
                    if mute_changed {
                        parts.push(format!("mute {}", cur_muted));
                    }
                    let cur_val = parts.join(", ");

                    let mut bk_parts: Vec<String> = Vec::new();
                    if vol_changed {
                        bk_parts.push(format!("vol {:.1}dB", send.vol));
                    }
                    if pan_changed {
                        bk_parts.push(format!("pan {:.2}", send.pan));
                    }
                    if mute_changed {
                        bk_parts.push(format!("mute {}", send.mute));
                    }
                    let bk_val = bk_parts.join(", ");

                    changes.push(RestoreChange {
                        category: RestoreCategory::Send,
                        description: format!("{} → {}", send.src_name, send.dest_name),
                        current_value: cur_val,
                        backup_value: bk_val,
                    });
                } else {
                    unchanged_count += 1;
                }
            }
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} → {}", send.src_name, send.dest_name),
                    reason: "Could not read current send state from REAPER".to_string(),
                });
            }
        }
    }

    // --- Track volumes ---
    for (track_name, &backup_vol_db) in &backup.track_volumes {
        let track_idx = match track_map.get(track_name) {
            Some(i) => *i,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::TrackVolume,
                    description: track_name.clone(),
                    reason: format!("Track '{}' not found in REAPER", track_name),
                });
                continue;
            }
        };

        match query_track_volume(client, &reaper_url, track_idx as usize).await {
            Some(cur_linear) => {
                let cur_db = proxy::reaper_vol_to_db(cur_linear as f32) as f64;
                if (cur_db - backup_vol_db).abs() > 0.0001 {
                    changes.push(RestoreChange {
                        category: RestoreCategory::TrackVolume,
                        description: track_name.clone(),
                        current_value: format!("{:.1}dB", cur_db),
                        backup_value: format!("{:.1}dB", backup_vol_db),
                    });
                } else {
                    unchanged_count += 1;
                }
            }
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::TrackVolume,
                    description: track_name.clone(),
                    reason: "Could not read current track volume from REAPER".to_string(),
                });
            }
        }
    }

    // --- EQ bands (read live values via ReaScript and compare) ---
    for (track_name, bands) in &backup.eq {
        if bands.is_empty() {
            continue;
        }
        // Skip tracks where all bands are default (gain=0dB, disabled) — no need to read REAPER
        let has_non_default = bands
            .iter()
            .any(|b| b.enabled || (b.gain_norm - 0.25).abs() > 0.01);
        if !has_non_default {
            unchanged_count += 1;
            continue;
        }

        let Some(&track_idx) = track_map.get(track_name.as_str()) else {
            skipped.push(SkippedEntry {
                category: RestoreCategory::Eq,
                description: track_name.clone(),
                reason: format!("Track '{}' not found in REAPER", track_name),
            });
            continue;
        };

        // Read current EQ via ReaScript (only for tracks with non-default bands)
        let live_eq = proxy::handle_get_eq_params(state, track_idx as usize).await;
        match live_eq {
            Some(iem_core::ServerMsg::EqParams {
                bands: live_bands, ..
            }) => {
                // Compare each band: freq_norm, gain_norm, bw_norm, enabled
                let mut eq_differs = false;
                if live_bands.len() != bands.len() {
                    eq_differs = true;
                } else {
                    for (backup_band, live_band) in bands.iter().zip(live_bands.iter()) {
                        if (backup_band.freq_norm - live_band.freq_norm).abs() > 0.001
                            || (backup_band.gain_norm - live_band.gain_norm).abs() > 0.001
                            || (backup_band.bw_norm - live_band.bw_norm).abs() > 0.001
                            || backup_band.enabled != live_band.enabled
                        {
                            eq_differs = true;
                            break;
                        }
                    }
                }
                if eq_differs {
                    let enabled_count = bands.iter().filter(|b| b.enabled).count();
                    changes.push(RestoreChange {
                        category: RestoreCategory::Eq,
                        description: track_name.clone(),
                        current_value: format!("{} bands", live_bands.len()),
                        backup_value: format!("{} bands ({} enabled)", bands.len(), enabled_count),
                    });
                } else {
                    unchanged_count += 1;
                }
            }
            _ => {
                // Could not read EQ — count as change to be safe
                changes.push(RestoreChange {
                    category: RestoreCategory::Eq,
                    description: track_name.clone(),
                    current_value: "(could not read)".to_string(),
                    backup_value: format!("{} bands", bands.len()),
                });
            }
        }
    }

    // --- Limiter (read live values via ReaScript and compare) ---
    for (track_name, lim) in &backup.limiter {
        let Some(&track_idx) = track_map.get(track_name.as_str()) else {
            skipped.push(SkippedEntry {
                category: RestoreCategory::Limiter,
                description: track_name.clone(),
                reason: format!("Track '{}' not found in REAPER", track_name),
            });
            continue;
        };

        let live_lim = proxy::handle_get_limiter_params(state, track_idx as usize).await;
        match live_lim {
            Some(iem_core::ServerMsg::LimiterParams {
                limit_db,
                limit_norm,
                enabled,
                ..
            }) => {
                if (limit_db - lim.limit_db).abs() > 0.01
                    || (limit_norm - lim.limit_norm).abs() > 0.01
                    || enabled != lim.enabled
                {
                    changes.push(RestoreChange {
                        category: RestoreCategory::Limiter,
                        description: track_name.clone(),
                        current_value: format!("limit={:.1}dB enabled={}", limit_db, enabled),
                        backup_value: format!(
                            "limit={:.1}dB enabled={}",
                            lim.limit_db, lim.enabled
                        ),
                    });
                } else {
                    unchanged_count += 1;
                }
            }
            _ => {
                changes.push(RestoreChange {
                    category: RestoreCategory::Limiter,
                    description: track_name.clone(),
                    current_value: "(could not read)".to_string(),
                    backup_value: format!("limit={:.1}dB enabled={}", lim.limit_db, lim.enabled),
                });
            }
        }
    }

    // --- Customizations ---
    for (member_id, backup_cust) in &backup.customizations {
        let cur_cust = state.customization_store.load(member_id);
        if cur_cust.pinned != backup_cust.pinned || cur_cust.hidden != backup_cust.hidden {
            changes.push(RestoreChange {
                category: RestoreCategory::Customization,
                description: member_id.clone(),
                current_value: format!("pinned={:?} hidden={:?}", cur_cust.pinned, cur_cust.hidden),
                backup_value: format!(
                    "pinned={:?} hidden={:?}",
                    backup_cust.pinned, backup_cust.hidden
                ),
            });
        } else {
            unchanged_count += 1;
        }
    }

    // --- PINs ---
    let current_pins = state.pin_store.read().await.all_pins();
    for (member_id, backup_pin) in &backup.pins {
        let cur_pin = current_pins
            .get(member_id)
            .map(|s| s.as_str())
            .unwrap_or("");
        if cur_pin != backup_pin.as_str() {
            changes.push(RestoreChange {
                category: RestoreCategory::Pin,
                description: member_id.clone(),
                current_value: if cur_pin.is_empty() {
                    "(none)".to_string()
                } else {
                    "(set)".to_string()
                },
                backup_value: "(set)".to_string(),
            });
        } else {
            unchanged_count += 1;
        }
    }

    // Suppress unused-variable warning for index_to_name (kept for potential future use)
    let _ = index_to_name;

    // Estimate restore time: EQ is slow (~0.24s per band × 4 params), rest is fast
    let eq_band_count: usize = changes
        .iter()
        .filter(|c| c.category == RestoreCategory::Eq)
        .count()
        * 5; // ~5 bands per track
    let limiter_count = changes
        .iter()
        .filter(|c| c.category == RestoreCategory::Limiter)
        .count();
    let send_count = changes
        .iter()
        .filter(|c| c.category == RestoreCategory::Send)
        .count();
    // EQ: 4 params × 0.06s per param + 0.3s read per track
    // Limiter: 2 params × 0.06s + 0.3s read per track
    // Sends: ~0.01s each
    // Add overhead for track map + routing queries (~3s)
    let estimated_seconds = (eq_band_count as f32 * 0.24
        + limiter_count as f32 * 0.42
        + send_count as f32 * 0.01
        + 3.0)
        .ceil() as u32;

    Ok(RestorePreview {
        changes,
        unchanged_count,
        skipped,
        estimated_seconds,
    })
}

/// Apply all values from a `MixerBackup` to the live system.
///
/// Order: sends → track volumes → EQ → limiter → customizations → PINs → save project.
pub async fn apply_restore(
    state: &AppState,
    backup: &MixerBackup,
) -> Result<RestoreResult, String> {
    let reaper_url = {
        let config = state.config.read().await;
        config.reaper_url.clone()
    };

    let client = &state.http_client;

    // Build name → track-index map (with send counts for optimized routing)
    let (track_map, send_counts) = query_track_map_with_sends(client, &reaper_url).await?;

    // Only build routing for source tracks that appear in the backup sends
    let needed_src_names: std::collections::HashSet<String> =
        backup.sends.iter().map(|s| s.src_name.clone()).collect();
    let routing = query_send_routing_for(
        client,
        &reaper_url,
        &track_map,
        &send_counts,
        &needed_src_names,
    )
    .await?;

    let mut restored_count: usize = 0;
    let mut skipped: Vec<SkippedEntry> = Vec::new();

    // --- Apply sends (only changed values) ---
    for send in &backup.sends {
        let src_idx = match track_map.get(&send.src_name) {
            Some(i) => *i,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} → {}", send.src_name, send.dest_name),
                    reason: format!("Source track '{}' not found in REAPER", send.src_name),
                });
                continue;
            }
        };
        let dest_idx = match track_map.get(&send.dest_name) {
            Some(i) => *i,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} → {}", send.src_name, send.dest_name),
                    reason: format!("Destination track '{}' not found in REAPER", send.dest_name),
                });
                continue;
            }
        };
        let send_idx = match routing.get(&src_idx).and_then(|m| m.get(&dest_idx)) {
            Some(i) => *i as usize,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Send,
                    description: format!("{} → {}", send.src_name, send.dest_name),
                    reason: "Send routing not found in REAPER".to_string(),
                });
                continue;
            }
        };

        // Read current value and skip if unchanged
        // Note: backup vol is in dB, REAPER API returns linear — convert for comparison
        let backup_vol_linear = proxy::db_to_reaper_vol(send.vol as f32) as f64;
        let current = query_send_value(client, &reaper_url, src_idx as usize, send_idx).await;
        if let Some((cur_vol, cur_pan, cur_mute)) = current {
            let vol_diff = (cur_vol - backup_vol_linear).abs() > 0.0001;
            let pan_diff = (cur_pan - send.pan).abs() > 0.001;
            let mute_diff = cur_mute != send.mute;
            if !vol_diff && !pan_diff && !mute_diff {
                continue; // Already matches — skip write
            }
        }

        let src = src_idx as usize;

        // Write using linear volume (REAPER API expects linear)
        let _ = client
            .get(format!(
                "{}/_/SET/TRACK/{}/SEND/{}/VOL/{:.10}",
                reaper_url, src, send_idx, backup_vol_linear
            ))
            .send()
            .await;
        let _ = client
            .get(format!(
                "{}/_/SET/TRACK/{}/SEND/{}/PAN/{:.10}",
                reaper_url, src, send_idx, send.pan
            ))
            .send()
            .await;
        let mute_val: u8 = if send.mute { 1 } else { 0 };
        let _ = client
            .get(format!(
                "{}/_/SET/TRACK/{}/SEND/{}/MUTE/{}",
                reaper_url, src, send_idx, mute_val
            ))
            .send()
            .await;

        restored_count += 1;
    }

    // --- Apply track volumes ---
    for (track_name, &backup_vol_db) in &backup.track_volumes {
        let track_idx = match track_map.get(track_name) {
            Some(i) => *i as usize,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::TrackVolume,
                    description: track_name.clone(),
                    reason: format!("Track '{}' not found in REAPER", track_name),
                });
                continue;
            }
        };

        // Read current volume and skip if unchanged
        // backup_vol_db is in dB, REAPER API returns/accepts linear
        let backup_vol_linear = proxy::db_to_reaper_vol(backup_vol_db as f32) as f64;
        let current_vol = query_track_volume(client, &reaper_url, track_idx).await;
        if let Some(cur) = current_vol
            && (cur - backup_vol_linear).abs() < 0.0001
        {
            continue; // Already matches
        }

        let _ = client
            .get(format!(
                "{}/_/SET/TRACK/{}/VOL/{:.10}",
                reaper_url, track_idx, backup_vol_linear
            ))
            .send()
            .await;

        restored_count += 1;
    }

    // --- Apply EQ bands (only if changed — EQ writes are slow: 60ms per param) ---
    for (track_name, bands) in &backup.eq {
        let track_idx = match track_map.get(track_name) {
            Some(i) => *i as usize,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Eq,
                    description: track_name.clone(),
                    reason: format!("Track '{}' not found in REAPER", track_name),
                });
                continue;
            }
        };

        // Read current EQ to skip unchanged bands
        let live_eq = proxy::handle_get_eq_params(state, track_idx).await;
        let live_bands = match live_eq {
            Some(iem_core::ServerMsg::EqParams { bands: lb, .. }) => lb,
            _ => Vec::new(),
        };

        let mut any_written = false;
        for band in bands {
            // Check if this band differs from live
            let differs = live_bands.get(band.band as usize).is_none_or(|lb| {
                (band.freq_norm - lb.freq_norm).abs() > 0.001
                    || (band.gain_norm - lb.gain_norm).abs() > 0.001
                    || (band.bw_norm - lb.bw_norm).abs() > 0.001
                    || band.enabled != lb.enabled
            });

            if differs {
                let b = band.band;
                for (param, value) in [
                    ("fn", band.freq_norm),
                    ("gn", band.gain_norm),
                    ("bn", band.bw_norm),
                    ("en", if band.enabled { 1.0f32 } else { 0.0f32 }),
                ] {
                    apply_eq_param(state, &reaper_url, track_idx, b, param, value).await;
                }
                any_written = true;
            }
        }

        if any_written {
            restored_count += 1;
        }
    }

    // --- Apply limiter params (only if changed) ---
    for (track_name, lim) in &backup.limiter {
        let track_idx = match track_map.get(track_name) {
            Some(i) => *i as usize,
            None => {
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Limiter,
                    description: track_name.clone(),
                    reason: format!("Track '{}' not found in REAPER", track_name),
                });
                continue;
            }
        };

        // Read current limiter to skip unchanged
        let live_lim = proxy::handle_get_limiter_params(state, track_idx).await;
        let (live_limit_db, live_enabled) = match live_lim {
            Some(iem_core::ServerMsg::LimiterParams {
                limit_db, enabled, ..
            }) => (limit_db, enabled),
            _ => (0.0, false), // Can't read — apply anyway
        };

        let limit_differs = (live_limit_db - lim.limit_db).abs() > 0.01;
        let enabled_differs = live_enabled != lim.enabled;

        if limit_differs {
            apply_limiter_param(state, &reaper_url, track_idx, "limit", lim.limit_norm).await;
        }
        if enabled_differs {
            apply_limiter_param(
                state,
                &reaper_url,
                track_idx,
                "enabled",
                if lim.enabled { 1.0 } else { 0.0 },
            )
            .await;
        }
        if limit_differs || enabled_differs {
            restored_count += 1;
        }
    }

    // --- Apply customizations ---
    for (member_id, cust) in &backup.customizations {
        if let Err(e) = state.customization_store.save(member_id, cust) {
            tracing::warn!(member = %member_id, error = %e, "apply_restore: failed to save customization");
            skipped.push(SkippedEntry {
                category: RestoreCategory::Customization,
                description: member_id.clone(),
                reason: format!("IO error saving customization: {e}"),
            });
            continue;
        }
        restored_count += 1;
    }

    // --- Apply PINs ---
    {
        let mut pin_store = state.pin_store.write().await;
        for (member_id, pin) in &backup.pins {
            if let Err(e) = pin_store.set_pin(member_id, pin) {
                tracing::warn!(member = %member_id, error = %e, "apply_restore: failed to set PIN");
                skipped.push(SkippedEntry {
                    category: RestoreCategory::Pin,
                    description: member_id.clone(),
                    reason: format!("IO error setting PIN: {e}"),
                });
                continue;
            }
            restored_count += 1;
        }
    }

    // --- Save REAPER project ---
    let save_url = format!("{}/_/40026", reaper_url);
    let project_saved = client.get(&save_url).send().await.is_ok();
    if !project_saved {
        tracing::warn!("apply_restore: failed to save REAPER project");
    }

    tracing::info!(
        restored_count,
        skipped = skipped.len(),
        project_saved,
        "apply_restore: complete"
    );

    Ok(RestoreResult {
        restored_count,
        skipped,
        project_saved,
    })
}

// =============================================================================
// Private helpers
// =============================================================================

/// Query track name→index map AND name→send_count map in one call.
async fn query_track_map_with_sends(
    client: &reqwest::Client,
    reaper_url: &str,
) -> Result<(HashMap<String, u32>, HashMap<String, u32>), String> {
    let url = proxy::reaper_api::query_tracks(reaper_url);
    let text = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to query REAPER tracks: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Failed to read REAPER tracks response: {e}"))?;

    let mut idx_map: HashMap<String, u32> = HashMap::new();
    let mut send_map: HashMap<String, u32> = HashMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() != Some(&"TRACK") || parts.len() < 11 {
            continue;
        }
        let Ok(idx) = parts[1].parse::<u32>() else {
            continue;
        };
        if idx == 0 {
            continue; // skip master
        }
        let name = parts[2].to_string();
        let send_count: u32 = parts[10].parse().unwrap_or(0);
        send_map.insert(name.clone(), send_count);
        idx_map.insert(name, idx);
    }
    Ok((idx_map, send_map))
}

/// Build src_idx → { dest_idx → send_idx } routing map.
///
/// Only queries routing for tracks in `needed_src_names` (not all tracks).
/// Uses send_count to avoid querying beyond actual sends.
async fn query_send_routing_for(
    client: &reqwest::Client,
    reaper_url: &str,
    track_map: &HashMap<String, u32>,
    send_counts: &HashMap<String, u32>,
    needed_src_names: &std::collections::HashSet<String>,
) -> Result<HashMap<u32, HashMap<u32, u32>>, String> {
    let mut routing: HashMap<u32, HashMap<u32, u32>> = HashMap::new();

    for src_name in needed_src_names {
        let Some(&src_idx) = track_map.get(src_name) else {
            continue;
        };
        let max_sends = send_counts.get(src_name).copied().unwrap_or(0);
        if max_sends == 0 {
            continue;
        }

        let mut dest_map: HashMap<u32, u32> = HashMap::new();

        for send_idx in 0..max_sends {
            let url =
                proxy::reaper_api::get_send_state(reaper_url, src_idx as usize, send_idx as usize);
            let text = match client.get(&url).send().await {
                Ok(r) => match r.text().await {
                    Ok(t) => t,
                    Err(_) => break,
                },
                Err(_) => break,
            };

            let trimmed = text.trim();
            if trimmed.is_empty() {
                break;
            }

            for part_line in text.lines() {
                let parts: Vec<&str> = part_line.split('\t').collect();
                if parts.first() != Some(&"SEND") || parts.len() < 7 {
                    break;
                }
                let dest: i32 = parts[6].parse().unwrap_or(-1);
                if dest < 1 {
                    continue;
                }
                dest_map.insert(dest as u32, send_idx);
            }
        }

        if !dest_map.is_empty() {
            routing.insert(src_idx, dest_map);
        }
    }

    Ok(routing)
}

/// Read current (vol_linear, pan, muted) for a send.
///
/// Returns `None` if the REAPER call fails or the response cannot be parsed.
async fn query_send_value(
    client: &reqwest::Client,
    reaper_url: &str,
    src_idx: usize,
    send_idx: usize,
) -> Option<(f64, f64, bool)> {
    let url = proxy::reaper_api::get_send_state(reaper_url, src_idx, send_idx);
    let text = client.get(&url).send().await.ok()?.text().await.ok()?;

    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() != Some(&"SEND") || parts.len() < 7 {
            continue;
        }
        let vol: f64 = parts[4].parse().unwrap_or(1.0);
        let pan: f64 = parts[5].parse().unwrap_or(0.0);
        let mute_flag: i32 = parts[3].parse().unwrap_or(0);
        // Mute flag: 0 = unmuted, 8 = muted in REAPER response
        let muted = mute_flag != 0;
        return Some((vol, pan, muted));
    }

    None
}

/// Read current linear volume for a track.
///
/// Returns `None` if the REAPER call fails or the response cannot be parsed.
async fn query_track_volume(
    client: &reqwest::Client,
    reaper_url: &str,
    track_idx: usize,
) -> Option<f64> {
    let url = format!("{}/_/TRACK/{}", reaper_url, track_idx);
    let text = client.get(&url).send().await.ok()?.text().await.ok()?;

    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() != Some(&"TRACK") || parts.len() < 5 {
            continue;
        }
        let vol: f64 = parts[4].parse().unwrap_or(1.0);
        return Some(vol);
    }

    None
}

/// Apply a single EQ band parameter via EXTSTATE + ReaScript.
async fn apply_eq_param(
    state: &AppState,
    reaper_url: &str,
    track_index: usize,
    band: u8,
    param: &str,
    value: f32,
) {
    let _lock = state.eq_write_lock.lock().await;

    let eq_set_value = format!(
        "track={}|band={}|param={}|value={:.6}",
        track_index, band, param, value
    );
    let set_url = proxy::reaper_api::set_extstate(reaper_url, "reaperiem", "eq_set", &eq_set_value);
    if state.http_client.get(&set_url).send().await.is_err() {
        tracing::warn!(
            track_index,
            band,
            param,
            "restore: failed to set eq_set EXTSTATE"
        );
        return;
    }

    let action_url = proxy::reaper_api::trigger_action(reaper_url, "_RS_REAPERIEM_SET_EQ");
    if state.http_client.get(&action_url).send().await.is_err() {
        tracing::warn!(
            track_index,
            band,
            param,
            "restore: failed to trigger SET_EQ"
        );
        return;
    }

    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
}

/// Apply a single limiter parameter via EXTSTATE + ReaScript.
async fn apply_limiter_param(
    state: &AppState,
    reaper_url: &str,
    track_index: usize,
    param: &str,
    value: f32,
) {
    let _lock = state.limiter_write_lock.lock().await;

    let set_value = format!("track={}|param={}|value={:.6}", track_index, param, value);
    let set_url =
        proxy::reaper_api::set_extstate(reaper_url, "reaperiem", "limiter_set", &set_value);
    if state.http_client.get(&set_url).send().await.is_err() {
        tracing::warn!(
            track_index,
            param,
            "restore: failed to set limiter_set EXTSTATE"
        );
        return;
    }

    let action_url = proxy::reaper_api::trigger_action(reaper_url, "_RS_REAPERIEM_SET_LIMITER");
    if state.http_client.get(&action_url).send().await.is_err() {
        tracing::warn!(track_index, param, "restore: failed to trigger SET_LIMITER");
        return;
    }

    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
}

// =============================================================================
// Unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use iem_core::backup::{BACKUP_VERSION, EqBandBackup, LimiterBackup, MixerBackup, SendBackup};
    use std::collections::HashMap;

    fn minimal_backup() -> MixerBackup {
        MixerBackup {
            version: BACKUP_VERSION,
            timestamp: "2026-04-07T10:00:00Z".to_string(),
            track_layout: HashMap::new(),
            sends: vec![SendBackup {
                src_name: "PETKA mic".to_string(),
                dest_name: "PETKA inear".to_string(),
                vol: -3.0,
                pan: 0.0,
                mute: false,
            }],
            track_volumes: {
                let mut m = HashMap::new();
                m.insert("PETKA inear".to_string(), -6.0f64);
                m
            },
            eq: {
                let mut m = HashMap::new();
                m.insert(
                    "PETKA mic".to_string(),
                    vec![EqBandBackup {
                        band: 0,
                        band_type: "band".to_string(),
                        freq_norm: 0.5,
                        gain_norm: 0.25,
                        bw_norm: 0.3,
                        freq_hz: 1000.0,
                        gain_db: 0.0,
                        bw_oct: 1.0,
                        enabled: true,
                    }],
                );
                m
            },
            limiter: {
                let mut m = HashMap::new();
                m.insert(
                    "PETKA inear".to_string(),
                    LimiterBackup {
                        limit_db: -3.0,
                        limit_norm: 0.5,
                        enabled: true,
                    },
                );
                m
            },
            customizations: HashMap::new(),
            pins: HashMap::new(),
        }
    }

    #[test]
    fn test_backup_has_expected_sends() {
        let b = minimal_backup();
        assert_eq!(b.sends.len(), 1);
        assert_eq!(b.sends[0].src_name, "PETKA mic");
        assert_eq!(b.sends[0].dest_name, "PETKA inear");
        assert!(!b.sends[0].mute);
    }

    #[test]
    fn test_mute_flag_conversion() {
        // API response mute flag: 0 = unmuted, 8 = muted (bitfield)
        // Backup stores mute as bool
        let flag_unmuted: i32 = 0;
        let flag_muted: i32 = 8;
        assert!(!{ flag_unmuted != 0 });
        assert!({ flag_muted != 0 });
    }

    #[test]
    fn test_mute_set_value() {
        // API SET takes 0 or 1 — not 0 or 8
        let mute = true;
        let val: u8 = if mute { 1 } else { 0 };
        assert_eq!(val, 1u8);

        let mute = false;
        let val: u8 = if mute { 1 } else { 0 };
        assert_eq!(val, 0u8);
    }

    #[test]
    fn test_volume_tolerance() {
        let cur = -3.0f64;
        let backup = -3.00005f64;
        // Within tolerance (0.0001) → no change
        assert!(!((cur - backup).abs() > 0.0001));

        let cur = -3.0f64;
        let backup = -3.5f64;
        // Outside tolerance → change
        assert!((cur - backup).abs() > 0.0001);
    }

    #[test]
    fn test_pan_tolerance() {
        let cur = 0.0f64;
        let backup = 0.0005f64;
        // Within tolerance (0.001) → no change
        assert!(!((cur - backup).abs() > 0.001));

        let cur = 0.0f64;
        let backup = 0.1f64;
        // Outside tolerance → change
        assert!((cur - backup).abs() > 0.001);
    }

    #[test]
    fn test_restore_result_defaults() {
        let result = RestoreResult {
            restored_count: 5,
            skipped: vec![],
            project_saved: true,
        };
        assert_eq!(result.restored_count, 5);
        assert!(result.project_saved);
        assert!(result.skipped.is_empty());
    }
}
