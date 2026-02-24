//! API client for server communication

use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::auth::{AuthState, get_token};

/// Base URL for API calls (same origin)
const API_BASE: &str = "/api";

/// Member info from server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberInfo {
    pub id: String,
    pub name: String,
}

/// Channel in a mixer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub track_index: usize,
    pub name: String,
    pub level_db: f32,
    pub pan: f32,
    pub muted: bool,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub stereo_pair: Option<String>,
    #[serde(default)]
    pub stereo_side: Option<String>,
}

/// Full mixer state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerState {
    pub member_id: String,
    pub channels: Vec<Channel>,
}

/// Poll response with meters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollResponse {
    pub member_id: String,
    pub channels: Vec<Channel>,
    pub meters: HashMap<usize, f32>,
    pub connected: bool,
}

/// Batch operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchOperation {
    MoreMe,
    Reset,
}

/// Login response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub member: String,
    pub engineer: bool,
    pub expires_in: u64,
}

/// API error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

/// Get list of band members
pub async fn get_members() -> Result<Vec<MemberInfo>, String> {
    let resp = Request::get(&format!("{}/members", API_BASE))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.ok() {
        resp.json().await.map_err(|e| format!("Parse error: {}", e))
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Login with PIN
pub async fn login(member: &str, pin: &str) -> Result<AuthState, String> {
    #[derive(Serialize)]
    struct LoginRequest<'a> {
        member: &'a str,
        pin: &'a str,
    }

    let resp = Request::post(&format!("{}/auth", API_BASE))
        .json(&LoginRequest { member, pin })
        .map_err(|e| format!("Request error: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.ok() {
        let login_resp: LoginResponse = resp
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;

        Ok(AuthState {
            token: login_resp.token,
            member: login_resp.member,
            engineer: login_resp.engineer,
        })
    } else if resp.status() == 401 {
        Err("Invalid PIN".to_string())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Get mixer state for a member
pub async fn get_mixer_state(member_id: &str) -> Result<MixerState, String> {
    let mut req = Request::get(&format!("{}/mixer/{}", API_BASE, member_id));

    if let Some(token) = get_token() {
        req = req.header("Authorization", &format!("Bearer {}", token));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.ok() {
        resp.json().await.map_err(|e| format!("Parse error: {}", e))
    } else if resp.status() == 401 {
        Err("Unauthorized".to_string())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Set send level for a channel
pub async fn set_send_level(
    member_id: &str,
    track_index: usize,
    level_db: f32,
) -> Result<(), String> {
    #[derive(Serialize)]
    struct LevelRequest {
        level_db: f32,
    }

    let mut req = Request::post(&format!(
        "{}/mixer/{}/track/{}/level",
        API_BASE, member_id, track_index
    ));

    if let Some(token) = get_token() {
        req = req.header("Authorization", &format!("Bearer {}", token));
    }

    let resp = req
        .json(&LevelRequest { level_db })
        .map_err(|e| format!("Request error: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.ok() {
        Ok(())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Set send pan for a channel
pub async fn set_send_pan(member_id: &str, track_index: usize, pan: f32) -> Result<(), String> {
    #[derive(Serialize)]
    struct PanRequest {
        pan: f32,
    }

    let mut req = Request::post(&format!(
        "{}/mixer/{}/track/{}/pan",
        API_BASE, member_id, track_index
    ));

    if let Some(token) = get_token() {
        req = req.header("Authorization", &format!("Bearer {}", token));
    }

    let resp = req
        .json(&PanRequest { pan })
        .map_err(|e| format!("Request error: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.ok() {
        Ok(())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Set send mute for a channel
pub async fn set_send_mute(member_id: &str, track_index: usize, muted: bool) -> Result<(), String> {
    #[derive(Serialize)]
    struct MuteRequest {
        muted: bool,
    }

    let mut req = Request::post(&format!(
        "{}/mixer/{}/track/{}/mute",
        API_BASE, member_id, track_index
    ));

    if let Some(token) = get_token() {
        req = req.header("Authorization", &format!("Bearer {}", token));
    }

    let resp = req
        .json(&MuteRequest { muted })
        .map_err(|e| format!("Request error: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.ok() {
        Ok(())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Poll mixer state with meters (for frequent updates)
pub async fn poll_mixer_state(member_id: &str) -> Result<PollResponse, String> {
    let mut req = Request::get(&format!("{}/mixer/{}/poll", API_BASE, member_id));

    if let Some(token) = get_token() {
        req = req.header("Authorization", &format!("Bearer {}", token));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.ok() {
        resp.json().await.map_err(|e| format!("Parse error: {}", e))
    } else if resp.status() == 401 {
        Err("Unauthorized".to_string())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Batch operations (+Me, Reset)
pub async fn batch_control(member_id: &str, operation: BatchOperation) -> Result<(), String> {
    #[derive(Serialize)]
    struct BatchRequest {
        operation: BatchOperation,
    }

    let mut req = Request::post(&format!("{}/mixer/{}/batch", API_BASE, member_id));

    if let Some(token) = get_token() {
        req = req.header("Authorization", &format!("Bearer {}", token));
    }

    let resp = req
        .json(&BatchRequest { operation })
        .map_err(|e| format!("Request error: {}", e))?
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if resp.ok() {
        Ok(())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}
