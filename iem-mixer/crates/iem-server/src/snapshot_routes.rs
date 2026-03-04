//! REST API routes for snapshot management

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Router,
};
use iem_core::{ApiError, MixSnapshot};
use serde::{Deserialize, Serialize};

use crate::{snapshot_store::SnapshotStore, AppState};

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
) -> Result<Json<Vec<SnapshotInfo>>, (StatusCode, Json<ApiError>)> {
    // Verify member exists
    let config = state.config.read().await;
    if config.find_member(&member).is_none() {
        return Err((StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))));
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
    Json(req): Json<CreateSnapshotRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    // Verify member exists
    let config = state.config.read().await;
    if config.find_member(&member).is_none() {
        return Err((StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))));
    }
    drop(config);

    // Get current mixer state from cache
    let cache = state.mixer_cache.read().await;
    let channels = cache.member_states.get(&member).cloned().unwrap_or_default();
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

    state.snapshot_store.save_snapshot(&member, snapshot).map_err(|e| {
        tracing::error!("Failed to save snapshot: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("IO_ERROR", "Failed to save snapshot")),
        )
    })?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "timestamp": timestamp }))))
}

/// Delete a snapshot
async fn delete_snapshot(
    State(state): State<AppState>,
    Path((member, timestamp)): Path<(String, i64)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
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
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found("Snapshot")),
        ))
    }
}

/// Pin a snapshot with a label
async fn pin_snapshot(
    State(state): State<AppState>,
    Path((member, timestamp)): Path<(String, i64)>,
    Json(req): Json<PinRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
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
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found("Snapshot")),
        ))
    }
}

/// Unpin a snapshot
async fn unpin_snapshot(
    State(state): State<AppState>,
    Path((member, timestamp)): Path<(String, i64)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
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
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found("Snapshot")),
        ))
    }
}

/// Restore a snapshot by applying all channel values to REAPER
async fn restore_snapshot(
    State(state): State<AppState>,
    Path((member, timestamp)): Path<(String, i64)>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    // Get the snapshot
    let snapshot = state
        .snapshot_store
        .get_snapshot(&member, timestamp)
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(ApiError::not_found("Snapshot"))))?;

    // Get config for REAPER URL and member info
    let config = state.config.read().await;
    let member_info = config.find_member(&member).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(ApiError::not_found("Member")))
    })?;
    let member_index = member_info.index;
    let reaper_url = config.reaper_url.clone();
    drop(config);

    // Build batch commands for all channels
    let mut operations = Vec::new();
    for (track_index, ch) in &snapshot.channels {
        operations.push(iem_core::BatchOperation {
            track_index: *track_index,
            level: Some(ch.vol),
            pan: Some(ch.pan),
            mute: Some(ch.mute),
        });
    }

    // Execute in parallel using the same pattern as batch_control
    let mut handles = Vec::new();
    for op in operations {
        let url_base = reaper_url.clone();
        let client = state.http_client.clone();
        let send_index = member_index;
        let track_index = op.track_index;

        handles.push(tokio::spawn(async move {
            let mut results = Vec::new();

            // Set level
            if let Some(level) = op.level {
                let url = format!(
                    "{}/_/SET/TRACK/{}/SEND/{}/VOL/{:.6}",
                    url_base, track_index, send_index, level
                );
                if let Err(e) = client.get(&url).send().await {
                    tracing::error!("Restore level failed: {}", e);
                }
                results.push(("level", track_index));
            }

            // Set pan
            if let Some(pan) = op.pan {
                let url = format!(
                    "{}/_/SET/TRACK/{}/SEND/{}/PAN/{:.6}",
                    url_base, track_index, send_index, pan
                );
                if let Err(e) = client.get(&url).send().await {
                    tracing::error!("Restore pan failed: {}", e);
                }
                results.push(("pan", track_index));
            }

            // Set mute
            if let Some(mute) = op.mute {
                let mute_val = if mute { 1 } else { 0 };
                let url = format!(
                    "{}/_/SET/TRACK/{}/SEND/{}/MUTE/{}",
                    url_base, track_index, send_index, mute_val
                );
                if let Err(e) = client.get(&url).send().await {
                    tracing::error!("Restore mute failed: {}", e);
                }
                results.push(("mute", track_index));
            }

            results
        }));
    }

    // Wait for all commands to complete
    let mut total_ops = 0;
    for handle in handles {
        if let Ok(results) = handle.await {
            total_ops += results.len();
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
