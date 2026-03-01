//! Client-side authentication helpers

use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

const TOKEN_KEY: &str = "iem_token";

/// Stored auth state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthState {
    pub token: String,
    pub member: String,
    pub engineer: bool,
}

/// Get current auth state from localStorage
pub fn get_auth() -> Option<AuthState> {
    LocalStorage::get::<AuthState>(TOKEN_KEY).ok()
}

/// Save auth state to localStorage
pub fn save_auth(state: &AuthState) {
    let _ = LocalStorage::set(TOKEN_KEY, state);
}

/// Get auth token for API calls
pub fn get_token() -> Option<String> {
    get_auth().map(|a| a.token)
}
