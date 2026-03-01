//! Vaani — Voice to polished text, right at your cursor.
//!
//! This is the library crate for the Vaani Tauri application.
//! It exposes all modules and the Tauri plugin entry point.

pub mod app;
pub mod audio;
pub mod commands;
pub mod config;
pub mod enhance;
pub mod error;
pub mod hotkey;
pub mod keychain;
pub mod output;
pub mod prompts;
pub mod sounds;
pub mod state;
pub mod storage;
pub mod transcribe;
pub mod tray;
pub mod updater;

use app::VaaniApp;
use config::load_config;
use std::sync::Arc;
use tauri::Manager;

/// Tauri entry point — called from main.rs.
///
/// Sets up the app state, system tray, and runs the Tauri event loop.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Load configuration
    let config = load_config();
    tracing::info!(mode = %config.active_mode, "Vaani starting");

    // Create app state
    let vaani = Arc::new(VaaniApp::new(config));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(vaani)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config_cmd,
            commands::get_api_keys_status,
            commands::set_api_key,
            commands::list_microphones,
            commands::start_mic_test,
            commands::get_mic_level,
            commands::stop_mic_test,
            commands::get_hotkey,
            commands::set_hotkey,
            commands::check_permissions,
            commands::request_accessibility,
            commands::open_accessibility_settings,
            commands::complete_onboarding,
            commands::get_version,
            commands::open_log_file,
            commands::open_config_dir,
            commands::close_window,
        ])
        .setup(|app| {
            // Set up system tray
            tray::setup_tray(app.handle())?;

            // Background update check (non-blocking)
            let vaani_ref = app.state::<Arc<VaaniApp>>().inner().clone();
            tauri::async_runtime::spawn(async move {
                match updater::check_for_update(&vaani_ref.http_client).await {
                    Ok(Some(status)) if status.update_available => {
                        tracing::info!(
                            latest = %status.latest,
                            url = %status.release_url,
                            "New version available"
                        );
                    }
                    _ => {}
                }
            });

            tracing::info!("Vaani ready");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Failed to run Vaani");
}
