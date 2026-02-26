//! Background REAPER poller
//!
//! Runs every 150ms, queries REAPER for meter and send state,
//! detects changes via cache diff, and broadcasts updates to WebSocket clients.

use iem_core::ServerMsg;
use std::collections::HashMap;
use std::time::Duration;

use crate::AppState;
use crate::proxy::{
    categorize_track, query_send_state, reaper_api, reaper_pan_to_ui, reaper_vol_to_db,
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

    // Check which members have active WS connections
    let active_members: Vec<String> = {
        let cache = state.mixer_cache.read().await;
        cache.active_members.iter().cloned().collect()
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
    let config = state.config.read().await;
    for member_id in &active_members {
        let member_index = match config.member_index(member_id) {
            Some(idx) => idx,
            None => continue,
        };

        // Build channel templates
        let channels: Vec<iem_core::Channel> = inputs
            .iter()
            .enumerate()
            .map(|(i, input)| {
                let (category, stereo_pair, stereo_side) = categorize_track(&input.name);
                iem_core::Channel {
                    track_index: i + 1,
                    name: input.name.clone(),
                    level_db: 0.0,
                    pan: 0.5,
                    muted: false,
                    category,
                    stereo_pair,
                    stereo_side,
                }
            })
            .collect();

        // Query all send states in parallel
        let send_futures: Vec<_> = channels
            .iter()
            .map(|ch| {
                let client = state.http_client.clone();
                let url = reaper_url.clone();
                let track_index = ch.track_index;
                async move {
                    let result = query_send_state(&client, &url, track_index, member_index).await;
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
        let mut cache = state.mixer_cache.write().await;
        if let Some(cached_channels) = cache.member_states.get(member_id) {
            // Send per-channel updates for changed channels
            for new_ch in &result_channels {
                if let Some(old_ch) = cached_channels
                    .iter()
                    .find(|c| c.track_index == new_ch.track_index)
                {
                    if (old_ch.level_db - new_ch.level_db).abs() > 0.01
                        || old_ch.muted != new_ch.muted
                        || (old_ch.pan - new_ch.pan).abs() > 0.001
                    {
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

        // Update cache
        cache
            .member_states
            .insert(member_id.clone(), result_channels);
    }
    drop(config);
}
