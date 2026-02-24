//! Client-side authentication helpers

use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

const TOKEN_KEY: &str = "iem_token";
const MEMBER_KEY: &str = "iem_member";

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

/// Clear auth state
pub fn clear_auth() {
    LocalStorage::delete(TOKEN_KEY);
    LocalStorage::delete(MEMBER_KEY);
}

/// Check if user is authenticated
pub fn is_authenticated() -> bool {
    get_auth().is_some()
}

/// Check if user can access a specific member's mixer
pub fn can_access_member(member: &str) -> bool {
    match get_auth() {
        Some(auth) => auth.engineer || auth.member == member,
        None => false,
    }
}

/// Get current member ID (if authenticated)
pub fn current_member() -> Option<String> {
    get_auth().map(|a| a.member)
}

/// Get auth token for API calls
pub fn get_token() -> Option<String> {
    get_auth().map(|a| a.token)
}
