//! Standalone server binary for E2E testing and CI
//!
//! This binary runs the iem-server without Tauri desktop shell.

use iem_core::Config;
use iem_server::ServerConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize simple logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("iem_server=info".parse().unwrap()),
        )
        .init();

    tracing::info!("Starting IEM Mixer Server v{}", iem_core::VERSION);

    // Use default config (or load from env/file if needed)
    let config = Config::default();

    // Allow PORT env var to override config (useful for CI where port 80 requires root)
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(config.port);

    let server_config = ServerConfig { port, config };

    tracing::info!("Server listening on http://0.0.0.0:{}", port);
    iem_server::start_server(server_config).await?;

    Ok(())
}
