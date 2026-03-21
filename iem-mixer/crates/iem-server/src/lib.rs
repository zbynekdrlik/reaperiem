//! IEM Mixer Server
//!
//! Axum-based API server that:
//! - Serves embedded WASM frontend assets
//! - Provides authentication (PIN → JWT)
//! - Proxies requests to REAPER HTTP API
//! - Provides real-time WebSocket updates

#[cfg(feature = "audio")]
pub mod audio_stream;
pub mod auth;
pub mod customization_store;
pub mod pin_store;
pub mod poller;
pub mod preset_routes;
pub mod preset_store;
pub mod proxy;
pub mod routes;
pub mod snapshot_routes;
pub mod snapshot_store;

use axum::Router;
use axum::http::{HeaderName, HeaderValue};
use iem_core::{Config, DiscoveredMember, ServerMsg};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{RwLock, broadcast};
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;

/// Engineer listen mode state machine.
/// Controls poller mute suppression during listen and restore phases.
#[derive(Debug, Clone, Default)]
pub enum EngineerListenState {
    /// Not listening — poller broadcasts real REAPER mute state
    #[default]
    Idle,
    /// Listening to a member — poller suppresses mix mute broadcasts
    Active(String),
    /// ReaScript restoring mute states — poller still suppresses until deadline
    Restoring(Instant),
}

impl EngineerListenState {
    pub fn is_suppressing(&self) -> bool {
        match self {
            Self::Idle => false,
            Self::Active(_) => true,
            Self::Restoring(deadline) => Instant::now() <= *deadline,
        }
    }
}


/// Write data to a file atomically by writing to a temp file then renaming.
/// Prevents corruption on crash/power failure.
pub fn atomic_write(path: &std::path::Path, data: &str) -> std::io::Result<()> {
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, data)?;
    std::fs::rename(&tmp_path, path)
}

#[cfg(feature = "audio")]
use std::sync::Mutex;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Application configuration
    pub config: Arc<RwLock<Config>>,
    /// HTTP client for REAPER proxy
    pub http_client: reqwest::Client,
    /// Broadcast channel for WebSocket state updates (member_id, event)
    pub event_tx: broadcast::Sender<(String, ServerMsg)>,
    /// Cache of last-known state per member (for diff detection)
    pub mixer_cache: Arc<RwLock<MixerCache>>,
    /// Runtime PIN storage (persisted to pins.json)
    pub pin_store: Arc<RwLock<pin_store::PinStore>>,
    /// Snapshot storage for mix history
    pub snapshot_store: Arc<snapshot_store::SnapshotStore>,
    /// Preset storage for saved mix configurations
    pub preset_store: Arc<preset_store::PresetStore>,
    /// Channel customization storage (pin/hide preferences)
    pub customization_store: Arc<customization_store::CustomizationStore>,
    /// Band members discovered from REAPER (source of truth)
    pub discovered_members: Arc<RwLock<Vec<DiscoveredMember>>>,
    /// Engineer listen mode state (Idle / Active / Restoring).
    /// Used to suppress mix channel mute broadcasts during listen and restore phases.
    pub engineer_listen_target: Arc<RwLock<EngineerListenState>>,
    /// Broadcast channel for audio Opus frames (engineer listening)
    #[cfg(feature = "audio")]
    pub audio_tx: broadcast::Sender<bytes::Bytes>,
    /// Audio pipeline health diagnostics
    #[cfg(feature = "audio")]
    pub audio_diagnostics: Arc<Mutex<audio_stream::AudioDiagnostics>>,
}

/// Global IEM output volume state for a member
#[derive(Debug, Clone)]
pub struct GlobalVolState {
    pub level_db: f32,
    pub muted: bool,
}

/// Cached mixer state for change detection
#[derive(Default)]
pub struct MixerCache {
    /// Last known channel states per member (member_id -> channels)
    pub member_states: HashMap<String, Vec<iem_core::Channel>>,
    /// Last known meter values (track_index -> [left, right] peak_linear)
    pub meters: HashMap<usize, [f32; 2]>,
    /// Whether REAPER is currently reachable
    pub connected: bool,
    /// Members with active WebSocket connections (member_id -> connection count)
    pub active_members: HashMap<String, usize>,
    /// Timestamps of recent commands, keyed by (member_id, track_index).
    /// Used by the poller to suppress echo broadcasts for recently-commanded channels.
    pub command_timestamps: HashMap<(String, usize), std::time::Instant>,
    /// Last known global IEM output volume per member (member_id -> state)
    pub global_volumes: HashMap<String, GlobalVolState>,
    /// Output track indices per member (member_id -> 1-based track index)
    pub output_track_indices: HashMap<String, usize>,
    /// Input track indices resolved by name from REAPER (track_name -> 1-based track index)
    pub input_track_indices: HashMap<String, usize>,
    /// Last known REAPER track count (for change detection)
    pub last_track_count: Option<usize>,
    /// Date of last auto-snapshot per member (member_id -> "YYYY-MM-DD")
    /// Used to ensure only one auto-snapshot per day per member
    pub snapshot_last_date: HashMap<String, String>,
    /// Solo state per member — transient, in-memory only (member_id -> soloed track indices)
    pub solo_states: HashMap<String, Vec<usize>>,
}

impl MixerCache {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AppState {
    pub fn new(config: Config, config_dir: &std::path::Path) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        #[cfg(feature = "audio")]
        let (audio_tx, _) = broadcast::channel(64);
        Self {
            config: Arc::new(RwLock::new(config)),
            http_client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(2))
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("failed to build HTTP client"),
            event_tx,
            mixer_cache: Arc::new(RwLock::new(MixerCache::new())),
            pin_store: Arc::new(RwLock::new(pin_store::PinStore::load(config_dir))),
            snapshot_store: Arc::new(snapshot_store::SnapshotStore::new(config_dir)),
            preset_store: Arc::new(preset_store::PresetStore::new(config_dir)),
            customization_store: Arc::new(customization_store::CustomizationStore::new(config_dir)),
            discovered_members: Arc::new(RwLock::new(Vec::new())),
            engineer_listen_target: Arc::new(RwLock::new(EngineerListenState::Idle)),
            #[cfg(feature = "audio")]
            audio_tx,
            #[cfg(feature = "audio")]
            audio_diagnostics: Arc::new(Mutex::new(audio_stream::AudioDiagnostics::default())),
        }
    }
}

/// Server configuration
pub struct ServerConfig {
    pub port: u16,
    pub config: Config,
    /// Directory where config and runtime data live (for pins.json, etc.)
    pub config_dir: std::path::PathBuf,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 80,
            config: Config::default(),
            config_dir: std::path::PathBuf::from("."),
        }
    }
}

/// Embedded WASM assets (built by Trunk)
#[derive(rust_embed::Embed)]
#[folder = "../../iem-ui/dist/"]
pub struct Assets;

/// Start the server, optionally signaling readiness via a oneshot channel
pub async fn start_server(
    server_config: ServerConfig,
    ready_tx: Option<tokio::sync::oneshot::Sender<()>>,
) -> anyhow::Result<()> {
    // Install rustls crypto provider (required when tls feature brings rustls into dep tree)
    #[cfg(feature = "tls")]
    {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    let state = AppState::new(server_config.config, &server_config.config_dir);

    // Auto-detect public IP for LAN/WAN detection (if not configured)
    {
        let needs_detection = state.config.read().await.local_public_ip.is_none();
        if needs_detection {
            match detect_public_ip(&state.http_client).await {
                Some(ip) => {
                    tracing::info!(ip = %ip, "Auto-detected public IP for LAN/WAN detection");
                    state.config.write().await.local_public_ip = Some(ip);
                }
                None => {
                    tracing::warn!(
                        "Could not auto-detect public IP; LAN/WAN detection will use private IP fallback"
                    );
                }
            }
        }
    }

    // Discover members from REAPER (source of truth)
    let members = poller::discover_members(&state).await;
    {
        let mut discovered = state.discovered_members.write().await;
        *discovered = members;
    }
    let discovered_count = state.discovered_members.read().await.len();
    tracing::info!(count = discovered_count, "Members discovered from REAPER");

    // Spawn background REAPER poller
    poller::spawn_poller(state.clone());

    // Spawn audio listener (captures VBAN UDP packets from REAPER)
    #[cfg(feature = "audio")]
    audio_stream::spawn_audio_listener(state.audio_tx.clone(), state.audio_diagnostics.clone());

    let cors = CorsLayer::permissive();

    // Security headers to prevent common attacks
    let x_frame_options = SetResponseHeaderLayer::overriding(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
    let x_content_type_options = SetResponseHeaderLayer::overriding(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    let referrer_policy = SetResponseHeaderLayer::overriding(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    // CSP allows WASM + inline scripts (Trunk), inline styles (Leptos), and WebSocket connections
    let csp = SetResponseHeaderLayer::overriding(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self' ws: wss:; img-src 'self' data:; font-src 'self'",
        ),
    );

    let app = Router::new()
        .merge(routes::api_routes(state.clone()))
        .merge(routes::static_routes())
        .layer(cors)
        .layer(x_frame_options)
        .layer(x_content_type_options)
        .layer(referrer_policy)
        .layer(csp)
        .with_state(state.clone());

    // Spawn HTTPS server on port 443 (if TLS enabled and certs exist)
    #[cfg(feature = "tls")]
    {
        let config = state.config.read().await;
        if config.tls {
            let config_dir = dirs::config_dir().unwrap_or_default().join("iem-mixer");
            let cert_path = config_dir.join(&config.tls_cert);
            let key_path = config_dir.join(&config.tls_key);
            let https_port = config.https_port;
            drop(config);

            if cert_path.exists() && key_path.exists() {
                match axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert_path, &key_path)
                    .await
                {
                    Ok(rustls_config) => {
                        let https_addr = SocketAddr::from(([0, 0, 0, 0], https_port));
                        let https_app = app.clone();
                        tokio::spawn(async move {
                            tracing::info!("HTTPS server on https://iem.newlevel.media");
                            if let Err(e) = axum_server::bind_rustls(https_addr, rustls_config)
                                .serve(https_app.into_make_service())
                                .await
                            {
                                tracing::error!("HTTPS server failed: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("Failed to load TLS certificates: {}", e);
                    }
                }
            } else {
                tracing::warn!("TLS enabled but cert files not found at {:?}", cert_path);
            }
        }
    }

    // Wrap HTTP app with HTTPS redirect middleware when TLS + domain configured
    #[cfg(feature = "tls")]
    let app = {
        let config = state.config.read().await;
        if config.tls {
            if let Some(ref domain) = config.https_domain {
                let domain = domain.clone();
                drop(config);
                app.layer(axum::middleware::from_fn(move |req, next| {
                    let domain = domain.clone();
                    https_redirect(req, next, domain)
                }))
            } else {
                drop(config);
                app
            }
        } else {
            drop(config);
            app
        }
    };

    // HTTP server (always runs)
    let addr = SocketAddr::from(([0, 0, 0, 0], server_config.port));
    tracing::info!("Starting server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Signal readiness AFTER successful bind
    if let Some(tx) = ready_tx {
        let _ = tx.send(());
    }

    axum::serve(listener, app).await?;

    Ok(())
}

/// Redirect HTTP requests to HTTPS when the Host header matches the configured domain.
/// Requests via IP address (e.g., from Tauri desktop app) pass through unchanged.
/// Requests proxied through Cloudflare Tunnel (X-Forwarded-Proto: https) pass through unchanged.
#[cfg(feature = "tls")]
async fn https_redirect(
    req: axum::extract::Request,
    next: axum::middleware::Next,
    domain: String,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    // Skip redirect if already coming through HTTPS (via Cloudflare Tunnel)
    let forwarded_proto = req
        .headers()
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if forwarded_proto == "https" {
        return next.run(req).await;
    }

    let host = req
        .headers()
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    // Strip port from host header for comparison
    let host_name = host.split(':').next().unwrap_or("");
    if host_name == domain {
        let path = req
            .uri()
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");
        let location = format!("https://{domain}{path}");
        axum::response::Redirect::permanent(&location).into_response()
    } else {
        next.run(req).await
    }
}

/// Auto-detect the server's public IP by querying an external service.
/// Used for LAN/WAN detection when `local_public_ip` is not configured.
async fn detect_public_ip(client: &reqwest::Client) -> Option<String> {
    // Try multiple services for reliability
    let services = [
        "https://api.ipify.org",
        "https://ifconfig.me/ip",
        "https://icanhazip.com",
    ];
    for url in services {
        match client
            .get(url)
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(ip) = resp.text().await {
                    let ip = ip.trim().to_string();
                    if !ip.is_empty() && ip.len() < 46 {
                        return Some(ip);
                    }
                }
            }
            _ => continue,
        }
    }
    None
}

/// Get the local network URL for remote access
pub fn get_remote_url(port: u16) -> String {
    let ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "localhost".to_string());
    format!("http://{}:{}", ip, port)
}

#[cfg(all(test, feature = "tls"))]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
    };
    use tower::ServiceExt;

    async fn dummy_handler() -> &'static str {
        "OK"
    }

    fn create_test_app(domain: &str) -> Router {
        let domain = domain.to_string();
        Router::new()
            .route("/api/version", get(dummy_handler))
            .layer(middleware::from_fn(move |req, next| {
                let d = domain.clone();
                async move { https_redirect(req, next, d).await }
            }))
    }

    #[tokio::test]
    async fn test_redirect_without_forwarded_proto() {
        // Direct HTTP request to iem.newlevel.media should redirect to HTTPS
        let app = create_test_app("iem.newlevel.media");
        let req = Request::builder()
            .uri("/api/version")
            .header("host", "iem.newlevel.media")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(
            response.headers().get("location").unwrap(),
            "https://iem.newlevel.media/api/version"
        );
    }

    #[tokio::test]
    async fn test_no_redirect_with_forwarded_proto_https() {
        // Request via Cloudflare Tunnel (X-Forwarded-Proto: https) should NOT redirect
        let app = create_test_app("iem.newlevel.media");
        let req = Request::builder()
            .uri("/api/version")
            .header("host", "iem.newlevel.media")
            .header("x-forwarded-proto", "https")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_redirect_with_forwarded_proto_http() {
        // Request with X-Forwarded-Proto: http should still redirect
        let app = create_test_app("iem.newlevel.media");
        let req = Request::builder()
            .uri("/api/version")
            .header("host", "iem.newlevel.media")
            .header("x-forwarded-proto", "http")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    }

    #[tokio::test]
    async fn test_no_redirect_for_different_host() {
        // Request via IP address should pass through unchanged
        let app = create_test_app("iem.newlevel.media");
        let req = Request::builder()
            .uri("/api/version")
            .header("host", "10.77.9.231")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_redirect_preserves_path_and_query() {
        let app = create_test_app("iem.newlevel.media");
        let req = Request::builder()
            .uri("/api/mixer/1?token=abc123")
            .header("host", "iem.newlevel.media")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(
            response.headers().get("location").unwrap(),
            "https://iem.newlevel.media/api/mixer/1?token=abc123"
        );
    }
}
