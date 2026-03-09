//! Authentication middleware and JWT handling

use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use iem_core::{ApiError, AuthClaims};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

use crate::AppState;

/// Constant-time string comparison to prevent timing attacks on PIN verification.
/// Returns true if both strings are equal, false otherwise.
/// The comparison takes the same amount of time regardless of where strings differ.
#[inline]
fn constant_time_eq(a: &str, b: &str) -> bool {
    // Length check is not constant-time, but PIN length is fixed (4 digits)
    // and the length itself is not secret
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Login request payload
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub member: String,
    pub pin: String,
}

/// Change PIN request payload
#[derive(Debug, Deserialize)]
pub struct ChangePinRequest {
    pub old_pin: String,
    pub new_pin: String,
}

/// Login response with JWT token
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub member: String,
    pub engineer: bool,
    pub expires_in: u64,
}

/// Token expiration for members (24 hours)
const MEMBER_TOKEN_EXPIRY_SECS: u64 = 24 * 60 * 60;
/// Token expiration for engineers (4 hours - shorter for elevated access)
const ENGINEER_TOKEN_EXPIRY_SECS: u64 = 4 * 60 * 60;

/// Handle login and return JWT
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    let pin_store = state.pin_store.read().await;

    // Helper: check if member exists in discovered members
    let member_exists = {
        let discovered = state.discovered_members.read().await;
        discovered.iter().any(|m| m.id() == req.member)
    };

    // 1. Check engineer PIN (config or default "1177")
    // Uses constant-time comparison to prevent timing attacks
    let eng_pin = config
        .engineer_pin
        .as_deref()
        .unwrap_or(iem_core::config::DEFAULT_ENGINEER_PIN);
    if constant_time_eq(&req.pin, eng_pin) {
        // Verify member exists (engineer still needs a valid member target)
        if !req.member.is_empty() && !member_exists {
            return Err((StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))));
        }
        return issue_token(&config, "engineer", true);
    }

    // 2. Check PinStore for custom PIN (member changed their PIN)
    // Uses constant-time comparison to prevent timing attacks
    if let Some(custom_pin) = pin_store.get_pin(&req.member) {
        if !constant_time_eq(&req.pin, &custom_pin) {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiError::new("INVALID_PIN", "Invalid PIN")),
            ));
        }
        if !member_exists {
            return Err((StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))));
        }
        return issue_token(&config, &req.member, false);
    }

    // 3. Fall through to config validation (config pins + default "7711")
    drop(pin_store);

    // First verify member exists in discovered members (REAPER source of truth)
    if !member_exists {
        return Err((StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))));
    }

    // Check default member PIN ("7711")
    if constant_time_eq(&req.pin, iem_core::config::DEFAULT_MEMBER_PIN) {
        return issue_token(&config, &req.member, false);
    }

    // Invalid PIN
    Err((
        StatusCode::UNAUTHORIZED,
        Json(ApiError::new("INVALID_PIN", "Invalid PIN")),
    ))
}

/// Issue a JWT token for the given member/engineer
fn issue_token(
    config: &iem_core::Config,
    member_id: &str,
    engineer: bool,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ApiError>)> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Use shorter expiry for engineer tokens (elevated access)
    let expiry_secs = if engineer {
        ENGINEER_TOKEN_EXPIRY_SECS
    } else {
        MEMBER_TOKEN_EXPIRY_SECS
    };

    let claims = AuthClaims {
        sub: member_id.to_string(),
        engineer,
        exp: now + expiry_secs,
        iat: now,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("TOKEN_ERROR", "Failed to create token")),
        )
    })?;

    Ok(Json(LoginResponse {
        token,
        member: member_id.to_string(),
        engineer,
        expires_in: expiry_secs,
    }))
}

/// Change PIN for the authenticated member
pub async fn change_pin(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ChangePinRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    // Extract member from JWT
    let claims = extract_claims_from_header(&headers, &config.jwt_secret)?;

    if claims.engineer {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiError::new(
                "FORBIDDEN",
                "Engineers cannot change PIN via this endpoint",
            )),
        ));
    }

    // Validate new_pin is exactly 4 digits
    if req.new_pin.len() != 4 || !req.new_pin.chars().all(|c| c.is_ascii_digit()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                "INVALID_FORMAT",
                "PIN must be exactly 4 digits",
            )),
        ));
    }

    // Verify old_pin is correct (using constant-time comparison)
    let pin_store = state.pin_store.read().await;
    let old_pin_valid = if let Some(custom_pin) = pin_store.get_pin(&claims.sub) {
        constant_time_eq(&req.old_pin, &custom_pin)
    } else if let Some(config_pin) = config.pins.get(&claims.sub) {
        constant_time_eq(&req.old_pin, config_pin)
    } else {
        constant_time_eq(&req.old_pin, iem_core::config::DEFAULT_MEMBER_PIN)
    };
    drop(pin_store);

    if !old_pin_valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new("INVALID_PIN", "Current PIN is incorrect")),
        ));
    }

    // Save new PIN
    let mut pin_store = state.pin_store.write().await;
    pin_store.set_pin(&claims.sub, &req.new_pin).map_err(|e| {
        tracing::error!("Failed to save PIN: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::new("IO_ERROR", "Failed to save PIN")),
        )
    })?;

    tracing::info!("PIN changed for member: {}", claims.sub);
    Ok(StatusCode::OK)
}

/// Extract claims from Authorization header
fn extract_claims_from_header(
    headers: &axum::http::HeaderMap,
    jwt_secret: &str,
) -> Result<AuthClaims, (StatusCode, Json<ApiError>)> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => {
            return Err((StatusCode::UNAUTHORIZED, Json(ApiError::unauthorized())));
        }
    };

    extract_claims(token, jwt_secret)
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, Json(ApiError::unauthorized())))
}

/// Verify JWT and return claims
pub async fn verify_token(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    // Extract token from Authorization header
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    let token = match auth_header {
        Some(header) if header.starts_with("Bearer ") => &header[7..],
        _ => {
            return Err((StatusCode::UNAUTHORIZED, Json(ApiError::unauthorized())));
        }
    };

    // Validate token
    let token_data = decode::<AuthClaims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| (StatusCode::UNAUTHORIZED, Json(ApiError::unauthorized())))?;

    // Check expiration
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if token_data.claims.exp < now {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError::new("TOKEN_EXPIRED", "Token has expired")),
        ));
    }

    // Continue with request
    drop(config);
    Ok(next.run(req).await)
}

/// Extract claims from a valid JWT token
pub fn extract_claims(token: &str, secret: &str) -> Option<AuthClaims> {
    decode::<AuthClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|data| data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use iem_core::Config;
    use std::collections::HashMap;

    fn test_config() -> Config {
        Config {
            reaper_url: "http://localhost:8080".to_string(),
            port: 80,
            jwt_secret: "test_secret_for_auth_testing".to_string(),
            members: vec![],
            inputs: vec![],
            dante_outputs: HashMap::new(),
            tls: false,
            https_port: 443,
            tls_cert: "cert.pem".to_string(),
            tls_key: "key.pem".to_string(),
            https_domain: None,
            pins: HashMap::new(),
            engineer_pin: Some("1177".to_string()),
            local_public_ip: None,
        }
    }

    #[test]
    fn test_member_token_expiry_24h() {
        let config = test_config();
        let result = issue_token(&config, "petka", false);

        assert!(result.is_ok());
        let response = result.unwrap().0;

        // Member tokens should have 24h expiry
        assert!(!response.engineer);
        assert_eq!(response.expires_in, 24 * 60 * 60);

        // Verify the token claims
        let claims = extract_claims(&response.token, &config.jwt_secret).unwrap();
        assert_eq!(claims.sub, "petka");
        assert!(!claims.engineer);

        // Verify expiry is approximately 24h from now (within 5 sec tolerance)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expected_exp = now + 24 * 60 * 60;
        assert!((claims.exp as i64 - expected_exp as i64).abs() < 5);
    }

    #[test]
    fn test_engineer_token_expiry_4h() {
        let config = test_config();
        let result = issue_token(&config, "engineer", true);

        assert!(result.is_ok());
        let response = result.unwrap().0;

        // Engineer tokens should have 4h expiry (shorter for elevated access)
        assert!(response.engineer);
        assert_eq!(response.expires_in, 4 * 60 * 60);

        // Verify the token claims
        let claims = extract_claims(&response.token, &config.jwt_secret).unwrap();
        assert_eq!(claims.sub, "engineer");
        assert!(claims.engineer);

        // Verify expiry is approximately 4h from now (within 5 sec tolerance)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expected_exp = now + 4 * 60 * 60;
        assert!((claims.exp as i64 - expected_exp as i64).abs() < 5);
    }

    #[test]
    fn test_token_expiry_constants() {
        // Verify expiry constants are correct
        assert_eq!(MEMBER_TOKEN_EXPIRY_SECS, 24 * 60 * 60); // 24 hours
        assert_eq!(ENGINEER_TOKEN_EXPIRY_SECS, 4 * 60 * 60); // 4 hours

        // Engineer expiry should be shorter than member expiry
        assert!(ENGINEER_TOKEN_EXPIRY_SECS < MEMBER_TOKEN_EXPIRY_SECS);
    }
}
