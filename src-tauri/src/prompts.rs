//! Prompt loading and assembly for Vaani's text enhancement pipeline.
//!
//! Bundled prompts are embedded at compile time via `include_str!`.
//! User overrides at `~/.vaani/prompts/` take priority when present.

use tracing::debug;

/// Default fallback when no prompt files exist at all.
const DEFAULT_FALLBACK: &str = "You are a helpful writing assistant. \
    Clean up the following text, fixing grammar, removing filler words, \
    and improving clarity while preserving the speaker's intent and meaning.";

// ── Bundled prompts (embedded at compile time) ─────────────────────────────

const BUNDLED_SYSTEM: &str = include_str!("../prompts/system.txt");
const BUNDLED_CONTEXT: &str = include_str!("../prompts/context.txt");
const BUNDLED_MODE_MINIMAL: &str = include_str!("../prompts/modes/minimal.txt");
const BUNDLED_MODE_PROFESSIONAL: &str = include_str!("../prompts/modes/professional.txt");
const BUNDLED_MODE_CASUAL: &str = include_str!("../prompts/modes/casual.txt");
const BUNDLED_MODE_CODE: &str = include_str!("../prompts/modes/code.txt");
const BUNDLED_MODE_FUNNY: &str = include_str!("../prompts/modes/funny.txt");

/// Path to the bundled prompts directory (relative to project).
/// In production, these are embedded or resolved from the app bundle.
#[allow(dead_code)]
const BUNDLED_PROMPTS_DIR: &str = "prompts";

// ── Helpers ────────────────────────────────────────────────────────────────

/// Return the user-override prompts directory (`~/.vaani/prompts/`).
fn user_prompts_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".vaani").join("prompts"))
}

/// Look up the bundled content for the given filename.
///
/// Returns `None` for unrecognised filenames (e.g. unknown modes).
fn bundled_content(filename: &str) -> Option<&'static str> {
    match filename {
        "system.txt" => Some(BUNDLED_SYSTEM),
        "context.txt" => Some(BUNDLED_CONTEXT),
        "modes/minimal.txt" => Some(BUNDLED_MODE_MINIMAL),
        "modes/professional.txt" => Some(BUNDLED_MODE_PROFESSIONAL),
        "modes/casual.txt" => Some(BUNDLED_MODE_CASUAL),
        "modes/code.txt" => Some(BUNDLED_MODE_CODE),
        "modes/funny.txt" => Some(BUNDLED_MODE_FUNNY),
        _ => None,
    }
}

/// Load a prompt file, checking user override first, then bundled.
///
/// Returns `None` if neither source has the file, or if the content is empty
/// after trimming whitespace.
fn load_prompt_file(filename: &str) -> Option<String> {
    // 1. Check user override at ~/.vaani/prompts/{filename}
    if let Some(user_dir) = user_prompts_dir() {
        let user_path = user_dir.join(filename);
        if let Ok(content) = std::fs::read_to_string(&user_path) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                debug!(path = %user_path.display(), "Loaded user prompt override");
                return Some(trimmed.to_string());
            }
        }
    }

    // 2. Fall back to bundled (compile-time embedded) content
    if let Some(content) = bundled_content(filename) {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            debug!(filename, "Loaded bundled prompt");
            return Some(trimmed.to_string());
        }
    }

    debug!(filename, "No prompt found");
    None
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Build the full system prompt for the given enhancement mode.
///
/// Loads three components — `system.txt`, `context.txt`, and `modes/{mode}.txt`
/// — joining non-empty parts with double newlines. If all parts are missing,
/// returns a sensible hardcoded default.
pub fn build_system_prompt(mode: &str) -> String {
    let parts: Vec<String> = [
        load_prompt_file("system.txt"),
        load_prompt_file("context.txt"),
        load_prompt_file(&format!("modes/{mode}.txt")),
    ]
    .into_iter()
    .flatten()
    .collect();

    if parts.is_empty() {
        debug!("No prompt files found, using default fallback");
        return DEFAULT_FALLBACK.to_string();
    }

    let prompt = parts.join("\n\n");
    debug!(mode, len = prompt.len(), "Assembled system prompt");
    prompt
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MODES;

    #[test]
    fn build_system_prompt_professional_is_not_empty() {
        let prompt = build_system_prompt("professional");
        assert!(
            !prompt.is_empty(),
            "Professional prompt should not be empty"
        );
    }

    #[test]
    fn build_system_prompt_unknown_mode_still_works() {
        // Unknown mode has no mode file, but system.txt + context.txt still load.
        let prompt = build_system_prompt("nonexistent_mode_xyz");
        assert!(
            !prompt.is_empty(),
            "Unknown mode should still produce output"
        );
        // It should NOT be the bare default fallback because system.txt exists.
        assert_ne!(
            prompt, DEFAULT_FALLBACK,
            "Should include bundled system/context, not just the fallback"
        );
    }

    #[test]
    fn build_system_prompt_all_modes_produce_output() {
        for &mode in MODES {
            let prompt = build_system_prompt(mode);
            assert!(
                !prompt.is_empty(),
                "Mode '{mode}' should produce a non-empty prompt"
            );
        }
    }

    #[test]
    fn build_system_prompt_contains_mode_content() {
        let prompt = build_system_prompt("professional");
        let lower = prompt.to_lowercase();
        assert!(
            lower.contains("formal") || lower.contains("professional"),
            "Professional prompt should mention 'formal' or 'professional', got: {prompt}"
        );
    }

    #[test]
    fn default_fallback_when_no_prompts() {
        // Directly verify the constant is sensible.
        assert!(DEFAULT_FALLBACK.contains("grammar"));
        assert!(DEFAULT_FALLBACK.contains("filler words"));
        assert!(DEFAULT_FALLBACK.contains("intent"));
    }

    #[test]
    fn load_prompt_file_returns_none_for_missing() {
        let result = load_prompt_file("this_file_definitely_does_not_exist.txt");
        assert!(
            result.is_none(),
            "Missing file should return None, got: {result:?}"
        );
    }

    #[test]
    fn bundled_system_txt_is_loaded() {
        let content = load_prompt_file("system.txt");
        assert!(content.is_some(), "system.txt should be loadable");
        let text = content.expect("already asserted");
        assert!(
            text.contains("filler"),
            "system.txt should mention filler words"
        );
    }

    #[test]
    fn bundled_context_txt_is_loaded() {
        let content = load_prompt_file("context.txt");
        assert!(content.is_some(), "context.txt should be loadable");
    }

    #[test]
    fn prompt_parts_are_joined_with_double_newline() {
        let prompt = build_system_prompt("casual");
        // The prompt should contain system + context + mode, separated by \n\n
        assert!(
            prompt.contains("\n\n"),
            "Parts should be joined with double newlines"
        );
    }

    #[test]
    fn each_mode_prompt_has_distinct_content() {
        let prompts: Vec<String> = MODES.iter().map(|&m| build_system_prompt(m)).collect();

        // Each pair of modes should produce different prompts
        for i in 0..prompts.len() {
            for j in (i + 1)..prompts.len() {
                assert_ne!(
                    prompts[i], prompts[j],
                    "Modes '{}' and '{}' should produce different prompts",
                    MODES[i], MODES[j]
                );
            }
        }
    }
}
