//! Clipboard-based text pasting at the current cursor position.
//!
//! The primary workflow is:
//! 1. Save the user's current clipboard contents.
//! 2. Place new text on the clipboard.
//! 3. Simulate the platform paste keystroke (Cmd+V on macOS, Ctrl+V on Linux).
//! 4. Wait for the target application to consume the paste.
//! 5. Restore the original clipboard contents.

use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use tracing::debug;

use crate::error::VaaniError;

// ── Public API ──────────────────────────────────────────────────────────────

/// Paste `text` at the current cursor position by writing it to the clipboard
/// and simulating the platform paste keystroke.
///
/// The user's original clipboard contents are saved before the operation and
/// restored after a configurable delay (`restore_delay_ms`), giving the
/// foreground application time to consume the paste event.
///
/// Returns `Ok(())` immediately if `text` is empty.
pub fn paste_text(text: &str, restore_delay_ms: u64) -> Result<(), VaaniError> {
    if text.is_empty() {
        debug!("paste_text called with empty string, nothing to do");
        return Ok(());
    }

    let mut clipboard = Clipboard::new()
        .map_err(|e| VaaniError::Paste(format!("Failed to access clipboard: {e}")))?;

    // ── Save original clipboard contents ────────────────────────────────
    let original = clipboard.get_text().ok();
    debug!(
        original_len = original.as_ref().map_or(0, |s| s.len()),
        "Saved original clipboard contents"
    );

    // ── Set clipboard to the new text ───────────────────────────────────
    clipboard
        .set_text(text)
        .map_err(|e| VaaniError::Paste(format!("Failed to access clipboard: {e}")))?;
    debug!(text_len = text.len(), "Clipboard set with new text");

    // ── Simulate paste keystroke ────────────────────────────────────────
    simulate_paste()?;
    debug!("Paste keystroke simulated");

    // ── Wait, then restore original clipboard ───────────────────────────
    thread::sleep(Duration::from_millis(restore_delay_ms));

    match original {
        Some(ref contents) => {
            clipboard
                .set_text(contents)
                .map_err(|e| VaaniError::Paste(format!("Failed to access clipboard: {e}")))?;
            debug!("Original clipboard contents restored");
        }
        None => {
            // The clipboard was empty (or non-text) before; clear it.
            clipboard
                .clear()
                .map_err(|e| VaaniError::Paste(format!("Failed to access clipboard: {e}")))?;
            debug!("Clipboard cleared (was empty before paste)");
        }
    }

    Ok(())
}

/// Simulate typing `text` character by character using synthetic key events.
///
/// This is intended for streaming paste (Phase 3). The function signature is
/// defined now for forward compatibility; the implementation types each
/// character individually via `enigo`.
///
/// Returns `Ok(())` immediately if `text` is empty.
pub fn type_text(text: &str) -> Result<(), VaaniError> {
    if text.is_empty() {
        debug!("type_text called with empty string, nothing to do");
        return Ok(());
    }

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| VaaniError::Paste(format!("Failed to simulate keystroke: {e}")))?;

    debug!(text_len = text.len(), "Typing text character by character");

    enigo
        .text(text)
        .map_err(|e| VaaniError::Paste(format!("Failed to simulate keystroke: {e}")))?;

    debug!("Finished typing text");
    Ok(())
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// Simulate the platform-specific paste keystroke.
///
/// - macOS: Cmd+V (`Meta` + `v`)
/// - Linux: Ctrl+V (`Control` + `v`)
fn simulate_paste() -> Result<(), VaaniError> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| VaaniError::Paste(format!("Failed to simulate keystroke: {e}")))?;

    let modifier = platform_paste_modifier();

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| VaaniError::Paste(format!("Failed to simulate keystroke: {e}")))?;

    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| {
            // Attempt to release the modifier even if the 'v' key fails,
            // to avoid leaving the modifier stuck.
            let _ = enigo.key(modifier, Direction::Release);
            VaaniError::Paste(format!("Failed to simulate keystroke: {e}"))
        })?;

    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| VaaniError::Paste(format!("Failed to simulate keystroke: {e}")))?;

    Ok(())
}

/// Return the platform-specific modifier key used for paste.
#[cfg(target_os = "macos")]
fn platform_paste_modifier() -> Key {
    Key::Meta
}

#[cfg(target_os = "linux")]
fn platform_paste_modifier() -> Key {
    Key::Control
}

// Fallback for other platforms (e.g. Windows) so the crate still compiles,
// though Vaani does not officially support them.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_paste_modifier() -> Key {
    Key::Control
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_text_empty_string_returns_ok() {
        // An empty string should short-circuit without touching the clipboard
        // or simulating any keystrokes.
        let result = paste_text("", 50);
        assert!(
            result.is_ok(),
            "paste_text with empty string should return Ok"
        );
    }

    #[test]
    fn type_text_empty_string_returns_ok() {
        // An empty string should short-circuit without creating an Enigo instance.
        let result = type_text("");
        assert!(
            result.is_ok(),
            "type_text with empty string should return Ok"
        );
    }

    #[test]
    fn platform_paste_modifier_is_defined() {
        // Ensure the compile-time platform selection yields a valid key.
        let key = platform_paste_modifier();
        // On macOS CI this will be Key::Meta; on Linux CI this will be Key::Control.
        // We simply assert that the function returns without panicking and that the
        // returned key is the expected variant for the current platform.
        #[cfg(target_os = "macos")]
        assert!(matches!(key, Key::Meta));

        #[cfg(target_os = "linux")]
        assert!(matches!(key, Key::Control));

        // Suppress unused-variable warning on other platforms.
        let _ = key;
    }
}
