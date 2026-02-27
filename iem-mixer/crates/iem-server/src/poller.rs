//! Background REAPER poller
//!
//! Runs every 150ms, queries REAPER for meter and send state,
//! detects changes via cache diff, and broadcasts updates to WebSocket clients.

use iem_core::ServerMsg;
use std::collections::HashMap;
use std::time::Duration;

use crate::AppState;
use crate::proxy::{
    build_channel_templates, query_send_state, reaper_api, reaper_pan_to_ui, reaper_vol_to_db,
};

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
    let mut meters: HashMap<usize, f32> = HashMap::new();
    let mut connected = false;

    let tracks_url = reaper_api::query_tracks(&reaper_url);
    if let Ok(resp) = state.http_client.get(&tracks_url).send().await {
        if let Ok(text) = resp.text().await {
            connected = true;
            for line in text.lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.first() == Some(&"TRACK")
                    && parts.len() > 7
                    && let Ok(track_idx) = parts[1].parse::<usize>()
                    && let Ok(peak_centibels) = parts[6].parse::<f32>()
                {
                    let peak_db = peak_centibels / 100.0;
                    let peak_linear = if peak_db <= -60.0 {
                        0.0
                    } else {
                        10.0_f32.powf(peak_db / 20.0)
                    };
                    meters.insert(track_idx, peak_linear);
                }
            }
            // Debug: log meter summary periodically (every ~10s = 66 poll cycles)
            use std::sync::atomic::{AtomicU64, Ordering};
            static POLL_COUNT: AtomicU64 = AtomicU64::new(0);
            let count = POLL_COUNT.fetch_add(1, Ordering::Relaxed);
            if count % 66 == 0 {
                let non_zero: Vec<_> = meters.iter().filter(|(_, v)| **v > 0.001).take(5).collect();
                tracing::debug!(
                    meter_count = meters.len(),
                    non_zero_count = non_zero.len(),
                    sample = ?non_zero,
                    "Meter poll summary"
                );
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
        return;
    }

    // Always broadcast meters
    let _ = state.event_tx.send((
        String::new(), // broadcast to all
        ServerMsg::Meters {
            meters: meters.clone(),
        },
    ));

    // Update cached meters
    {
        let mut cache = state.mixer_cache.write().await;
        cache.meters = meters;
    }

    // 2. For each active member, query send states in parallel
    // Clone needed config data and drop lock before async work
    let config = state.config.read().await;
    let member_indices: Vec<(String, usize)> = active_members
        .iter()
        .filter_map(|mid| config.member_index(mid).map(|idx| (mid.clone(), idx)))
        .collect();
    let channel_templates = build_channel_templates(&inputs);
    drop(config);

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
            if let Ok((level, mute, pan)) = result {
                if let Some(ch) = result_channels
                    .iter_mut()
                    .find(|c| c.track_index == track_index)
                {
                    ch.level_db = reaper_vol_to_db(level);
                    ch.muted = mute;
                    ch.pan = reaper_pan_to_ui(pan);
                }
            }
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
                    let changed = (old_ch.level_db - new_ch.level_db).abs() > 0.01
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
            let _ = state.event_tx.send((
                member_id.clone(),
                ServerMsg::State {
                    channels: result_channels.clone(),
                    connected: true,
                },
            ));
        }

        // Update cache (always, even when suppressing broadcast, so it converges)
        cache
            .member_states
            .insert(member_id.clone(), result_channels);

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

        let mut timestamps: HashMap<(String, usize), std::time::Instant> = HashMap::new();
        // Simulate a command sent 100ms ago
        timestamps.insert(
            ("petka".to_string(), 1),
            now - Duration::from_millis(100),
        );

        // Check: should be suppressed (100ms < 500ms window)
        let key = ("petka".to_string(), 1);
        let recently_commanded = timestamps
            .get(&key)
            .is_some_and(|ts| now.duration_since(*ts) < echo_window);
        assert!(recently_commanded, "Should suppress echo within 500ms window");
    }

    /// Test that broadcasts happen normally outside the suppression window
    #[test]
    fn test_no_suppression_outside_window() {
        let now = std::time::Instant::now();
        let echo_window = Duration::from_millis(500);

        let mut timestamps: HashMap<(String, usize), std::time::Instant> = HashMap::new();
        // Simulate a command sent 600ms ago
        timestamps.insert(
            ("petka".to_string(), 1),
            now - Duration::from_millis(600),
        );

        let key = ("petka".to_string(), 1);
        let recently_commanded = timestamps
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

        let mut timestamps: HashMap<(String, usize), std::time::Instant> = HashMap::new();
        // Command for track 1 only
        timestamps.insert(
            ("petka".to_string(), 1),
            now - Duration::from_millis(100),
        );

        // Track 2 should NOT be suppressed
        let key = ("petka".to_string(), 2);
        let recently_commanded = timestamps
            .get(&key)
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

        let mut timestamps: HashMap<(String, usize), std::time::Instant> = HashMap::new();
        // Fresh timestamp (100ms ago)
        timestamps.insert(
            ("petka".to_string(), 1),
            now - Duration::from_millis(100),
        );
        // Stale timestamp (3s ago)
        timestamps.insert(
            ("petka".to_string(), 2),
            now - Duration::from_secs(3),
        );

        timestamps.retain(|_, ts| now.duration_since(*ts) < stale_cutoff);

        assert_eq!(timestamps.len(), 1, "Stale entries should be cleaned up");
        assert!(
            timestamps.contains_key(&("petka".to_string(), 1)),
            "Fresh entry should be retained"
        );
    }

    /// Test that MixerCache initializes with empty command_timestamps
    #[test]
    fn test_mixer_cache_new_has_empty_timestamps() {
        let cache = MixerCache::new();
        assert!(cache.command_timestamps.is_empty());
    }
}
