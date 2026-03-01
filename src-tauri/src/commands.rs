//! Tauri v2 command handlers bridging the JS frontend to Rust backend.
//!
//! Each `#[tauri::command]` function is invoked from the webview via
//! `invoke("command_name", { ...args })`. All commands receive the shared
//! `VaaniApp` through Tauri's managed state.
//!
//! # Registration
//!
//! Register all commands in `lib.rs` with:
//! ```ignore
//! .invoke_handler(tauri::generate_handler![
//!     commands::get_config,
//!     commands::save_config_cmd,
//!     commands::get_api_keys_status,
//!     commands::set_api_key,
//!     commands::list_microphones,
//!     commands::start_mic_test,
//!     commands::get_mic_level,
//!     commands::stop_mic_test,
//!     commands::get_hotkey,
//!     commands::set_hotkey,
//!     commands::check_permissions,
//!     commands::request_accessibility,
//!     commands::open_accessibility_settings,
//!     commands::complete_onboarding,
//!     commands::get_version,
//!     commands::open_log_file,
//!     commands::open_config_dir,
//!     commands::close_window,
//! ])
//! ```

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::app::VaaniApp;
use crate::config::VaaniConfig;
use crate::error::VaaniError;
use crate::keychain::create_secret_storage;

// ── Serializable response types ────────────────────────────────────────────

/// Status of API keys detected in the environment.
#[derive(Debug, Serialize)]
pub struct ApiKeysStatus {
    pub openai: bool,
    pub anthropic: bool,
}

/// Status of system permissions.
#[derive(Debug, Serialize)]
pub struct PermissionsStatus {
    pub mic: bool,
    pub accessibility: bool,
}

/// A microphone device with its index and display name.
#[derive(Debug, Serialize)]
pub struct MicrophoneInfo {
    pub index: u32,
    pub name: String,
}

// ── Commands ───────────────────────────────────────────────────────────────

/// Returns the current application configuration.
#[tauri::command]
pub fn get_config(app: State<'_, Arc<VaaniApp>>) -> Result<VaaniConfig, VaaniError> {
    let config = app.config.lock().unwrap_or_else(|e| e.into_inner()).clone();
    Ok(config)
}

/// Accepts a partial JSON object, merges its fields into the current config,
/// validates, persists to disk, and updates the in-memory state.
#[tauri::command]
pub fn save_config_cmd(
    app: State<'_, Arc<VaaniApp>>,
    data: serde_json::Value,
) -> Result<(), VaaniError> {
    let mut config = app.config.lock().unwrap_or_else(|e| e.into_inner()).clone();

    merge_config_fields(&mut config, &data);
    config.validate()?;
    crate::config::save_config(&config)?;

    *app.config.lock().unwrap_or_else(|e| e.into_inner()) = config;
    Ok(())
}

/// Checks whether OpenAI and Anthropic API keys are available.
/// Checks Keychain first, then falls back to environment variables.
#[tauri::command]
pub fn get_api_keys_status(_app: State<'_, Arc<VaaniApp>>) -> Result<ApiKeysStatus, VaaniError> {
    let storage = create_secret_storage();

    let openai = storage
        .get("openai_api_key")
        .ok()
        .flatten()
        .map(|k| !k.is_empty())
        .unwrap_or(false)
        || std::env::var("VAANI_OPENAI_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .map(|k| !k.is_empty())
            .unwrap_or(false);

    let anthropic = storage
        .get("anthropic_api_key")
        .ok()
        .flatten()
        .map(|k| !k.is_empty())
        .unwrap_or(false)
        || std::env::var("VAANI_ANTHROPIC_API_KEY")
            .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
            .map(|k| !k.is_empty())
            .unwrap_or(false);

    Ok(ApiKeysStatus { openai, anthropic })
}

/// Stores an API key in the system keychain.
///
/// `provider` should be `"openai"` or `"anthropic"`. The key is stored
/// under `{provider}_api_key` in the keychain.
#[tauri::command]
pub fn set_api_key(
    _app: State<'_, Arc<VaaniApp>>,
    provider: String,
    key: String,
) -> Result<(), VaaniError> {
    let keychain_key = format!("{provider}_api_key");
    let storage = create_secret_storage();

    if key.is_empty() {
        storage.delete(&keychain_key)?;
        tracing::info!(provider = %provider, "API key removed from keychain");
    } else {
        storage.set(&keychain_key, &key)?;
        tracing::info!(provider = %provider, "API key stored in keychain");
    }

    Ok(())
}

/// Returns all available audio input devices.
#[tauri::command]
pub fn list_microphones(_app: State<'_, Arc<VaaniApp>>) -> Result<Vec<MicrophoneInfo>, VaaniError> {
    let devices = crate::audio::capture::list_input_devices()?;
    let mics = devices
        .into_iter()
        .map(|(index, name)| MicrophoneInfo { index, name })
        .collect();
    Ok(mics)
}

/// Starts a microphone test session. Stub — actual mic test requires an
/// `AudioRecorder` which holds a non-Send cpal `Stream`.
#[tauri::command]
pub fn start_mic_test(_app: State<'_, Arc<VaaniApp>>) -> Result<(), VaaniError> {
    tracing::info!("Mic test start requested (stub)");
    Ok(())
}

/// Returns the current microphone input level (0.0 to 1.0).
#[tauri::command]
pub fn get_mic_level(app: State<'_, Arc<VaaniApp>>) -> Result<f32, VaaniError> {
    Ok(app.current_mic_level())
}

/// Stops a microphone test session. Stub for now.
#[tauri::command]
pub fn stop_mic_test(_app: State<'_, Arc<VaaniApp>>) -> Result<(), VaaniError> {
    tracing::info!("Mic test stop requested (stub)");
    Ok(())
}

/// Returns the currently configured hotkey string.
#[tauri::command]
pub fn get_hotkey(app: State<'_, Arc<VaaniApp>>) -> Result<String, VaaniError> {
    let hotkey = app
        .config
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .hotkey
        .clone();
    Ok(hotkey)
}

/// Updates the hotkey in config and persists to disk.
#[tauri::command]
pub fn set_hotkey(app: State<'_, Arc<VaaniApp>>, hotkey: String) -> Result<(), VaaniError> {
    let mut config = app.config.lock().unwrap_or_else(|e| e.into_inner()).clone();

    config.hotkey = hotkey;
    crate::config::save_config(&config)?;

    *app.config.lock().unwrap_or_else(|e| e.into_inner()) = config;
    tracing::info!("Hotkey updated");
    Ok(())
}

/// Checks microphone and accessibility permissions. Returns all-true stub
/// until Phase 5 adds real permission checks.
#[tauri::command]
pub fn check_permissions(_app: State<'_, Arc<VaaniApp>>) -> Result<PermissionsStatus, VaaniError> {
    Ok(PermissionsStatus {
        mic: true,
        accessibility: true,
    })
}

/// Requests accessibility permission. Stub — returns true.
#[tauri::command]
pub fn request_accessibility(_app: State<'_, Arc<VaaniApp>>) -> Result<bool, VaaniError> {
    tracing::info!("Accessibility permission requested (stub)");
    Ok(true)
}

/// Opens the macOS System Settings accessibility pane.
#[tauri::command]
pub fn open_accessibility_settings(_app: State<'_, Arc<VaaniApp>>) -> Result<(), VaaniError> {
    open_macos_accessibility_pane()
}

/// Marks onboarding as completed and persists the change.
#[tauri::command]
pub fn complete_onboarding(app: State<'_, Arc<VaaniApp>>) -> Result<(), VaaniError> {
    let mut config = app.config.lock().unwrap_or_else(|e| e.into_inner()).clone();

    config.onboarding_completed = true;
    crate::config::save_config(&config)?;

    *app.config.lock().unwrap_or_else(|e| e.into_inner()) = config;
    tracing::info!("Onboarding completed");
    Ok(())
}

/// Returns the application version from Cargo.toml.
#[tauri::command]
pub fn get_version(_app: State<'_, Arc<VaaniApp>>) -> Result<String, VaaniError> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

/// Opens the log file. Stub for now.
#[tauri::command]
pub fn open_log_file(_app: State<'_, Arc<VaaniApp>>) -> Result<(), VaaniError> {
    tracing::info!("Open log file requested (stub)");
    Ok(())
}

/// Opens the Vaani configuration directory in the system file manager.
#[tauri::command]
pub fn open_config_dir(_app: State<'_, Arc<VaaniApp>>) -> Result<(), VaaniError> {
    let dir = crate::config::config_dir();
    open_path_in_file_manager(&dir)
}

/// Closes the calling webview window.
#[tauri::command]
pub fn close_window(window: tauri::WebviewWindow) -> Result<(), VaaniError> {
    window
        .close()
        .map_err(|e| VaaniError::Config(format!("Failed to close window: {e}")))
}

// ── Private helpers ────────────────────────────────────────────────────────

/// Merges fields from a partial JSON value into an existing `VaaniConfig`.
///
/// Only fields present in `data` are overwritten; absent fields are left
/// unchanged.
fn merge_config_fields(config: &mut VaaniConfig, data: &serde_json::Value) {
    if let Some(v) = data.get("hotkey").and_then(|v| v.as_str()) {
        config.hotkey = v.to_string();
    }
    if let Some(v) = data.get("sample_rate").and_then(|v| v.as_u64()) {
        config.sample_rate = v as u32;
    }
    if let Some(v) = data.get("vad_threshold").and_then(|v| v.as_f64()) {
        config.vad_threshold = v as f32;
    }
    if let Some(v) = data.get("max_recording_seconds").and_then(|v| v.as_u64()) {
        config.max_recording_seconds = v as u32;
    }
    if let Some(v) = data.get("microphone_device") {
        if v.is_null() {
            config.microphone_device = None;
        } else if let Some(idx) = v.as_u64() {
            config.microphone_device = Some(idx as u32);
        }
    }
    if let Some(v) = data.get("stt_model").and_then(|v| v.as_str()) {
        config.stt_model = v.to_string();
    }
    if let Some(v) = data.get("llm_model").and_then(|v| v.as_str()) {
        config.llm_model = v.to_string();
    }
    if let Some(v) = data.get("active_mode").and_then(|v| v.as_str()) {
        config.active_mode = v.to_string();
    }
    if let Some(v) = data.get("sounds_enabled").and_then(|v| v.as_bool()) {
        config.sounds_enabled = v;
    }
    if let Some(v) = data.get("paste_restore_delay_ms").and_then(|v| v.as_u64()) {
        config.paste_restore_delay_ms = v as u32;
    }
    if let Some(v) = data.get("launch_at_login").and_then(|v| v.as_bool()) {
        config.launch_at_login = v;
    }
    if let Some(v) = data.get("onboarding_completed").and_then(|v| v.as_bool()) {
        config.onboarding_completed = v;
    }
}

/// Opens the macOS Accessibility preference pane via the system URL scheme.
fn open_macos_accessibility_pane() -> Result<(), VaaniError> {
    let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|e| VaaniError::Config(format!("Failed to open accessibility settings: {e}")))?;

    tracing::info!("Opened accessibility settings");
    Ok(())
}

/// Opens a path in the platform-native file manager.
fn open_path_in_file_manager(path: &std::path::Path) -> Result<(), VaaniError> {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };

    std::process::Command::new(program)
        .arg(path)
        .spawn()
        .map_err(|e| VaaniError::Config(format!("Failed to open {}: {e}", path.display())))?;

    tracing::info!(path = %path.display(), "Opened in file manager");
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_version_returns_package_version() {
        let version = env!("CARGO_PKG_VERSION");
        assert!(!version.is_empty(), "CARGO_PKG_VERSION should be set");
        // Verify it looks like a semver string
        let parts: Vec<&str> = version.split('.').collect();
        assert!(
            parts.len() >= 2,
            "Version should have at least major.minor, got: {version}"
        );
    }

    #[test]
    fn api_keys_status_reflects_env() {
        // Clear any existing keys
        std::env::remove_var("VAANI_OPENAI_API_KEY");
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("VAANI_ANTHROPIC_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");

        // Without keys
        let openai_present = std::env::var("VAANI_OPENAI_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .map(|k| !k.is_empty())
            .unwrap_or(false);
        assert!(!openai_present);

        // Set a key and check
        std::env::set_var("VAANI_OPENAI_API_KEY", "sk-test-key");
        let openai_present = std::env::var("VAANI_OPENAI_API_KEY")
            .map(|k| !k.is_empty())
            .unwrap_or(false);
        assert!(openai_present);

        // Clean up
        std::env::remove_var("VAANI_OPENAI_API_KEY");
    }

    #[test]
    fn microphone_info_serializes() {
        let mic = MicrophoneInfo {
            index: 0,
            name: "Built-in Microphone".to_string(),
        };
        let json = serde_json::to_value(&mic).expect("should serialize");
        assert_eq!(json["index"], 0);
        assert_eq!(json["name"], "Built-in Microphone");
    }

    #[test]
    fn permissions_status_serializes() {
        let status = PermissionsStatus {
            mic: true,
            accessibility: false,
        };
        let json = serde_json::to_value(&status).expect("should serialize");
        assert_eq!(json["mic"], true);
        assert_eq!(json["accessibility"], false);
    }

    #[test]
    fn api_keys_status_serializes() {
        let status = ApiKeysStatus {
            openai: true,
            anthropic: false,
        };
        let json = serde_json::to_value(&status).expect("should serialize");
        assert_eq!(json["openai"], true);
        assert_eq!(json["anthropic"], false);
    }

    #[test]
    fn open_accessibility_command_exists() {
        // Verify the URL scheme we use for macOS accessibility settings
        let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
        assert!(url.starts_with("x-apple.systempreferences:"));
        assert!(url.contains("Privacy_Accessibility"));
    }

    #[test]
    fn merge_config_partial_update() {
        let mut config = VaaniConfig::default();
        let data = serde_json::json!({
            "active_mode": "casual",
            "sounds_enabled": false,
        });

        merge_config_fields(&mut config, &data);

        assert_eq!(config.active_mode, "casual");
        assert!(!config.sounds_enabled);
        // Unchanged fields keep defaults
        assert_eq!(config.hotkey, "alt");
        assert_eq!(config.sample_rate, 16_000);
    }

    #[test]
    fn merge_config_null_microphone_device() {
        let mut config = VaaniConfig {
            microphone_device: Some(2),
            ..Default::default()
        };
        let data = serde_json::json!({
            "microphone_device": null,
        });

        merge_config_fields(&mut config, &data);

        assert_eq!(config.microphone_device, None);
    }

    #[test]
    fn merge_config_all_fields() {
        let mut config = VaaniConfig::default();
        let data = serde_json::json!({
            "hotkey": "ctrl",
            "sample_rate": 44100,
            "vad_threshold": 0.1,
            "max_recording_seconds": 300,
            "microphone_device": 3,
            "stt_model": "whisper-2",
            "llm_model": "claude-sonnet-4-20250514",
            "active_mode": "code",
            "sounds_enabled": false,
            "paste_restore_delay_ms": 200,
            "launch_at_login": true,
            "onboarding_completed": true,
        });

        merge_config_fields(&mut config, &data);

        assert_eq!(config.hotkey, "ctrl");
        assert_eq!(config.sample_rate, 44100);
        assert!((config.vad_threshold - 0.1).abs() < f32::EPSILON);
        assert_eq!(config.max_recording_seconds, 300);
        assert_eq!(config.microphone_device, Some(3));
        assert_eq!(config.stt_model, "whisper-2");
        assert_eq!(config.llm_model, "claude-sonnet-4-20250514");
        assert_eq!(config.active_mode, "code");
        assert!(!config.sounds_enabled);
        assert_eq!(config.paste_restore_delay_ms, 200);
        assert!(config.launch_at_login);
        assert!(config.onboarding_completed);
    }

    #[test]
    fn merge_config_empty_object_changes_nothing() {
        let original = VaaniConfig::default();
        let mut config = original.clone();
        let data = serde_json::json!({});

        merge_config_fields(&mut config, &data);

        assert_eq!(config, original);
    }
}
