//! API client for server communication

use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthState, get_token};

/// Base URL for API calls (same origin)
const API_BASE: &str = "/api";

/// Member info from server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberInfo {
    pub id: String,
    pub name: String,
}

/// Re-export Channel from iem_core to avoid duplicate type
pub use iem_core::Channel;

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
