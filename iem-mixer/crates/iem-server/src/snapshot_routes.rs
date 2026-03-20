//! REST API routes for snapshot management

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
};
use iem_core::{ApiError, MixSnapshot};
use serde::{Deserialize, Serialize};

use crate::{AppState, snapshot_store::SnapshotStore};

/// Snapshot info for list response
#[derive(Serialize)]
pub struct SnapshotInfo {
    pub timestamp: i64,
    pub label: String,
    pub pinned: bool,
    pub channel_count: usize,
}

impl From<&MixSnapshot> for SnapshotInfo {
    fn from(s: &MixSnapshot) -> Self {
        Self {
            timestamp: s.timestamp,
            label: s.label.clone(),
            pinned: s.pinned,
            channel_count: s.channels.len(),
        }
    }
}

/// Request to create a manual snapshot
#[derive(Deserialize)]
pub struct CreateSnapshotRequest {
    pub label: Option<String>,
}

/// Request to pin a snapshot
#[derive(Deserialize)]
pub struct PinRequest {
    pub label: String,
}

/// Snapshot routes
pub fn snapshot_routes() -> Router<AppState> {
    Router::new()
        // List snapshots
        .route("/api/snapshots/{member}", get(list_snapshots))
        // Create manual snapshot
        .route("/api/snapshots/{member}", post(create_snapshot))
        // Delete snapshot
        .route(
            "/api/snapshots/{member}/{timestamp}",
            delete(delete_snapshot),
        )
        // Pin snapshot
        .route(
            "/api/snapshots/{member}/{timestamp}/pin",
            post(pin_snapshot),
        )
        // Unpin snapshot
        .route(
            "/api/snapshots/{member}/{timestamp}/unpin",
            post(unpin_snapshot),
        )
        // Restore snapshot
        .route(
            "/api/snapshots/{member}/{timestamp}/restore",
            post(restore_snapshot),
        )
}

/// List all snapshots for a member (newest first)
async fn list_snapshots(
    State(state): State<AppState>,
    Path(member): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<SnapshotInfo>>, (StatusCode, Json<ApiError>)> {
    // Verify member exists and auth
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member, &config.jwt_secret)?;
    {
        let discovered = state.discovered_members.read().await;
        if !discovered.iter().any(|m| m.id() == member) {
            return Err((StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))));
        }
    }
    drop(config);

    let snapshots = state.snapshot_store.list(&member);
    let info: Vec<SnapshotInfo> = snapshots.iter().map(SnapshotInfo::from).collect();
    Ok(Json(info))
}

/// Create a manual snapshot
async fn create_snapshot(
    State(state): State<AppState>,
    Path(member): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateSnapshotRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    // Verify member exists and auth
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member, &config.jwt_secret)?;
    {
        let discovered = state.discovered_members.read().await;
        if !discovered.iter().any(|m| m.id() == member) {
            return Err((StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))));
        }
    }
    drop(config);

    // Get current mixer state from cache
    let cache = state.mixer_cache.read().await;
    let channels = cache
        .member_states
        .get(&member)
        .cloned()
        .unwrap_or_default();
    drop(cache);

    if channels.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "NO_STATE",
                "No mixer state available to snapshot",
            )),
        ));
    }

    // Create snapshot
    let channel_map = SnapshotStore::channels_from_state(&channels);
    let label = req.label.unwrap_or_else(|| "manual".to_string());
    let snapshot = MixSnapshot::new_manual(label, channel_map);
    let timestamp = snapshot.timestamp;

    state
        .snapshot_store
        .save_snapshot(&member, snapshot)
        .map_err(|e| {
            tracing::error!("Failed to save snapshot: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("IO_ERROR", "Failed to save snapshot")),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "timestamp": timestamp })),
    ))
}

/// Delete a snapshot
async fn delete_snapshot(
    State(state): State<AppState>,
    Path((member, timestamp)): Path<(String, i64)>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member, &config.jwt_secret)?;
    drop(config);
    let deleted = state
        .snapshot_store
        .delete_snapshot(&member, timestamp)
        .map_err(|e| {
            tracing::error!("Failed to delete snapshot: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("IO_ERROR", "Failed to delete snapshot")),
            )
        })?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((StatusCode::NOT_FOUND, Json(ApiError::not_found("Snapshot"))))
    }
}

/// Pin a snapshot with a label
async fn pin_snapshot(
    State(state): State<AppState>,
    Path((member, timestamp)): Path<(String, i64)>,
    headers: axum::http::HeaderMap,
    Json(req): Json<PinRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member, &config.jwt_secret)?;
    drop(config);
    let pinned = state
        .snapshot_store
        .pin_snapshot(&member, timestamp, req.label)
        .map_err(|e| {
            tracing::error!("Failed to pin snapshot: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("IO_ERROR", "Failed to pin snapshot")),
            )
        })?;

    if pinned {
        Ok(StatusCode::OK)
    } else {
        Err((StatusCode::NOT_FOUND, Json(ApiError::not_found("Snapshot"))))
    }
}

/// Unpin a snapshot
async fn unpin_snapshot(
    State(state): State<AppState>,
    Path((member, timestamp)): Path<(String, i64)>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member, &config.jwt_secret)?;
    drop(config);
    let unpinned = state
        .snapshot_store
        .unpin_snapshot(&member, timestamp)
        .map_err(|e| {
            tracing::error!("Failed to unpin snapshot: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new("IO_ERROR", "Failed to unpin snapshot")),
            )
        })?;

    if unpinned {
        Ok(StatusCode::OK)
    } else {
        Err((StatusCode::NOT_FOUND, Json(ApiError::not_found("Snapshot"))))
    }
}

/// Restore a snapshot by applying all channel values to REAPER
async fn restore_snapshot(
    State(state): State<AppState>,
    Path((member, timestamp)): Path<(String, i64)>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member, &config.jwt_secret)?;
    drop(config);
    // Get the snapshot
    let snapshot = state
        .snapshot_store
        .get_snapshot(&member, timestamp)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Snapshot"))))?;

    // Get member index from discovered members and REAPER URL from config
    let discovered = state.discovered_members.read().await;
    let member_index = discovered
        .iter()
        .find(|m| m.id() == member)
        .map(|m| m.send_index)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))))?;
    drop(discovered);
    let config = state.config.read().await;
    let reaper_url = config.reaper_url.clone();
    drop(config);

    // Execute in parallel: send level, pan, mute for each channel
    let mut handles = Vec::new();
    for (track_index, ch) in &snapshot.channels {
        let url_base = reaper_url.clone();
        let client = state.http_client.clone();
        let send_index = member_index;
        let track_idx = *track_index;
        let vol = ch.vol;
        let pan = ch.pan;
        let mute = ch.mute;

        handles.push(tokio::spawn(async move {
            let mut op_count = 0;

            // Set level
            let url = format!(
                "{}/_/SET/TRACK/{}/SEND/{}/VOL/{:.6}",
                url_base, track_idx, send_index, vol
            );
            if let Err(e) = client.get(&url).send().await {
                tracing::error!("Restore level failed: {}", e);
            }
            op_count += 1;

            // Set pan
            let url = format!(
                "{}/_/SET/TRACK/{}/SEND/{}/PAN/{:.6}",
                url_base, track_idx, send_index, pan
            );
            if let Err(e) = client.get(&url).send().await {
                tracing::error!("Restore pan failed: {}", e);
            }
            op_count += 1;

            // Set mute
            let mute_val = if mute { 1 } else { 0 };
            let url = format!(
                "{}/_/SET/TRACK/{}/SEND/{}/MUTE/{}",
                url_base, track_idx, send_index, mute_val
            );
            if let Err(e) = client.get(&url).send().await {
                tracing::error!("Restore mute failed: {}", e);
            }
            op_count += 1;

            op_count
        }));
    }

    // Wait for all commands to complete
    let mut total_ops = 0;
    for handle in handles {
        if let Ok(count) = handle.await {
            total_ops += count;
        }
    }

    tracing::info!(
        member = %member,
        timestamp = timestamp,
        operations = total_ops,
        "Restored snapshot"
    );

    Ok(Json(serde_json::json!({
        "restored": true,
        "channels": snapshot.channels.len(),
        "operations": total_ops
    })))
}
