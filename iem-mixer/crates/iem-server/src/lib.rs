//! IEM Mixer Server
//!
//! Axum-based API server that:
//! - Serves embedded WASM frontend assets
//! - Provides authentication (PIN → JWT)
//! - Proxies requests to REAPER HTTP API
//! - Provides real-time WebSocket updates

pub mod auth;
pub mod poller;
pub mod proxy;
pub mod routes;

use axum::Router;
use iem_core::{Config, ServerMsg};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tower_http::cors::CorsLayer;

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
}

/// Global IEM output volume state for a member
#[derive(Debug, Clone)]
pub struct GlobalVolState {
    pub level_db: f32,
    pub muted: bool,
}

/// Cached mixer state for change detection
pub struct MixerCache {
    /// Last known channel states per member (member_id -> channels)
    pub member_states: HashMap<String, Vec<iem_core::Channel>>,
    /// Last known meter values (track_index -> peak_linear)
    pub meters: HashMap<usize, f32>,
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
}

impl MixerCache {
    pub fn new() -> Self {
        Self {
            member_states: HashMap::new(),
            meters: HashMap::new(),
            connected: false,
            active_members: HashMap::new(),
            command_timestamps: HashMap::new(),
            global_volumes: HashMap::new(),
            output_track_indices: HashMap::new(),
        }
    }
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            config: Arc::new(RwLock::new(config)),
            http_client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(2))
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("failed to build HTTP client"),
            event_tx,
            mixer_cache: Arc::new(RwLock::new(MixerCache::new())),
        }
    }
}

/// Server configuration
pub struct ServerConfig {
    pub port: u16,
    pub config: Config,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 80,
            config: Config::default(),
        }
    }
}

/// Embedded WASM assets (built by Trunk)
#[derive(rust_embed::Embed)]
#[folder = "../../iem-ui/dist/"]
pub struct Assets;

/// Start the server
pub async fn start_server(server_config: ServerConfig) -> anyhow::Result<()> {
    let state = AppState::new(server_config.config);

    // Spawn background REAPER poller
    poller::spawn_poller(state.clone());

    let cors = CorsLayer::permissive();

    let app = Router::new()
        .merge(routes::api_routes())
        .merge(routes::static_routes())
        .layer(cors)
        .with_state(state.clone());

    // Spawn HTTPS server on port 443 (if TLS enabled and certs exist)
    #[cfg(feature = "tls")]
    {
        let config = state.config.read().await;
        if config.tls {
            let config_dir = dirs::config_dir()
                .unwrap_or_default()
                .join("iem-mixer");
            let cert_path = config_dir.join(&config.tls_cert);
            let key_path = config_dir.join(&config.tls_key);
            let https_port = config.https_port;
            drop(config);

            if cert_path.exists() && key_path.exists() {
                let rustls_config =
                    axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert_path, &key_path)
                        .await?;
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
            } else {
                tracing::warn!(
                    "TLS enabled but cert files not found at {:?}",
                    cert_path
                );
            }
        }
    }

    // HTTP server (always runs)
    let addr = SocketAddr::from(([0, 0, 0, 0], server_config.port));
    tracing::info!("Starting server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Get the local network URL for remote access
pub fn get_remote_url(port: u16) -> String {
    let ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "localhost".to_string());
    format!("http://{}:{}", ip, port)
}
