use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::error::VaaniError;

/// Canonical list of all enhancement modes.
///
/// Every module that needs to know about modes must reference this constant
/// rather than maintaining its own list.
pub const MODES: &[&str] = &["minimal", "professional", "casual", "code", "funny"];

// ── Default helpers (used by `#[serde(default = "...")]`) ──────────────────

fn default_hotkey() -> String {
    "alt".to_string()
}

fn default_sample_rate() -> u32 {
    16_000
}

fn default_vad_threshold() -> f32 {
    0.05
}

fn default_max_recording_seconds() -> u32 {
    600
}

fn default_microphone_device() -> Option<u32> {
    None
}

fn default_stt_model() -> String {
    "whisper-1".to_string()
}

fn default_llm_model() -> String {
    "claude-haiku-4-5-20251001".to_string()
}

fn default_active_mode() -> String {
    "professional".to_string()
}

fn default_sounds_enabled() -> bool {
    true
}

fn default_paste_restore_delay_ms() -> u32 {
    100
}

fn default_launch_at_login() -> bool {
    false
}

fn default_onboarding_completed() -> bool {
    false
}

// ── VaaniConfig ────────────────────────────────────────────────────────────

/// Application configuration persisted as YAML at `~/.vaani/config.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VaaniConfig {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,

    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,

    #[serde(default = "default_vad_threshold")]
    pub vad_threshold: f32,

    #[serde(default = "default_max_recording_seconds")]
    pub max_recording_seconds: u32,

    #[serde(default = "default_microphone_device")]
    pub microphone_device: Option<u32>,

    #[serde(default = "default_stt_model")]
    pub stt_model: String,

    #[serde(default = "default_llm_model")]
    pub llm_model: String,

    #[serde(default = "default_active_mode")]
    pub active_mode: String,

    #[serde(default = "default_sounds_enabled")]
    pub sounds_enabled: bool,

    #[serde(default = "default_paste_restore_delay_ms")]
    pub paste_restore_delay_ms: u32,

    #[serde(default = "default_launch_at_login")]
    pub launch_at_login: bool,

    #[serde(default = "default_onboarding_completed")]
    pub onboarding_completed: bool,
}

impl Default for VaaniConfig {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            sample_rate: default_sample_rate(),
            vad_threshold: default_vad_threshold(),
            max_recording_seconds: default_max_recording_seconds(),
            microphone_device: default_microphone_device(),
            stt_model: default_stt_model(),
            llm_model: default_llm_model(),
            active_mode: default_active_mode(),
            sounds_enabled: default_sounds_enabled(),
            paste_restore_delay_ms: default_paste_restore_delay_ms(),
            launch_at_login: default_launch_at_login(),
            onboarding_completed: default_onboarding_completed(),
        }
    }
}

impl VaaniConfig {
    /// Validate configuration values.
    ///
    /// Returns `Ok(())` when all values are within acceptable ranges, or
    /// `Err(VaaniError::Config(...))` describing the first violation found.
    pub fn validate(&self) -> Result<(), VaaniError> {
        if !MODES.contains(&self.active_mode.as_str()) {
            return Err(VaaniError::Config(format!(
                "Unknown mode '{}'. Valid modes: {}",
                self.active_mode,
                MODES.join(", ")
            )));
        }

        if !(0.01..=0.5).contains(&self.vad_threshold) {
            return Err(VaaniError::Config(format!(
                "vad_threshold must be between 0.01 and 0.5, got {}",
                self.vad_threshold
            )));
        }

        if self.sample_rate == 0 {
            return Err(VaaniError::Config(
                "sample_rate must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

// ── File-system helpers ────────────────────────────────────────────────────

/// Returns `~/.vaani/`, creating the directory if it does not exist.
pub fn config_dir() -> PathBuf {
    let dir = dirs::home_dir()
        .unwrap_or_else(|| {
            warn!("Could not determine home directory, falling back to /tmp");
            PathBuf::from("/tmp")
        })
        .join(".vaani");

    if !dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            warn!("Failed to create config directory {}: {}", dir.display(), e);
        }
    }

    dir
}

/// Returns the path to the YAML configuration file (`~/.vaani/config.yaml`).
pub fn config_path() -> PathBuf {
    config_dir().join("config.yaml")
}

// ── Load / Save ────────────────────────────────────────────────────────────

/// Migrate legacy mode names that were removed in earlier versions.
fn migrate_mode(config: &mut VaaniConfig) {
    match config.active_mode.as_str() {
        "bullets" | "cleanup" => {
            info!(
                "Migrating deprecated mode '{}' to 'minimal'",
                config.active_mode
            );
            config.active_mode = "minimal".to_string();
        }
        _ => {}
    }
}

/// Load configuration from `~/.vaani/config.yaml`.
///
/// Falls back to `VaaniConfig::default()` when:
/// - the file does not exist,
/// - the file cannot be read, or
/// - the YAML is unparseable.
///
/// After loading, any legacy mode names are silently migrated.
pub fn load_config() -> VaaniConfig {
    let path = config_path();

    let mut config = match std::fs::read_to_string(&path) {
        Ok(contents) => match serde_yaml::from_str::<VaaniConfig>(&contents) {
            Ok(cfg) => {
                info!("Loaded config from {}", path.display());
                cfg
            }
            Err(e) => {
                warn!(
                    "Failed to parse config at {}: {}. Using defaults.",
                    path.display(),
                    e
                );
                VaaniConfig::default()
            }
        },
        Err(e) => {
            // `NotFound` is normal on first launch — log at info, not warn.
            if e.kind() == std::io::ErrorKind::NotFound {
                info!("No config file at {}. Using defaults.", path.display());
            } else {
                warn!(
                    "Failed to read config at {}: {}. Using defaults.",
                    path.display(),
                    e
                );
            }
            VaaniConfig::default()
        }
    };

    migrate_mode(&mut config);
    config
}

/// Persist configuration to `~/.vaani/config.yaml`.
pub fn save_config(config: &VaaniConfig) -> Result<(), VaaniError> {
    let path = config_path();

    let yaml = serde_yaml::to_string(config)
        .map_err(|e| VaaniError::Config(format!("Failed to serialize config: {e}")))?;

    std::fs::write(&path, yaml).map_err(|e| {
        VaaniError::Config(format!("Failed to write config to {}: {e}", path.display()))
    })?;

    info!("Saved config to {}", path.display());
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: write `contents` to a temp config file and load it via
    /// `serde_yaml` directly (to avoid coupling tests to the real home dir).
    fn parse_yaml(yaml: &str) -> Result<VaaniConfig, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Helper: round-trip through YAML strings.
    fn round_trip(config: &VaaniConfig) -> VaaniConfig {
        let yaml = serde_yaml::to_string(config).expect("serialize");
        serde_yaml::from_str(&yaml).expect("deserialize")
    }

    // 1. Default config has correct values
    #[test]
    fn default_config_has_correct_values() {
        let cfg = VaaniConfig::default();
        assert_eq!(cfg.hotkey, "alt");
        assert_eq!(cfg.sample_rate, 16_000);
        assert!((cfg.vad_threshold - 0.05).abs() < f32::EPSILON);
        assert_eq!(cfg.max_recording_seconds, 600);
        assert_eq!(cfg.microphone_device, None);
        assert_eq!(cfg.stt_model, "whisper-1");
        assert_eq!(cfg.llm_model, "claude-haiku-4-5-20251001");
        assert_eq!(cfg.active_mode, "professional");
        assert!(cfg.sounds_enabled);
        assert_eq!(cfg.paste_restore_delay_ms, 100);
        assert!(!cfg.launch_at_login);
        assert!(!cfg.onboarding_completed);
    }

    // 2. YAML round-trip: serialize then deserialize matches
    #[test]
    fn yaml_round_trip() {
        let original = VaaniConfig {
            hotkey: "ctrl".to_string(),
            sample_rate: 44_100,
            vad_threshold: 0.1,
            max_recording_seconds: 300,
            microphone_device: Some(2),
            stt_model: "whisper-1".to_string(),
            llm_model: "claude-haiku-4-5-20251001".to_string(),
            active_mode: "casual".to_string(),
            sounds_enabled: false,
            paste_restore_delay_ms: 200,
            launch_at_login: true,
            onboarding_completed: true,
        };

        let restored = round_trip(&original);
        assert_eq!(original, restored);
    }

    // 3. Partial YAML (missing fields) fills in defaults
    #[test]
    fn partial_yaml_fills_defaults() {
        let yaml = "hotkey: ctrl\nactive_mode: casual\n";
        let cfg: VaaniConfig = parse_yaml(yaml).expect("should parse");
        assert_eq!(cfg.hotkey, "ctrl");
        assert_eq!(cfg.active_mode, "casual");
        // Everything else should be default
        assert_eq!(cfg.sample_rate, 16_000);
        assert!((cfg.vad_threshold - 0.05).abs() < f32::EPSILON);
        assert_eq!(cfg.max_recording_seconds, 600);
        assert_eq!(cfg.microphone_device, None);
        assert!(cfg.sounds_enabled);
        assert_eq!(cfg.paste_restore_delay_ms, 100);
        assert!(!cfg.launch_at_login);
        assert!(!cfg.onboarding_completed);
    }

    // 4. Completely invalid YAML returns defaults (doesn't panic)
    #[test]
    fn invalid_yaml_returns_defaults() {
        let yaml = "{{{{not: yaml: at: all::::";
        let result = parse_yaml(yaml);
        // The parse itself fails, so load_config would fall back to default.
        assert!(result.is_err());

        // Confirm the fallback path produces a valid default.
        let cfg = VaaniConfig::default();
        assert_eq!(cfg.active_mode, "professional");
    }

    // 5. Missing file returns defaults
    #[test]
    fn missing_file_returns_defaults() {
        let tmp = TempDir::new().expect("create temp dir");
        let fake_path = tmp.path().join("nonexistent.yaml");

        // Simulate what load_config does: try to read, fall back to default.
        let cfg = match std::fs::read_to_string(&fake_path) {
            Ok(contents) => serde_yaml::from_str(&contents).unwrap_or_default(),
            Err(_) => VaaniConfig::default(),
        };

        assert_eq!(cfg.hotkey, "alt");
        assert_eq!(cfg.active_mode, "professional");
    }

    // 6. Mode migration: "bullets" -> "minimal", "cleanup" -> "minimal"
    #[test]
    fn mode_migration_bullets_to_minimal() {
        let mut cfg = VaaniConfig {
            active_mode: "bullets".to_string(),
            ..Default::default()
        };
        migrate_mode(&mut cfg);
        assert_eq!(cfg.active_mode, "minimal");
    }

    #[test]
    fn mode_migration_cleanup_to_minimal() {
        let mut cfg = VaaniConfig {
            active_mode: "cleanup".to_string(),
            ..Default::default()
        };
        migrate_mode(&mut cfg);
        assert_eq!(cfg.active_mode, "minimal");
    }

    #[test]
    fn mode_migration_leaves_valid_modes_untouched() {
        for &mode in MODES {
            let mut cfg = VaaniConfig {
                active_mode: mode.to_string(),
                ..Default::default()
            };
            migrate_mode(&mut cfg);
            assert_eq!(cfg.active_mode, mode);
        }
    }

    // 7. Validate: valid config passes
    #[test]
    fn validate_valid_config_passes() {
        let cfg = VaaniConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_all_modes_pass() {
        for &mode in MODES {
            let cfg = VaaniConfig {
                active_mode: mode.to_string(),
                ..Default::default()
            };
            assert!(cfg.validate().is_ok(), "mode '{}' should be valid", mode);
        }
    }

    // 8. Validate: invalid mode fails
    #[test]
    fn validate_invalid_mode_fails() {
        let cfg = VaaniConfig {
            active_mode: "nonexistent".to_string(),
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("nonexistent"),
            "error should name the bad mode"
        );
        assert!(msg.contains("Valid modes"), "error should list valid modes");
    }

    // 9. Validate: vad_threshold out of range fails
    #[test]
    fn validate_vad_threshold_too_low() {
        let cfg = VaaniConfig {
            vad_threshold: 0.001,
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("vad_threshold"));
    }

    #[test]
    fn validate_vad_threshold_too_high() {
        let cfg = VaaniConfig {
            vad_threshold: 0.9,
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("vad_threshold"));
    }

    #[test]
    fn validate_vad_threshold_boundaries() {
        // Lower bound (0.01) should pass
        let cfg_low = VaaniConfig {
            vad_threshold: 0.01,
            ..Default::default()
        };
        assert!(cfg_low.validate().is_ok());

        // Upper bound (0.5) should pass
        let cfg_high = VaaniConfig {
            vad_threshold: 0.5,
            ..Default::default()
        };
        assert!(cfg_high.validate().is_ok());
    }

    #[test]
    fn validate_sample_rate_zero_fails() {
        let cfg = VaaniConfig {
            sample_rate: 0,
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("sample_rate"));
    }

    // 10. MODES contains exactly 5 entries with correct names
    #[test]
    fn modes_constant_is_correct() {
        assert_eq!(MODES.len(), 5);
        assert_eq!(MODES[0], "minimal");
        assert_eq!(MODES[1], "professional");
        assert_eq!(MODES[2], "casual");
        assert_eq!(MODES[3], "code");
        assert_eq!(MODES[4], "funny");
    }

    // ── Integration-style: write and read from disk ────────────────────────

    #[test]
    fn save_and_load_from_disk() {
        let tmp = TempDir::new().expect("create temp dir");
        let path = tmp.path().join("config.yaml");

        let original = VaaniConfig {
            hotkey: "meta".to_string(),
            active_mode: "code".to_string(),
            sounds_enabled: false,
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&original).expect("serialize");
        std::fs::write(&path, &yaml).expect("write");

        let contents = std::fs::read_to_string(&path).expect("read");
        let loaded: VaaniConfig = serde_yaml::from_str(&contents).expect("deserialize");

        assert_eq!(original, loaded);
    }

    #[test]
    fn partial_yaml_from_disk() {
        let tmp = TempDir::new().expect("create temp dir");
        let path = tmp.path().join("config.yaml");

        std::fs::write(&path, "active_mode: funny\n").expect("write");

        let contents = std::fs::read_to_string(&path).expect("read");
        let cfg: VaaniConfig = serde_yaml::from_str(&contents).expect("deserialize");

        assert_eq!(cfg.active_mode, "funny");
        assert_eq!(cfg.hotkey, "alt");
        assert_eq!(cfg.sample_rate, 16_000);
    }

    #[test]
    fn migration_via_yaml_on_disk() {
        let tmp = TempDir::new().expect("create temp dir");
        let path = tmp.path().join("config.yaml");

        std::fs::write(&path, "active_mode: bullets\n").expect("write");

        let contents = std::fs::read_to_string(&path).expect("read");
        let mut cfg: VaaniConfig = serde_yaml::from_str(&contents).expect("deserialize");
        migrate_mode(&mut cfg);

        assert_eq!(cfg.active_mode, "minimal");
    }
}
