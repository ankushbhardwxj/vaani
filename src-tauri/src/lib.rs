//! Vaani — Voice to polished text, right at your cursor.
//!
//! This is the library crate for the Vaani Tauri application.
//! It exposes all modules and the Tauri plugin entry point.

pub mod app;
pub mod audio;
pub mod config;
pub mod enhance;
pub mod error;
pub mod hotkey;
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
use tauri::{Listener, Manager, RunEvent};

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
        .manage(vaani)
        .setup(|app| {
            // Set up system tray
            tray::setup_tray(app.handle())?;

            // ── Wire up tray toggle recording event ────────────────────────
            let vaani_for_tray = app.state::<Arc<VaaniApp>>().inner().clone();
            app.listen("tray-toggle-recording", move |_event| {
                tracing::info!("Toggle recording event received");
                vaani_for_tray.toggle_recording();
            });

            // ── Start global hotkey listener ───────────────────────────────
            let vaani_for_hotkey = app.state::<Arc<VaaniApp>>().inner().clone();
            let hotkey_str = vaani_for_hotkey
                .config
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .hotkey
                .clone();

            match hotkey::start_listener(&hotkey_str, move |event| match event {
                hotkey::HotkeyEvent::Pressed => {
                    tracing::info!("Hotkey pressed — starting recording");
                    vaani_for_hotkey.toggle_recording();
                }
                hotkey::HotkeyEvent::Released => {
                    let current = vaani_for_hotkey.current_state();
                    if current == state::AppState::Recording {
                        tracing::info!("Hotkey released — stopping recording");
                        vaani_for_hotkey.toggle_recording();
                    }
                }
            }) {
                Ok(()) => tracing::info!(hotkey = %hotkey_str, "Global hotkey listener started"),
                Err(e) => tracing::error!("Failed to start hotkey listener: {e}"),
            }

            // ── Background update check (non-blocking) ────────────────────
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
        .build(tauri::generate_context!())
        .expect("Failed to build Vaani")
        .run(|_app, event| {
            // Keep the app running when all windows are closed (tray-only app).
            if let RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
