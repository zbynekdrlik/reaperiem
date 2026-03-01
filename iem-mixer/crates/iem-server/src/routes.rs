//! API and static file routes

use axum::{
    Json, Router,
    body::Body,
    extract::Path,
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{any, get, post},
};
use serde::Serialize;

use crate::{AppState, Assets, auth, proxy};
use axum::middleware;
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

/// API routes split into public and protected groups
pub fn api_routes(state: AppState) -> Router<AppState> {
    // Public routes — no authentication required
    let public = Router::new()
        // Version endpoint (used by CI for deployment verification)
        .route("/api/version", get(get_version))
        // Auth login (public, returns JWT)
        .route("/api/auth", post(auth::login))
        // Member list (public, needed for landing page)
        .route("/api/members", get(get_members));

    // Protected routes — require valid JWT via verify_token middleware
    let protected = Router::new()
        // Change PIN (authenticated)
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
        // Raw REAPER proxy (engineer only)
        .route("/api/reaper/{*path}", any(reaper_proxy))
        .route_layer(middleware::from_fn_with_state(state, auth::verify_token));

    // WebSocket with token query param validation (handled inside ws_mixer)
    let ws_routes = Router::new()
        .route("/ws/{member_id}", get(proxy::ws_mixer));

    public.merge(protected).merge(ws_routes)
}

/// Get list of band members
async fn get_members(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    let members: Vec<MemberInfo> = config
        .members
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

/// REAPER proxy handler
async fn reaper_proxy(
    state: axum::extract::State<AppState>,
    method: Method,
    path: Path<String>,
    body: Body,
) -> impl IntoResponse {
    proxy::proxy_reaper(state, method, path, body).await
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

/// Serve an embedded file
fn serve_embedded_file(path: &str) -> Response {
    // Try exact path first
    if let Some(file) = <Assets as RustEmbed>::get(path) {
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        // HTML files should not be cached (ensures fresh UI after deploys)
        // Hashed assets (JS/WASM/CSS in /assets/) can be cached long-term
        let cache_control = if path.ends_with(".html") || path == "index.html" {
            "no-cache, must-revalidate"
        } else if path == "sw.js" || path == "manifest.json" {
            "no-cache, must-revalidate"
        } else {
            "public, max-age=31536000"
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
