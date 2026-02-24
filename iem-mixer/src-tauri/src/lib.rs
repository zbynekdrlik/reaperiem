//! IEM Mixer Tauri Application
//!
//! Desktop shell providing tray icon, window, and installer.
//! All UI is served by the embedded Axum server.

pub mod tray;

use iem_core::Config;
use iem_server::ServerConfig;
use std::sync::OnceLock;
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_updater::UpdaterExt;

/// Global AppHandle storage for cross-thread access
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Run the Tauri application
pub fn run() {
    // Set panic handler
    std::panic::set_hook(Box::new(|info| {
        tracing::error!("PANIC: {}", info);
    }));

    // Initialize logging
    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("iem-mixer")
        .join("logs");
    std::fs::create_dir_all(&log_dir).ok();

    let file_appender = tracing_appender::rolling::daily(&log_dir, "iem-mixer.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false);

    let env_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive("iem_mixer=debug".parse().unwrap())
        .add_directive("iem_core=debug".parse().unwrap())
        .add_directive("iem_server=info".parse().unwrap());

    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .init();

    tracing::info!(log_dir = %log_dir.display(), "Logging initialized");
    tracing::info!("Starting IEM Mixer v{}", iem_core::VERSION);

    // Load configuration
    let config_path = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("iem-mixer")
        .join("config.yaml");

    let config = if config_path.exists() {
        Config::load(&config_path).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to load config, using defaults");
            Config::default()
        })
    } else {
        tracing::info!("No config file found, using defaults");
        Config::default()
    };

    let port = config.port;
    let members = config.members.clone();

    // Create Tokio runtime
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let rt_handle = rt.handle().clone();

    // Spawn web server
    let server_config = ServerConfig { port, config };
    rt_handle.spawn(async move {
        if let Err(e) = iem_server::start_server(server_config).await {
            tracing::error!("Web server error: {}", e);
        }
    });

    // Keep runtime alive in background thread
    std::thread::spawn(move || {
        rt.block_on(std::future::pending::<()>());
    });

    // Build and run Tauri app
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus existing window when second instance tries to start
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
                let _ = window.show();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Hide instead of close
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(move |app| {
            let handle = app.handle().clone();

            // Store AppHandle globally
            let _ = APP_HANDLE.set(handle.clone());

            // Setup tray with member list
            if let Err(e) = tray::setup_tray(&handle, port, &members) {
                tracing::error!("Failed to setup tray: {}", e);
            }

            // Check for updates in background
            let update_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                check_for_updates(update_handle).await;
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running IEM Mixer");
}

/// Check for updates and prompt user if available
async fn check_for_updates(app: AppHandle) {
    tracing::info!("Checking for updates...");

    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            tracing::debug!("Updater not available: {}", e);
            return;
        }
    };

    match updater.check().await {
        Ok(Some(update)) => {
            tracing::info!(
                "Update available: {} -> {}",
                iem_core::VERSION,
                update.version
            );

            // Show dialog asking to update
            let msg = format!(
                "A new version ({}) is available.\nCurrent version: {}\n\nDo you want to download and install it?",
                update.version,
                iem_core::VERSION
            );

            use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
            let confirmed = app
                .dialog()
                .message(msg)
                .kind(MessageDialogKind::Info)
                .title("Update Available")
                .buttons(MessageDialogButtons::OkCancelCustom("Update", "Later"))
                .blocking_show();

            if confirmed {
                tracing::info!("User accepted update, downloading...");

                // Download and install
                match update.download_and_install(|_, _| {}, || {}).await {
                    Ok(_) => {
                        tracing::info!("Update installed, restarting...");
                        app.restart();
                    }
                    Err(e) => {
                        tracing::error!("Update failed: {}", e);
                        app.dialog()
                            .message(format!("Update failed: {}", e))
                            .kind(MessageDialogKind::Error)
                            .title("Update Error")
                            .blocking_show();
                    }
                }
            } else {
                tracing::info!("User declined update");
            }
        }
        Ok(None) => {
            tracing::info!("No updates available");
        }
        Err(e) => {
            tracing::warn!("Update check failed: {}", e);
        }
    }
}
