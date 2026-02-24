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

use crate::AppState;

/// Login request payload
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub member: String,
    pub pin: String,
}

/// Login response with JWT token
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub member: String,
    pub engineer: bool,
    pub expires_in: u64,
}

/// Token expiration time (24 hours)
const TOKEN_EXPIRY_SECS: u64 = 24 * 60 * 60;

/// Handle login and return JWT
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;

    // Validate PIN
    let validation = config.validate_pin(&req.member, &req.pin);
    match validation {
        iem_core::config::PinValidation::Invalid => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiError::new("INVALID_PIN", "Invalid PIN")),
            ));
        }
        iem_core::config::PinValidation::Member(ref member_id) => {
            // Check member exists
            if config.find_member(member_id).is_none() {
                return Err((StatusCode::NOT_FOUND, Json(ApiError::not_found("Member"))));
            }
        }
        iem_core::config::PinValidation::Engineer => {
            // Engineer PIN is always valid
        }
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let is_engineer = matches!(validation, iem_core::config::PinValidation::Engineer);
    let member_id = match &validation {
        iem_core::config::PinValidation::Member(id) => id.clone(),
        iem_core::config::PinValidation::Engineer => "engineer".to_string(),
        _ => unreachable!(),
    };

    let claims = AuthClaims {
        sub: member_id.clone(),
        engineer: is_engineer,
        exp: now + TOKEN_EXPIRY_SECS,
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
        member: member_id,
        engineer: is_engineer,
        expires_in: TOKEN_EXPIRY_SECS,
    }))
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
