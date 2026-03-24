//! REAPER HTTP API proxy

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
};
use iem_core::{ApiError, BatchControlRequest, BatchOperation, PollResponse};
use std::collections::HashMap;

use crate::AppState;

/// Query parameters for WebSocket connection
#[derive(Debug, serde::Deserialize)]
pub struct WsQuery {
    pub token: Option<String>,
}

/// Proxy a request to REAPER
///
/// Forwards requests from /api/reaper/* to the REAPER HTTP API
pub async fn proxy_reaper(
    State(state): State<AppState>,
    method: Method,
    Path(path): Path<String>,
    body: Body,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    let reaper_url = format!("{}/{}", config.reaper_url, path);
    drop(config);

    tracing::debug!(url = %reaper_url, method = %method, "Proxying to REAPER");

    // Convert body to bytes
    let body_bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!(error = %e, "Failed to read request body");
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError::new("BODY_ERROR", "Failed to read request body")),
            ));
        }
    };

    // Build proxy request
    let req = state
        .http_client
        .request(method.clone(), &reaper_url)
        .body(body_bytes.to_vec());

    // Send request
    let resp = req.send().await.map_err(|e| {
        tracing::error!(error = %e, "REAPER proxy error");
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError::new(
                "REAPER_ERROR",
                format!("REAPER unavailable: {}", e),
            )),
        )
    })?;

    // Build response
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::OK);
    let body = resp.bytes().await.map_err(|e| {
        tracing::error!(error = %e, "Failed to read REAPER response");
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError::new(
                "REAPER_ERROR",
                "Failed to read REAPER response",
            )),
        )
    })?;

    Ok((status, body.to_vec()).into_response())
}

/// Get current mixer state for a member
pub async fn get_mixer_state(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<iem_core::MixerState>, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member_id, &config.jwt_secret)?;

    // Use discovered members (REAPER source of truth) instead of config
    let discovered = state.discovered_members.read().await;
    let member_index = discovered
        .iter()
        .find(|m| m.id() == member_id)
        .map(|m| m.send_index)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;
    drop(discovered);
    let reaper_url = config.reaper_url.clone();

    let resolved = state.mixer_cache.read().await.input_track_indices.clone();
    let resolved_ref = if resolved.is_empty() {
        None
    } else {
        Some(&resolved)
    };
    let channels = build_channel_templates(&config.inputs, resolved_ref);
    drop(config);

    // Try to get actual levels from REAPER (all channels in parallel)
    let mut result_channels = channels.clone();
    let send_futures: Vec<_> = result_channels
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
    for (track_index, result) in send_results {
        if let Ok((level, mute, pan)) = result
            && let Some(ch) = result_channels
                .iter_mut()
                .find(|c| c.track_index == track_index)
        {
            ch.level_db = reaper_vol_to_db(level);
            ch.muted = mute;
            ch.pan = reaper_pan_to_ui(pan);
        }
    }

    // For engineer: append mix channels (member inear → ENGINEER sends)
    if member_id == "engineer" {
        let discovered = state.discovered_members.read().await;
        let mut mix_channels = build_mix_channel_templates(&discovered, "engineer");

        // Query mix send states using discovered mix_send_index (NOT hardcoded 0!)
        let mix_futures: Vec<_> = mix_channels
            .iter()
            .filter_map(|ch| {
                let mix_si = discovered
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
        drop(discovered);

        let mix_results = futures::future::join_all(mix_futures).await;
        for (track_index, result) in mix_results {
            if let Ok((level, mute, pan)) = result
                && let Some(ch) = mix_channels
                    .iter_mut()
                    .find(|c| c.track_index == track_index)
            {
                ch.level_db = reaper_vol_to_db(level);
                ch.muted = mute;
                ch.pan = reaper_pan_to_ui(pan);
            }
        }

        result_channels.extend(mix_channels);
    }

    Ok(Json(iem_core::MixerState {
        member_id: member_id.clone(),
        channels: result_channels,
    }))
}

/// Poll current mixer state with meters (optimized for frequent calls)
pub async fn poll_mixer_state(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<PollResponse>, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member_id, &config.jwt_secret)?;

    // Use discovered members (REAPER source of truth) instead of config
    let discovered = state.discovered_members.read().await;
    let member_index = discovered
        .iter()
        .find(|m| m.id() == member_id)
        .map(|m| m.send_index)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;
    drop(discovered);
    let reaper_url = config.reaper_url.clone();

    let resolved = state.mixer_cache.read().await.input_track_indices.clone();
    let resolved_ref = if resolved.is_empty() {
        None
    } else {
        Some(&resolved)
    };
    let channels = build_channel_templates(&config.inputs, resolved_ref);
    drop(config);

    let mut result_channels = channels;
    let mut meters: HashMap<usize, [f32; 2]> = HashMap::new();
    let mut connected = false;

    // Query REAPER for all track states in a batch
    // First try to get all track info
    let tracks_url = reaper_api::query_tracks(&reaper_url);
    if let Ok(resp) = state.http_client.get(&tracks_url).send().await
        && let Ok(text) = resp.text().await
    {
        connected = true;
        // Parse track data for meters
        // REAPER TRACK format varies by VU availability:
        //   With VU (14 fields): TRACK idx name flags vol pan VU_L VU_R width panmode sendcnt recvcnt hwout color
        //   Without VU (12 fields): TRACK idx name flags vol pan width panmode sendcnt recvcnt hwout color
        // VU fields only present when track is record-armed (input monitoring)
        for line in text.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.first() == Some(&"TRACK")
                && parts.len() >= 14
                && let Ok(track_idx) = parts[1].parse::<usize>()
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
            // No VU (< 14 fields) → don't insert → frontend defaults to 0.0
        }

        // Try meter bridge EXTSTATE for true L/R per-channel peaks
        let extstate_url = reaper_api::get_extstate(&reaper_url, "REAPERIEM_METERS", "peaks");
        if let Ok(resp) = state.http_client.get(&extstate_url).send().await
            && let Ok(text) = resp.text().await
        {
            // REAPER EXTSTATE response: "EXTSTATE\tSECTION\tKEY\tvalue"
            if let Some(value) = text.split('\t').nth(3)
                && !value.is_empty()
            {
                let bridge_meters = crate::poller::parse_meter_bridge(value);
                if !bridge_meters.is_empty() {
                    meters = bridge_meters;
                }
            }
        }
    }

    // Query send states for all channels in parallel
    let send_futures: Vec<_> = result_channels
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
            connected = true;
        }
    }

    Ok(Json(PollResponse {
        member_id: member_id.clone(),
        channels: result_channels,
        meters,
        connected,
    }))
}

/// Batch control operations (Reset, MuteAll)
pub async fn batch_control(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<BatchControlRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member_id, &config.jwt_secret)?;

    // Use discovered members (REAPER source of truth) instead of config
    let discovered = state.discovered_members.read().await;
    let member_index = discovered
        .iter()
        .find(|m| m.id() == member_id)
        .map(|m| m.send_index)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;
    drop(discovered);

    let reaper_url = config.reaper_url.clone();
    let inputs = config.inputs.clone();

    // Use resolved track indices from poller (name-based lookup) instead of sequential i+1
    let resolved = state.mixer_cache.read().await.input_track_indices.clone();

    drop(config);

    match payload.operation {
        BatchOperation::Reset => {
            let vol = db_to_reaper_vol(0.0);

            // Reset all channels in parallel using resolved track indices
            let urls: Vec<String> = inputs
                .iter()
                .enumerate()
                .flat_map(|(i, input)| {
                    // Use resolved index from poller if available, else fall back to i+1
                    let t = resolved.get(&input.name).copied().unwrap_or(i + 1);
                    vec![
                        reaper_api::set_send_vol(&reaper_url, t, member_index, vol),
                        reaper_api::set_send_mute(&reaper_url, t, member_index, 0),
                        reaper_api::set_send_pan(&reaper_url, t, member_index, 0.0),
                    ]
                })
                .collect();

            let results =
                futures::future::join_all(urls.iter().map(|url| state.http_client.get(url).send()))
                    .await;
            let errors: Vec<_> = results.iter().filter(|r| r.is_err()).collect();
            if !errors.is_empty() {
                tracing::warn!(
                    error_count = errors.len(),
                    "Batch reset: some REAPER calls failed"
                );
                return Err((
                    StatusCode::BAD_GATEWAY,
                    Json(ApiError::new(
                        "REAPER_ERROR",
                        format!(
                            "{} of {} reset commands failed",
                            errors.len(),
                            results.len()
                        ),
                    )),
                ));
            }
        }
        BatchOperation::MuteAll => {
            // Mute all input channels for this member using resolved track indices
            let mut urls: Vec<String> = inputs
                .iter()
                .enumerate()
                .map(|(i, input)| {
                    let t = resolved.get(&input.name).copied().unwrap_or(i + 1);
                    reaper_api::set_send_mute(&reaper_url, t, member_index, 1)
                })
                .collect();

            // For engineer: also mute mix channels (member inear → ENGINEER sends)
            // CRITICAL: Use discovered mix_send_index, NOT hardcoded 0!
            // Send 0 on member inear tracks is the hardware output (Dante to speakers).
            // Muting Send 0 kills the member's audio entirely.
            if member_id == "engineer" {
                let discovered = state.discovered_members.read().await;
                let mix_urls: Vec<String> = discovered
                    .iter()
                    .filter(|m| m.id() != "engineer")
                    .filter_map(|m| {
                        m.mix_send_index
                            .map(|si| reaper_api::set_send_mute(&reaper_url, m.track_index, si, 1))
                    })
                    .collect();
                urls.extend(mix_urls);
            }

            let results =
                futures::future::join_all(urls.iter().map(|url| state.http_client.get(url).send()))
                    .await;
            let errors: Vec<_> = results.iter().filter(|r| r.is_err()).collect();
            if !errors.is_empty() {
                tracing::warn!(
                    error_count = errors.len(),
                    "Batch mute_all: some REAPER calls failed"
                );
                return Err((
                    StatusCode::BAD_GATEWAY,
                    Json(ApiError::new(
                        "REAPER_ERROR",
                        format!("{} of {} mute commands failed", errors.len(), results.len()),
                    )),
                ));
            }
        }
    }

    Ok(StatusCode::OK)
}

/// Query send state from REAPER (single HTTP call for all fields)
pub(crate) async fn query_send_state(
    client: &reqwest::Client,
    reaper_url: &str,
    track_index: usize,
    send_index: usize,
) -> Result<(f32, bool, f32), ()> {
    let url = reaper_api::get_send_state(reaper_url, track_index, send_index);
    let resp = client.get(&url).send().await.map_err(|_| ())?;
    let text = resp.text().await.map_err(|_| ())?;
    parse_send_state(&text).ok_or(())
}

/// Parse a REAPER SEND response for volume
/// Response format: SEND\ttrack\tsend\tflag\tVOLUME\tpan\tmode
#[cfg(test)]
fn parse_send_volume(text: &str) -> Option<f32> {
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() == Some(&"SEND")
            && parts.len() >= 5
            && let Ok(val) = parts[4].parse::<f32>()
        {
            return Some(val);
        }
    }
    None
}

/// Parse a REAPER SEND response for mute (flag at position 3)
/// Response format: SEND\ttrack\tsend\tMUTE\tvolume\tpan\tmode
/// Mute flag is a bitfield: bit 3 (value 8) = muted
#[cfg(test)]
fn parse_send_mute(text: &str) -> Option<bool> {
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() == Some(&"SEND")
            && parts.len() >= 4
            && let Ok(val) = parts[3].parse::<i32>()
        {
            return Some((val & 8) != 0);
        }
    }
    None
}

/// Parse a REAPER SEND response for pan
/// Response format: SEND\ttrack\tsend\tflag\tvolume\tPAN\tmode
#[cfg(test)]
fn parse_send_pan(text: &str) -> Option<f32> {
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() == Some(&"SEND")
            && parts.len() >= 6
            && let Ok(val) = parts[5].parse::<f32>()
        {
            return Some(val);
        }
    }
    None
}

/// Parse the destination track index from a REAPER SEND response.
/// Response format: SEND\ttrack\tsend\tflag\tvolume\tpan\tDESTINATION
/// Returns Some(destination) where negative values indicate hardware outputs (e.g. -1),
/// or None if no SEND line is found (meaning the send doesn't exist).
pub(crate) fn parse_send_destination(text: &str) -> Option<i32> {
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() == Some(&"SEND") && parts.len() >= 7 {
            return parts[6].parse().ok();
        }
    }
    None
}

/// Parse a full REAPER SEND response for vol, mute, and pan in one call
/// Response format: SEND\ttrack\tsend\tMUTE_FLAG\tVOLUME\tPAN\tmode
/// Mute flag is a bitfield: bit 3 (value 8) = muted
fn parse_send_state(text: &str) -> Option<(f32, bool, f32)> {
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() == Some(&"SEND") && parts.len() >= 6 {
            let vol = parts[4].parse::<f32>().ok()?;
            let mute_flag = parts[3].parse::<i32>().ok()?;
            let pan = parts[5].parse::<f32>().ok()?;
            return Some((vol, (mute_flag & 8) != 0, pan));
        }
    }
    None
}

/// Query the destination track of a specific send on a given track.
/// Returns Some(dest_track_index) or None if query fails.
pub(crate) async fn query_send_destination(
    client: &reqwest::Client,
    reaper_url: &str,
    track_index: usize,
    send_index: usize,
) -> Option<i32> {
    let url = reaper_api::get_send_state(reaper_url, track_index, send_index);
    let resp = client.get(&url).send().await.ok()?;
    let text = resp.text().await.ok()?;
    parse_send_destination(&text)
}

/// Parse a REAPER response value (format: "COMMAND\tVALUE")
/// Used for simple responses like NTRACK (only in tests now)
#[cfg(test)]
fn parse_reaper_value(text: &str) -> Option<f32> {
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2
            && let Ok(val) = parts[1].parse::<f32>()
        {
            return Some(val);
        }
    }
    None
}

/// Build channel templates from config inputs with category and stereo info.
/// If `resolved_indices` is provided, track indices are looked up by name
/// (handling REAPER track insertions/reordering). Falls back to sequential i+1.
pub(crate) fn build_channel_templates(
    inputs: &[iem_core::config::InputTrack],
    resolved_indices: Option<&HashMap<String, usize>>,
) -> Vec<iem_core::Channel> {
    inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let track_index = resolved_indices
                .and_then(|m| m.get(&input.name).copied())
                .unwrap_or(i + 1);
            let (category, stereo_pair, stereo_side) = categorize_track(&input.name);
            iem_core::Channel {
                track_index,
                name: input.name.clone(),
                level_db: 0.0,
                pan: 0.5,
                muted: false,
                category,
                stereo_pair,
                stereo_side,
            }
        })
        .collect()
}

/// Build channel templates for member mix monitoring (engineer only).
/// Each discovered member (except engineer) becomes a "mixes" channel
/// with track_index = member's inear track index.
pub(crate) fn build_mix_channel_templates(
    discovered: &[iem_core::DiscoveredMember],
    engineer_id: &str,
) -> Vec<iem_core::Channel> {
    discovered
        .iter()
        .filter(|m| m.id() != engineer_id)
        .map(|m| {
            let name = m
                .name
                .strip_suffix(" inear")
                .or_else(|| m.name.strip_suffix(" INEAR"))
                .unwrap_or(&m.name)
                .to_string();
            iem_core::Channel {
                track_index: m.track_index,
                name,
                level_db: 0.0,
                pan: 0.5,
                muted: false,
                category: "mixes".to_string(),
                stereo_pair: None,
                stereo_side: None,
            }
        })
        .collect()
}

/// Categorize a track by name
pub(crate) fn categorize_track(name: &str) -> (String, Option<String>, Option<String>) {
    let name_lower = name.to_lowercase();

    // Determine category — check hand/engineer BEFORE mic/gtr because
    // "HAND1 mic" contains both "hand" and "mic" but must be "tech"
    let category = if name_lower.contains("hand") || name_lower.contains("engineer") {
        "tech"
    } else if name_lower.contains("mic") || name_lower.contains("gtr") {
        "mics"
    } else {
        "stems"
    };

    // Check for stereo pair
    let (stereo_pair, stereo_side) = if name.ends_with(" L") {
        let pair_name = name.trim_end_matches(" L").to_string();
        (Some(pair_name.to_lowercase()), Some("L".to_string()))
    } else if name.ends_with(" R") {
        let pair_name = name.trim_end_matches(" R").to_string();
        (Some(pair_name.to_lowercase()), Some("R".to_string()))
    } else {
        (None, None)
    };

    (category.to_string(), stereo_pair, stereo_side)
}

/// Validate track_index is within the configured input count
fn validate_track_index(
    track_index: usize,
    input_count: usize,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    if track_index < 1 || track_index > input_count {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request(&format!(
                "track_index {} out of range 1..{}",
                track_index, input_count
            ))),
        ));
    }
    Ok(())
}

/// Validate level_db is a finite number
fn validate_level_db(level_db: f32) -> Result<(), (StatusCode, Json<ApiError>)> {
    if level_db.is_nan() || level_db.is_infinite() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request("level_db must be a finite number")),
        ));
    }
    Ok(())
}

/// Validate pan is between -1.0 and 1.0
fn validate_pan(pan: f32) -> Result<(), (StatusCode, Json<ApiError>)> {
    if !(-1.0..=1.0).contains(&pan) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::bad_request("pan must be between -1.0 and 1.0")),
        ));
    }
    Ok(())
}

/// Set send level for a member's mix
pub async fn set_send_level(
    State(state): State<AppState>,
    Path((member_id, track_index)): Path<(String, usize)>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<SetLevelRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member_id, &config.jwt_secret)?;

    // Use discovered members (REAPER source of truth)
    let discovered = state.discovered_members.read().await;
    let member_index = discovered
        .iter()
        .find(|m| m.id() == member_id)
        .map(|m| m.send_index)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;
    drop(discovered);

    // Validate inputs
    validate_track_index(track_index, config.inputs.len())?;
    validate_level_db(payload.level_db)?;

    let reaper_url = config.reaper_url.clone();

    // Get track name for debugging (owned String to avoid borrow issues)
    let track_name = config
        .inputs
        .get(track_index.saturating_sub(1))
        .map(|i| i.name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    drop(config);

    // Convert dB to REAPER volume
    // REAPER uses linear amplitude where 1.0 = 0 dB
    let vol = db_to_reaper_vol(payload.level_db);

    // Build REAPER API URL for setting send volume
    let url = reaper_api::set_send_vol(&reaper_url, track_index, member_index, vol);

    tracing::info!(
        member_id = %member_id,
        member_index = member_index,
        track_index = track_index,
        track_name = %track_name,
        level_db = payload.level_db,
        reaper_vol = vol,
        url = %url,
        "LEVEL REQUEST"
    );

    // Call REAPER
    state.http_client.get(&url).send().await.map_err(|e| {
        tracing::error!(error = %e, "REAPER error");
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError::new("REAPER_ERROR", "REAPER unavailable")),
        )
    })?;

    Ok(StatusCode::OK)
}

/// Set send pan for a member's mix
pub async fn set_send_pan(
    State(state): State<AppState>,
    Path((member_id, track_index)): Path<(String, usize)>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<SetPanRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member_id, &config.jwt_secret)?;

    // Use discovered members (REAPER source of truth)
    let discovered = state.discovered_members.read().await;
    let member_index = discovered
        .iter()
        .find(|m| m.id() == member_id)
        .map(|m| m.send_index)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;
    drop(discovered);

    // Validate inputs
    validate_track_index(track_index, config.inputs.len())?;
    validate_pan(payload.pan)?;

    let reaper_url = config.reaper_url.clone();
    drop(config);

    // Convert UI pan (0.0-1.0) to REAPER pan (-1.0 to 1.0)
    let reaper_pan = ui_pan_to_reaper(payload.pan);
    let url = reaper_api::set_send_pan(&reaper_url, track_index, member_index, reaper_pan);

    tracing::debug!(
        url = %url,
        ui_pan = payload.pan,
        reaper_pan = reaper_pan,
        "Setting send pan"
    );

    state.http_client.get(&url).send().await.map_err(|e| {
        tracing::error!(error = %e, "REAPER pan error");
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError::new("REAPER_ERROR", "REAPER unavailable")),
        )
    })?;

    Ok(StatusCode::OK)
}

/// Set send mute for a member's mix
pub async fn set_send_mute(
    State(state): State<AppState>,
    Path((member_id, track_index)): Path<(String, usize)>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<SetMuteRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member_id, &config.jwt_secret)?;

    // Use discovered members (REAPER source of truth)
    let discovered = state.discovered_members.read().await;
    let member_index = discovered
        .iter()
        .find(|m| m.id() == member_id)
        .map(|m| m.send_index)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;
    drop(discovered);

    // Validate track index
    validate_track_index(track_index, config.inputs.len())?;

    let reaper_url = config.reaper_url.clone();

    // Get track name for debugging (owned String to avoid borrow issues)
    let track_name = config
        .inputs
        .get(track_index.saturating_sub(1))
        .map(|i| i.name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    drop(config);

    let mute_val = if payload.muted { 1 } else { 0 };
    let url = reaper_api::set_send_mute(&reaper_url, track_index, member_index, mute_val);

    tracing::info!(
        member_id = %member_id,
        member_index = member_index,
        track_index = track_index,
        track_name = %track_name,
        muted = payload.muted,
        url = %url,
        "MUTE REQUEST"
    );

    state.http_client.get(&url).send().await.map_err(|e| {
        tracing::error!(error = %e, "REAPER mute error");
        (
            StatusCode::BAD_GATEWAY,
            Json(ApiError::new("REAPER_ERROR", "REAPER unavailable")),
        )
    })?;

    Ok(StatusCode::OK)
}

/// Request to set level
#[derive(Debug, serde::Deserialize)]
pub struct SetLevelRequest {
    pub level_db: f32,
}

/// Request to set pan
#[derive(Debug, serde::Deserialize)]
pub struct SetPanRequest {
    pub pan: f32,
}

/// Request to set mute
#[derive(Debug, serde::Deserialize)]
pub struct SetMuteRequest {
    pub muted: bool,
}

/// Convert dB to REAPER volume scale
///
/// REAPER uses standard linear amplitude:
/// - 0.0 = -inf dB (silence)
/// - 1.0 = 0 dB (unity)
/// - 2.0 ≈ +6 dB
pub(crate) fn db_to_reaper_vol(db: f32) -> f32 {
    if db <= -60.0 {
        0.0
    } else {
        10.0_f32.powf(db / 20.0).clamp(0.0, 4.0)
    }
}

/// Convert REAPER volume to dB
pub(crate) fn reaper_vol_to_db(vol: f32) -> f32 {
    if vol <= 0.0 {
        -60.0
    } else {
        20.0 * vol.log10()
    }
}

/// Quantize to 0.2 dB steps (matches UI quantization)
pub(crate) fn quantize_02(value: f32) -> f32 {
    (value * 5.0).round() / 5.0
}

/// Convert REAPER pan (-1.0 to 1.0) to UI pan (0.0 to 1.0)
///
/// REAPER uses: -1.0 = left, 0.0 = center, 1.0 = right
/// UI uses:     0.0 = left, 0.5 = center, 1.0 = right
pub(crate) fn reaper_pan_to_ui(reaper_pan: f32) -> f32 {
    ((reaper_pan + 1.0) / 2.0).clamp(0.0, 1.0)
}

/// Convert UI pan (0.0 to 1.0) to REAPER pan (-1.0 to 1.0)
pub(crate) fn ui_pan_to_reaper(ui_pan: f32) -> f32 {
    ((ui_pan * 2.0) - 1.0).clamp(-1.0, 1.0)
}

// =============================================================================
// WebSocket handler
// =============================================================================

/// WebSocket mixer endpoint - upgrades HTTP to WebSocket
/// Requires token query param for authentication: /ws/{member_id}?token=<JWT>
pub async fn ws_mixer(
    ws: axum::extract::ws::WebSocketUpgrade,
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    Query(query): Query<WsQuery>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    // Token is required for WebSocket connections
    let token = query.token.as_deref().ok_or_else(|| {
        tracing::warn!(member = %member_id, "WS connection without token");
        (StatusCode::UNAUTHORIZED, Json(ApiError::unauthorized()))
    })?;

    // Validate token and check expiration
    let claims = crate::auth::extract_claims(token, &config.jwt_secret).ok_or_else(|| {
        tracing::warn!(member = %member_id, "WS connection with invalid token");
        (StatusCode::UNAUTHORIZED, Json(ApiError::unauthorized()))
    })?;

    // Check token expiration
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    if claims.exp < now {
        tracing::warn!(member = %member_id, "WS connection with expired token");
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new("TOKEN_EXPIRED", "Token has expired")),
        ));
    }

    // Verify member access: member can only connect to own mixer, engineer can access any
    if !claims.engineer && claims.sub != member_id {
        tracing::warn!(
            member = %member_id,
            token_sub = %claims.sub,
            "WS connection denied: cross-member access"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiError::new(
                "FORBIDDEN",
                "Access denied to this member's mixer",
            )),
        ));
    }

    // Detect network mode before dropping config (needs local_public_ip)
    let network_mode = crate::routes::detect_network_mode(&headers, &config.local_public_ip);

    drop(config);

    // Validate member exists (from REAPER discovered members)
    let discovered = state.discovered_members.read().await;
    let member_exists = discovered.iter().any(|m| m.id() == member_id);
    drop(discovered);
    if !member_exists {
        return Err((StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))));
    }

    Ok(ws.on_upgrade(move |socket| handle_ws(socket, state, member_id, network_mode)))
}

/// Handle a WebSocket connection for a member
async fn handle_ws(
    mut socket: axum::extract::ws::WebSocket,
    state: AppState,
    member_id: String,
    network_mode: String,
) {
    use axum::extract::ws::Message;
    use iem_core::{ClientMsg, ServerMsg};

    tracing::info!(member_id = %member_id, network_mode = %network_mode, "WebSocket connected");

    // Register this member as active (ref-counted for multi-tab support)
    {
        let mut cache = state.mixer_cache.write().await;
        *cache.active_members.entry(member_id.clone()).or_insert(0) += 1;
    }

    // Subscribe to broadcast channel
    let mut rx = state.event_tx.subscribe();

    // Send initial full state
    if let Ok(initial_state) = build_full_state(&state, &member_id).await {
        let json = serde_json::to_string(&initial_state).unwrap_or_default();
        let _ = socket.send(Message::Text(json.into())).await;
    }

    // Send initial customization state
    {
        let cust = state.customization_store.load(&member_id);
        let msg = ServerMsg::CustomizationUpdate {
            pinned: cust.pinned,
            hidden: cust.hidden,
        };
        let json = serde_json::to_string(&msg).unwrap_or_default();
        let _ = socket.send(Message::Text(json.into())).await;
    }

    // Send initial solo state (if any active solos for this member)
    {
        let cache = state.mixer_cache.read().await;
        if let Some(soloed) = cache.solo_states.get(&member_id) {
            let msg = ServerMsg::SoloUpdate {
                soloed: soloed.clone(),
            };
            let json = serde_json::to_string(&msg).unwrap_or_default();
            let _ = socket.send(Message::Text(json.into())).await;
        }
    }

    // Send network mode (local/remote) — updates on every WS reconnect,
    // so switching between WiFi and mobile data triggers a fresh detection
    {
        let msg = ServerMsg::NetworkMode { mode: network_mode };
        let json = serde_json::to_string(&msg).unwrap_or_default();
        let _ = socket.send(Message::Text(json.into())).await;
    }

    loop {
        tokio::select! {
            // Client → Server: process commands
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<ClientMsg>(&text) {
                            // Handle customization updates locally (no REAPER command)
                            if let ClientMsg::UpdateCustomization { ref pinned, ref hidden } = cmd {
                                let cust = iem_core::Customization {
                                    pinned: pinned.clone(),
                                    hidden: hidden.clone(),
                                };
                                if let Err(e) = state.customization_store.save(&member_id, &cust) {
                                    tracing::error!(member_id = %member_id, "Failed to save customization: {}", e);
                                }
                                // Broadcast to other tabs of same member
                                let _ = state.event_tx.send((
                                    member_id.clone(),
                                    ServerMsg::CustomizationUpdate {
                                        pinned: pinned.clone(),
                                        hidden: hidden.clone(),
                                    },
                                ));
                                continue;
                            }

                            // Handle EQ commands (async EXTSTATE + ReaScript flow)
                            if let ClientMsg::GetEqParams { track_index } = cmd {
                                let state_clone = state.clone();
                                let member_clone = member_id.clone();
                                tokio::spawn(async move {
                                    if let Some(eq_msg) = handle_get_eq_params(&state_clone, track_index).await {
                                        let _ = state_clone.event_tx.send((member_clone, eq_msg));
                                    }
                                });
                                continue;
                            }
                            if let ClientMsg::GetEqParamsMulti { ref track_indices } = cmd {
                                let state_clone = state.clone();
                                let member_clone = member_id.clone();
                                let indices = track_indices.clone();
                                tokio::spawn(async move {
                                    let mut bands_map = std::collections::HashMap::new();
                                    for ti in indices {
                                        if let Some(iem_core::ServerMsg::EqParams { track_index, bands, .. }) =
                                            handle_get_eq_params(&state_clone, ti).await
                                        {
                                            bands_map.insert(track_index, bands);
                                        }
                                    }
                                    let msg = iem_core::ServerMsg::EqParamsMulti { bands: bands_map };
                                    let _ = state_clone.event_tx.send((member_clone, msg));
                                });
                                continue;
                            }
                            if let ClientMsg::SetEqBand { track_index, band, ref param, value } = cmd {
                                let state_clone = state.clone();
                                let param_clone = param.clone();
                                tokio::spawn(async move {
                                    handle_set_eq_band(&state_clone, track_index, band, &param_clone, value).await;
                                    // Don't broadcast EqParams after SetEqBand — the client
                                    // already has optimistic local state from the drag.
                                    // Broadcasting causes server echo → reactive_graph recursive
                                    // closure panic in the frontend's sync Effect.
                                });
                                continue;
                            }

                            // Handle solo state updates (no REAPER command — solo is UI-only sync)
                            if let ClientMsg::SetSolo { ref soloed } = cmd {
                                {
                                    let mut cache = state.mixer_cache.write().await;
                                    if soloed.is_empty() {
                                        cache.solo_states.remove(&member_id);
                                    } else {
                                        cache.solo_states
                                            .insert(member_id.clone(), soloed.clone());
                                    }
                                }
                                let _ = state.event_tx.send((
                                    member_id.clone(),
                                    ServerMsg::SoloUpdate {
                                        soloed: soloed.clone(),
                                    },
                                ));
                                continue;
                            }

                            match apply_command_to_cache(&state, &member_id, &cmd).await {
                                Ok((url, broadcast)) => {
                                    // Broadcast to other clients of same member for cross-device sync
                                    if let Some(event) = broadcast {
                                        let _ = state.event_tx.send((member_id.clone(), event));
                                    }
                                    send_to_reaper(
                                        state.http_client.clone(),
                                        url,
                                        member_id.clone(),
                                        cmd,
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        member_id = %member_id,
                                        error = %e,
                                        "WS command failed"
                                    );
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {} // Ignore ping/pong/binary
                }
            }
            // Server → Client: forward relevant broadcasts
            event = rx.recv() => {
                match event {
                    Ok((mid, server_msg)) => {
                        // Send meters and connection changes to all;
                        // send state/channel/global updates only to the relevant member
                        let should_send = match &server_msg {
                            ServerMsg::Meters { .. } => true,
                            ServerMsg::ConnectionChanged { .. } => true,
                            _ => mid == member_id || mid.is_empty(),
                        };
                        if should_send {
                            let json = serde_json::to_string(&server_msg).unwrap_or_default();
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break; // Client disconnected
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(member_id = %member_id, skipped = n, "WS broadcast lagged");
                        // Continue - we'll get the next update
                    }
                    Err(_) => break, // Channel closed
                }
            }
        }
    }

    tracing::info!(member_id = %member_id, "WebSocket disconnected");

    // Cleanup: decrement ref count, remove only when no tabs remain
    let mut cache = state.mixer_cache.write().await;
    if let Some(count) = cache.active_members.get_mut(&member_id) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            cache.active_members.remove(&member_id);
            cache.member_states.remove(&member_id);
            cache.solo_states.remove(&member_id);
        }
    }
}

/// Build full state message for initial WebSocket connection
async fn build_full_state(state: &AppState, member_id: &str) -> Result<iem_core::ServerMsg, ()> {
    // Get member send index from discovered members (REAPER source of truth)
    let discovered = state.discovered_members.read().await;
    let member_index = discovered
        .iter()
        .find(|m| m.id() == member_id)
        .map(|m| m.send_index)
        .ok_or(())?;
    drop(discovered);

    let config = state.config.read().await;
    let reaper_url = config.reaper_url.clone();
    let resolved = state.mixer_cache.read().await.input_track_indices.clone();
    let resolved_ref = if resolved.is_empty() {
        None
    } else {
        Some(&resolved)
    };
    let channels = build_channel_templates(&config.inputs, resolved_ref);
    drop(config);

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
    let mut connected = false;

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
            connected = true;
        }
    }

    // For engineer: append mix channels (member inear → ENGINEER sends)
    if member_id == "engineer" {
        let discovered = state.discovered_members.read().await;
        let mut mix_channels = build_mix_channel_templates(&discovered, "engineer");

        // Use discovered mix_send_index (NOT hardcoded 0!)
        let mix_futures: Vec<_> = mix_channels
            .iter()
            .filter_map(|ch| {
                let mix_si = discovered
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
        drop(discovered);

        let mix_results = futures::future::join_all(mix_futures).await;
        for (track_index, result) in mix_results {
            if let Ok((level, mute, pan)) = result {
                if let Some(ch) = mix_channels
                    .iter_mut()
                    .find(|c| c.track_index == track_index)
                {
                    ch.level_db = reaper_vol_to_db(level);
                    ch.muted = mute;
                    ch.pan = reaper_pan_to_ui(pan);
                }
                connected = true;
            }
        }

        result_channels.extend(mix_channels);
    }

    // Read cached global volume and output track index for this member
    let cache = state.mixer_cache.read().await;
    let (global_level_db, global_muted) = match cache.global_volumes.get(member_id) {
        Some(gv) => (Some(gv.level_db), Some(gv.muted)),
        None => (None, None),
    };
    let output_track_index = cache.output_track_indices.get(member_id).copied();
    let (stems_level_db, stems_muted) = match cache.stems_volumes.get(member_id) {
        Some(sv) => (Some(sv.level_db), Some(sv.muted)),
        None => (None, None),
    };
    let stems_bus_index = cache.stems_bus_indices.get(member_id).copied();
    drop(cache);

    Ok(iem_core::ServerMsg::State {
        channels: result_channels,
        connected,
        global_level_db,
        global_muted,
        output_track_index,
        stems_level_db,
        stems_muted,
        stems_bus_index,
    })
}

/// Apply a client command to the local cache and record a command timestamp.
/// This prevents the poller from broadcasting echo updates for recently-commanded channels.
/// Returns the REAPER HTTP URL to send, or an error.
async fn apply_command_to_cache(
    state: &AppState,
    member_id: &str,
    cmd: &iem_core::ClientMsg,
) -> Result<(String, Option<iem_core::ServerMsg>), String> {
    // Get member send index from discovered members (REAPER source of truth)
    let discovered = state.discovered_members.read().await;
    let member_index = discovered
        .iter()
        .find(|m| m.id() == member_id)
        .map(|m| m.send_index)
        .ok_or_else(|| "Unknown member".to_string())?;
    // Collect mix channel track indices and their mix_send_index for engineer validation
    let mix_members: Vec<(usize, Option<usize>)> = if member_id == "engineer" {
        discovered
            .iter()
            .filter(|m| m.id() != "engineer")
            .map(|m| (m.track_index, m.mix_send_index))
            .collect()
    } else {
        Vec::new()
    };
    let mix_track_indices: Vec<usize> = mix_members.iter().map(|(ti, _)| *ti).collect();
    drop(discovered);

    let config = state.config.read().await;
    let reaper_url = config.reaper_url.clone();
    let input_count = config.inputs.len();
    drop(config);

    // Helper: check if a track_index is valid (input range OR engineer mix channel)
    let is_valid_track =
        |ti: usize| -> bool { (ti >= 1 && ti <= input_count) || mix_track_indices.contains(&ti) };

    // Helper: determine REAPER send_index for a given track_index.
    // Mix channels use their discovered mix_send_index (NOT hardcoded 0!).
    // Regular input channels use the member's send_index.
    let send_index_for = |ti: usize| -> Result<usize, String> {
        if let Some((_, mix_si)) = mix_members.iter().find(|(track, _)| *track == ti) {
            mix_si.ok_or_else(|| {
                format!(
                    "SAFETY: No mix_send_index for track {} — cannot route to engineer",
                    ti
                )
            })
        } else {
            Ok(member_index)
        }
    };

    // Validate incoming WS command values
    match cmd {
        iem_core::ClientMsg::SetLevel {
            track_index,
            level_db,
        } => {
            if !is_valid_track(*track_index) {
                return Err(format!("track_index {} out of range", track_index));
            }
            if level_db.is_nan() || level_db.is_infinite() {
                return Err("level_db must be finite".to_string());
            }
        }
        iem_core::ClientMsg::SetPan { track_index, pan } => {
            if !is_valid_track(*track_index) {
                return Err(format!("track_index {} out of range", track_index));
            }
            if pan.is_nan() || pan.is_infinite() || *pan < -1.0 || *pan > 1.0 {
                return Err("pan must be between -1.0 and 1.0".to_string());
            }
        }
        iem_core::ClientMsg::SetMute { track_index, .. } => {
            if !is_valid_track(*track_index) {
                return Err(format!("track_index {} out of range", track_index));
            }
        }
        iem_core::ClientMsg::SetGlobalLevel { level_db } => {
            if level_db.is_nan() || level_db.is_infinite() {
                return Err("level_db must be finite".to_string());
            }
        }
        iem_core::ClientMsg::SetGlobalMute { .. } => {}
        iem_core::ClientMsg::SetStemsLevel { level_db } => {
            if level_db.is_nan() || level_db.is_infinite() {
                return Err("level_db must be finite".to_string());
            }
        }
        iem_core::ClientMsg::SetStemsMute { .. } => {}
        iem_core::ClientMsg::UpdateCustomization { .. } => {
            // Handled in WS handler before apply_command_to_cache is called
            return Err("UpdateCustomization should not reach apply_command_to_cache".to_string());
        }
        iem_core::ClientMsg::SetSolo { .. } => {
            // Handled in WS handler before apply_command_to_cache is called
            return Err("SetSolo should not reach apply_command_to_cache".to_string());
        }
        iem_core::ClientMsg::ListenStart { .. } | iem_core::ClientMsg::ListenStop => {
            // Audio commands are handled by ws_audio, not the mixer WS
            return Err("Audio commands should use /ws/audio endpoint".to_string());
        }
        iem_core::ClientMsg::GetEqParams { .. }
        | iem_core::ClientMsg::SetEqBand { .. }
        | iem_core::ClientMsg::GetEqParamsMulti { .. } => {
            // EQ commands are handled in WS handler before apply_command_to_cache is called
            return Err("EQ commands should not reach apply_command_to_cache".to_string());
        }
    }

    let (url, track_index, broadcast) = match cmd {
        iem_core::ClientMsg::SetLevel {
            track_index,
            level_db,
        } => {
            let vol = db_to_reaper_vol(*level_db);
            // Pre-write cache with quantized roundtrip value (eliminates f32 artifacts)
            let cached_db = quantize_02(reaper_vol_to_db(vol));
            let mut cache = state.mixer_cache.write().await;
            let mut event = None;
            if let Some(channels) = cache.member_states.get_mut(member_id)
                && let Some(ch) = channels.iter_mut().find(|c| c.track_index == *track_index)
            {
                ch.level_db = cached_db;
                event = Some(iem_core::ServerMsg::ChannelUpdate {
                    track_index: *track_index,
                    level_db: cached_db,
                    muted: ch.muted,
                    pan: ch.pan,
                });
            }
            cache.command_timestamps.insert(
                (member_id.to_string(), *track_index),
                std::time::Instant::now(),
            );
            drop(cache);
            let si = send_index_for(*track_index)?;
            (
                reaper_api::set_send_vol(&reaper_url, *track_index, si, vol),
                *track_index,
                event,
            )
        }
        iem_core::ClientMsg::SetMute { track_index, muted } => {
            let mute_val: u8 = if *muted { 1 } else { 0 };
            let mut cache = state.mixer_cache.write().await;
            let mut event = None;
            if let Some(channels) = cache.member_states.get_mut(member_id)
                && let Some(ch) = channels.iter_mut().find(|c| c.track_index == *track_index)
            {
                ch.muted = *muted;
                event = Some(iem_core::ServerMsg::ChannelUpdate {
                    track_index: *track_index,
                    level_db: ch.level_db,
                    muted: *muted,
                    pan: ch.pan,
                });
            }
            cache.command_timestamps.insert(
                (member_id.to_string(), *track_index),
                std::time::Instant::now(),
            );
            drop(cache);
            let si = send_index_for(*track_index)?;
            (
                reaper_api::set_send_mute(&reaper_url, *track_index, si, mute_val),
                *track_index,
                event,
            )
        }
        iem_core::ClientMsg::SetPan { track_index, pan } => {
            let reaper_pan = ui_pan_to_reaper(*pan);
            let cached_pan = reaper_pan_to_ui(reaper_pan);
            let mut cache = state.mixer_cache.write().await;
            let mut event = None;
            if let Some(channels) = cache.member_states.get_mut(member_id)
                && let Some(ch) = channels.iter_mut().find(|c| c.track_index == *track_index)
            {
                ch.pan = cached_pan;
                event = Some(iem_core::ServerMsg::ChannelUpdate {
                    track_index: *track_index,
                    level_db: ch.level_db,
                    muted: ch.muted,
                    pan: cached_pan,
                });
            }
            cache.command_timestamps.insert(
                (member_id.to_string(), *track_index),
                std::time::Instant::now(),
            );
            drop(cache);
            let si = send_index_for(*track_index)?;
            (
                reaper_api::set_send_pan(&reaper_url, *track_index, si, reaper_pan),
                *track_index,
                event,
            )
        }
        iem_core::ClientMsg::SetGlobalLevel { level_db } => {
            let vol = db_to_reaper_vol(*level_db);
            let cached_db = reaper_vol_to_db(vol);
            let mut cache = state.mixer_cache.write().await;
            let output_track = match cache.output_track_indices.get(member_id) {
                Some(&idx) => idx,
                None => {
                    drop(cache);
                    return Err("Output track not yet discovered".to_string());
                }
            };
            let current_muted = cache
                .global_volumes
                .get(member_id)
                .map(|gv| gv.muted)
                .unwrap_or(false);
            if let Some(gv) = cache.global_volumes.get_mut(member_id) {
                gv.level_db = cached_db;
            } else {
                cache.global_volumes.insert(
                    member_id.to_string(),
                    crate::GlobalVolState {
                        level_db: cached_db,
                        muted: false,
                    },
                );
            }
            // Use output track index as key for echo suppression (offset to avoid collision with send tracks)
            cache.command_timestamps.insert(
                (member_id.to_string(), output_track + 100000),
                std::time::Instant::now(),
            );
            drop(cache);
            (
                reaper_api::set_track_vol(&reaper_url, output_track, vol),
                output_track,
                Some(iem_core::ServerMsg::GlobalVolumeUpdate {
                    level_db: cached_db,
                    muted: current_muted,
                }),
            )
        }
        iem_core::ClientMsg::UpdateCustomization { .. } => {
            unreachable!("UpdateCustomization handled before apply_command_to_cache")
        }
        iem_core::ClientMsg::SetSolo { .. } => {
            unreachable!("SetSolo handled before apply_command_to_cache")
        }
        iem_core::ClientMsg::ListenStart { .. } | iem_core::ClientMsg::ListenStop => {
            unreachable!("Audio commands handled by ws_audio, not mixer WS")
        }
        iem_core::ClientMsg::GetEqParams { .. }
        | iem_core::ClientMsg::SetEqBand { .. }
        | iem_core::ClientMsg::GetEqParamsMulti { .. } => {
            unreachable!("EQ commands handled before apply_command_to_cache")
        }
        iem_core::ClientMsg::SetGlobalMute { muted } => {
            let mute_val: u8 = if *muted { 1 } else { 0 };
            let mut cache = state.mixer_cache.write().await;
            let output_track = match cache.output_track_indices.get(member_id) {
                Some(&idx) => idx,
                None => {
                    drop(cache);
                    return Err("Output track not yet discovered".to_string());
                }
            };
            let current_db = cache
                .global_volumes
                .get(member_id)
                .map(|gv| gv.level_db)
                .unwrap_or(0.0);
            if let Some(gv) = cache.global_volumes.get_mut(member_id) {
                gv.muted = *muted;
            } else {
                cache.global_volumes.insert(
                    member_id.to_string(),
                    crate::GlobalVolState {
                        level_db: 0.0,
                        muted: *muted,
                    },
                );
            }
            cache.command_timestamps.insert(
                (member_id.to_string(), output_track + 100000),
                std::time::Instant::now(),
            );
            drop(cache);
            (
                reaper_api::set_track_mute(&reaper_url, output_track, mute_val),
                output_track,
                Some(iem_core::ServerMsg::GlobalVolumeUpdate {
                    level_db: current_db,
                    muted: *muted,
                }),
            )
        }
        iem_core::ClientMsg::SetStemsLevel { level_db } => {
            let vol = db_to_reaper_vol(*level_db);
            let cached_db = reaper_vol_to_db(vol);
            let mut cache = state.mixer_cache.write().await;
            let stems_track = match cache.stems_bus_indices.get(member_id) {
                Some(&idx) => idx,
                None => {
                    drop(cache);
                    return Err("Stems bus track not yet discovered".to_string());
                }
            };
            let current_muted = cache
                .stems_volumes
                .get(member_id)
                .map(|sv| sv.muted)
                .unwrap_or(false);
            if let Some(sv) = cache.stems_volumes.get_mut(member_id) {
                sv.level_db = cached_db;
            } else {
                cache.stems_volumes.insert(
                    member_id.to_string(),
                    crate::GlobalVolState {
                        level_db: cached_db,
                        muted: false,
                    },
                );
            }
            cache.command_timestamps.insert(
                (member_id.to_string(), stems_track + 200000),
                std::time::Instant::now(),
            );
            drop(cache);
            (
                reaper_api::set_track_vol(&reaper_url, stems_track, vol),
                stems_track,
                Some(iem_core::ServerMsg::StemsVolumeUpdate {
                    level_db: cached_db,
                    muted: current_muted,
                }),
            )
        }
        iem_core::ClientMsg::SetStemsMute { muted } => {
            let mute_val: u8 = if *muted { 1 } else { 0 };
            let mut cache = state.mixer_cache.write().await;
            let stems_track = match cache.stems_bus_indices.get(member_id) {
                Some(&idx) => idx,
                None => {
                    drop(cache);
                    return Err("Stems bus track not yet discovered".to_string());
                }
            };
            let current_db = cache
                .stems_volumes
                .get(member_id)
                .map(|sv| sv.level_db)
                .unwrap_or(0.0);
            if let Some(sv) = cache.stems_volumes.get_mut(member_id) {
                sv.muted = *muted;
            } else {
                cache.stems_volumes.insert(
                    member_id.to_string(),
                    crate::GlobalVolState {
                        level_db: 0.0,
                        muted: *muted,
                    },
                );
            }
            cache.command_timestamps.insert(
                (member_id.to_string(), stems_track + 200000),
                std::time::Instant::now(),
            );
            drop(cache);
            (
                reaper_api::set_track_mute(&reaper_url, stems_track, mute_val),
                stems_track,
                Some(iem_core::ServerMsg::StemsVolumeUpdate {
                    level_db: current_db,
                    muted: *muted,
                }),
            )
        }
    };

    tracing::debug!(
        member_id = %member_id,
        track_index = track_index,
        cmd = ?cmd,
        "Cache pre-write applied"
    );

    Ok((url, broadcast))
}

/// Send a REAPER HTTP command (fire-and-forget, spawned as background task)
fn send_to_reaper(
    client: reqwest::Client,
    url: String,
    member_id: String,
    cmd: iem_core::ClientMsg,
) {
    tokio::spawn(async move {
        if let Err(e) = client.get(&url).send().await {
            tracing::error!(error = %e, member_id = %member_id, cmd = ?cmd, "REAPER HTTP failed");
        }
    });
}

// =============================================================================
// EQ handlers (EXTSTATE + ReaScript async flow)
// =============================================================================

/// Handle GetEqParams: read EQ state from REAPER via EXTSTATE + ReaScript
pub async fn handle_get_eq_params(
    state: &AppState,
    track_index: usize,
) -> Option<iem_core::ServerMsg> {
    let config = state.config.read().await;
    let reaper_url = config.reaper_url.clone();
    drop(config);

    // 1. Set EXTSTATE with track index
    let set_url = reaper_api::set_extstate(
        &reaper_url,
        "reaperiem",
        "eq_read_track",
        &track_index.to_string(),
    );
    if state.http_client.get(&set_url).send().await.is_err() {
        tracing::error!(track_index, "EQ: failed to set eq_read_track EXTSTATE");
        return None;
    }

    // 2. Trigger read_eq_params.lua action
    let action_url = reaper_api::trigger_action(&reaper_url, "_RS_REAPERIEM_READ_EQ");
    if state.http_client.get(&action_url).send().await.is_err() {
        tracing::error!(track_index, "EQ: failed to trigger READ_EQ action");
        return None;
    }

    // 3. Wait for script execution
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // 4. Read result from EXTSTATE
    let get_url = reaper_api::get_extstate(&reaper_url, "reaperiem", "eq_params");
    let resp = state.http_client.get(&get_url).send().await.ok()?;
    let text = resp.text().await.ok()?;

    // Parse EXTSTATE response: "EXTSTATE\tsection\tkey\tvalue"
    let value = text.split('\t').nth(3)?;
    if value.is_empty() || value.starts_with("ERROR") {
        tracing::warn!(track_index, value, "EQ: read failed");
        return None;
    }

    // Parse the result
    parse_eq_params_response(track_index, value)
}

/// Parse the eq_params EXTSTATE response into a ServerMsg::EqParams
fn parse_eq_params_response(track_index: usize, value: &str) -> Option<iem_core::ServerMsg> {
    // Format: "OK:track=N,name=NAME,fx=N,bands=N,gg=X,bypass=X|b0:type,fn=F,gn=G,bn=B,fh=Hz,gd=dB,bo=oct|..."
    // Or: "NO_EQ:TRACKNAME"
    if value.starts_with("NO_EQ:") {
        let track_name = value.strip_prefix("NO_EQ:").unwrap_or("").to_string();
        return Some(iem_core::ServerMsg::EqParams {
            track_index,
            track_name,
            bands: vec![],
        });
    }

    if !value.starts_with("OK:") {
        return None;
    }

    let parts: Vec<&str> = value.splitn(2, '|').collect();
    let header = parts[0]; // "OK:track=N,name=NAME,fx=N,bands=N,gg=X,bypass=X"
    let band_data = if parts.len() > 1 { parts[1] } else { "" };

    // Parse track name from header
    let track_name = header
        .split(',')
        .find(|s| s.starts_with("name="))
        .and_then(|s| s.strip_prefix("name="))
        .unwrap_or("")
        .to_string();

    // Parse bands
    let mut bands = Vec::new();
    if !band_data.is_empty() {
        for band_str in band_data.split('|') {
            if let Some(band) = parse_eq_band(band_str) {
                bands.push(band);
            }
        }
    }

    Some(iem_core::ServerMsg::EqParams {
        track_index,
        track_name,
        bands,
    })
}

/// Parse a single band string like "b0:lowshelf,fn=0.283000,gn=0.184000,bn=0.295000,fh=250,gd=-2.7,bo=1.18"
/// Uses REAPER-formatted display values (fh, gd, bo) when available for accurate display.
/// Falls back to approximations from normalized values (fn, gn, bn) for backward compatibility.
fn parse_eq_band(s: &str) -> Option<iem_core::EqBand> {
    let colon_pos = s.find(':')?;
    let after_colon = &s[colon_pos + 1..];
    let fields: Vec<&str> = after_colon.split(',').collect();

    if fields.is_empty() {
        return None;
    }

    let band_type = fields[0].to_string();

    let get_field = |prefix: &str| -> Option<f32> {
        fields
            .iter()
            .find(|f| f.starts_with(prefix))
            .and_then(|f| f.strip_prefix(prefix))
            .and_then(|v| v.parse::<f32>().ok())
    };

    let freq_norm = get_field("fn=")?;
    let gain_norm = get_field("gn=")?;
    let bw_norm = get_field("bn=")?;

    // Use REAPER-formatted values if available, otherwise approximate
    let freq_hz = get_field("fh=").unwrap_or(20.0 * 1000.0_f32.powf(freq_norm));
    let gain_db = get_field("gd=").unwrap_or((gain_norm - 0.25) * 96.0);
    let bw = get_field("bo=").unwrap_or(0.01 + bw_norm * 3.99);

    Some(iem_core::EqBand {
        band_type,
        freq_hz,
        gain_db,
        bw,
        freq_norm,
        gain_norm,
        bw_norm,
    })
}

/// Handle SetEqBand: set a single EQ parameter on a track via EXTSTATE + ReaScript
async fn handle_set_eq_band(
    state: &AppState,
    track_index: usize,
    band: u8,
    param: &str,
    value: f32,
) {
    let config = state.config.read().await;
    let reaper_url = config.reaper_url.clone();
    drop(config);

    // 1. Set EXTSTATE with parameters: "track=N|band=B|param=P|value=V"
    let eq_set_value = format!(
        "track={}|band={}|param={}|value={:.6}",
        track_index, band, param, value
    );
    let set_url = reaper_api::set_extstate(&reaper_url, "reaperiem", "eq_set", &eq_set_value);
    if state.http_client.get(&set_url).send().await.is_err() {
        tracing::error!(
            track_index,
            band,
            param,
            "EQ: failed to set eq_set EXTSTATE"
        );
        return;
    }

    // 2. Trigger set_eq_param.lua action
    let action_url = reaper_api::trigger_action(&reaper_url, "_RS_REAPERIEM_SET_EQ");
    if state.http_client.get(&action_url).send().await.is_err() {
        tracing::error!(
            track_index,
            band,
            param,
            "EQ: failed to trigger SET_EQ action"
        );
        return;
    }

    // 3. Wait for script execution
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // 4. Read result (for logging only)
    let get_url = reaper_api::get_extstate(&reaper_url, "reaperiem", "eq_set_result");
    if let Ok(resp) = state.http_client.get(&get_url).send().await
        && let Ok(text) = resp.text().await
        && let Some(result) = text.split('\t').nth(3)
    {
        if result.starts_with("ERROR") {
            tracing::error!(track_index, band, param, result, "EQ: set_eq_param failed");
        } else {
            tracing::debug!(track_index, band, param, result, "EQ: param set OK");
        }
    }
}

/// REAPER HTTP API URL builder
/// CRITICAL: All REAPER API commands MUST use the `/_/` prefix!
/// Without this prefix, REAPER returns empty responses.
pub(crate) mod reaper_api {
    /// Build URL for setting send volume
    pub fn set_send_vol(base_url: &str, track: usize, send: usize, vol: f32) -> String {
        format!(
            "{}/_/SET/TRACK/{}/SEND/{}/VOL/{}",
            base_url, track, send, vol
        )
    }

    /// Build URL for setting send mute
    pub fn set_send_mute(base_url: &str, track: usize, send: usize, mute: u8) -> String {
        format!(
            "{}/_/SET/TRACK/{}/SEND/{}/MUTE/{}",
            base_url, track, send, mute
        )
    }

    /// Build URL for setting send pan
    pub fn set_send_pan(base_url: &str, track: usize, send: usize, pan: f32) -> String {
        format!(
            "{}/_/SET/TRACK/{}/SEND/{}/PAN/{}",
            base_url, track, send, pan
        )
    }

    /// Build URL for getting send volume (retained for test coverage of /_/ prefix)
    #[cfg(test)]
    pub fn get_send_vol(base_url: &str, track: usize, send: usize) -> String {
        format!("{}/_/GET/TRACK/{}/SEND/{}/VOL", base_url, track, send)
    }

    /// Build URL for getting send mute (retained for test coverage of /_/ prefix)
    #[cfg(test)]
    pub fn get_send_mute(base_url: &str, track: usize, send: usize) -> String {
        format!("{}/_/GET/TRACK/{}/SEND/{}/MUTE", base_url, track, send)
    }

    /// Build URL for getting send pan (retained for test coverage of /_/ prefix)
    #[cfg(test)]
    pub fn get_send_pan(base_url: &str, track: usize, send: usize) -> String {
        format!("{}/_/GET/TRACK/{}/SEND/{}/PAN", base_url, track, send)
    }

    /// Build URL for setting track volume (output bus volume)
    pub fn set_track_vol(base_url: &str, track: usize, vol: f32) -> String {
        format!("{}/_/SET/TRACK/{}/VOL/{}", base_url, track, vol)
    }

    /// Build URL for setting track mute (output bus mute)
    pub fn set_track_mute(base_url: &str, track: usize, mute: u8) -> String {
        format!("{}/_/SET/TRACK/{}/MUTE/{}", base_url, track, mute)
    }

    /// Build URL for querying tracks
    pub fn query_tracks(base_url: &str) -> String {
        format!("{}/_/NTRACK;TRACK", base_url)
    }

    /// Build URL for reading EXTSTATE (key-value store in REAPER)
    pub fn get_extstate(base_url: &str, section: &str, key: &str) -> String {
        format!("{}/_/GET/EXTSTATE/{}/{}", base_url, section, key)
    }

    /// Build URL for setting EXTSTATE (key-value store in REAPER)
    pub fn set_extstate(base_url: &str, section: &str, key: &str, value: &str) -> String {
        format!("{}/_/SET/EXTSTATE/{}/{}/{}", base_url, section, key, value)
    }

    /// Build URL for triggering a REAPER action by ID
    pub fn trigger_action(base_url: &str, action_id: &str) -> String {
        format!("{}/_/{}", base_url, action_id)
    }

    /// Build URL for getting full send state (returns vol, mute, pan in one call)
    pub fn get_send_state(base_url: &str, track: usize, send: usize) -> String {
        format!("{}/_/GET/TRACK/{}/SEND/{}", base_url, track, send)
    }
}

// =============================================================================
// Audio WebSocket handler (engineer-only audio streaming)
// =============================================================================

/// WebSocket audio endpoint - streams Opus frames to the engineer's browser
/// Requires token query param for authentication: /ws/audio?token=<JWT>
/// Engineer-only: non-engineer tokens are rejected with 403
#[cfg(feature = "audio")]
pub async fn ws_audio(
    ws: axum::extract::ws::WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    // Token is required
    let token = query.token.as_deref().ok_or_else(|| {
        tracing::warn!("Audio WS connection without token");
        (StatusCode::UNAUTHORIZED, Json(ApiError::unauthorized()))
    })?;

    // Validate token
    let claims = crate::auth::extract_claims(token, &config.jwt_secret).ok_or_else(|| {
        tracing::warn!("Audio WS connection with invalid token");
        (StatusCode::UNAUTHORIZED, Json(ApiError::unauthorized()))
    })?;

    // Check token expiration
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    if claims.exp < now {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new("TOKEN_EXPIRED", "Token has expired")),
        ));
    }

    // Engineer-only access
    if !claims.engineer {
        tracing::warn!(sub = %claims.sub, "Audio WS denied: not engineer");
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiError::new(
                "FORBIDDEN",
                "Audio streaming is engineer-only",
            )),
        ));
    }

    drop(config);

    Ok(ws.on_upgrade(move |socket| handle_audio_ws(socket, state)))
}

/// Handle the audio WebSocket connection.
/// Uses a bounded frame dropper (mpsc channel, cap=5) between the broadcast receiver
/// and WebSocket sender to prevent TCP buffer bloat from causing latency accumulation.
#[cfg(feature = "audio")]
async fn handle_audio_ws(mut socket: axum::extract::ws::WebSocket, state: AppState) {
    use axum::extract::ws::Message;
    use iem_core::{ClientMsg, ServerMsg};
    use tokio::time::{Duration, Instant};

    tracing::info!("Audio WebSocket connected");

    // Frame dropper: bounded channel between broadcast receiver and WebSocket sender.
    // When TCP backpressure stalls the sender, stale frames are dropped instead of queuing.
    let (frame_tx, mut frame_rx) = tokio::sync::mpsc::channel::<bytes::Bytes>(5);
    let mut producer_handle: Option<tokio::task::JoinHandle<()>> = None;
    let mut last_audio_time = Instant::now();
    let mut is_listening = false;

    loop {
        tokio::select! {
            // Client commands (text)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<ClientMsg>(&text) {
                            match cmd {
                                ClientMsg::ListenStart { member_id } => {
                                    tracing::info!(target_member = %member_id, "Audio listen started");

                                    // On band member page: mute all other member sends to ENGINEER
                                    // except the target member's send (app-level solo via send muting)
                                    if member_id != "engineer" {
                                        let config = state.config.read().await;
                                        let reaper_url = config.reaper_url.clone();
                                        drop(config);

                                        // Get all non-engineer members with mix_send_index
                                        let discovered = state.discovered_members.read().await;
                                        let members_with_sends: Vec<(String, usize, usize)> = discovered.iter()
                                            .filter(|m| m.id() != "engineer" && m.mix_send_index.is_some())
                                            .map(|m| (m.id(), m.track_index, m.mix_send_index.unwrap()))
                                            .collect();
                                        drop(discovered);

                                        // 1. Query current mute states and save them
                                        let mut saved_mutes = Vec::new();
                                        for (_, track_idx, send_idx) in &members_with_sends {
                                            if let Ok((_vol, muted, _pan)) = query_send_state(
                                                &state.http_client, &reaper_url, *track_idx, *send_idx
                                            ).await {
                                                saved_mutes.push((*track_idx, *send_idx, muted));
                                            }
                                        }

                                        // 2. Mute all sends except target member's, unmute target's
                                        for (mid, track_idx, send_idx) in &members_with_sends {
                                            let mute_val = if *mid == member_id { 0 } else { 1 };
                                            let url = reaper_api::set_send_mute(&reaper_url, *track_idx, *send_idx, mute_val);
                                            let _ = state.http_client.get(&url).send().await;
                                        }

                                        // 3. Store saved states for restoration
                                        *state.engineer_listen_target.write().await =
                                            crate::ListenTarget::Member(saved_mutes);
                                    }

                                    // Abort any existing producer before starting a new one
                                    if let Some(handle) = producer_handle.take() {
                                        handle.abort();
                                    }

                                    // Spawn frame dropper producer: reads broadcast, drops stale on backpressure
                                    let mut broadcast_rx = state.audio_tx.subscribe();
                                    let dropper_tx = frame_tx.clone();
                                    producer_handle = Some(tokio::spawn(async move {
                                        loop {
                                            match broadcast_rx.recv().await {
                                                Ok(frame) => {
                                                    // try_send: if channel full (TCP backpressure), drop this frame
                                                    let _ = dropper_tx.try_send(frame);
                                                }
                                                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                                    tracing::debug!("Audio dropper skipped {} stale frames", n);
                                                }
                                                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                                            }
                                        }
                                    }));

                                    is_listening = true;
                                    last_audio_time = Instant::now();
                                    let status = ServerMsg::AudioStatus {
                                        status: "listening".to_string(),
                                        target: Some(member_id),
                                    };
                                    let json = serde_json::to_string(&status).unwrap_or_default();
                                    if socket.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                                }
                                ClientMsg::ListenStop => {
                                    tracing::info!("Audio listen stopped");

                                    // Stop the producer
                                    if let Some(handle) = producer_handle.take() {
                                        handle.abort();
                                    }

                                    // Restore saved mute states if band member listen was active
                                    let listen_state = state.engineer_listen_target.read().await.clone();
                                    if let crate::ListenTarget::Member(saved_mutes) = listen_state {
                                        let config = state.config.read().await;
                                        let reaper_url = config.reaper_url.clone();
                                        drop(config);
                                        for (track_idx, send_idx, was_muted) in &saved_mutes {
                                            let mute_val = if *was_muted { 1 } else { 0 };
                                            let url = reaper_api::set_send_mute(&reaper_url, *track_idx, *send_idx, mute_val);
                                            let _ = state.http_client.get(&url).send().await;
                                        }
                                    }
                                    *state.engineer_listen_target.write().await = crate::ListenTarget::Idle;

                                    is_listening = false;
                                    let status = ServerMsg::AudioStatus {
                                        status: "stopped".to_string(),
                                        target: None,
                                    };
                                    let json = serde_json::to_string(&status).unwrap_or_default();
                                    if socket.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                                }
                                _ => {} // Ignore non-audio commands
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }

            // Audio frames from dropper channel — only when listening
            frame = frame_rx.recv(), if is_listening => {
                match frame {
                    Some(data) => {
                        last_audio_time = Instant::now();
                        if socket.send(Message::Binary(data.to_vec().into())).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        // Channel closed — producer died
                        break;
                    }
                }
            }

            // Not listening — just wait
            _ = tokio::time::sleep(Duration::from_secs(60)), if !is_listening => {}

            // No-source timeout: if listening but no audio for 5 seconds
            _ = tokio::time::sleep(Duration::from_secs(5)), if is_listening => {
                if last_audio_time.elapsed() > Duration::from_secs(5) {
                    let status = ServerMsg::AudioStatus {
                        status: "no_source".to_string(),
                        target: None,
                    };
                    let json = serde_json::to_string(&status).unwrap_or_default();
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                    // Reset timer so we don't spam
                    last_audio_time = Instant::now();
                }
            }
        }
    }

    // Cleanup
    if let Some(handle) = producer_handle {
        handle.abort();
    }

    tracing::info!("Audio WebSocket disconnected");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_to_reaper_vol_unity() {
        // 0 dB = linear 1.0 (standard: vol = 10^(0/20) = 1.0)
        let vol = db_to_reaper_vol(0.0);
        assert!((vol - 1.0).abs() < 0.01, "0 dB should be 1.0, got {}", vol);
    }

    #[test]
    fn test_db_to_reaper_vol_minus_6() {
        // -6 dB ≈ 0.501
        let vol = db_to_reaper_vol(-6.0);
        assert!(
            (vol - 0.501).abs() < 0.01,
            "-6 dB should be ~0.501, got {}",
            vol
        );
    }

    #[test]
    fn test_db_to_reaper_vol_plus_6() {
        // +6 dB ≈ 1.995
        let vol = db_to_reaper_vol(6.0);
        assert!(
            (vol - 1.995).abs() < 0.01,
            "+6 dB should be ~1.995, got {}",
            vol
        );
    }

    #[test]
    fn test_db_to_reaper_vol_minus_inf() {
        // -60 dB and below should be 0
        assert_eq!(db_to_reaper_vol(-60.0), 0.0);
        assert_eq!(db_to_reaper_vol(-100.0), 0.0);
    }

    #[test]
    fn test_reaper_vol_to_db_unity() {
        // linear 1.0 = 0 dB
        let db = reaper_vol_to_db(1.0);
        assert!(db.abs() < 0.1, "1.0 should be ~0 dB, got {}", db);
    }

    #[test]
    fn test_reaper_vol_to_db_half() {
        // linear 0.5 ≈ -6.02 dB
        let db = reaper_vol_to_db(0.5);
        assert!(
            (db - (-6.02)).abs() < 0.1,
            "0.5 should be ~-6 dB, got {}",
            db
        );
    }

    #[test]
    fn test_reaper_vol_to_db_zero() {
        // 0.0 should be -60 dB (our floor)
        assert_eq!(reaper_vol_to_db(0.0), -60.0);
    }

    #[test]
    fn test_db_conversion_roundtrip() {
        // Test roundtrip conversion at various levels
        for db in [-20.0, -10.0, -6.0, 0.0, 6.0] {
            let vol = db_to_reaper_vol(db);
            let back = reaper_vol_to_db(vol);
            assert!(
                (back - db).abs() < 0.5,
                "Roundtrip failed for {} dB: got {} dB",
                db,
                back
            );
        }
    }

    #[test]
    fn test_db_roundtrip_quantized_02() {
        // With quantize_02, the roundtrip through REAPER's linear domain
        // should produce exact 0.2-step values
        for db in [-4.0_f32, -6.0, -10.2, -3.8, 0.0, 6.0, -12.0, -0.4] {
            let vol = db_to_reaper_vol(db);
            let roundtrip = quantize_02(reaper_vol_to_db(vol));
            assert_eq!(
                roundtrip, db,
                "Quantized round-trip for {db} dB failed: got {roundtrip}"
            );
        }
    }

    #[test]
    fn test_categorize_track_mics() {
        let (cat, pair, side) = categorize_track("MAREK mic");
        assert_eq!(cat, "mics");
        assert!(pair.is_none());
        assert!(side.is_none());
    }

    #[test]
    fn test_categorize_track_stems() {
        let (cat, _, _) = categorize_track("DRUMS L");
        assert_eq!(cat, "stems");
    }

    #[test]
    fn test_categorize_track_tech() {
        let (cat, _, _) = categorize_track("ENGINEER hand");
        assert_eq!(cat, "tech");
    }

    #[test]
    fn test_categorize_track_stereo_left() {
        let (_, pair, side) = categorize_track("DRUMS L");
        assert_eq!(pair, Some("drums".to_string()));
        assert_eq!(side, Some("L".to_string()));
    }

    #[test]
    fn test_categorize_track_stereo_right() {
        let (_, pair, side) = categorize_track("DRUMS R");
        assert_eq!(pair, Some("drums".to_string()));
        assert_eq!(side, Some("R".to_string()));
    }

    #[test]
    fn test_parse_reaper_value_valid() {
        let input = "VOL\t0.716\n";
        assert_eq!(parse_reaper_value(input), Some(0.716));
    }

    #[test]
    fn test_parse_reaper_value_multiline() {
        let input = "NTRACK\t10\nTRACK\t1\tname\t0.5";
        // Should return first parseable value
        assert_eq!(parse_reaper_value(input), Some(10.0));
    }

    #[test]
    fn test_parse_reaper_value_invalid() {
        let input = "ERROR";
        assert_eq!(parse_reaper_value(input), None);
    }

    // ================================================================
    // REAPER SEND response parsing tests - CRITICAL for reading state!
    // ================================================================

    #[test]
    fn test_parse_send_volume() {
        // Actual REAPER response: SEND\ttrack\tsend\tflag\tVOLUME\tpan\tmode
        let input = "SEND\t1\t1\t0\t0.300000\t0.000000\t24";
        assert_eq!(parse_send_volume(input), Some(0.300000));
    }

    #[test]
    fn test_parse_send_volume_unity() {
        let input = "SEND\t1\t2\t0\t1.000000\t0.000000\t24";
        let vol = parse_send_volume(input).unwrap();
        assert!((vol - 1.0).abs() < 0.001, "Expected 1.0, got {}", vol);
    }

    #[test]
    fn test_parse_send_mute_on() {
        // Flag at position 3 is a bitfield: bit 3 (value 8) = muted
        let input = "SEND\t1\t1\t8\t1.000000\t0.000000\t24";
        assert_eq!(parse_send_mute(input), Some(true));
    }

    #[test]
    fn test_parse_send_mute_off() {
        let input = "SEND\t1\t1\t0\t1.000000\t0.000000\t24";
        assert_eq!(parse_send_mute(input), Some(false));
    }

    #[test]
    fn test_parse_send_pan_center() {
        // Pan is at position 5 (0.0 = center in REAPER)
        let input = "SEND\t1\t1\t0\t1.000000\t0.000000\t24";
        assert_eq!(parse_send_pan(input), Some(0.0));
    }

    #[test]
    fn test_parse_send_pan_left() {
        let input = "SEND\t1\t1\t0\t1.000000\t-1.000000\t24";
        assert_eq!(parse_send_pan(input), Some(-1.0));
    }

    #[test]
    fn test_parse_send_pan_right() {
        let input = "SEND\t1\t1\t0\t1.000000\t1.000000\t24";
        assert_eq!(parse_send_pan(input), Some(1.0));
    }

    #[test]
    fn test_parse_send_invalid_response() {
        // Non-SEND response should return None
        let input = "TRACK\t1\tname\t0.5";
        assert_eq!(parse_send_volume(input), None);
        assert_eq!(parse_send_mute(input), None);
        assert_eq!(parse_send_pan(input), None);
    }

    // ================================================================
    // REAPER API URL format tests - CRITICAL for controls to work!
    // ================================================================

    #[test]
    fn test_reaper_url_must_have_underscore_prefix() {
        // CRITICAL: REAPER HTTP API requires /_/ prefix for all commands
        // Without this, REAPER returns empty responses and controls don't work!
        let base = "http://iem.lan:8080";

        // All URLs must start with base/_/
        assert!(
            reaper_api::set_send_vol(base, 1, 1, 0.5).contains("/_/"),
            "set_send_vol must use /_/ prefix"
        );
        assert!(
            reaper_api::set_send_mute(base, 1, 1, 0).contains("/_/"),
            "set_send_mute must use /_/ prefix"
        );
        assert!(
            reaper_api::set_send_pan(base, 1, 1, 0.5).contains("/_/"),
            "set_send_pan must use /_/ prefix"
        );
        assert!(
            reaper_api::get_send_vol(base, 1, 1).contains("/_/"),
            "get_send_vol must use /_/ prefix"
        );
        assert!(
            reaper_api::get_send_mute(base, 1, 1).contains("/_/"),
            "get_send_mute must use /_/ prefix"
        );
        assert!(
            reaper_api::get_send_pan(base, 1, 1).contains("/_/"),
            "get_send_pan must use /_/ prefix"
        );
        assert!(
            reaper_api::query_tracks(base).contains("/_/"),
            "query_tracks must use /_/ prefix"
        );
        assert!(
            reaper_api::set_track_vol(base, 1, 0.5).contains("/_/"),
            "set_track_vol must use /_/ prefix"
        );
        assert!(
            reaper_api::set_track_mute(base, 1, 0).contains("/_/"),
            "set_track_mute must use /_/ prefix"
        );
    }

    #[test]
    fn test_reaper_url_set_send_vol_format() {
        let url = reaper_api::set_send_vol("http://iem.lan:8080", 1, 2, 1.0);
        assert_eq!(url, "http://iem.lan:8080/_/SET/TRACK/1/SEND/2/VOL/1");
    }

    #[test]
    fn test_reaper_url_set_send_mute_format() {
        let url = reaper_api::set_send_mute("http://iem.lan:8080", 3, 4, 1);
        assert_eq!(url, "http://iem.lan:8080/_/SET/TRACK/3/SEND/4/MUTE/1");
    }

    #[test]
    fn test_reaper_url_set_send_pan_format() {
        let url = reaper_api::set_send_pan("http://iem.lan:8080", 5, 6, 0.25);
        assert_eq!(url, "http://iem.lan:8080/_/SET/TRACK/5/SEND/6/PAN/0.25");
    }

    #[test]
    fn test_reaper_url_get_send_vol_format() {
        let url = reaper_api::get_send_vol("http://iem.lan:8080", 1, 1);
        assert_eq!(url, "http://iem.lan:8080/_/GET/TRACK/1/SEND/1/VOL");
    }

    #[test]
    fn test_reaper_url_query_tracks_format() {
        let url = reaper_api::query_tracks("http://iem.lan:8080");
        assert_eq!(url, "http://iem.lan:8080/_/NTRACK;TRACK");
    }

    #[test]
    fn test_reaper_url_set_track_vol_format() {
        let url = reaper_api::set_track_vol("http://iem.lan:8080", 23, 1.0);
        assert_eq!(url, "http://iem.lan:8080/_/SET/TRACK/23/VOL/1");
    }

    #[test]
    fn test_reaper_url_set_track_mute_format() {
        let url = reaper_api::set_track_mute("http://iem.lan:8080", 23, 1);
        assert_eq!(url, "http://iem.lan:8080/_/SET/TRACK/23/MUTE/1");
    }

    #[test]
    fn test_reaper_url_get_extstate_format() {
        let url = reaper_api::get_extstate("http://iem.lan:8080", "REAPERIEM_METERS", "peaks");
        assert_eq!(
            url,
            "http://iem.lan:8080/_/GET/EXTSTATE/REAPERIEM_METERS/peaks"
        );
    }

    #[test]
    fn test_reaper_url_trigger_action_format() {
        let url = reaper_api::trigger_action("http://iem.lan:8080", "_RS_REAPERIEM_METER_BRIDGE");
        assert_eq!(url, "http://iem.lan:8080/_/_RS_REAPERIEM_METER_BRIDGE");
    }

    // ================================================================
    // Pan conversion tests - CRITICAL for correct pan display!
    // ================================================================

    #[test]
    fn test_reaper_pan_to_ui_center() {
        // REAPER center (0.0) -> UI center (0.5)
        let ui_pan = reaper_pan_to_ui(0.0);
        assert!(
            (ui_pan - 0.5).abs() < 0.001,
            "REAPER 0.0 should be UI 0.5, got {}",
            ui_pan
        );
    }

    #[test]
    fn test_reaper_pan_to_ui_left() {
        // REAPER left (-1.0) -> UI left (0.0)
        let ui_pan = reaper_pan_to_ui(-1.0);
        assert!(
            ui_pan.abs() < 0.001,
            "REAPER -1.0 should be UI 0.0, got {}",
            ui_pan
        );
    }

    #[test]
    fn test_reaper_pan_to_ui_right() {
        // REAPER right (1.0) -> UI right (1.0)
        let ui_pan = reaper_pan_to_ui(1.0);
        assert!(
            (ui_pan - 1.0).abs() < 0.001,
            "REAPER 1.0 should be UI 1.0, got {}",
            ui_pan
        );
    }

    #[test]
    fn test_ui_pan_to_reaper_center() {
        // UI center (0.5) -> REAPER center (0.0)
        let reaper_pan = ui_pan_to_reaper(0.5);
        assert!(
            reaper_pan.abs() < 0.001,
            "UI 0.5 should be REAPER 0.0, got {}",
            reaper_pan
        );
    }

    #[test]
    fn test_ui_pan_to_reaper_left() {
        // UI left (0.0) -> REAPER left (-1.0)
        let reaper_pan = ui_pan_to_reaper(0.0);
        assert!(
            (reaper_pan - (-1.0)).abs() < 0.001,
            "UI 0.0 should be REAPER -1.0, got {}",
            reaper_pan
        );
    }

    #[test]
    fn test_ui_pan_to_reaper_right() {
        // UI right (1.0) -> REAPER right (1.0)
        let reaper_pan = ui_pan_to_reaper(1.0);
        assert!(
            (reaper_pan - 1.0).abs() < 0.001,
            "UI 1.0 should be REAPER 1.0, got {}",
            reaper_pan
        );
    }

    #[test]
    fn test_pan_conversion_roundtrip() {
        // Test roundtrip conversion at various UI positions
        for ui_pan in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let reaper = ui_pan_to_reaper(ui_pan);
            let back = reaper_pan_to_ui(reaper);
            assert!(
                (back - ui_pan).abs() < 0.001,
                "Roundtrip failed for UI {}: REAPER {} -> UI {}",
                ui_pan,
                reaper,
                back
            );
        }
    }

    #[test]
    fn test_pan_conversion_clamps() {
        // Test that out-of-range values are clamped
        assert!((reaper_pan_to_ui(-2.0) - 0.0).abs() < 0.001);
        assert!((reaper_pan_to_ui(2.0) - 1.0).abs() < 0.001);
        assert!((ui_pan_to_reaper(-1.0) - (-1.0)).abs() < 0.001);
        assert!((ui_pan_to_reaper(2.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_send_state_full() {
        let input = "SEND\t1\t0\t0\t1.000000\t0.000000\t24";
        let (vol, mute, pan) = parse_send_state(input).unwrap();
        assert!((vol - 1.0).abs() < 0.001);
        assert!(!mute);
        assert!((pan - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_send_state_muted() {
        // Mute flag 8 = bit 3 set = muted
        let input = "SEND\t1\t0\t8\t0.300000\t-0.500000\t24";
        let (vol, mute, pan) = parse_send_state(input).unwrap();
        assert!((vol - 0.3).abs() < 0.001);
        assert!(mute);
        assert!((pan - (-0.5)).abs() < 0.001);
    }

    #[test]
    fn test_parse_send_state_invalid() {
        assert!(parse_send_state("TRACK\t1\tname").is_none());
        assert!(parse_send_state("").is_none());
    }

    #[test]
    fn test_reaper_url_get_send_state_format() {
        let url = reaper_api::get_send_state("http://iem.lan:8080", 1, 2);
        assert_eq!(url, "http://iem.lan:8080/_/GET/TRACK/1/SEND/2");
        assert!(url.contains("/_/"), "get_send_state must use /_/ prefix");
    }

    // ================================================================
    // Volume roundtrip precision tests (echo suppression depends on this)
    // ================================================================

    #[test]
    fn test_db_roundtrip_precision_full_range() {
        // With quantize_02, the cache pre-write stores quantize_02(reaper_vol_to_db(vol)).
        // The poller also quantizes. Both should produce identical values.
        for db_5x in (-300..=60).step_by(1) {
            let db = db_5x as f32 / 5.0; // 0.2 dB steps
            let vol = db_to_reaper_vol(db);
            let cached = quantize_02(reaper_vol_to_db(vol));
            let polled = quantize_02(reaper_vol_to_db(vol));
            assert_eq!(
                cached, polled,
                "Cache and poller disagree at {:.1}dB: cached={:.4}, polled={:.4}",
                db, cached, polled
            );
        }
    }

    #[test]
    fn test_cache_prewrite_prevents_diff_detection() {
        // Simulate cache pre-write with quantize_02:
        // 1. User sends SetLevel { level_db: -12.0 }
        // 2. Cache stores quantize_02(reaper_vol_to_db(db_to_reaper_vol(-12.0)))
        // 3. Poller reads REAPER value, stores quantize_02(reaper_vol_to_db(vol))
        // 4. Diff should be 0.0 (identical values, no broadcast)
        let user_db = -12.0_f32;
        let reaper_vol = db_to_reaper_vol(user_db);
        let cached_db = quantize_02(reaper_vol_to_db(reaper_vol));
        let polled_db = quantize_02(reaper_vol_to_db(reaper_vol));

        let diff = (cached_db - polled_db).abs();
        assert!(
            diff < 0.05,
            "Cache pre-write value ({}) and polled value ({}) differ by {} dB (threshold 0.05)",
            cached_db,
            polled_db,
            diff
        );
    }

    // ================================================================
    // Track categorization regression tests - HAND tracks must be tech
    // ================================================================

    #[test]
    fn test_categorize_hand_mic_as_tech() {
        // Bug: "HAND1 mic" contains "mic" → wrongly categorized as "mics"
        // HAND tracks must always be "tech", even though they have "mic" in the name
        let (cat, _, _) = categorize_track("HAND1 mic");
        assert_eq!(cat, "tech", "HAND1 mic must be tech, not mics");

        let (cat, _, _) = categorize_track("HAND2 mic");
        assert_eq!(cat, "tech");

        let (cat, _, _) = categorize_track("HAND3 mic");
        assert_eq!(cat, "tech");

        let (cat, _, _) = categorize_track("HAND4 mic");
        assert_eq!(cat, "tech");

        let (cat, _, _) = categorize_track("ENGINEER mic");
        assert_eq!(cat, "tech", "ENGINEER mic must be tech, not mics");
    }

    #[test]
    fn test_categorize_regular_mic_still_mics() {
        // Regular member mics must still be categorized as "mics"
        let (cat, _, _) = categorize_track("PETKA mic");
        assert_eq!(cat, "mics");

        let (cat, _, _) = categorize_track("STEVO mic");
        assert_eq!(cat, "mics");

        let (cat, _, _) = categorize_track("MAREK gtr");
        assert_eq!(cat, "mics");
    }

    /// Helper: parse stereo meters from NTRACK text (mirrors poll_mixer_state logic)
    fn parse_meters_from_ntrack(text: &str) -> HashMap<usize, [f32; 2]> {
        let mut meters = HashMap::new();
        for line in text.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.first() == Some(&"TRACK")
                && parts.len() >= 14
                && let Ok(track_idx) = parts[1].parse::<usize>()
                && let (Ok(peak_db10), Ok(pos_db10)) =
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
        meters
    }

    /// 14-field TRACK line should produce meter data with correct dB×10 conversion
    #[test]
    fn test_ntrack_14_fields_produces_meter() {
        // -100 dB×10 = -10 dB, -80 dB×10 = -8 dB
        let line = "TRACK\t1\tPETKA mic\t0\t1.000000\t0.000000\t-100\t-80\t1.000000\t0\t9\t0\t0\t0";
        let meters = parse_meters_from_ntrack(line);
        assert!(meters.contains_key(&1));
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

    /// 12-field TRACK line (no meter fields) must NOT produce meter data.
    /// Without meter fields, field[6] is width (1.000000), not a dB value.
    #[test]
    fn test_ntrack_12_fields_no_meter() {
        let line = "TRACK\t1\tPETKA mic\t0\t1.000000\t0.000000\t1.000000\t0\t9\t0\t0\t0";
        let meters = parse_meters_from_ntrack(line);
        assert!(
            !meters.contains_key(&1),
            "12-field line must NOT produce meter — field[6] is width, not meter"
        );
    }

    /// Regression: width value 1.000000 must NOT be parsed as meter data
    #[test]
    fn test_ntrack_width_not_parsed_as_meter() {
        let text = "\
NTRACK\t3
TRACK\t1\tPETKA mic\t0\t1.000000\t0.000000\t1.000000\t0\t9\t0\t0\t0
TRACK\t2\tSTEVO mic\t0\t1.000000\t0.000000\t1.000000\t0\t9\t0\t0\t0
TRACK\t3\tMAREK mic\t0\t1.000000\t0.000000\t1.000000\t0\t9\t0\t0\t0";
        let meters = parse_meters_from_ntrack(text);
        assert!(
            meters.is_empty(),
            "No 12-field tracks should produce meters, got {} entries",
            meters.len()
        );
    }

    /// REAPER meter floor (-1500 = -150 dB) must produce 0.0 (silence)
    #[test]
    fn test_reaper_meter_floor_is_silence() {
        let line = "TRACK\t1\tPETKA mic\t192\t1.000000\t0.000000\t-1500\t-1500\t1.000000\t3\t9\t0\t0\t24421844";
        let meters = parse_meters_from_ntrack(line);
        assert_eq!(
            meters.get(&1),
            Some(&[0.0, 0.0]),
            "-1500 (dB×10 = -150 dB) must be silence"
        );
    }

    /// Values above REAPER meter floor should produce signal
    #[test]
    fn test_reaper_above_floor_shows_signal() {
        // -140 dB×10 = -14 dB, -120 dB×10 = -12 dB
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
            assert_eq!(*left, 0.0, "Track {} L should be silent, got {}", idx, left);
            assert_eq!(
                *right, 0.0,
                "Track {} R should be silent, got {}",
                idx, right
            );
        }
    }

    // ================================================================
    // build_mix_channel_templates tests (v1.49.0 Engineer Mixes Tab)
    // ================================================================

    fn make_discovered_members() -> Vec<iem_core::DiscoveredMember> {
        vec![
            iem_core::DiscoveredMember {
                name: "PETRONELA".to_string(),
                track_index: 23,
                dante_output_l: 1,
                dante_output_r: 2,
                send_index: 0,
                mix_send_index: Some(1), // Send 1 targets engineer (Send 0 = hw out)
            },
            iem_core::DiscoveredMember {
                name: "STEVO".to_string(),
                track_index: 24,
                dante_output_l: 3,
                dante_output_r: 4,
                send_index: 1,
                mix_send_index: Some(1),
            },
            iem_core::DiscoveredMember {
                name: "MAREK".to_string(),
                track_index: 25,
                dante_output_l: 5,
                dante_output_r: 6,
                send_index: 2,
                mix_send_index: Some(1),
            },
            iem_core::DiscoveredMember {
                name: "ENGINEER".to_string(),
                track_index: 32,
                dante_output_l: 19,
                dante_output_r: 20,
                send_index: 9,
                mix_send_index: None, // Engineer doesn't send to itself
            },
        ]
    }

    #[test]
    fn test_mix_channels_exclude_engineer() {
        let discovered = make_discovered_members();
        let mix = build_mix_channel_templates(&discovered, "engineer");
        // Should have 3 channels (PETRONELA, STEVO, MAREK) — engineer excluded
        assert_eq!(mix.len(), 3);
        assert!(
            !mix.iter().any(|ch| ch.name == "ENGINEER"),
            "Engineer should be excluded from mix channels"
        );
    }

    #[test]
    fn test_mix_channels_have_correct_category() {
        let discovered = make_discovered_members();
        let mix = build_mix_channel_templates(&discovered, "engineer");
        for ch in &mix {
            assert_eq!(
                ch.category, "mixes",
                "Mix channels must have category 'mixes'"
            );
        }
    }

    #[test]
    fn test_mix_channels_use_member_track_index() {
        let discovered = make_discovered_members();
        let mix = build_mix_channel_templates(&discovered, "engineer");
        assert_eq!(mix[0].track_index, 23, "PETRONELA track_index");
        assert_eq!(mix[1].track_index, 24, "STEVO track_index");
        assert_eq!(mix[2].track_index, 25, "MAREK track_index");
    }

    #[test]
    fn test_mix_channels_name_strips_inear_suffix() {
        // DiscoveredMember.name already excludes " inear" — just the uppercase name
        let discovered = make_discovered_members();
        let mix = build_mix_channel_templates(&discovered, "engineer");
        assert_eq!(mix[0].name, "PETRONELA");
        assert_eq!(mix[1].name, "STEVO");
        assert_eq!(mix[2].name, "MAREK");
    }

    #[test]
    fn test_mix_channels_default_values() {
        let discovered = make_discovered_members();
        let mix = build_mix_channel_templates(&discovered, "engineer");
        for ch in &mix {
            assert_eq!(ch.level_db, 0.0, "Default level should be 0 dB");
            assert_eq!(ch.pan, 0.5, "Default pan should be 0.5 (center)");
            assert!(!ch.muted, "Default muted should be false");
            assert!(ch.stereo_pair.is_none(), "No stereo pair");
            assert!(ch.stereo_side.is_none(), "No stereo side");
        }
    }

    #[test]
    fn test_mix_channels_empty_when_no_members() {
        let mix = build_mix_channel_templates(&[], "engineer");
        assert!(mix.is_empty());
    }

    #[test]
    fn test_mix_channels_empty_when_only_engineer() {
        let discovered = vec![iem_core::DiscoveredMember {
            name: "ENGINEER".to_string(),
            track_index: 32,
            dante_output_l: 19,
            dante_output_r: 20,
            send_index: 9,
            mix_send_index: None,
        }];
        let mix = build_mix_channel_templates(&discovered, "engineer");
        assert!(mix.is_empty(), "No mix channels when only engineer exists");
    }

    // ================================================================
    // Send destination parsing tests — CRITICAL for engineer mute safety
    // ================================================================

    #[test]
    fn test_parse_send_destination_member_to_engineer() {
        // Real REAPER response: member inear track send to engineer (track 32)
        let response = "SEND\t23\t1\t0\t1.00000000\t0.00000000\t32\n";
        assert_eq!(
            parse_send_destination(response),
            Some(32_i32),
            "Should parse destination track 32 (engineer)"
        );
    }

    #[test]
    fn test_parse_send_destination_hw_output_negative() {
        // Real REAPER response: hardware output send has destination -1
        // This MUST return Some(-1), NOT None — None means "send doesn't exist"
        let response = "SEND\t23\t0\t0\t1.00000000\t0.00000000\t-1\n";
        assert_eq!(
            parse_send_destination(response),
            Some(-1_i32),
            "Hardware output destination -1 must parse as Some(-1), not None"
        );
    }

    #[test]
    fn test_parse_send_destination_invalid() {
        assert_eq!(parse_send_destination("TRACK\t1\tname"), None);
        assert_eq!(parse_send_destination(""), None);
        assert_eq!(parse_send_destination("SEND\t1\t0\t0\t1.0\t0.0"), None); // Too few fields
    }

    #[test]
    fn test_parse_send_destination_multiline() {
        // Response might have extra lines
        let response = "SEND\t23\t1\t8\t0.50000000\t0.00000000\t32\nSOMETHING\telse\n";
        assert_eq!(parse_send_destination(response), Some(32_i32));
    }

    #[test]
    fn test_parse_send_destination_none_means_no_send() {
        // Empty response = send doesn't exist (REAPER returned nothing)
        assert_eq!(parse_send_destination(""), None);
        // Non-SEND response = not a send query result
        assert_eq!(parse_send_destination("TRACK\t1\tname"), None);
    }

    // ================================================================
    // Mix send index usage tests — ensures hardcoded 0 is never used
    // ================================================================

    #[test]
    fn test_send_index_for_mix_channel_uses_discovered_index() {
        // Simulate the send_index_for logic with discovered members
        let discovered = make_discovered_members();
        let member_index = 9_usize; // engineer's send_index

        // For a mix track (member inear), should use mix_send_index, not 0
        let mix_track_indices: Vec<usize> = discovered
            .iter()
            .filter(|m| m.id() != "engineer")
            .map(|m| m.track_index)
            .collect();

        let send_index_for = |ti: usize| -> Option<usize> {
            if mix_track_indices.contains(&ti) {
                discovered
                    .iter()
                    .find(|m| m.track_index == ti)
                    .and_then(|m| m.mix_send_index)
            } else {
                Some(member_index)
            }
        };

        // PETRONELA inear (track 23) → mix_send_index = 1 (NOT 0!)
        assert_eq!(
            send_index_for(23),
            Some(1),
            "Mix channel must use mix_send_index (1), not hardcoded 0"
        );
        // Regular input track (track 5) → member's send_index
        assert_eq!(
            send_index_for(5),
            Some(member_index),
            "Input track should use member's send_index"
        );
    }

    #[test]
    fn test_mute_all_uses_mix_send_index_not_zero() {
        // Simulate the MuteAll URL generation for engineer
        let discovered = make_discovered_members();
        let reaper_url = "http://iem.lan:8080";

        let mix_urls: Vec<String> = discovered
            .iter()
            .filter(|m| m.id() != "engineer")
            .filter_map(|m| {
                m.mix_send_index
                    .map(|si| reaper_api::set_send_mute(reaper_url, m.track_index, si, 1))
            })
            .collect();

        assert_eq!(mix_urls.len(), 3, "Should have 3 mix mute URLs");

        // ALL URLs must use SEND/1 (mix_send_index), NOT SEND/0 (hw output)
        for url in &mix_urls {
            assert!(
                url.contains("/SEND/1/"),
                "Mute URL must use SEND/1 (engineer send), not SEND/0 (hw out): {}",
                url
            );
            assert!(
                !url.contains("/SEND/0/"),
                "SEND/0 is the hardware output — muting it kills member audio: {}",
                url
            );
        }
    }

    #[test]
    fn test_mute_all_skips_members_without_mix_send_index() {
        // If a member doesn't have mix_send_index, it should be skipped
        let mut discovered = make_discovered_members();
        // Remove mix_send_index from PETRONELA
        discovered[0].mix_send_index = None;

        let mix_urls: Vec<String> = discovered
            .iter()
            .filter(|m| m.id() != "engineer")
            .filter_map(|m| {
                m.mix_send_index.map(|si| {
                    reaper_api::set_send_mute("http://iem.lan:8080", m.track_index, si, 1)
                })
            })
            .collect();

        // Only STEVO and MAREK should have URLs (PETRONELA skipped)
        assert_eq!(
            mix_urls.len(),
            2,
            "Members without mix_send_index must be skipped"
        );
    }

    // ================================================================
    // build_channel_templates resolved indices tests (Issue #85)
    // ================================================================

    fn make_test_inputs() -> Vec<iem_core::config::InputTrack> {
        vec![
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
            iem_core::config::InputTrack {
                name: "DRUMS".to_string(),
                dante_input: 3,
                default_level_db: 0.0,
            },
        ]
    }

    #[test]
    fn test_build_channel_templates_fallback_without_resolved() {
        // No resolved map (None) → sequential i+1 (backward compat)
        let inputs = make_test_inputs();
        let channels = build_channel_templates(&inputs, None);
        assert_eq!(channels[0].track_index, 1);
        assert_eq!(channels[1].track_index, 2);
        assert_eq!(channels[2].track_index, 3);
    }

    #[test]
    fn test_build_channel_templates_with_resolved_indices() {
        // Resolved map shifts indices (e.g., track inserted at position 1)
        let inputs = make_test_inputs();
        let mut resolved = std::collections::HashMap::new();
        resolved.insert("PETKA mic".to_string(), 2_usize);
        resolved.insert("STEVO mic".to_string(), 3);
        resolved.insert("DRUMS".to_string(), 4);
        let channels = build_channel_templates(&inputs, Some(&resolved));
        assert_eq!(channels[0].track_index, 2, "PETKA mic should be at 2");
        assert_eq!(channels[1].track_index, 3, "STEVO mic should be at 3");
        assert_eq!(channels[2].track_index, 4, "DRUMS should be at 4");
    }

    #[test]
    fn test_build_channel_templates_partial_resolution() {
        // Some names resolved, others fall back to sequential
        let inputs = make_test_inputs();
        let mut resolved = std::collections::HashMap::new();
        resolved.insert("PETKA mic".to_string(), 5_usize);
        // STEVO mic and DRUMS not in resolved map
        let channels = build_channel_templates(&inputs, Some(&resolved));
        assert_eq!(channels[0].track_index, 5, "PETKA mic resolved to 5");
        assert_eq!(channels[1].track_index, 2, "STEVO mic falls back to i+1=2");
        assert_eq!(channels[2].track_index, 3, "DRUMS falls back to i+1=3");
    }

    // ================================================================
    // EQ parameter parsing tests
    // ================================================================

    #[test]
    fn test_parse_eq_band_lowshelf_with_formatted_values() {
        // New format with REAPER-formatted display values (fh, gd, bo)
        let s = "b0:lowshelf,fn=0.283000,gn=0.183911,bn=0.295000,fh=250.0,gd=-2.7,bo=1.18";
        let band = parse_eq_band(s).unwrap();
        assert_eq!(band.band_type, "lowshelf");
        assert!((band.freq_norm - 0.283).abs() < 0.001);
        assert!((band.gain_norm - 0.183911).abs() < 0.001);
        assert!((band.bw_norm - 0.295).abs() < 0.001);
        // Display values come from REAPER-formatted fields (accurate)
        assert!(
            (band.freq_hz - 250.0).abs() < 0.1,
            "freq_hz should use fh= value"
        );
        assert!(
            (band.gain_db - -2.7).abs() < 0.1,
            "gain_db should use gd= value"
        );
        assert!((band.bw - 1.18).abs() < 0.01, "bw should use bo= value");
    }

    #[test]
    fn test_parse_eq_band_lowshelf_fallback() {
        // Old format without formatted values — falls back to approximation
        let s = "b0:lowshelf,fn=0.283000,gn=0.184000,bn=0.295000";
        let band = parse_eq_band(s).unwrap();
        assert_eq!(band.band_type, "lowshelf");
        assert!(
            band.freq_hz > 0.0,
            "freq_hz should be computed from fallback"
        );
        assert!(
            band.gain_db < 0.0,
            "gain below 0.25 should give negative dB"
        );
        assert!(band.bw > 0.0, "bw should be positive");
    }

    #[test]
    fn test_parse_eq_band_regular_with_formatted() {
        let s = "b1:band,fn=0.500000,gn=0.250000,bn=0.500000,fh=632.5,gd=0.0,bo=2.00";
        let band = parse_eq_band(s).unwrap();
        assert_eq!(band.band_type, "band");
        assert!((band.freq_hz - 632.5).abs() < 0.1);
        assert!((band.gain_db - 0.0).abs() < 0.01);
        assert!((band.bw - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_eq_band_regular_fallback() {
        let s = "b1:band,fn=0.500000,gn=0.250000,bn=0.500000";
        let band = parse_eq_band(s).unwrap();
        assert_eq!(band.band_type, "band");
        // fn=0.5 -> 20 * 1000^0.5 = 20 * ~31.62 = ~632 Hz
        assert!(band.freq_hz > 600.0 && band.freq_hz < 700.0);
        // gn=0.25 -> (0.25 - 0.25) * 96 = 0.0 dB
        assert!((band.gain_db - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_eq_params_response_ok() {
        let value = "OK:track=3,name=MAREK mic,fx=1,bands=2,gg=0.0dB,bypass=0|b0:lowshelf,fn=0.283000,gn=0.184000,bn=0.295000,fh=250.0,gd=-2.7,bo=1.18|b1:band,fn=0.500000,gn=0.250000,bn=0.500000,fh=632.5,gd=0.0,bo=2.00";
        let msg = parse_eq_params_response(3, value).unwrap();
        match msg {
            iem_core::ServerMsg::EqParams {
                track_index,
                track_name,
                bands,
            } => {
                assert_eq!(track_index, 3);
                assert_eq!(track_name, "MAREK mic");
                assert_eq!(bands.len(), 2);
                assert_eq!(bands[0].band_type, "lowshelf");
                assert_eq!(bands[1].band_type, "band");
            }
            _ => panic!("Expected EqParams"),
        }
    }

    #[test]
    fn test_parse_eq_params_response_no_eq() {
        let value = "NO_EQ:MAREK mic";
        let msg = parse_eq_params_response(3, value).unwrap();
        match msg {
            iem_core::ServerMsg::EqParams {
                track_index,
                track_name,
                bands,
            } => {
                assert_eq!(track_index, 3);
                assert_eq!(track_name, "MAREK mic");
                assert!(bands.is_empty());
            }
            _ => panic!("Expected EqParams"),
        }
    }

    #[test]
    fn test_parse_eq_params_response_error() {
        let value = "ERROR:track_not_found:99";
        assert!(parse_eq_params_response(99, value).is_none());
    }

    #[test]
    fn test_reaper_url_set_extstate_format() {
        let url =
            reaper_api::set_extstate("http://iem.lan:8080", "reaperiem", "eq_read_track", "3");
        assert_eq!(
            url,
            "http://iem.lan:8080/_/SET/EXTSTATE/reaperiem/eq_read_track/3"
        );
        assert!(url.contains("/_/"), "set_extstate must use /_/ prefix");
    }
}
