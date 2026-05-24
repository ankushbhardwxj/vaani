//! Sound effect playback for recording start/stop feedback.
//!
//! Uses `rodio` to play bundled WAV files asynchronously.
//! Sound playback is non-blocking and tolerates missing files
//! or unavailable audio output devices gracefully.

use std::io::Cursor;
use std::path::PathBuf;

use crate::error::VaaniError;

/// Available sound effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundEffect {
    RecordStart,
    RecordStop,
}

/// Returns the filename for a sound effect.
fn sound_filename(effect: SoundEffect) -> &'static str {
    match effect {
        SoundEffect::RecordStart => "record_start.wav",
        SoundEffect::RecordStop => "record_stop.wav",
    }
}

/// Returns the expected file path for a sound effect.
///
/// Looks in the `sounds/` directory relative to the executable,
/// falling back to `src-tauri/sounds/` for development.
pub fn sound_file_path(effect: SoundEffect) -> PathBuf {
    let filename = sound_filename(effect);

    // Try relative to executable first (production)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let prod_path = dir.join("sounds").join(filename);
            if prod_path.exists() {
                return prod_path;
            }
            // macOS .app bundle: Resources directory
            let resources_path = dir
                .join("..")
                .join("Resources")
                .join("sounds")
                .join(filename);
            if resources_path.exists() {
                return resources_path;
            }
        }
    }

    // Fallback: development path
    PathBuf::from("src-tauri").join("sounds").join(filename)
}

/// Play a sound effect asynchronously (non-blocking).
///
/// If the sound file is missing or audio output is unavailable,
/// logs a warning and returns Ok — sounds are optional.
pub fn play_sound(effect: SoundEffect) -> Result<(), VaaniError> {
    let path = sound_file_path(effect);

    let wav_bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "Sound file not found, skipping playback"
            );
            return Ok(());
        }
    };

    // Spawn a thread for playback so we don't block the caller.
    // The thread owns the OutputStream and Sink, keeping them alive
    // until playback completes.
    std::thread::Builder::new()
        .name("vaani-sound".into())
        .spawn(move || {
            let output = match rodio::OutputStream::try_default() {
                Ok(output) => output,
                Err(e) => {
                    tracing::warn!("No audio output device: {e}");
                    return;
                }
            };
            let (_stream, handle) = output;

            let cursor = Cursor::new(wav_bytes);
            let source = match rodio::Decoder::new(cursor) {
                Ok(source) => source,
                Err(e) => {
                    tracing::warn!("Failed to decode sound: {e}");
                    return;
                }
            };

            let sink = match rodio::Sink::try_new(&handle) {
                Ok(sink) => sink,
                Err(e) => {
                    tracing::warn!("Failed to create audio sink: {e}");
                    return;
                }
            };

            sink.append(source);
            sink.sleep_until_end();
        })
        .map_err(|e| VaaniError::Audio(format!("Failed to spawn sound thread: {e}")))?;

    tracing::debug!(effect = ?effect, "Playing sound");
    Ok(())
}

/// Play a sound effect only if sounds are enabled.
pub fn play_sound_if_enabled(effect: SoundEffect, enabled: bool) -> Result<(), VaaniError> {
    if !enabled {
        return Ok(());
    }
    play_sound(effect)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_effect_variants_are_distinct() {
        assert_ne!(SoundEffect::RecordStart, SoundEffect::RecordStop);
    }

    #[test]
    fn sound_file_path_maps_correctly() {
        let start_path = sound_file_path(SoundEffect::RecordStart);
        let stop_path = sound_file_path(SoundEffect::RecordStop);

        assert!(
            start_path.to_string_lossy().contains("record_start"),
            "Start path should contain 'record_start': {start_path:?}"
        );
        assert!(
            stop_path.to_string_lossy().contains("record_stop"),
            "Stop path should contain 'record_stop': {stop_path:?}"
        );
    }

    #[test]
    fn play_sound_if_enabled_false_returns_ok() {
        let result = play_sound_if_enabled(SoundEffect::RecordStart, false);
        assert!(result.is_ok());
    }

    #[test]
    fn play_sound_missing_file_returns_ok() {
        // With no sound files present, play_sound should gracefully return Ok
        let result = play_sound(SoundEffect::RecordStart);
        assert!(
            result.is_ok(),
            "Missing sound file should not cause an error"
        );
    }
}
