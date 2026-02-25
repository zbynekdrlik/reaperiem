//! REAPER HTTP API proxy

use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
};
use iem_core::{ApiError, BatchControlRequest, BatchOperation, PollResponse};
use std::collections::HashMap;

use crate::AppState;

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
) -> Result<Json<iem_core::MixerState>, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    // Verify member exists
    let _member = config
        .find_member(&member_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;

    let member_index = config.member_index(&member_id).unwrap();
    let reaper_url = config.reaper_url.clone();

    // Build channels from inputs with category and stereo info
    let channels: Vec<iem_core::Channel> = config
        .inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let (category, stereo_pair, stereo_side) = categorize_track(&input.name);
            iem_core::Channel {
                track_index: i + 1,
                name: input.name.clone(),
                level_db: input.default_level_db,
                pan: 0.0,
                muted: false,
                category,
                stereo_pair,
                stereo_side,
            }
        })
        .collect();

    drop(config);

    // Try to get actual levels from REAPER
    let mut result_channels = channels.clone();
    for ch in &mut result_channels {
        if let Ok((level, mute, pan)) = query_send_state(
            &state.http_client,
            &reaper_url,
            ch.track_index,
            member_index,
        )
        .await
        {
            ch.level_db = reaper_vol_to_db(level);
            ch.muted = mute;
            ch.pan = pan;
        }
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
) -> Result<Json<PollResponse>, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    // Verify member exists
    let _member = config
        .find_member(&member_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;

    let member_index = config.member_index(&member_id).unwrap();
    let reaper_url = config.reaper_url.clone();

    // Build channels from inputs
    let channels: Vec<iem_core::Channel> = config
        .inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let (category, stereo_pair, stereo_side) = categorize_track(&input.name);
            iem_core::Channel {
                track_index: i + 1,
                name: input.name.clone(),
                level_db: input.default_level_db,
                pan: 0.0,
                muted: false,
                category,
                stereo_pair,
                stereo_side,
            }
        })
        .collect();

    drop(config);

    let mut result_channels = channels;
    let mut meters: HashMap<usize, f32> = HashMap::new();
    let mut connected = false;

    // Query REAPER for all track states in a batch
    // First try to get all track info
    let tracks_url = format!("{}/_/NTRACK;TRACK", reaper_url);
    if let Ok(resp) = state.http_client.get(&tracks_url).send().await
        && let Ok(text) = resp.text().await
    {
        connected = true;
        // Parse track data for meters
        for line in text.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.first() == Some(&"TRACK")
                && parts.len() > 12
                && let Ok(track_idx) = parts[1].parse::<usize>()
                && let Ok(peak) = parts[12].parse::<f32>()
            {
                meters.insert(track_idx, peak);
            }
        }
    }

    // Query send states for each channel
    for ch in &mut result_channels {
        if let Ok((level, mute, pan)) = query_send_state(
            &state.http_client,
            &reaper_url,
            ch.track_index,
            member_index,
        )
        .await
        {
            ch.level_db = reaper_vol_to_db(level);
            ch.muted = mute;
            ch.pan = pan;
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

/// Batch control operations (+Me, Reset)
pub async fn batch_control(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    Json(payload): Json<BatchControlRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    let member = config
        .find_member(&member_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;

    let member_index = config.member_index(&member_id).unwrap();
    let reaper_url = config.reaper_url.clone();
    let my_input = format!("{} mic", member.name.to_uppercase());
    let inputs = config.inputs.clone();

    drop(config);

    match payload.operation {
        BatchOperation::MoreMe => {
            // Boost own mic by +6dB, reduce others by -3dB
            for (i, input) in inputs.iter().enumerate() {
                let track_index = i + 1;

                // Skip R side of stereo pairs
                if input.name.ends_with(" R") {
                    continue;
                }

                // Get current level
                let current_vol = if let Ok((vol, _, _)) =
                    query_send_state(&state.http_client, &reaper_url, track_index, member_index)
                        .await
                {
                    vol
                } else {
                    1.0 // Default to 0dB
                };

                let current_db = reaper_vol_to_db(current_vol);
                let is_me = input.name.to_uppercase() == my_input;

                let new_db = if is_me {
                    (current_db + 6.0).min(6.0)
                } else {
                    (current_db - 3.0).max(-60.0)
                };

                let new_vol = db_to_reaper_vol(new_db);
                let url = format!(
                    "{}/_/SET/TRACK/{}/SEND/{}/VOL/{}",
                    reaper_url, track_index, member_index, new_vol
                );
                let _ = state.http_client.get(&url).send().await;

                // Also set partner for stereo pairs
                if let Some(partner_idx) = find_stereo_partner(&inputs, &input.name) {
                    let partner_url = format!(
                        "{}/_/SET/TRACK/{}/SEND/{}/VOL/{}",
                        reaper_url, partner_idx, member_index, new_vol
                    );
                    let _ = state.http_client.get(&partner_url).send().await;
                }
            }
        }
        BatchOperation::Reset => {
            // Reset all to 0dB, unmuted, centered pan
            for (i, _input) in inputs.iter().enumerate() {
                let track_index = i + 1;
                let vol = db_to_reaper_vol(0.0);

                // Set volume to 0dB
                let vol_url = format!(
                    "{}/_/SET/TRACK/{}/SEND/{}/VOL/{}",
                    reaper_url, track_index, member_index, vol
                );
                let _ = state.http_client.get(&vol_url).send().await;

                // Unmute
                let mute_url = format!(
                    "{}/_/SET/TRACK/{}/SEND/{}/MUTE/0",
                    reaper_url, track_index, member_index
                );
                let _ = state.http_client.get(&mute_url).send().await;

                // Center pan
                let pan_url = format!(
                    "{}/_/SET/TRACK/{}/SEND/{}/PAN/0.5",
                    reaper_url, track_index, member_index
                );
                let _ = state.http_client.get(&pan_url).send().await;
            }
        }
    }

    Ok(StatusCode::OK)
}

/// Query send state from REAPER
async fn query_send_state(
    client: &reqwest::Client,
    reaper_url: &str,
    track_index: usize,
    send_index: usize,
) -> Result<(f32, bool, f32), ()> {
    // Query volume
    let vol_url = format!(
        "{}/_/GET/TRACK/{}/SEND/{}/VOL",
        reaper_url, track_index, send_index
    );
    let vol = if let Ok(resp) = client.get(&vol_url).send().await {
        if let Ok(text) = resp.text().await {
            parse_reaper_value(&text).unwrap_or(1.0)
        } else {
            return Err(());
        }
    } else {
        return Err(());
    };

    // Query mute
    let mute_url = format!(
        "{}/_/GET/TRACK/{}/SEND/{}/MUTE",
        reaper_url, track_index, send_index
    );
    let mute = if let Ok(resp) = client.get(&mute_url).send().await {
        if let Ok(text) = resp.text().await {
            parse_reaper_value(&text).unwrap_or(0.0) > 0.5
        } else {
            false
        }
    } else {
        false
    };

    // Query pan
    let pan_url = format!(
        "{}/_/GET/TRACK/{}/SEND/{}/PAN",
        reaper_url, track_index, send_index
    );
    let pan = if let Ok(resp) = client.get(&pan_url).send().await {
        if let Ok(text) = resp.text().await {
            parse_reaper_value(&text).unwrap_or(0.5)
        } else {
            0.5
        }
    } else {
        0.5
    };

    Ok((vol, mute, pan))
}

/// Parse a REAPER response value (format: "COMMAND\tVALUE")
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

/// Categorize a track by name
fn categorize_track(name: &str) -> (String, Option<String>, Option<String>) {
    let name_lower = name.to_lowercase();

    // Determine category
    let category = if name_lower.contains("mic") || name_lower.contains("gtr") {
        "mics"
    } else if name_lower.contains("hand") || name_lower.contains("engineer") {
        "tech"
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

/// Find the stereo partner track index
fn find_stereo_partner(inputs: &[iem_core::InputTrack], name: &str) -> Option<usize> {
    if name.ends_with(" L") {
        let partner_name = name.replace(" L", " R");
        inputs
            .iter()
            .position(|i| i.name == partner_name)
            .map(|p| p + 1)
    } else if name.ends_with(" R") {
        let partner_name = name.replace(" R", " L");
        inputs
            .iter()
            .position(|i| i.name == partner_name)
            .map(|p| p + 1)
    } else {
        None
    }
}

/// Set send level for a member's mix
pub async fn set_send_level(
    State(state): State<AppState>,
    Path((member_id, track_index)): Path<(String, usize)>,
    Json(payload): Json<SetLevelRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    // Verify member exists
    let member_index = config
        .member_index(&member_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;

    let reaper_url = config.reaper_url.clone();
    drop(config);

    // Convert dB to REAPER volume (approximate)
    // REAPER uses 0-1 scale where 0.716 ≈ 0dB
    let vol = db_to_reaper_vol(payload.level_db);

    // Build REAPER API URL for setting send volume
    let url = format!(
        "{}/_/SET/TRACK/{}/SEND/{}/VOL/{}",
        reaper_url, track_index, member_index, vol
    );

    tracing::debug!(url = %url, level_db = payload.level_db, "Setting send level");

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
    Json(payload): Json<SetPanRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    let member_index = config
        .member_index(&member_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;

    let reaper_url = config.reaper_url.clone();
    drop(config);

    // REAPER pan is -1.0 to 1.0
    let url = format!(
        "{}/_/SET/TRACK/{}/SEND/{}/PAN/{}",
        reaper_url, track_index, member_index, payload.pan
    );

    tracing::debug!(url = %url, pan = payload.pan, "Setting send pan");

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
    Json(payload): Json<SetMuteRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    let member_index = config
        .member_index(&member_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;

    let reaper_url = config.reaper_url.clone();
    drop(config);

    let mute_val = if payload.muted { 1 } else { 0 };
    let url = format!(
        "{}/_/SET/TRACK/{}/SEND/{}/MUTE/{}",
        reaper_url, track_index, member_index, mute_val
    );

    tracing::debug!(url = %url, muted = payload.muted, "Setting send mute");

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
/// REAPER uses a logarithmic scale where:
/// - 0.0 = -inf dB
/// - 0.716 ≈ 0 dB (unity)
/// - 1.0 ≈ +6 dB
fn db_to_reaper_vol(db: f32) -> f32 {
    if db <= -60.0 {
        0.0
    } else {
        // Approximate conversion
        // vol = 10^(db/20) * 0.716
        let linear = 10.0_f32.powf(db / 20.0);
        (linear * 0.716).clamp(0.0, 4.0)
    }
}

/// Convert REAPER volume to dB
fn reaper_vol_to_db(vol: f32) -> f32 {
    if vol <= 0.0 {
        -60.0
    } else {
        20.0 * (vol / 0.716).log10()
    }
}

/// REAPER HTTP API URL builder
/// CRITICAL: All REAPER API commands MUST use the `/_/` prefix!
/// Without this prefix, REAPER returns empty responses.
mod reaper_api {
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

    /// Build URL for getting send volume
    pub fn get_send_vol(base_url: &str, track: usize, send: usize) -> String {
        format!("{}/_/GET/TRACK/{}/SEND/{}/VOL", base_url, track, send)
    }

    /// Build URL for getting send mute
    pub fn get_send_mute(base_url: &str, track: usize, send: usize) -> String {
        format!("{}/_/GET/TRACK/{}/SEND/{}/MUTE", base_url, track, send)
    }

    /// Build URL for getting send pan
    pub fn get_send_pan(base_url: &str, track: usize, send: usize) -> String {
        format!("{}/_/GET/TRACK/{}/SEND/{}/PAN", base_url, track, send)
    }

    /// Build URL for querying tracks
    pub fn query_tracks(base_url: &str) -> String {
        format!("{}/_/NTRACK;TRACK", base_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_to_reaper_vol_unity() {
        // 0 dB should be approximately 0.716
        let vol = db_to_reaper_vol(0.0);
        assert!(
            (vol - 0.716).abs() < 0.01,
            "0dB should be ~0.716, got {}",
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
        // 0.716 should be approximately 0 dB
        let db = reaper_vol_to_db(0.716);
        assert!(db.abs() < 0.1, "0.716 should be ~0dB, got {}", db);
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
                "Roundtrip failed for {}dB: got {}dB",
                db,
                back
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

    #[test]
    fn test_find_stereo_partner_left() {
        let inputs = vec![
            iem_core::InputTrack {
                name: "DRUMS L".to_string(),
                dante_input: 1,
                default_level_db: 0.0,
            },
            iem_core::InputTrack {
                name: "DRUMS R".to_string(),
                dante_input: 2,
                default_level_db: 0.0,
            },
        ];
        // "DRUMS L" at index 0 should find partner "DRUMS R" at index 1 (returns 1-based: 2)
        assert_eq!(find_stereo_partner(&inputs, "DRUMS L"), Some(2));
    }

    #[test]
    fn test_find_stereo_partner_right() {
        let inputs = vec![
            iem_core::InputTrack {
                name: "DRUMS L".to_string(),
                dante_input: 1,
                default_level_db: 0.0,
            },
            iem_core::InputTrack {
                name: "DRUMS R".to_string(),
                dante_input: 2,
                default_level_db: 0.0,
            },
        ];
        // "DRUMS R" at index 1 should find partner "DRUMS L" at index 0 (returns 1-based: 1)
        assert_eq!(find_stereo_partner(&inputs, "DRUMS R"), Some(1));
    }

    #[test]
    fn test_find_stereo_partner_none() {
        let inputs = vec![iem_core::InputTrack {
            name: "MAREK mic".to_string(),
            dante_input: 1,
            default_level_db: 0.0,
        }];
        assert_eq!(find_stereo_partner(&inputs, "MAREK mic"), None);
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
    }

    #[test]
    fn test_reaper_url_set_send_vol_format() {
        let url = reaper_api::set_send_vol("http://iem.lan:8080", 1, 2, 0.716);
        assert_eq!(url, "http://iem.lan:8080/_/SET/TRACK/1/SEND/2/VOL/0.716");
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
}
