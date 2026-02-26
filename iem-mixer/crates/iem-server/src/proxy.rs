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
                pan: 0.5, // Center in UI range (0.0-1.0)
                muted: false,
                category,
                stereo_pair,
                stereo_side,
            }
        })
        .collect();

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
                pan: 0.5, // Center in UI range (0.0-1.0)
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
    let tracks_url = reaper_api::query_tracks(&reaper_url);
    if let Ok(resp) = state.http_client.get(&tracks_url).send().await
        && let Ok(text) = resp.text().await
    {
        connected = true;
        // Parse track data for meters
        // REAPER TRACK format: TRACK\tindex\tname\tflags\tvol\tpan\tvu_peak_L\tvu_peak_R\t...
        // Field 6 = VU peak L (integer, centibels relative to 0 dBFS)
        for line in text.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.first() == Some(&"TRACK")
                && parts.len() > 7
                && let Ok(track_idx) = parts[1].parse::<usize>()
                && let Ok(peak_centibels) = parts[6].parse::<f32>()
            {
                // Convert centibels to linear (0.0-1.0 where 1.0 = 0 dBFS)
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

/// Batch control operations (Reset)
pub async fn batch_control(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    Json(payload): Json<BatchControlRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    let _member = config
        .find_member(&member_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;

    let member_index = config.member_index(&member_id).unwrap();
    let reaper_url = config.reaper_url.clone();
    let inputs = config.inputs.clone();

    drop(config);

    match payload.operation {
        BatchOperation::Reset => {
            // Reset all to 0dB, unmuted, centered pan
            for (i, _input) in inputs.iter().enumerate() {
                let track_index = i + 1;
                let vol = db_to_reaper_vol(0.0);

                // Set volume to 0dB
                let vol_url = reaper_api::set_send_vol(&reaper_url, track_index, member_index, vol);
                let _ = state.http_client.get(&vol_url).send().await;

                // Unmute
                let mute_url = reaper_api::set_send_mute(&reaper_url, track_index, member_index, 0);
                let _ = state.http_client.get(&mute_url).send().await;

                // Center pan (REAPER uses 0.0 for center, not 0.5)
                let pan_url = reaper_api::set_send_pan(&reaper_url, track_index, member_index, 0.0);
                let _ = state.http_client.get(&pan_url).send().await;
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
#[cfg(test)]
fn parse_send_mute(text: &str) -> Option<bool> {
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() == Some(&"SEND")
            && parts.len() >= 4
            && let Ok(val) = parts[3].parse::<i32>()
        {
            return Some(val != 0);
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

/// Parse a full REAPER SEND response for vol, mute, and pan in one call
/// Response format: SEND\ttrack\tsend\tMUTE_FLAG\tVOLUME\tPAN\tmode
fn parse_send_state(text: &str) -> Option<(f32, bool, f32)> {
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.first() == Some(&"SEND") && parts.len() >= 6 {
            let vol = parts[4].parse::<f32>().ok()?;
            let mute_flag = parts[3].parse::<i32>().ok()?;
            let pan = parts[5].parse::<f32>().ok()?;
            return Some((vol, mute_flag != 0, pan));
        }
    }
    None
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

/// Categorize a track by name
pub(crate) fn categorize_track(name: &str) -> (String, Option<String>, Option<String>) {
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

    // Get track name for debugging (owned String to avoid borrow issues)
    let track_name = config
        .inputs
        .get(track_index.saturating_sub(1))
        .map(|i| i.name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    drop(config);

    // Convert dB to REAPER volume (approximate)
    // REAPER uses 0-1 scale where 0.716 ≈ 0dB
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
    Json(payload): Json<SetPanRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    let member_index = config
        .member_index(&member_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;

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
    Json(payload): Json<SetMuteRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    let member_index = config
        .member_index(&member_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;

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
/// REAPER uses a logarithmic scale where:
/// - 0.0 = -inf dB
/// - 0.716 ≈ 0 dB (unity)
/// - 1.0 ≈ +6 dB
pub(crate) fn db_to_reaper_vol(db: f32) -> f32 {
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
pub(crate) fn reaper_vol_to_db(vol: f32) -> f32 {
    if vol <= 0.0 {
        -60.0
    } else {
        20.0 * (vol / 0.716).log10()
    }
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
pub async fn ws_mixer(
    ws: axum::extract::ws::WebSocketUpgrade,
    State(state): State<AppState>,
    Path(member_id): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state, member_id))
}

/// Handle a WebSocket connection for a member
async fn handle_ws(mut socket: axum::extract::ws::WebSocket, state: AppState, member_id: String) {
    use axum::extract::ws::Message;
    use iem_core::{ClientMsg, ServerMsg};

    tracing::info!(member_id = %member_id, "WebSocket connected");

    // Register this member as active (so poller queries their state)
    {
        let mut cache = state.mixer_cache.write().await;
        cache.active_members.insert(member_id.clone());
    }

    // Subscribe to broadcast channel
    let mut rx = state.event_tx.subscribe();

    // Send initial full state
    if let Ok(initial_state) = build_full_state(&state, &member_id).await {
        let json = serde_json::to_string(&initial_state).unwrap_or_default();
        let _ = socket.send(Message::Text(json.into())).await;
    }

    loop {
        tokio::select! {
            // Client → Server: process commands
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<ClientMsg>(&text) {
                            execute_command(&state, &member_id, cmd).await;
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
                        // send state/channel updates only to the relevant member
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

    // Cleanup: remove from active members
    let mut cache = state.mixer_cache.write().await;
    cache.active_members.remove(&member_id);
    cache.member_states.remove(&member_id);
}

/// Build full state message for initial WebSocket connection
async fn build_full_state(state: &AppState, member_id: &str) -> Result<iem_core::ServerMsg, ()> {
    let config = state.config.read().await;

    let member_index = config.member_index(member_id).ok_or(())?;
    let reaper_url = config.reaper_url.clone();

    let channels: Vec<iem_core::Channel> = config
        .inputs
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

    Ok(iem_core::ServerMsg::State {
        channels: result_channels,
        connected,
    })
}

/// Execute a client command by forwarding to REAPER
async fn execute_command(state: &AppState, member_id: &str, cmd: iem_core::ClientMsg) {
    let config = state.config.read().await;
    let member_index = match config.member_index(member_id) {
        Some(idx) => idx,
        None => return,
    };
    let reaper_url = config.reaper_url.clone();
    drop(config);

    match cmd {
        iem_core::ClientMsg::SetLevel {
            track_index,
            level_db,
        } => {
            let vol = db_to_reaper_vol(level_db);
            let url = reaper_api::set_send_vol(&reaper_url, track_index, member_index, vol);
            let _ = state.http_client.get(&url).send().await;
        }
        iem_core::ClientMsg::SetMute { track_index, muted } => {
            let mute_val: u8 = if muted { 1 } else { 0 };
            let url = reaper_api::set_send_mute(&reaper_url, track_index, member_index, mute_val);
            let _ = state.http_client.get(&url).send().await;
        }
        iem_core::ClientMsg::SetPan { track_index, pan } => {
            let reaper_pan = ui_pan_to_reaper(pan);
            let url = reaper_api::set_send_pan(&reaper_url, track_index, member_index, reaper_pan);
            let _ = state.http_client.get(&url).send().await;
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

    /// Build URL for querying tracks
    pub fn query_tracks(base_url: &str) -> String {
        format!("{}/_/NTRACK;TRACK", base_url)
    }

    /// Build URL for getting full send state (returns vol, mute, pan in one call)
    pub fn get_send_state(base_url: &str, track: usize, send: usize) -> String {
        format!("{}/_/GET/TRACK/{}/SEND/{}", base_url, track, send)
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
        let input = "SEND\t1\t2\t0\t0.716000\t0.000000\t24";
        let vol = parse_send_volume(input).unwrap();
        assert!((vol - 0.716).abs() < 0.001, "Expected 0.716, got {}", vol);
    }

    #[test]
    fn test_parse_send_mute_on() {
        // Flag at position 3 indicates mute state
        let input = "SEND\t1\t1\t1\t0.716000\t0.000000\t24";
        assert_eq!(parse_send_mute(input), Some(true));
    }

    #[test]
    fn test_parse_send_mute_off() {
        let input = "SEND\t1\t1\t0\t0.716000\t0.000000\t24";
        assert_eq!(parse_send_mute(input), Some(false));
    }

    #[test]
    fn test_parse_send_pan_center() {
        // Pan is at position 5 (0.0 = center in REAPER)
        let input = "SEND\t1\t1\t0\t0.716000\t0.000000\t24";
        assert_eq!(parse_send_pan(input), Some(0.0));
    }

    #[test]
    fn test_parse_send_pan_left() {
        let input = "SEND\t1\t1\t0\t0.716000\t-1.000000\t24";
        assert_eq!(parse_send_pan(input), Some(-1.0));
    }

    #[test]
    fn test_parse_send_pan_right() {
        let input = "SEND\t1\t1\t0\t0.716000\t1.000000\t24";
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
        let input = "SEND\t1\t0\t0\t0.716000\t0.000000\t24";
        let (vol, mute, pan) = parse_send_state(input).unwrap();
        assert!((vol - 0.716).abs() < 0.001);
        assert!(!mute);
        assert!((pan - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_send_state_muted() {
        let input = "SEND\t1\t0\t1\t0.300000\t-0.500000\t24";
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
}
