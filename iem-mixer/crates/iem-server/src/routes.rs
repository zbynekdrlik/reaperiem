//! API and static file routes

use axum::{
    Json, Router,
    body::Body,
    extract::Path,
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{any, delete, get, post, put},
};
use serde::{Deserialize, Serialize};

use crate::{AppState, Assets, auth, backup_routes, preset_routes, proxy, snapshot_routes};
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
        // Member photo (profile picture) (#16)
        .route("/api/members/{member_id}/photo", get(get_photo))
        .route("/api/members/{member_id}/photo", post(post_photo))
        .route("/api/members/{member_id}/photo", delete(delete_photo))
        // Raw REAPER proxy
        .route("/api/reaper/{*path}", any(reaper_proxy))
        // Audio WebSocket (engineer-only audio streaming) — must be before /ws/{member_id}
        .route("/ws/audio", get(ws_audio_handler))
        // Talkback WebSocket (engineer push-to-talk) (#123) — must be before /ws/{member_id}
        .route("/ws/talkback", get(ws_talkback_handler))
        // Audio diagnostics (engineer-only)
        .route("/api/audio/diagnostics", get(audio_diagnostics_handler))
        // Talkback diagnostics (#123)
        .route(
            "/api/talkback/diagnostics",
            get(talkback_diagnostics_handler),
        )
        // Web Push notification subscription (#133)
        .route("/api/push/vapid-key", get(get_vapid_key))
        .route(
            "/api/client-error",
            // 10_240 bytes = 10 KiB. Written as a plain literal (not 10 * 1024)
            // so cargo-mutants has no binary operator to mutate. The exact
            // value is pinned by router-layer integration tests in proxy.rs
            // (`client_error_router_rejects_body_just_under_large` and
            // `client_error_router_rejects_oversize_body`). #153
            post(proxy::client_error).layer(axum::extract::DefaultBodyLimit::max(10_240)),
        )
        .route("/api/push/subscribe", post(push_subscribe))
        // WebSocket
        .route("/ws/{member_id}", get(proxy::ws_mixer))
        // Snapshot routes
        .merge(snapshot_routes::snapshot_routes())
        // Preset routes
        .merge(preset_routes::preset_routes())
        // Backup routes (engineer-only)
        .merge(backup_routes::backup_routes())
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
            has_photo: state.photo_store.exists(&m.id()),
        })
        .collect();
    axum::Json(members)
}

#[derive(serde::Serialize)]
struct MemberInfo {
    id: String,
    name: String,
    has_photo: bool,
}

/// Return the VAPID public key for browser push subscription (#133).
async fn get_vapid_key(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    if config.vapid_private_key.is_empty() {
        return (StatusCode::OK, Json(serde_json::json!({ "key": null })));
    }
    match iem_core::config::Config::vapid_public_key_base64url(&config.vapid_private_key) {
        Ok(pub_key) => (StatusCode::OK, Json(serde_json::json!({ "key": pub_key }))),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "invalid VAPID key" })),
        ),
    }
}

/// Store a push subscription (engineer-only) (#133).
async fn push_subscribe(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Verify engineer token
    let config = state.config.read().await;
    let claims = match auth::extract_claims(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .unwrap_or(""),
        &config.jwt_secret,
    ) {
        Some(c) if c.engineer => c,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "engineer access required" })),
            );
        }
    };
    drop(config);
    // Suppress unused variable warning (claims used only for engineer check)
    let _ = claims;

    // Parse subscription
    let endpoint = body["endpoint"].as_str().unwrap_or("").to_string();
    let p256dh = body["keys"]["p256dh"].as_str().unwrap_or("").to_string();
    let auth_key = body["keys"]["auth"].as_str().unwrap_or("").to_string();

    if endpoint.is_empty() || p256dh.is_empty() || auth_key.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "missing endpoint, p256dh, or auth" })),
        );
    }

    let sub = crate::push_store::PushSubscription {
        endpoint,
        p256dh,
        auth: auth_key,
    };
    let mut store = state.push_store.write().await;
    match store.add(sub) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("save failed: {}", e) })),
        ),
    }
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

#[derive(Deserialize)]
struct PhotoUpload {
    photo: String, // base64-encoded JPEG
}

/// Get a member's profile photo (no auth — landing page needs it)
async fn get_photo(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
) -> Result<Response, StatusCode> {
    match state.photo_store.load(&member_id) {
        Some(data) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/jpeg")
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(data))
            .unwrap()),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// Upload a member's profile photo (auth: own or engineer)
async fn post_photo(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<PhotoUpload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<iem_core::ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member_id, &config.jwt_secret)?;
    drop(config);

    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD
        .decode(&payload.photo)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(iem_core::ApiError::new("INVALID_DATA", "Invalid base64")),
            )
        })?;

    if data.len() > 256 * 1024 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(iem_core::ApiError::new("TOO_LARGE", "Photo exceeds 256 KB")),
        ));
    }

    state.photo_store.save(&member_id, &data).map_err(|e| {
        tracing::error!("Failed to save photo: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(iem_core::ApiError::new("IO_ERROR", "Failed to save photo")),
        )
    })?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Delete a member's profile photo (auth: own or engineer)
async fn delete_photo(
    State(state): State<AppState>,
    Path(member_id): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<iem_core::ApiError>)> {
    let config = state.config.read().await;
    crate::auth::verify_member_access(&headers, &member_id, &config.jwt_secret)?;
    drop(config);

    state.photo_store.delete(&member_id).map_err(|e| {
        tracing::error!("Failed to delete photo: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(iem_core::ApiError::new(
                "IO_ERROR",
                "Failed to delete photo",
            )),
        )
    })?;

    Ok(Json(serde_json::json!({ "ok": true })))
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

// Talkback WebSocket handler — delegates to proxy::ws_talkback (#123)
#[cfg(feature = "audio")]
async fn ws_talkback_handler(
    ws: axum::extract::ws::WebSocketUpgrade,
    state: axum::extract::State<AppState>,
    query: axum::extract::Query<proxy::WsQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<iem_core::ApiError>)> {
    proxy::ws_talkback(ws, state, query).await
}

#[cfg(not(feature = "audio"))]
async fn ws_talkback_handler() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Talkback not available")
}

// Talkback diagnostics — shows recv_vst_addr and active_talker (#123)
#[cfg(feature = "audio")]
async fn talkback_diagnostics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let tb = state.talkback_state.read().await;
    let addr = tb
        .recv_vst_addr
        .map(|a| a.to_string())
        .unwrap_or_else(|| "none".to_string());
    let talker = tb
        .active_talker
        .clone()
        .unwrap_or_else(|| "none".to_string());
    Json(serde_json::json!({
        "recv_vst_addr": addr,
        "active_talker": talker,
    }))
}

#[cfg(not(feature = "audio"))]
async fn talkback_diagnostics_handler() -> impl IntoResponse {
    Json(serde_json::json!({"error": "not available"}))
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

        let mut resp = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, cache_control);

        // Service worker must never be cached by CDN — stale sw.js breaks push notifications
        if path == "sw.js" {
            resp = resp.header("CDN-Cache-Control", "no-store");
        }

        return resp.body(Body::from(file.data.into_owned())).unwrap();
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
