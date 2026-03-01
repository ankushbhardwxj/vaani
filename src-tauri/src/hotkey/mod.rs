pub mod macos;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rdev::{self, EventType, Key};

use crate::error::VaaniError;

/// Events emitted by the global hotkey listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

/// Parses a hotkey string into the corresponding [`rdev::Key`].
///
/// Supports modifier key names (case-insensitive):
/// - `"alt"` -> [`Key::Alt`]
/// - `"ctrl"` -> [`Key::ControlLeft`]
/// - `"shift"` -> [`Key::ShiftLeft`]
/// - `"meta"` / `"cmd"` -> [`Key::MetaLeft`]
///
/// Returns [`VaaniError::Hotkey`] for unrecognized strings.
pub fn parse_hotkey(hotkey: &str) -> Result<Key, VaaniError> {
    match hotkey.trim().to_lowercase().as_str() {
        "alt" => Ok(Key::Alt),
        "ctrl" => Ok(Key::ControlLeft),
        "shift" => Ok(Key::ShiftLeft),
        "meta" | "cmd" => Ok(Key::MetaLeft),
        other => Err(VaaniError::Hotkey(format!("Unknown hotkey: {other}"))),
    }
}

/// Starts a global hotkey listener that tracks press/release of the specified
/// modifier key.
///
/// The listener runs on a dedicated background thread because [`rdev::listen`]
/// blocks indefinitely. The `callback` is invoked with [`HotkeyEvent::Pressed`]
/// on key-down and [`HotkeyEvent::Released`] on key-up.
///
/// # Errors
///
/// Returns [`VaaniError::Hotkey`] if:
/// - The hotkey string cannot be parsed (see [`parse_hotkey`]).
/// - The background listener thread fails to spawn.
pub fn start_listener(
    hotkey: &str,
    callback: impl Fn(HotkeyEvent) + Send + 'static,
) -> Result<(), VaaniError> {
    let target_key = parse_hotkey(hotkey)?;
    let is_pressed = Arc::new(AtomicBool::new(false));

    let is_pressed_clone = Arc::clone(&is_pressed);

    tracing::info!(hotkey = %hotkey, key = ?target_key, "starting global hotkey listener");

    std::thread::Builder::new()
        .name("vaani-hotkey-listener".into())
        .spawn(move || {
            let handler = move |event: rdev::Event| {
                match event.event_type {
                    EventType::KeyPress(key) if key == target_key => {
                        // Only fire Pressed on the initial key-down, not on
                        // auto-repeat (where is_pressed is already true).
                        if !is_pressed_clone.swap(true, Ordering::SeqCst) {
                            tracing::debug!(key = ?target_key, "hotkey pressed");
                            callback(HotkeyEvent::Pressed);
                        }
                    }
                    EventType::KeyRelease(key) if key == target_key => {
                        if is_pressed_clone.swap(false, Ordering::SeqCst) {
                            tracing::debug!(key = ?target_key, "hotkey released");
                            callback(HotkeyEvent::Released);
                        }
                    }
                    _ => {}
                }
            };

            if let Err(err) = rdev::listen(handler) {
                tracing::error!(?err, "global hotkey listener failed");
            }
        })
        .map_err(|e| VaaniError::Hotkey(format!("failed to spawn listener thread: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // 1. parse_hotkey("alt") returns Ok with correct key
    // -----------------------------------------------------------------------
    #[test]
    fn parse_hotkey_alt() {
        let key = parse_hotkey("alt").expect("should parse 'alt'");
        assert_eq!(key, Key::Alt);
    }

    // -----------------------------------------------------------------------
    // 2. parse_hotkey("ALT") is case-insensitive
    // -----------------------------------------------------------------------
    #[test]
    fn parse_hotkey_case_insensitive() {
        let key = parse_hotkey("ALT").expect("should parse 'ALT'");
        assert_eq!(key, Key::Alt);
    }

    // -----------------------------------------------------------------------
    // 3. parse_hotkey("ctrl") returns Ok
    // -----------------------------------------------------------------------
    #[test]
    fn parse_hotkey_ctrl() {
        let key = parse_hotkey("ctrl").expect("should parse 'ctrl'");
        assert_eq!(key, Key::ControlLeft);
    }

    // -----------------------------------------------------------------------
    // 4. parse_hotkey("shift") returns Ok
    // -----------------------------------------------------------------------
    #[test]
    fn parse_hotkey_shift() {
        let key = parse_hotkey("shift").expect("should parse 'shift'");
        assert_eq!(key, Key::ShiftLeft);
    }

    // -----------------------------------------------------------------------
    // 5. parse_hotkey("meta") returns Ok
    // -----------------------------------------------------------------------
    #[test]
    fn parse_hotkey_meta() {
        let key = parse_hotkey("meta").expect("should parse 'meta'");
        assert_eq!(key, Key::MetaLeft);
    }

    // -----------------------------------------------------------------------
    // 6. parse_hotkey("cmd") returns Ok (alias for meta)
    // -----------------------------------------------------------------------
    #[test]
    fn parse_hotkey_cmd_alias() {
        let key = parse_hotkey("cmd").expect("should parse 'cmd'");
        assert_eq!(key, Key::MetaLeft);
    }

    // -----------------------------------------------------------------------
    // 7. parse_hotkey("invalid_key") returns Err
    // -----------------------------------------------------------------------
    #[test]
    fn parse_hotkey_invalid() {
        let err = parse_hotkey("invalid_key").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Unknown hotkey"),
            "expected 'Unknown hotkey' in: {msg}"
        );
        assert!(
            msg.contains("invalid_key"),
            "expected 'invalid_key' in: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // 8. parse_hotkey("") returns Err
    // -----------------------------------------------------------------------
    #[test]
    fn parse_hotkey_empty_string() {
        let err = parse_hotkey("").unwrap_err();
        assert!(err.to_string().contains("Unknown hotkey"));
    }

    // -----------------------------------------------------------------------
    // 9. HotkeyEvent variants are distinct
    // -----------------------------------------------------------------------
    #[test]
    fn hotkey_event_variants_are_distinct() {
        assert_ne!(HotkeyEvent::Pressed, HotkeyEvent::Released);
    }
}
