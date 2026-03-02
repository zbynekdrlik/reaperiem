//! Background REAPER poller
//!
//! Runs every 150ms, queries REAPER for meter and send state,
//! detects changes via cache diff, and broadcasts updates to WebSocket clients.

use iem_core::ServerMsg;
use std::collections::HashMap;
use std::time::Duration;

use crate::proxy::{
    build_channel_templates, query_send_state, reaper_api, reaper_pan_to_ui, reaper_vol_to_db,
};
use crate::{AppState, GlobalVolState};

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
    // Build member track_name -> id mapping for output track discovery
    let member_track_names: HashMap<String, String> = config
        .members
        .iter()
        .map(|m| (m.track_name(), m.id()))
        .collect();
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
    let mut meters: HashMap<usize, [f32; 2]> = HashMap::new();
    let mut connected = false;
    // Discovered output tracks: member_id -> (track_index, vol_linear, flags)
    let mut output_tracks: HashMap<String, (usize, f32, i32)> = HashMap::new();

    let tracks_url = reaper_api::query_tracks(&reaper_url);
    if let Ok(resp) = state.http_client.get(&tracks_url).send().await {
        if let Ok(text) = resp.text().await {
            connected = true;
            for line in text.lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.first() == Some(&"TRACK") && parts.len() > 7 {
                    if let Ok(track_idx) = parts[1].parse::<usize>() {
                        // Meter fields only present when track has 14+ fields
                        // (record-armed tracks include last_meter_peak [6] and last_meter_pos [7])
                        // Without these fields (12 fields): field [6] is width, NOT a meter value
                        if parts.len() >= 14 {
                            if let (Ok(peak_db10), Ok(pos_db10)) =
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
                        }
                        // No VU → don't insert → frontend defaults to 0.0

                        // Check if this is a member output track (e.g. "PETKA inear")
                        // Fields: TRACK idx name flags vol pan vu_peak_L vu_peak_R ...
                        let track_name = parts[2];
                        if let Some(member_id) = member_track_names.get(track_name) {
                            let vol: f32 = parts[4].parse().unwrap_or(1.0);
                            let flags: i32 = parts[3].parse().unwrap_or(0);
                            output_tracks.insert(member_id.clone(), (track_idx, vol, flags));
                        }
                    }
                }
            }
            // Debug: log meter summary periodically (every ~10s = 66 poll cycles)
            use std::sync::atomic::{AtomicU64, Ordering};
            static POLL_COUNT: AtomicU64 = AtomicU64::new(0);
            let count = POLL_COUNT.fetch_add(1, Ordering::Relaxed);
            if count % 66 == 0 {
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

    // Update cached meters and output track state
    {
        let now = std::time::Instant::now();
        let echo_suppress_window = Duration::from_millis(500);
        let mut cache = state.mixer_cache.write().await;
        cache.meters = meters;

        // Update output track indices and global volumes, broadcast changes
        for (member_id, (track_idx, vol_linear, flags)) in &output_tracks {
            cache
                .output_track_indices
                .insert(member_id.clone(), *track_idx);

            let level_db = reaper_vol_to_db(*vol_linear);
            let muted = (*flags & 8) != 0;

            let changed = match cache.global_volumes.get(member_id) {
                Some(gv) => (gv.level_db - level_db).abs() > 0.01 || gv.muted != muted,
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
            let (global_level_db, global_muted) = cache
                .global_volumes
                .get(member_id)
                .map(|gv| (Some(gv.level_db), Some(gv.muted)))
                .unwrap_or((None, None));
            let _ = state.event_tx.send((
                member_id.clone(),
                ServerMsg::State {
                    channels: result_channels.clone(),
                    connected: true,
                    global_level_db,
                    global_muted,
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
        let line =
            "TRACK\t1\tPETKA mic\t0\t1.000000\t0.000000\t-100\t-80\t1.000000\t0\t9\t0\t0\t0";
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
        assert!(left > 0.0, "-140 (dB×10) L should show signal, got {}", left);
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
        assert!(l > r, "L (-50 = -5 dB) should be louder than R (-60 = -6 dB)");
    }
}
