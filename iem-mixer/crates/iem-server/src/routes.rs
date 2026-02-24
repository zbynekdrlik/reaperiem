//! API and static file routes

use axum::{
    Router,
    body::Body,
    extract::Path,
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{any, get, post},
};

use crate::{AppState, Assets, auth, proxy};
use rust_embed::RustEmbed;

/// API routes (protected by authentication)
pub fn api_routes() -> Router<AppState> {
    Router::new()
        // Auth (public)
        .route("/api/auth", post(auth::login))
        // Member list (public)
        .route("/api/members", get(get_members))
        // Mixer state (should be protected)
        .route("/api/mixer/{member_id}", get(proxy::get_mixer_state))
        // Mixer controls (should be protected)
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

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, "public, max-age=31536000")
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
