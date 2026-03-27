//! API and static file routes

use axum::{
    Json, Router,
    body::Body,
    extract::Path,
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{any, get, post, put},
};
use serde::{Deserialize, Serialize};

use crate::{AppState, Assets, auth, preset_routes, proxy, snapshot_routes};
use axum::extract::State;
use rust_embed::RustEmbed;

/// Version information for deployment verification
#[derive(Serialize)]
pub struct VersionInfo {
    pub version: &'static str,
    pub git_hash: &'static str,
    pub branch: &'static str,
    pub build_time: &'static str,
    pub deployed_at: String,
    pub full_version: String,
}

/// Get build version info - used by CI to verify correct version is deployed
async fn get_version() -> Json<VersionInfo> {
    Json(VersionInfo {
        version: iem_core::VERSION,
        git_hash: iem_core::git_hash(),
        branch: iem_core::git_branch(),
        build_time: iem_core::build_time(),
        deployed_at: iem_core::deployed_at(),
        full_version: iem_core::full_version(),
    })
}

/// API routes
///
/// Auth middleware is NOT enforced on routes yet — the frontend needs
/// to send Authorization headers and handle 401 responses before we
/// can wire the middleware. Auth infrastructure (login, JWT, change-pin)
/// is available for opt-in use.
pub fn api_routes(_state: AppState) -> Router<AppState> {
    Router::new()
        // Version endpoint (used by CI for deployment verification)
        .route("/api/version", get(get_version))
        // Auth login (returns JWT)
        .route("/api/auth", post(auth::login))
        // Member list (needed for landing page)
        .route("/api/members", get(get_members))
        // Change PIN (should be authenticated — TODO: wire auth)
        .route("/api/auth/change-pin", post(auth::change_pin))
        // Mixer state
        .route("/api/mixer/{member_id}", get(proxy::get_mixer_state))
        // Polling endpoint (optimized for frequent calls)
        .route("/api/mixer/{member_id}/poll", get(proxy::poll_mixer_state))
        // Batch operations (Reset)
        .route("/api/mixer/{member_id}/batch", post(proxy::batch_control))
        // Mixer controls
        .route(
            "/api/mixer/{member_id}/track/{track_index}/level",
            post(proxy::set_send_level),
        )
        .route(
            "/api/mixer/{member_id}/track/{track_index}/pan",
            post(proxy::set_send_pan),
        )
        .route(
            "/api/mixer/{member_id}/track/{track_index}/mute",
            post(proxy::set_send_mute),
        )
        // Network mode detection (local LAN vs remote internet)
        .route("/api/network-mode", get(get_network_mode))
        // Channel customization (pin/hide preferences)
        .route(
            "/api/mixer/{member_id}/customization",
            get(get_customization),
        )
        .route(
            "/api/mixer/{member_id}/customization",
            put(put_customization),
        )
        // Elevated member access (mix channels for non-engineer members)
        .route(
            "/api/members/{member_id}/elevated",
            get(get_elevated).put(put_elevated),
        )
        // Raw REAPER proxy
        .route("/api/reaper/{*path}", any(reaper_proxy))
        // Audio WebSocket (engineer-only audio streaming) — must be before /ws/{member_id}
        .route("/ws/audio", get(ws_audio_handler))
        // Audio diagnostics (engineer-only)
        .route("/api/audio/diagnostics", get(audio_diagnostics_handler))
        // WebSocket
        .route("/ws/{member_id}", get(proxy::ws_mixer))
        // Snapshot routes
        .merge(snapshot_routes::snapshot_routes())
        // Preset routes
        .merge(preset_routes::preset_routes())
}

/// Get list of band members (discovered from REAPER)
async fn get_members(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl IntoResponse {
    let discovered = state.discovered_members.read().await;
    let members: Vec<MemberInfo> = discovered
        .iter()
        .map(|m| MemberInfo {
            id: m.id(),
            name: m.name.clone(),
        })
        .collect();
    axum::Json(members)
}

#[derive(serde::Serialize)]
struct MemberInfo {
    id: String,
    name: String,
}

/// Detect network mode from request headers.
///
/// Since all traffic goes through Cloudflare Tunnel (iem.newlevel.media),
/// CF-Connecting-IP is always a public IP. We compare it against the
/// configured `local_public_ip` (the church network's public IP).
/// Match = on church WiFi, different = remote.
/// No proxy headers = direct connection = local.
///
/// Used by both the HTTP endpoint and WebSocket handler.
pub fn detect_network_mode(
    headers: &axum::http::HeaderMap,
    local_public_ip: &Option<String>,
) -> String {
    // Extract client IP from Cloudflare headers
    let client_ip = headers
        .get("cf-connecting-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim().to_string());

    match (&client_ip, local_public_ip) {
        // If we have both client IP and configured local IP, compare them
        (Some(client), Some(local)) => {
            if client == local {
                "local".to_string()
            } else {
                "remote".to_string()
            }
        }
        // No proxy headers = direct connection (not through Cloudflare) = local
        (None, _) => "local".to_string(),
        // No local_public_ip configured = fall back to private IP check
        (Some(ip), None) => {
            if is_private_ip(ip) {
                "local".to_string()
            } else {
                "remote".to_string()
            }
        }
    }
}

/// HTTP endpoint for network mode detection (kept for backwards compatibility)
async fn get_network_mode(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let config = state.config.read().await;
    let local_ip = config.local_public_ip.clone();
    drop(config);

    let mode = detect_network_mode(&headers, &local_ip);

    Json(NetworkModeResponse { mode })
}

#[derive(Serialize)]
struct NetworkModeResponse {
    mode: String,
}

/// Check if an IP address is in a private/local range
fn is_private_ip(ip: &str) -> bool {
    // Parse the IP and check against private ranges
    if let Ok(addr) = ip.parse::<std::net::IpAddr>() {
        match addr {
            std::net::IpAddr::V4(v4) => {
                v4.is_private()         // 10.x, 172.16-31.x, 192.168.x
                    || v4.is_loopback() // 127.x
                    || v4.is_link_local() // 169.254.x
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback() // ::1
            }
        }
    } else {
        false
    }
}

/// Get channel customization for a member
async fn get_customization(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(member_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<iem_core::Customization>, (StatusCode, Json<iem_core::ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member_id, &config.jwt_secret)?;
    drop(config);
    let cust = state.customization_store.load(&member_id);
    Ok(Json(cust))
}

/// Update channel customization for a member
#[derive(Deserialize)]
struct CustomizationPayload {
    pinned: Vec<usize>,
    hidden: Vec<usize>,
}

async fn put_customization(
    axum::extract::State(state): axum::extract::State<AppState>,
    Path(member_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<CustomizationPayload>,
) -> Result<(StatusCode, Json<iem_core::Customization>), (StatusCode, Json<iem_core::ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member_id, &config.jwt_secret)?;
    drop(config);
    let cust = iem_core::Customization {
        pinned: payload.pinned,
        hidden: payload.hidden,
    };
    match state.customization_store.save(&member_id, &cust) {
        Ok(()) => Ok((StatusCode::OK, Json(cust))),
        Err(e) => {
            tracing::error!("Failed to save customization: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(iem_core::ApiError::new(
                    "IO_ERROR",
                    "Failed to save customization",
                )),
            ))
        }
    }
}

/// Get elevated status for a member (accessible by the member themselves or engineer)
async fn get_elevated(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ElevatedResponse>, (StatusCode, Json<iem_core::ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member_id, &config.jwt_secret)?;
    drop(config);
    let store = state.elevated_store.read().await;
    Ok(Json(ElevatedResponse {
        elevated: store.is_elevated(&member_id),
    }))
}

/// Set elevated status for a member (engineer-only)
async fn put_elevated(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<ElevatedPayload>,
) -> Result<Json<ElevatedResponse>, (StatusCode, Json<iem_core::ApiError>)> {
    let config = state.config.read().await;
    let claims = crate::auth::verify_member_access(&headers, &member_id, &config.jwt_secret)?;
    drop(config);

    if !claims.engineer {
        return Err((
            StatusCode::FORBIDDEN,
            Json(iem_core::ApiError::new(
                "FORBIDDEN",
                "Only engineers can change elevated status",
            )),
        ));
    }

    let mut store = state.elevated_store.write().await;
    store
        .set_elevated(&member_id, payload.elevated)
        .map_err(|e| {
            tracing::error!("Failed to save elevated status: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(iem_core::ApiError::new(
                    "IO_ERROR",
                    "Failed to save elevated status",
                )),
            )
        })?;

    tracing::info!(
        member = %member_id,
        elevated = payload.elevated,
        "Elevated status changed"
    );

    // Re-discover members so mix_send_indices get populated for the newly elevated member
    let members = crate::poller::discover_members(&state).await;
    let mut discovered = state.discovered_members.write().await;
    *discovered = members;
    tracing::info!("Re-discovered members after elevated toggle");

    Ok(Json(ElevatedResponse {
        elevated: payload.elevated,
    }))
}

#[derive(Serialize)]
struct ElevatedResponse {
    elevated: bool,
}

#[derive(Deserialize)]
struct ElevatedPayload {
    elevated: bool,
}

/// Audio WebSocket handler — delegates to proxy::ws_audio when audio feature is enabled
#[cfg(feature = "audio")]
async fn ws_audio_handler(
    ws: axum::extract::ws::WebSocketUpgrade,
    state: axum::extract::State<AppState>,
    query: axum::extract::Query<proxy::WsQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<iem_core::ApiError>)> {
    proxy::ws_audio(ws, state, query).await
}

#[cfg(not(feature = "audio"))]
async fn ws_audio_handler() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        "Audio streaming not available (compiled without audio feature)",
    )
}

/// Audio diagnostics endpoint — returns pipeline health metrics (engineer-only)
#[cfg(feature = "audio")]
async fn audio_diagnostics_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<crate::audio_stream::AudioDiagnostics>, (StatusCode, Json<iem_core::ApiError>)> {
    // Require engineer auth
    let config = state.config.read().await;
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(iem_core::ApiError::unauthorized()),
            )
        })?;
    let claims = crate::auth::extract_claims(token, &config.jwt_secret).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(iem_core::ApiError::unauthorized()),
        )
    })?;
    if !claims.engineer {
        return Err((
            StatusCode::FORBIDDEN,
            Json(iem_core::ApiError::new(
                "FORBIDDEN",
                "Audio diagnostics is engineer-only",
            )),
        ));
    }
    drop(config);

    let diag = state
        .audio_diagnostics
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Ok(Json(diag))
}

#[cfg(not(feature = "audio"))]
async fn audio_diagnostics_handler() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        "Audio diagnostics not available (compiled without audio feature)",
    )
}

/// REAPER proxy handler (engineer-only, requires auth)
async fn reaper_proxy(
    state: axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
    method: Method,
    path: Path<String>,
    body: Body,
) -> Result<Response, (StatusCode, Json<iem_core::ApiError>)> {
    let config = state.config.read().await;
    let claims = crate::auth::verify_member_access(&headers, "engineer", &config.jwt_secret)?;
    if !claims.engineer {
        return Err((
            StatusCode::FORBIDDEN,
            Json(iem_core::ApiError::new(
                "FORBIDDEN",
                "REAPER proxy is engineer-only",
            )),
        ));
    }
    drop(config);
    Ok(proxy::proxy_reaper(state, method, path, body)
        .await
        .into_response())
}

/// Static file routes (WASM assets)
pub fn static_routes() -> Router<AppState> {
    Router::new()
        // Serve index.html for SPA routes
        .route("/", get(serve_index))
        .route("/login", get(serve_index))
        // Serve static assets
        .route("/assets/{*path}", get(serve_asset))
        // Catch-all: serve files or SPA index for member routes
        .route("/{*path}", get(serve_spa_route))
}

/// Serve index.html
async fn serve_index() -> impl IntoResponse {
    serve_embedded_file("index.html")
}

/// Serve index.html for SPA routes or static files
async fn serve_spa_route(Path(path): Path<String>) -> Response {
    // Check if it looks like a file request (has extension)
    if path.contains('.') {
        serve_embedded_file(&path)
    } else {
        // SPA route - serve index.html
        serve_embedded_file("index.html")
    }
}

/// Serve an asset from /assets/
async fn serve_asset(Path(path): Path<String>) -> impl IntoResponse {
    serve_embedded_file(&format!("assets/{}", path))
}

/// Check if a filename contains a content hash (12+ contiguous hex chars).
/// Content-hashed files are safe for immutable long-term caching.
/// Files without content hashes (e.g. snippets/*/audio_player.js) must not
/// be cached, as CDN caches stale content causing SRI hash mismatches.
fn has_content_hash(path: &str) -> bool {
    let filename = path.rsplit('/').next().unwrap_or(path);
    filename.len() >= 12
        && filename
            .as_bytes()
            .windows(12)
            .any(|w| w.iter().all(|b| b.is_ascii_hexdigit()))
}

/// Serve an embedded file
fn serve_embedded_file(path: &str) -> Response {
    // Try exact path first
    if let Some(file) = <Assets as RustEmbed>::get(path) {
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        let cache_control = if has_content_hash(path) {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache, must-revalidate"
        };

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, cache_control)
            .body(Body::from(file.data.into_owned()))
            .unwrap();
    }

    // Try with .html extension
    let html_path = format!("{}.html", path);
    if let Some(file) = <Assets as RustEmbed>::get(&html_path) {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from(file.data.into_owned()))
            .unwrap();
    }

    // 404
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("Not found"))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_private_ip_local_ranges() {
        // 10.x.x.x (Class A private)
        assert!(is_private_ip("10.0.0.1"));
        assert!(is_private_ip("10.77.9.231"));
        assert!(is_private_ip("10.255.255.255"));

        // 172.16-31.x.x (Class B private)
        assert!(is_private_ip("172.16.0.1"));
        assert!(is_private_ip("172.31.255.255"));

        // 192.168.x.x (Class C private)
        assert!(is_private_ip("192.168.1.1"));
        assert!(is_private_ip("192.168.0.100"));

        // Loopback
        assert!(is_private_ip("127.0.0.1"));
    }

    #[test]
    fn test_is_private_ip_public_ranges() {
        assert!(!is_private_ip("8.8.8.8"));
        assert!(!is_private_ip("1.1.1.1"));
        assert!(!is_private_ip("203.0.113.50"));
        assert!(!is_private_ip("172.32.0.1")); // Just outside 172.16-31 range
    }

    #[test]
    fn test_is_private_ip_invalid() {
        assert!(!is_private_ip("not_an_ip"));
        assert!(!is_private_ip(""));
    }

    #[test]
    fn test_is_private_ip_ipv6() {
        assert!(is_private_ip("::1")); // loopback
        assert!(!is_private_ip("2001:db8::1")); // documentation range (public)
    }

    #[test]
    fn test_detect_network_mode_matching_ip_is_local() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("cf-connecting-ip", "203.0.113.50".parse().unwrap());
        let local_ip = Some("203.0.113.50".to_string());
        assert_eq!(detect_network_mode(&headers, &local_ip), "local");
    }

    #[test]
    fn test_detect_network_mode_different_ip_is_remote() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("cf-connecting-ip", "198.51.100.10".parse().unwrap());
        let local_ip = Some("203.0.113.50".to_string());
        assert_eq!(detect_network_mode(&headers, &local_ip), "remote");
    }

    #[test]
    fn test_detect_network_mode_no_headers_is_local() {
        let headers = axum::http::HeaderMap::new();
        let local_ip = Some("203.0.113.50".to_string());
        assert_eq!(detect_network_mode(&headers, &local_ip), "local");
    }

    #[test]
    fn test_detect_network_mode_no_local_ip_private_is_local() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("cf-connecting-ip", "10.77.9.100".parse().unwrap());
        assert_eq!(detect_network_mode(&headers, &None), "local");
    }

    #[test]
    fn test_detect_network_mode_no_local_ip_public_is_remote() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("cf-connecting-ip", "8.8.8.8".parse().unwrap());
        assert_eq!(detect_network_mode(&headers, &None), "remote");
    }

    #[test]
    fn test_detect_network_mode_x_forwarded_for_fallback() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.50, 10.0.0.1".parse().unwrap());
        let local_ip = Some("203.0.113.50".to_string());
        // Should use first IP from x-forwarded-for
        assert_eq!(detect_network_mode(&headers, &local_ip), "local");
    }

    #[test]
    fn test_content_hash_detection_hashed_files() {
        // Trunk-generated files with content hashes → long cache
        assert!(has_content_hash("iem-ui-c72f48fccb666eb9.js"));
        assert!(has_content_hash("iem-ui-c72f48fccb666eb9_bg.wasm"));
        assert!(has_content_hash("style-ccead50460cc69d.css"));
    }

    #[test]
    fn test_content_hash_detection_unhashed_files() {
        // Files without content hashes → no-cache
        assert!(!has_content_hash("audio_player.js"));
        assert!(!has_content_hash("sw.js"));
        assert!(!has_content_hash("manifest.json"));
        assert!(!has_content_hash("index.html"));
        assert!(!has_content_hash("icon.svg"));
        assert!(!has_content_hash("icon-192.png"));
        assert!(!has_content_hash("icon-512.png"));
    }

    #[test]
    fn test_content_hash_detection_with_directory() {
        // Snippet paths: directory has hash but filename doesn't → no-cache
        assert!(!has_content_hash(
            "snippets/iem-ui-fe2cd2496a8b535b/audio_player.js"
        ));
        // Full path with hashed filename → long cache
        assert!(has_content_hash("assets/iem-ui-c72f48fccb666eb9.js"));
    }
}
