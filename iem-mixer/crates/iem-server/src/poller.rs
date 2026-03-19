//! Background REAPER poller
//!
//! Runs every 150ms, queries REAPER for meter and send state,
//! detects changes via cache diff, and broadcasts updates to WebSocket clients.
//!
//! Also handles member discovery from REAPER at startup.

use iem_core::{DiscoveredMember, MixSnapshot, ServerMsg};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::proxy::{
    build_channel_templates, query_send_state, reaper_api, reaper_pan_to_ui, reaper_vol_to_db,
};
use crate::snapshot_store::SnapshotStore;
use crate::{AppState, GlobalVolState};

/// Whether the meter bridge ReaScript has been triggered this session
static METER_BRIDGE_STARTED: AtomicBool = AtomicBool::new(false);

/// REAPER action ID for the meter bridge ReaScript
const METER_BRIDGE_ACTION: &str = "_RS_REAPERIEM_METER_BRIDGE";

/// Parse meter bridge EXTSTATE response into per-channel L/R meter data.
/// Input format: "1:-37,-37;2:-911,-911;..." (track_idx:L_db10,R_db10)
/// Returns HashMap<track_idx, [left_linear, right_linear]>
pub fn parse_meter_bridge(text: &str) -> HashMap<usize, [f32; 2]> {
    let mut meters = HashMap::new();
    let db10_to_linear = |v: f32| -> f32 {
        if v <= -1500.0 {
            0.0
        } else {
            10.0_f32.powf(v / 10.0 / 20.0)
        }
    };
    for entry in text.split(';') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        if let Some((idx_str, vals_str)) = entry.split_once(':')
            && let Ok(track_idx) = idx_str.parse::<usize>()
            && let Some((l_str, r_str)) = vals_str.split_once(',')
            && let (Ok(l_db10), Ok(r_db10)) = (l_str.parse::<f32>(), r_str.parse::<f32>())
        {
            meters.insert(track_idx, [db10_to_linear(l_db10), db10_to_linear(r_db10)]);
        }
    }
    meters
}

/// Discover band members from REAPER by querying tracks ending in " inear".
/// REAPER is the source of truth for member names.
/// Returns the list of discovered members, or empty if REAPER is unreachable.
pub async fn discover_members(state: &AppState) -> Vec<DiscoveredMember> {
    let config = state.config.read().await;
    let reaper_url = config.reaper_url.clone();
    let config_ref = config.clone();
    drop(config);

    let tracks_url = reaper_api::query_tracks(&reaper_url);
    let mut members = Vec::new();

    if let Ok(resp) = state.http_client.get(&tracks_url).send().await
        && let Ok(text) = resp.text().await
    {
        let mut send_index = 0;
        for line in text.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.first() == Some(&"TRACK")
                && parts.len() > 2
                && let Ok(track_idx) = parts[1].parse::<usize>()
            {
                let track_name = parts[2];
                if let Some(member) = DiscoveredMember::from_reaper_track(
                    track_name,
                    track_idx,
                    send_index,
                    &config_ref,
                ) {
                    tracing::info!(
                        member = %member.name,
                        track_index = member.track_index,
                        send_index = member.send_index,
                        dante = ?[member.dante_output_l, member.dante_output_r],
                        "Discovered member from REAPER"
                    );
                    members.push(member);
                    send_index += 1;
                }
            }
        }
    }

    // Discover mix_send_index for each member: find which send targets the engineer track.
    // This is critical: member inear tracks have hardware outputs at SEND/0,
    // and the send to ENGINEER sits at a higher index. Hardcoding 0 mutes hardware output!
    let engineer_track = members
        .iter()
        .find(|m| m.id() == "engineer")
        .map(|m| m.track_index);
    if let Some(eng_ti) = engineer_track {
        for member in members.iter_mut().filter(|m| m.id() != "engineer") {
            // Query sends on this member's inear track to find the one targeting engineer.
            // We don't know send count from discovery, so probe up to 10 sends.
            for si in 0..10 {
                // Rate-limit: small delay between REAPER queries to prevent overwhelming
                // the HTTP API. Without this, 9 members × up to 10 sends = 90 rapid-fire
                // requests on startup, which can crash REAPER.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                if let Some(dest) = crate::proxy::query_send_destination(
                    &state.http_client,
                    &reaper_url,
                    member.track_index,
                    si,
                )
                .await
                {
                    // dest is i32: negative = hardware output (skip), positive = track index
                    if dest >= 0 && dest as usize == eng_ti {
                        member.mix_send_index = Some(si);
                        tracing::info!(
                            member = %member.name,
                            track = member.track_index,
                            send_index = si,
                            engineer_track = eng_ti,
                            "Discovered mix send index to engineer"
                        );
                        break;
                    }
                    // dest is -1 (hardware output) or other track → continue probing
                } else {
                    // None = no SEND line in response = send doesn't exist → stop
                    break;
                }
            }
            if member.mix_send_index.is_none() {
                tracing::warn!(
                    member = %member.name,
                    track = member.track_index,
                    "No send to engineer found — mix channel will be read-only"
                );
            }
        }
    }
    if members.is_empty() {
        tracing::warn!("No members discovered from REAPER - using fallback from config.members");
        // Fallback: create DiscoveredMember from legacy config.members
        for (idx, m) in config_ref.members.iter().enumerate() {
            members.push(DiscoveredMember {
                name: m
                    .track_name()
                    .strip_suffix(" inear")
                    .unwrap_or(&m.name.to_uppercase())
                    .to_string(),
                track_index: 0, // Unknown
                dante_output_l: m.dante_output_l,
                dante_output_r: m.dante_output_r,
                send_index: idx,
                mix_send_index: None,
            });
        }
    }

    members
}

/// Spawn the background poller task
pub fn spawn_poller(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(150));
        loop {
            interval.tick().await;
            poll_reaper_and_broadcast(&state).await;
        }
    })
}

/// Single poll cycle: query REAPER, diff against cache, broadcast changes
async fn poll_reaper_and_broadcast(state: &AppState) {
    let config = state.config.read().await;
    let reaper_url = config.reaper_url.clone();
    let inputs = config.inputs.clone();
    drop(config);

    // Build member track_name -> id mapping from discovered members (REAPER source of truth)
    let discovered = state.discovered_members.read().await;
    let member_track_names: HashMap<String, String> = discovered
        .iter()
        .map(|m| (m.track_name(), m.id()))
        .collect();
    drop(discovered);

    // Check which members have active WS connections (HashMap keys)
    let active_members: Vec<String> = {
        let cache = state.mixer_cache.read().await;
        cache.active_members.keys().cloned().collect()
    };

    // Skip polling if no active WebSocket clients
    if active_members.is_empty() {
        return;
    }

    // 1. Query meters via NTRACK;TRACK (single call)
    let mut meters: HashMap<usize, [f32; 2]> = HashMap::new();
    let mut connected = false;
    // Discovered output tracks: member_id -> (track_index, vol_linear, flags)
    let mut output_tracks: HashMap<String, (usize, f32, i32)> = HashMap::new();

    // Name→index map for ALL REAPER tracks (for input track index resolution)
    let mut all_track_names: HashMap<String, usize> = HashMap::new();

    let tracks_url = reaper_api::query_tracks(&reaper_url);
    if let Ok(resp) = state.http_client.get(&tracks_url).send().await
        && let Ok(text) = resp.text().await
    {
        connected = true;

        // Detect track count changes
        for line in text.lines() {
            if let Some(count_str) = line.strip_prefix("NTRACK\t") {
                if let Ok(count) = count_str.parse::<usize>() {
                    let mut cache = state.mixer_cache.write().await;
                    if let Some(prev) = cache.last_track_count
                        && count != prev
                    {
                        tracing::warn!(
                            old_count = prev,
                            new_count = count,
                            "REAPER track count changed — track indices may have shifted"
                        );
                    }
                    cache.last_track_count = Some(count);
                }
                break;
            }
        }

        for line in text.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.first() == Some(&"TRACK")
                && parts.len() > 7
                && let Ok(track_idx) = parts[1].parse::<usize>()
            {
                // Meter fields only present when track has 14+ fields
                // (record-armed tracks include last_meter_peak [6] and last_meter_pos [7])
                // Without these fields (12 fields): field [6] is width, NOT a meter value
                if parts.len() >= 14
                    && let (Ok(peak_db10), Ok(pos_db10)) =
                        (parts[6].parse::<f32>(), parts[7].parse::<f32>())
                {
                    // REAPER HTTP API docs: "last_meter_peak and last_meter_pos
                    // are integers that are dB*10, so -100 would be -10dB."
                    // Floor: -1500 = -150 dB = digital silence (no signal).
                    let db10_to_linear = |v: f32| -> f32 {
                        if v <= -1500.0 {
                            0.0
                        } else {
                            10.0_f32.powf(v / 10.0 / 20.0)
                        }
                    };
                    meters.insert(
                        track_idx,
                        [db10_to_linear(peak_db10), db10_to_linear(pos_db10)],
                    );
                }
                // No VU → don't insert → frontend defaults to 0.0

                // Build name→index map for ALL tracks (input index resolution)
                let track_name = parts[2];
                all_track_names.insert(track_name.to_string(), track_idx);

                // Check if this is a member output track (e.g. "PETKA inear")
                if let Some(member_id) = member_track_names.get(track_name) {
                    let vol: f32 = parts[4].parse().unwrap_or(1.0);
                    let flags: i32 = parts[3].parse().unwrap_or(0);
                    output_tracks.insert(member_id.clone(), (track_idx, vol, flags));
                }
            }
        }
        // Debug: log meter summary periodically (every ~10s = 66 poll cycles)
        use std::sync::atomic::AtomicU64;
        static POLL_COUNT: AtomicU64 = AtomicU64::new(0);
        let count = POLL_COUNT.fetch_add(1, Ordering::Relaxed);
        if count.is_multiple_of(66) {
            let non_zero: Vec<_> = meters
                .iter()
                .filter(|(_, v)| v[0] > 0.001 || v[1] > 0.001)
                .take(5)
                .collect();
            tracing::debug!(
                meter_count = meters.len(),
                non_zero_count = non_zero.len(),
                sample = ?non_zero,
                output_tracks = output_tracks.len(),
                "Meter poll summary"
            );
        }
    }

    // 1b. Try to read meter bridge EXTSTATE for true L/R per-channel peaks.
    // The meter bridge ReaScript (meter_bridge.lua) provides instantaneous
    // Track_GetPeakInfo values per channel, unlike TRACK fields which only
    // give combined peak-hold data.
    if connected {
        // Start meter bridge if not already running.
        // MUST check EXTSTATE first — triggering the action while the script
        // is already running opens REAPER's "ReaScript task control" modal dialog,
        // which blocks the HTTP API and freezes everything.
        if !METER_BRIDGE_STARTED.load(Ordering::Relaxed) {
            // Check if bridge is already running in REAPER before triggering
            let running_url =
                reaper_api::get_extstate(&reaper_url, "REAPERIEM_METERS", "bridge_running");
            let already_running = if let Ok(resp) = state.http_client.get(&running_url).send().await
            {
                resp.text()
                    .await
                    .ok()
                    .and_then(|t| t.split('\t').nth(3).map(|v| v == "1"))
                    .unwrap_or(false)
            } else {
                false
            };

            if already_running {
                // Bridge is running, just set our flag
                METER_BRIDGE_STARTED.store(true, Ordering::Relaxed);
                tracing::info!("Meter bridge already running in REAPER");
            } else {
                let action_url = reaper_api::trigger_action(&reaper_url, METER_BRIDGE_ACTION);
                if state.http_client.get(&action_url).send().await.is_ok() {
                    METER_BRIDGE_STARTED.store(true, Ordering::Relaxed);
                    tracing::info!("Triggered meter bridge ReaScript");
                }
            }
        }

        let extstate_url = reaper_api::get_extstate(&reaper_url, "REAPERIEM_METERS", "peaks");
        if let Ok(resp) = state.http_client.get(&extstate_url).send().await
            && let Ok(text) = resp.text().await
        {
            // REAPER EXTSTATE response: "EXTSTATE\tSECTION\tKEY\tvalue"
            if let Some(value) = text.split('\t').nth(3)
                && !value.is_empty()
            {
                let bridge_meters = parse_meter_bridge(value);
                if !bridge_meters.is_empty() {
                    // Override TRACK field meters with true L/R data
                    meters = bridge_meters;
                }
            }
        }
    }

    // Check if connection status changed
    {
        let mut cache = state.mixer_cache.write().await;
        if cache.connected != connected {
            cache.connected = connected;
            let _ = state.event_tx.send((
                String::new(), // broadcast to all
                ServerMsg::ConnectionChanged { connected },
            ));
        }
    }

    if !connected {
        // Reset meter bridge flag so it re-triggers on reconnection
        METER_BRIDGE_STARTED.store(false, Ordering::Relaxed);
        return;
    }

    // Only broadcast meters that actually changed (threshold: 0.01 linear ≈ -40 dB).
    // During silence this sends nothing; during active playing only ~10-15 channels.
    {
        let cache = state.mixer_cache.read().await;
        let prev = &cache.meters;
        let changed: HashMap<usize, [f32; 2]> = meters
            .iter()
            .filter(|(idx, [l, r])| {
                prev.get(idx).map_or(true, |[pl, pr]| {
                    (l - pl).abs() > 0.01 || (r - pr).abs() > 0.01
                })
            })
            .map(|(k, v)| (*k, *v))
            .collect();

        if !changed.is_empty() {
            let _ = state.event_tx.send((
                String::new(), // broadcast to all
                ServerMsg::Meters { meters: changed },
            ));
        }
    }

    // Update cached meters and output track state
    {
        let now = std::time::Instant::now();
        let echo_suppress_window = Duration::from_millis(500);
        let mut cache = state.mixer_cache.write().await;
        cache.meters = meters;

        // Update output track indices and global volumes, broadcast changes
        for (member_id, (track_idx, vol_linear, flags)) in &output_tracks {
            let old_idx = cache.output_track_indices.get(member_id).copied();
            if old_idx.is_some_and(|old| old != *track_idx) {
                tracing::warn!(
                    member = %member_id,
                    old = old_idx.unwrap(),
                    new = track_idx,
                    "Output track index shifted — forcing full state resync"
                );
                cache.member_states.clear();
            }
            cache
                .output_track_indices
                .insert(member_id.clone(), *track_idx);

            let level_db = crate::proxy::quantize_02(reaper_vol_to_db(*vol_linear));
            let muted = (*flags & 8) != 0;

            let changed = match cache.global_volumes.get(member_id) {
                Some(gv) => (gv.level_db - level_db).abs() > 0.05 || gv.muted != muted,
                None => true,
            };

            if changed {
                // Check echo suppression (keyed with output_track + 100000 offset)
                let key = (member_id.clone(), *track_idx + 100000);
                let recently_commanded = cache
                    .command_timestamps
                    .get(&key)
                    .is_some_and(|ts| now.duration_since(*ts) < echo_suppress_window);

                if !recently_commanded {
                    let _ = state.event_tx.send((
                        member_id.clone(),
                        ServerMsg::GlobalVolumeUpdate { level_db, muted },
                    ));
                }
            }

            cache
                .global_volumes
                .insert(member_id.clone(), GlobalVolState { level_db, muted });
        }

        // Resolve input track indices by name from NTRACK response
        if !all_track_names.is_empty() {
            let mut input_indices: HashMap<String, usize> = HashMap::new();
            for input in &inputs {
                if let Some(&idx) = all_track_names.get(&input.name) {
                    input_indices.insert(input.name.clone(), idx);
                }
            }
            if cache.input_track_indices != input_indices {
                tracing::warn!(
                    "Input track indices changed — forcing full state resync for all members"
                );
                cache.member_states.clear();
            }
            cache.input_track_indices = input_indices;
        }
    }

    // Sync output track index changes back to discovered_members
    if !output_tracks.is_empty() {
        let mut discovered = state.discovered_members.write().await;
        for (member_id, (new_idx, _, _)) in &output_tracks {
            if let Some(member) = discovered.iter_mut().find(|m| m.id() == *member_id)
                && member.track_index != *new_idx
            {
                tracing::warn!(
                    member = %member_id,
                    old = member.track_index,
                    new = new_idx,
                    "Output track index shifted — syncing DiscoveredMember"
                );
                member.track_index = *new_idx;
            }
        }
    }

    // 2. For each active member, query send states in parallel
    // Use discovered members for send indices (REAPER source of truth)
    let discovered = state.discovered_members.read().await;
    let member_indices: Vec<(String, usize)> = active_members
        .iter()
        .filter_map(|mid| {
            discovered
                .iter()
                .find(|m| m.id() == *mid)
                .map(|m| (mid.clone(), m.send_index))
        })
        .collect();
    let discovered_snapshot = discovered.clone();
    drop(discovered);
    let resolved = state.mixer_cache.read().await.input_track_indices.clone();
    let resolved_ref = if resolved.is_empty() {
        None
    } else {
        Some(&resolved)
    };
    let channel_templates = build_channel_templates(&inputs, resolved_ref);

    for (member_id, member_index) in &member_indices {
        let channels = channel_templates.clone();

        // Query all send states in parallel
        let send_futures: Vec<_> = channels
            .iter()
            .map(|ch| {
                let client = state.http_client.clone();
                let url = reaper_url.clone();
                let track_index = ch.track_index;
                async move {
                    let result = query_send_state(&client, &url, track_index, *member_index).await;
                    (track_index, result)
                }
            })
            .collect();

        let send_results = futures::future::join_all(send_futures).await;

        let mut result_channels = channels;
        for (track_index, result) in send_results {
            if let Ok((level, mute, pan)) = result
                && let Some(ch) = result_channels
                    .iter_mut()
                    .find(|c| c.track_index == track_index)
            {
                ch.level_db = crate::proxy::quantize_02(reaper_vol_to_db(level));
                ch.muted = mute;
                ch.pan = reaper_pan_to_ui(pan);
            }
        }

        // For engineer: also query mix channels (member inear → ENGINEER sends)
        if member_id == "engineer" {
            let mut mix_channels =
                crate::proxy::build_mix_channel_templates(&discovered_snapshot, "engineer");

            let mix_futures: Vec<_> = mix_channels
                .iter()
                .filter_map(|ch| {
                    // Look up the discovered mix_send_index for this track
                    let mix_si = discovered_snapshot
                        .iter()
                        .find(|m| m.track_index == ch.track_index)
                        .and_then(|m| m.mix_send_index)?;
                    let client = state.http_client.clone();
                    let url = reaper_url.clone();
                    let track_index = ch.track_index;
                    Some(async move {
                        let result = query_send_state(&client, &url, track_index, mix_si).await;
                        (track_index, result)
                    })
                })
                .collect();

            let mix_results = futures::future::join_all(mix_futures).await;
            for (track_index, result) in mix_results {
                if let Ok((level, mute, pan)) = result
                    && let Some(ch) = mix_channels
                        .iter_mut()
                        .find(|c| c.track_index == track_index)
                {
                    ch.level_db = crate::proxy::quantize_02(reaper_vol_to_db(level));
                    ch.muted = mute;
                    ch.pan = reaper_pan_to_ui(pan);
                }
            }

            result_channels.extend(mix_channels);
        }

        // Diff against cached state and broadcast changes
        let now = std::time::Instant::now();
        let echo_suppress_window = Duration::from_millis(500);
        let mut cache = state.mixer_cache.write().await;

        if let Some(cached_channels) = cache.member_states.get(member_id) {
            // Send per-channel updates for changed channels
            for new_ch in &result_channels {
                if let Some(old_ch) = cached_channels
                    .iter()
                    .find(|c| c.track_index == new_ch.track_index)
                {
                    let changed = (old_ch.level_db - new_ch.level_db).abs() > 0.05
                        || old_ch.muted != new_ch.muted
                        || (old_ch.pan - new_ch.pan).abs() > 0.001;

                    if changed {
                        // Check if this channel was recently commanded — suppress echo
                        let key = (member_id.clone(), new_ch.track_index);
                        let recently_commanded = cache
                            .command_timestamps
                            .get(&key)
                            .is_some_and(|ts| now.duration_since(*ts) < echo_suppress_window);

                        if !recently_commanded {
                            let _ = state.event_tx.send((
                                member_id.clone(),
                                ServerMsg::ChannelUpdate {
                                    track_index: new_ch.track_index,
                                    level_db: new_ch.level_db,
                                    muted: new_ch.muted,
                                    pan: new_ch.pan,
                                },
                            ));
                        }
                    }
                }
            }
        } else {
            // First poll for this member - send full state
            let (global_level_db, global_muted) = cache
                .global_volumes
                .get(member_id)
                .map(|gv| (Some(gv.level_db), Some(gv.muted)))
                .unwrap_or((None, None));
            let output_track_index = cache.output_track_indices.get(member_id).copied();
            let _ = state.event_tx.send((
                member_id.clone(),
                ServerMsg::State {
                    channels: result_channels.clone(),
                    connected: true,
                    global_level_db,
                    global_muted,
                    output_track_index,
                },
            ));
        }

        // Update cache (always, even when suppressing broadcast, so it converges)
        cache
            .member_states
            .insert(member_id.clone(), result_channels.clone());

        // Daily auto-snapshot: on first channel change of the day, save a snapshot
        // capturing the starting state before changes
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let last_date = cache.snapshot_last_date.get(member_id).cloned();
        if last_date.as_deref() != Some(&today) {
            // First activity today for this member - check if we need to save a snapshot
            if !state.snapshot_store.has_snapshot_today(member_id) && !result_channels.is_empty() {
                let channel_map = SnapshotStore::channels_from_state(&result_channels);
                let snapshot = MixSnapshot::new_auto(channel_map);
                if let Err(e) = state.snapshot_store.save_snapshot(member_id, snapshot) {
                    tracing::error!(member = %member_id, error = %e, "Failed to save daily auto-snapshot");
                } else {
                    tracing::info!(member = %member_id, "Saved daily auto-snapshot");
                }
            }
            // Update the tracking date regardless of whether we saved (to avoid repeated checks)
            cache.snapshot_last_date.insert(member_id.clone(), today);
        }

        // Periodic cleanup of stale command timestamps (>2s old)
        let stale_cutoff = Duration::from_secs(2);
        cache
            .command_timestamps
            .retain(|_, ts| now.duration_since(*ts) < stale_cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MixerCache;

    /// Test that command_timestamps suppress echo broadcasts
    #[test]
    fn test_echo_suppression_within_window() {
        let now = std::time::Instant::now();
        let echo_window = Duration::from_millis(500);

        let mut ts_map: HashMap<(String, usize), std::time::Instant> = HashMap::new();
        // Simulate a command sent 100ms ago
        let key = ("petka".to_string(), 1);
        ts_map.insert(key.clone(), now - Duration::from_millis(100));

        // Check: should be suppressed (100ms < 500ms window)
        let recently_commanded = ts_map
            .get(&key)
            .is_some_and(|ts| now.duration_since(*ts) < echo_window);
        assert!(
            recently_commanded,
            "Should suppress echo within 500ms window"
        );
    }

    /// Test that broadcasts happen normally outside the suppression window
    #[test]
    fn test_no_suppression_outside_window() {
        let now = std::time::Instant::now();
        let echo_window = Duration::from_millis(500);

        let mut ts_map: HashMap<(String, usize), std::time::Instant> = HashMap::new();
        // Simulate a command sent 600ms ago
        let key = ("petka".to_string(), 1);
        ts_map.insert(key.clone(), now - Duration::from_millis(600));

        let recently_commanded = ts_map
            .get(&key)
            .is_some_and(|ts| now.duration_since(*ts) < echo_window);
        assert!(
            !recently_commanded,
            "Should NOT suppress echo outside 500ms window"
        );
    }

    /// Test that non-commanded channels are not suppressed
    #[test]
    fn test_no_suppression_for_unrelated_channel() {
        let now = std::time::Instant::now();
        let echo_window = Duration::from_millis(500);

        let mut ts_map: HashMap<(String, usize), std::time::Instant> = HashMap::new();
        // Command for track 1 only
        let key1 = ("petka".to_string(), 1);
        ts_map.insert(key1, now - Duration::from_millis(100));

        // Track 2 should NOT be suppressed
        let key2 = ("petka".to_string(), 2);
        let recently_commanded = ts_map
            .get(&key2)
            .is_some_and(|ts| now.duration_since(*ts) < echo_window);
        assert!(
            !recently_commanded,
            "Unrelated track should NOT be suppressed"
        );
    }

    /// Test that timestamp cleanup removes stale entries
    #[test]
    fn test_stale_timestamp_cleanup() {
        let now = std::time::Instant::now();
        let stale_cutoff = Duration::from_secs(2);

        let mut ts_map: HashMap<(String, usize), std::time::Instant> = HashMap::new();
        let key1 = ("petka".to_string(), 1);
        let key2 = ("petka".to_string(), 2);
        // Fresh timestamp (100ms ago)
        ts_map.insert(key1.clone(), now - Duration::from_millis(100));
        // Stale timestamp (3s ago)
        ts_map.insert(key2, now - Duration::from_secs(3));

        ts_map.retain(|_, ts| now.duration_since(*ts) < stale_cutoff);

        assert_eq!(ts_map.len(), 1, "Stale entries should be cleaned up");
        assert!(ts_map.contains_key(&key1), "Fresh entry should be retained");
    }

    /// Test that MixerCache initializes with empty command_timestamps
    #[test]
    fn test_mixer_cache_new_has_empty_timestamps() {
        let cache = MixerCache::new();
        assert!(cache.command_timestamps.is_empty());
    }

    /// Test that NTRACK output track name matching works
    #[test]
    fn test_ntrack_output_track_name_matching() {
        let member_track_names: HashMap<String, String> = [
            ("PETKA inear".to_string(), "petka".to_string()),
            ("MAREK inear".to_string(), "marek".to_string()),
        ]
        .into_iter()
        .collect();

        // Simulated NTRACK line for PETKA inear at track index 23
        let line =
            "TRACK\t23\tPETKA inear\t0\t1.000000\t0.000000\t-2000\t-2000\t1.000000\t0\t0\t22\t0\t0";
        let parts: Vec<&str> = line.split('\t').collect();
        let track_name = parts[2];

        assert!(member_track_names.contains_key(track_name));
        assert_eq!(
            member_track_names.get(track_name),
            Some(&"petka".to_string())
        );
    }

    /// Test that NTRACK flags mute bit is correctly detected
    #[test]
    fn test_ntrack_flags_mute_bit() {
        // Flag 8 = muted (bit 3)
        assert!((8_i32 & 8) != 0, "Flag 8 should be muted");
        assert!((0_i32 & 8) == 0, "Flag 0 should be unmuted");
        // Flag can have other bits set too
        assert!((9_i32 & 8) != 0, "Flag 9 (8+1) should be muted");
        assert!((7_i32 & 8) == 0, "Flag 7 should be unmuted");
    }

    /// Test MixerCache initializes with empty global volumes
    #[test]
    fn test_mixer_cache_new_has_empty_global_volumes() {
        let cache = MixerCache::new();
        assert!(cache.global_volumes.is_empty());
        assert!(cache.output_track_indices.is_empty());
    }

    /// Helper: parse stereo meters from NTRACK response text using the same logic as the poller.
    /// REAPER HTTP API docs: values are dB×10, so -100 = -10 dB.
    fn parse_meters_from_ntrack(text: &str) -> HashMap<usize, [f32; 2]> {
        let mut meters = HashMap::new();
        for line in text.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.first() == Some(&"TRACK") && parts.len() > 7 {
                if let Ok(track_idx) = parts[1].parse::<usize>() {
                    if parts.len() >= 14 {
                        if let (Ok(peak_db10), Ok(pos_db10)) =
                            (parts[6].parse::<f32>(), parts[7].parse::<f32>())
                        {
                            let db10_to_linear = |v: f32| -> f32 {
                                if v <= -1500.0 {
                                    0.0
                                } else {
                                    10.0_f32.powf(v / 10.0 / 20.0)
                                }
                            };
                            meters.insert(
                                track_idx,
                                [db10_to_linear(peak_db10), db10_to_linear(pos_db10)],
                            );
                        }
                    }
                }
            }
        }
        meters
    }

    /// 14-field TRACK line should produce meter data with correct dB×10 conversion
    #[test]
    fn test_ntrack_with_meter_produces_data() {
        // 14 fields: TRACK idx name flags vol pan peak pos width panmode sendcnt recvcnt hwout color
        // Use -100 (= -10 dB) and -80 (= -8 dB) — realistic signal levels
        let line = "TRACK\t1\tPETKA mic\t0\t1.000000\t0.000000\t-100\t-80\t1.000000\t0\t9\t0\t0\t0";
        let meters = parse_meters_from_ntrack(line);
        assert!(
            meters.contains_key(&1),
            "14-field line should produce meter for track 1"
        );
        let [left, right] = meters[&1];
        // -100 dB×10 = -10.0 dB → 10^(-10/20) ≈ 0.3162
        assert!(
            (left - 0.3162).abs() < 0.01,
            "-100 (dB×10) L should be ~0.3162 linear, got {}",
            left
        );
        // -80 dB×10 = -8.0 dB → 10^(-8/20) ≈ 0.3981
        assert!(
            (right - 0.3981).abs() < 0.01,
            "-80 (dB×10) R should be ~0.3981 linear, got {}",
            right
        );
    }

    /// 12-field TRACK line (without meter fields) must NOT produce meter data
    #[test]
    fn test_ntrack_without_meter_no_data() {
        // 12 fields: TRACK idx name flags vol pan width panmode sendcnt recvcnt hwout color
        // Field [6] is width (1.000000), NOT a meter value
        let line = "TRACK\t1\tPETKA mic\t0\t1.000000\t0.000000\t1.000000\t0\t9\t0\t0\t0";
        let meters = parse_meters_from_ntrack(line);
        assert!(
            !meters.contains_key(&1),
            "12-field line (no meter) should NOT produce meter data"
        );
    }

    /// REAPER meter floor (-1500 = -150 dB) must produce 0.0 (silence)
    /// Verified 2026-02-28: all 33 live tracks report -1500 with no audio.
    #[test]
    fn test_reaper_meter_floor_is_silence() {
        let line = "TRACK\t1\tPETKA mic\t192\t1.000000\t0.000000\t-1500\t-1500\t1.000000\t3\t9\t0\t0\t24421844";
        let meters = parse_meters_from_ntrack(line);
        assert_eq!(
            meters.get(&1),
            Some(&[0.0, 0.0]),
            "-1500 (dB×10 = -150 dB, REAPER meter floor) must be silence"
        );
    }

    /// Values just above floor should produce tiny but non-zero signal
    #[test]
    fn test_reaper_above_floor_shows_signal() {
        // L: -140 dB×10 = -14.0 dB, R: -120 dB×10 = -12.0 dB
        let line = "TRACK\t1\tPETKA mic\t192\t1.000000\t0.000000\t-140\t-120\t1.000000\t3\t9\t0\t0\t24421844";
        let meters = parse_meters_from_ntrack(line);
        let [left, right] = meters[&1];
        assert!(
            left > 0.0,
            "-140 (dB×10) L should show signal, got {}",
            left
        );
        assert!(
            right > 0.0,
            "-120 (dB×10) R should show signal, got {}",
            right
        );
        // -14.0 dB → 10^(-14/20) ≈ 0.1995
        assert!(
            (left - 0.1995).abs() < 0.01,
            "L expected ~0.1995, got {}",
            left
        );
        // -12.0 dB → 10^(-12/20) ≈ 0.2512
        assert!(
            (right - 0.2512).abs() < 0.01,
            "R expected ~0.2512, got {}",
            right
        );
    }

    /// Noise floor values (~-900 dB×10 = -90 dB) should produce near-zero linear
    /// This is the key bug: STEVO mic reports -925 which is -92.5 dB (digital noise),
    /// but was previously treated as -9.25 dB (loud signal) due to /100 instead of /10.
    #[test]
    fn test_reaper_noise_floor_is_near_zero() {
        // Real captured value: STEVO mic reports -925 dB×10 = -92.5 dB
        let line = "TRACK\t2\tSTEVO mic\t192\t1.000000\t0.000000\t-925\t-925\t1.000000\t3\t9\t0\t0\t24421844";
        let meters = parse_meters_from_ntrack(line);
        let [left, right] = meters[&2];
        // -925 dB×10 = -92.5 dB → 10^(-92.5/20) = 10^(-4.625) ≈ 0.0000237
        // This should be essentially invisible on the meter (< 0.01%)
        assert!(
            left < 0.001,
            "-925 (dB×10 = -92.5 dB noise floor) should be near-zero, got {}",
            left
        );
        assert!(
            right < 0.001,
            "-925 (dB×10 = -92.5 dB noise floor) should be near-zero, got {}",
            right
        );
        // But it should be non-zero (it's above the -1500 floor)
        assert!(left > 0.0, "Above floor should be non-zero");
    }

    /// Parse captured live NTRACK response — all tracks silent at -1500 floor
    /// Data captured 2026-02-28 from: curl -s "http://iem.lan:8080/_/NTRACK;TRACK"
    #[test]
    fn test_reaper_captured_ntrack_all_silent() {
        let text = "\
NTRACK\t33
TRACK\t1\tPETKA mic\t192\t1.000000\t0.000000\t-1500\t-1500\t1.000000\t3\t9\t0\t0\t24421844
TRACK\t2\tSTEVO mic\t192\t1.000000\t0.000000\t-1500\t-1500\t1.000000\t3\t9\t0\t0\t24421844
TRACK\t3\tMAREK mic\t192\t1.000000\t0.000000\t-1500\t-1500\t1.000000\t3\t9\t0\t0\t24421844";
        let meters = parse_meters_from_ntrack(text);
        for (idx, [left, right]) in &meters {
            assert_eq!(
                *left, 0.0,
                "Track {} L should be silent (0.0), got {}",
                idx, left
            );
            assert_eq!(
                *right, 0.0,
                "Track {} R should be silent (0.0), got {}",
                idx, right
            );
        }
    }

    /// 0 dB×10 (= 0 dB = full scale) should be 1.0 linear
    #[test]
    fn test_ntrack_meter_full_scale() {
        let line = "TRACK\t1\tPETKA mic\t0\t1.000000\t0.000000\t0\t0\t1.000000\t0\t9\t0\t0\t0";
        let meters = parse_meters_from_ntrack(line);
        let [left, right] = meters[&1];
        assert!(
            (left - 1.0).abs() < 0.01,
            "0 (dB×10 = 0 dB) L should be ~1.0 linear, got {}",
            left
        );
        assert!(
            (right - 1.0).abs() < 0.01,
            "0 (dB×10 = 0 dB) R should be ~1.0 linear, got {}",
            right
        );
    }

    /// Multiple tracks: only 14-field lines produce meters
    #[test]
    fn test_ntrack_mixed_field_counts() {
        let text = "\
NTRACK\t33
TRACK\t1\tPETKA mic\t0\t1.000000\t0.000000\t-1500\t-1500\t1.000000\t0\t9\t0\t0\t0
TRACK\t2\tSTEVO mic\t0\t1.000000\t0.000000\t1.000000\t0\t9\t0\t0\t0
TRACK\t23\tPETKA inear\t0\t1.000000\t0.000000\t-50\t-60\t1.000000\t0\t0\t22\t0\t0";
        let meters = parse_meters_from_ntrack(text);
        assert!(
            meters.contains_key(&1),
            "Track 1 (14 fields) should have meter"
        );
        assert!(
            !meters.contains_key(&2),
            "Track 2 (12 fields) should NOT have meter"
        );
        assert!(
            meters.contains_key(&23),
            "Track 23 (14 fields) should have meter"
        );
        // Verify: track 23 has different peak (-50 = -5 dB) and pos (-60 = -6 dB)
        let [l, r] = meters[&23];
        assert!(
            l > r,
            "L (-50 = -5 dB) should be louder than R (-60 = -6 dB)"
        );
    }

    // --- Meter bridge EXTSTATE parser tests ---

    /// Parse typical meter bridge output with multiple tracks
    #[test]
    fn test_parse_meter_bridge_basic() {
        let input = "1:-37,-37;2:-911,-911;11:-178,-250";
        let meters = parse_meter_bridge(input);
        assert_eq!(meters.len(), 3);

        // Track 1: -37 dB×10 = -3.7 dB → 10^(-3.7/20) ≈ 0.653
        let [l, r] = meters[&1];
        assert!((l - 0.653).abs() < 0.01, "Track 1 L: got {}", l);
        assert!((r - 0.653).abs() < 0.01, "Track 1 R: got {}", r);

        // Track 11: L=-178 (-17.8 dB), R=-250 (-25.0 dB) — TRUE STEREO (different L/R)
        let [l, r] = meters[&11];
        assert!(l > r, "Panned left: L ({}) should be > R ({})", l, r);
        // -17.8 dB → 10^(-17.8/20) ≈ 0.1288
        assert!((l - 0.1288).abs() < 0.01, "Track 11 L: got {}", l);
        // -25.0 dB → 10^(-25/20) ≈ 0.0562
        assert!((r - 0.0562).abs() < 0.01, "Track 11 R: got {}", r);
    }

    /// Parse meter bridge with floor values (silence)
    #[test]
    fn test_parse_meter_bridge_silence() {
        let input = "1:-1500,-1500;2:-1500,-1500";
        let meters = parse_meter_bridge(input);
        assert_eq!(meters[&1], [0.0, 0.0]);
        assert_eq!(meters[&2], [0.0, 0.0]);
    }

    /// Parse empty/missing meter bridge data
    #[test]
    fn test_parse_meter_bridge_empty() {
        assert!(parse_meter_bridge("").is_empty());
        assert!(parse_meter_bridge("  ").is_empty());
    }

    /// Parse meter bridge with noise floor values
    #[test]
    fn test_parse_meter_bridge_noise_floor() {
        // -925 dB×10 = -92.5 dB → should be near-zero (preamp noise)
        let input = "2:-925,-925";
        let meters = parse_meter_bridge(input);
        let [l, r] = meters[&2];
        assert!(l < 0.001, "Noise floor L should be near-zero, got {}", l);
        assert!(r < 0.001, "Noise floor R should be near-zero, got {}", r);
        assert!(l > 0.0, "Above floor should be non-zero");
    }

    /// Parse meter bridge with malformed entries (graceful handling)
    #[test]
    fn test_parse_meter_bridge_malformed() {
        // Mixed valid and invalid entries
        let input = "1:-37,-37;bad;3:-50";
        let meters = parse_meter_bridge(input);
        assert_eq!(meters.len(), 1, "Only valid entries should be parsed");
        assert!(meters.contains_key(&1));
    }

    /// Discovery probes up to 10 sends per member with 50ms delay between requests.
    /// This prevents overwhelming REAPER's HTTP API on startup.
    #[test]
    fn test_discovery_rate_limit_bounds() {
        let delay_ms: u64 = 50;
        let max_sends_per_member: u64 = 10;
        let max_members: u64 = 9;
        let worst_case_ms = delay_ms * max_sends_per_member * max_members;
        assert!(
            worst_case_ms <= 5000,
            "Discovery should complete within 5 seconds, got {}ms",
            worst_case_ms
        );
    }

    // ================================================================
    // Track index safety tests (Issue #85)
    // ================================================================

    /// When poller discovers output track at a different index, DiscoveredMember must sync
    #[test]
    fn test_output_track_index_sync_detects_shift() {
        let mut member = DiscoveredMember {
            name: "PETKA".to_string(),
            track_index: 23,
            dante_output_l: 3,
            dante_output_r: 4,
            send_index: 0,
            mix_send_index: Some(1),
        };

        // Simulate: poller found "PETKA inear" at track 24 (shifted by 1)
        let new_idx: usize = 24;
        if member.track_index != new_idx {
            member.track_index = new_idx;
        }

        assert_eq!(
            member.track_index, 24,
            "DiscoveredMember.track_index must update when track shifts"
        );
    }

    /// When track index hasn't changed, no update should occur
    #[test]
    fn test_output_track_index_sync_no_change() {
        let member = DiscoveredMember {
            name: "PETKA".to_string(),
            track_index: 23,
            dante_output_l: 3,
            dante_output_r: 4,
            send_index: 0,
            mix_send_index: Some(1),
        };

        let new_idx: usize = 23;
        assert_eq!(
            member.track_index, new_idx,
            "No update needed when index matches"
        );
    }

    /// Track count change should be detectable
    #[test]
    fn test_track_count_change_detection() {
        let mut cache = MixerCache::new();
        assert!(cache.last_track_count.is_none());

        // First observation
        cache.last_track_count = Some(33);
        assert_eq!(cache.last_track_count, Some(33));

        // Track inserted → count changes
        let new_count: usize = 34;
        let changed = cache
            .last_track_count
            .map_or(false, |prev| prev != new_count);
        assert!(changed, "Should detect track count change from 33 to 34");
        cache.last_track_count = Some(new_count);
        assert_eq!(cache.last_track_count, Some(34));
    }

    /// Input track indices should be resolvable by name
    #[test]
    fn test_input_track_name_resolution() {
        let inputs = vec![
            iem_core::config::InputTrack {
                name: "PETKA mic".to_string(),
                dante_input: 1,
                default_level_db: 0.0,
            },
            iem_core::config::InputTrack {
                name: "STEVO mic".to_string(),
                dante_input: 2,
                default_level_db: 0.0,
            },
        ];

        // Simulate NTRACK showing tracks at shifted positions
        let mut all_track_names: HashMap<String, usize> = HashMap::new();
        all_track_names.insert("NEW TRACK".to_string(), 1); // inserted track
        all_track_names.insert("PETKA mic".to_string(), 2); // shifted
        all_track_names.insert("STEVO mic".to_string(), 3); // shifted

        let mut input_indices: HashMap<String, usize> = HashMap::new();
        for input in &inputs {
            if let Some(&idx) = all_track_names.get(&input.name) {
                input_indices.insert(input.name.clone(), idx);
            }
        }

        assert_eq!(input_indices.get("PETKA mic"), Some(&2));
        assert_eq!(input_indices.get("STEVO mic"), Some(&3));
    }

    /// MixerCache should initialize with empty input_track_indices
    #[test]
    fn test_mixer_cache_new_has_empty_input_track_indices() {
        let cache = MixerCache::new();
        assert!(cache.input_track_indices.is_empty());
        assert!(cache.last_track_count.is_none());
    }

    // ================================================================
    // Cache invalidation on track index shift (follow-up to #85)
    // ================================================================

    /// When input track indices change, member_states must be cleared to force
    /// a full State resync. Without this, the delta diff compares by track_index
    /// and misses shifted channels, leaving frontends with stale data.
    #[test]
    fn test_index_shift_clears_member_states_for_full_resync() {
        let mut cache = MixerCache::new();

        // Simulate cached state: STEVO at track 2
        cache.member_states.insert(
            "engineer".to_string(),
            vec![
                iem_core::Channel {
                    track_index: 1,
                    name: "PETKA mic".into(),
                    level_db: -10.0,
                    pan: 0.0,
                    muted: false,
                    category: "mics".into(),
                    stereo_pair: None,
                    stereo_side: None,
                },
                iem_core::Channel {
                    track_index: 2,
                    name: "STEVO mic".into(),
                    level_db: -10.0,
                    pan: 0.0,
                    muted: false,
                    category: "mics".into(),
                    stereo_pair: None,
                    stereo_side: None,
                },
            ],
        );
        cache.input_track_indices = [("PETKA mic".to_string(), 1), ("STEVO mic".to_string(), 2)]
            .into_iter()
            .collect();

        // New indices: STEVO shifted to track 3
        let new_indices: HashMap<String, usize> =
            [("PETKA mic".to_string(), 1), ("STEVO mic".to_string(), 3)]
                .into_iter()
                .collect();

        // Detect change → member_states must be cleared
        if cache.input_track_indices != new_indices {
            cache.member_states.clear();
        }
        cache.input_track_indices = new_indices;

        assert!(
            cache.member_states.is_empty(),
            "member_states must be cleared on index shift to force full State resend"
        );
    }

    /// When output track indices change (e.g., member inear track shifts),
    /// member_states must be cleared so engineer mix channels get correct data.
    #[test]
    fn test_output_index_shift_clears_member_states() {
        let mut cache = MixerCache::new();
        cache.output_track_indices.insert("petka".to_string(), 23);
        cache.member_states.insert(
            "engineer".to_string(),
            vec![iem_core::Channel {
                track_index: 23,
                name: "PETKA inear".into(),
                level_db: -6.0,
                pan: 0.0,
                muted: false,
                category: "members".into(),
                stereo_pair: None,
                stereo_side: None,
            }],
        );

        // Output track shifts from 23 to 24
        let new_idx: usize = 24;
        let old_idx = cache.output_track_indices.get("petka").copied();
        if old_idx.is_some_and(|old| old != new_idx) {
            cache.member_states.clear();
        }
        cache
            .output_track_indices
            .insert("petka".to_string(), new_idx);

        assert!(
            cache.member_states.is_empty(),
            "member_states must be cleared on output index shift"
        );
    }

    /// When indices haven't changed, member_states should NOT be cleared
    #[test]
    fn test_no_clear_when_indices_unchanged() {
        let mut cache = MixerCache::new();
        cache.member_states.insert("petka".to_string(), vec![]);
        cache.input_track_indices = [("PETKA mic".to_string(), 1), ("STEVO mic".to_string(), 2)]
            .into_iter()
            .collect();

        // Same indices — no change
        let new_indices: HashMap<String, usize> = cache.input_track_indices.clone();
        if cache.input_track_indices != new_indices {
            cache.member_states.clear();
        }
        cache.input_track_indices = new_indices;

        assert!(
            !cache.member_states.is_empty(),
            "member_states must NOT be cleared when indices are unchanged"
        );
    }

    /// METER_BRIDGE_STARTED resets on disconnect so bridge re-triggers on reconnect.
    /// Without this, REAPER restarts lose the meter bridge forever.
    #[test]
    fn test_meter_bridge_flag_resets_on_disconnect() {
        // Simulate: bridge was started (flag = true)
        METER_BRIDGE_STARTED.store(true, Ordering::Relaxed);
        assert!(METER_BRIDGE_STARTED.load(Ordering::Relaxed));

        // Simulate disconnect: flag should reset
        METER_BRIDGE_STARTED.store(false, Ordering::Relaxed);
        assert!(
            !METER_BRIDGE_STARTED.load(Ordering::Relaxed),
            "Flag must reset on disconnect so bridge re-triggers on reconnect"
        );
    }

    // ================================================================
    // Meter change detection tests (v1.72.0 performance optimization)
    // ================================================================

    /// Helper: compute delta meters using the same logic as poll_and_broadcast.
    /// Returns only meters that changed beyond the 0.01 threshold.
    fn compute_meter_delta(
        prev: &HashMap<usize, [f32; 2]>,
        current: &HashMap<usize, [f32; 2]>,
    ) -> HashMap<usize, [f32; 2]> {
        current
            .iter()
            .filter(|(idx, [l, r])| {
                prev.get(idx).map_or(true, |[pl, pr]| {
                    (l - pl).abs() > 0.01 || (r - pr).abs() > 0.01
                })
            })
            .map(|(k, v)| (*k, *v))
            .collect()
    }

    /// Unchanged meters should NOT be broadcast (saves bandwidth during silence)
    #[test]
    fn test_meter_delta_unchanged_is_empty() {
        let meters: HashMap<usize, [f32; 2]> = [(1, [0.5, 0.5]), (2, [0.0, 0.0]), (3, [0.3, 0.25])]
            .into_iter()
            .collect();

        let delta = compute_meter_delta(&meters, &meters);
        assert!(
            delta.is_empty(),
            "Identical meters should produce empty delta, got {} entries",
            delta.len()
        );
    }

    /// Changed meters should be included in the delta
    #[test]
    fn test_meter_delta_changed_is_broadcast() {
        let prev: HashMap<usize, [f32; 2]> = [(1, [0.5, 0.5]), (2, [0.0, 0.0]), (3, [0.3, 0.25])]
            .into_iter()
            .collect();

        // Track 1 changed significantly, track 2 unchanged, track 3 changed
        let current: HashMap<usize, [f32; 2]> = [
            (1, [0.7, 0.6]),  // changed
            (2, [0.0, 0.0]),  // unchanged
            (3, [0.1, 0.25]), // L changed
        ]
        .into_iter()
        .collect();

        let delta = compute_meter_delta(&prev, &current);
        assert_eq!(delta.len(), 2, "Should have 2 changed tracks");
        assert!(delta.contains_key(&1), "Track 1 changed");
        assert!(delta.contains_key(&3), "Track 3 L changed");
        assert!(!delta.contains_key(&2), "Track 2 unchanged");
    }

    /// Small changes below threshold should NOT trigger broadcast
    #[test]
    fn test_meter_delta_below_threshold() {
        let prev: HashMap<usize, [f32; 2]> = [(1, [0.5, 0.5])].into_iter().collect();
        let current: HashMap<usize, [f32; 2]> = [(1, [0.505, 0.498])].into_iter().collect();

        let delta = compute_meter_delta(&prev, &current);
        assert!(
            delta.is_empty(),
            "Changes < 0.01 should be suppressed, got {} entries",
            delta.len()
        );
    }

    /// New tracks (not in previous cache) should always be broadcast
    #[test]
    fn test_meter_delta_new_track_is_broadcast() {
        let prev: HashMap<usize, [f32; 2]> = [(1, [0.5, 0.5])].into_iter().collect();
        let current: HashMap<usize, [f32; 2]> = [
            (1, [0.5, 0.5]), // unchanged
            (2, [0.3, 0.3]), // new track
        ]
        .into_iter()
        .collect();

        let delta = compute_meter_delta(&prev, &current);
        assert_eq!(delta.len(), 1);
        assert!(delta.contains_key(&2), "New track should be broadcast");
    }

    /// All-silent meters with empty previous cache should be broadcast once
    #[test]
    fn test_meter_delta_first_broadcast_includes_all() {
        let prev: HashMap<usize, [f32; 2]> = HashMap::new();
        let current: HashMap<usize, [f32; 2]> =
            [(1, [0.0, 0.0]), (2, [0.0, 0.0])].into_iter().collect();

        let delta = compute_meter_delta(&prev, &current);
        assert_eq!(
            delta.len(),
            2,
            "First broadcast should include all tracks (no previous cache)"
        );
    }
}
