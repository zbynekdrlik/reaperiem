//! IEM Mixer Server
//!
//! Axum-based API server that:
//! - Serves embedded WASM frontend assets
//! - Provides authentication (PIN → JWT)
//! - Proxies requests to REAPER HTTP API

pub mod auth;
pub mod proxy;
pub mod routes;

use axum::Router;
use iem_core::Config;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Application configuration
    pub config: Arc<RwLock<Config>>,
    /// HTTP client for REAPER proxy
    pub http_client: reqwest::Client,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            http_client: reqwest::Client::new(),
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

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(routes::api_routes())
        .merge(routes::static_routes())
        .layer(cors)
        .with_state(state);

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
