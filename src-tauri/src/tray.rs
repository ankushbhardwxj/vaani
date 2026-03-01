//! System tray setup and state-driven updates.
//!
//! Vaani runs as a menu bar (system tray) app with no main window.
//! The tray icon and menu reflect the current app state.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter};

use crate::state::AppState;

/// Identifiers for tray menu items.
const MENU_TOGGLE: &str = "toggle_recording";
const MENU_PREFERENCES: &str = "preferences";
const MENU_QUIT: &str = "quit";

/// Sets up the system tray icon and menu.
pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let toggle = MenuItem::with_id(app, MENU_TOGGLE, "Start Recording", true, None::<&str>)?;
    let preferences =
        MenuItem::with_id(app, MENU_PREFERENCES, "Preferences...", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit Vaani", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&toggle, &preferences, &quit])?;

    TrayIconBuilder::new()
        .icon(
            app.default_window_icon()
                .cloned()
                .unwrap_or_else(|| tauri::image::Image::new(&[], 0, 0)),
        )
        .menu(&menu)
        .tooltip("Vaani — Voice to Text")
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            match id {
                x if x == MENU_TOGGLE => {
                    tracing::info!("Tray: toggle recording clicked");
                    // Emit event to app for handling in app.rs
                    if let Err(e) = app.emit("tray-toggle-recording", ()) {
                        tracing::error!("Failed to emit toggle event: {e}");
                    }
                }
                x if x == MENU_PREFERENCES => {
                    tracing::info!("Tray: preferences clicked");
                    if let Err(e) = app.emit("tray-open-preferences", ()) {
                        tracing::error!("Failed to emit preferences event: {e}");
                    }
                }
                x if x == MENU_QUIT => {
                    tracing::info!("Tray: quit clicked");
                    app.exit(0);
                }
                _ => {
                    tracing::debug!("Tray: unknown menu event: {id}");
                }
            }
        })
        .on_tray_icon_event(|_tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                tracing::debug!("Tray icon left-clicked");
            }
        })
        .build(app)?;

    tracing::info!("System tray initialized");
    Ok(())
}

/// Returns the tray title string for a given app state.
pub fn title_for_state(state: AppState) -> &'static str {
    match state {
        AppState::Idle => "Vaani",
        AppState::Recording => "Vaani - Recording...",
        AppState::Processing => "Vaani - Processing...",
    }
}

/// Returns the tray menu label for the toggle item based on current state.
pub fn toggle_label_for_state(state: AppState) -> &'static str {
    match state {
        AppState::Idle => "Start Recording",
        AppState::Recording => "Stop Recording",
        AppState::Processing => "Processing...",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_for_idle() {
        assert_eq!(title_for_state(AppState::Idle), "Vaani");
    }

    #[test]
    fn title_for_recording() {
        assert!(title_for_state(AppState::Recording).contains("Recording"));
    }

    #[test]
    fn title_for_processing() {
        assert!(title_for_state(AppState::Processing).contains("Processing"));
    }

    #[test]
    fn toggle_label_idle_says_start() {
        assert_eq!(toggle_label_for_state(AppState::Idle), "Start Recording");
    }

    #[test]
    fn toggle_label_recording_says_stop() {
        assert_eq!(
            toggle_label_for_state(AppState::Recording),
            "Stop Recording"
        );
    }

    #[test]
    fn toggle_label_processing_says_processing() {
        assert!(toggle_label_for_state(AppState::Processing).contains("Processing"));
    }

    #[test]
    fn mode_list_is_available() {
        // Verify MODES is accessible from tray context
        assert_eq!(crate::config::MODES.len(), 5);
    }
}
