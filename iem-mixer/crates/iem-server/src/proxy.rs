//! REAPER HTTP API proxy

use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
};
use iem_core::ApiError;

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

    let _member_index = config.member_index(&member_id).unwrap();

    // Query REAPER for track states
    // In a real implementation, this would call the REAPER API
    // For now, return a placeholder
    let channels: Vec<iem_core::Channel> = config
        .inputs
        .iter()
        .enumerate()
        .map(|(i, input)| iem_core::Channel {
            track_index: i + 1,
            name: input.name.clone(),
            level_db: input.default_level_db,
            pan: 0.0,
            muted: false,
        })
        .collect();

    drop(config);

    Ok(Json(iem_core::MixerState {
        member_id: member_id.clone(),
        channels,
    }))
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
        "{}/SET/TRACK/{}/SEND/{}/VOL/{}",
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
        "{}/SET/TRACK/{}/SEND/{}/PAN/{}",
        reaper_url, track_index, member_index, payload.pan
    );

    state.http_client.get(&url).send().await.map_err(|_| {
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
        "{}/SET/TRACK/{}/SEND/{}/MUTE/{}",
        reaper_url, track_index, member_index, mute_val
    );

    state.http_client.get(&url).send().await.map_err(|_| {
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
#[allow(dead_code)]
fn reaper_vol_to_db(vol: f32) -> f32 {
    if vol <= 0.0 {
        -60.0
    } else {
        20.0 * (vol / 0.716).log10()
    }
}
