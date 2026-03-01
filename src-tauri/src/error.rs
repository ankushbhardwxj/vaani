use serde::Serialize;

/// Unified error type for the entire Vaani application.
///
/// All error messages are written to be understandable by end users (Product Managers),
/// not just developers. Avoid technical jargon in display messages.
#[derive(Debug, thiserror::Error)]
pub enum VaaniError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Voice detection error: {0}")]
    Vad(String),

    #[error("Transcription failed: {0}")]
    Transcribe(String),

    #[error("Text enhancement failed: {0}")]
    Enhance(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Keychain error: {0}")]
    Keychain(String),

    #[error("File error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Cannot {action} while {state}")]
    InvalidTransition { action: String, state: String },

    #[error("API key not configured for {0}. Please add it in Settings.")]
    MissingApiKey(String),

    #[error("Recording contains no speech. Try speaking louder or closer to the microphone.")]
    NoSpeechDetected,

    #[error("Hotkey error: {0}")]
    Hotkey(String),

    #[error("Paste error: {0}")]
    Paste(String),
}

/// Serialize implementation required for Tauri commands to return `Result<T, VaaniError>`.
impl Serialize for VaaniError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_are_user_readable() {
        let err = VaaniError::MissingApiKey("OpenAI".to_string());
        let msg = err.to_string();
        assert!(msg.contains("OpenAI"));
        assert!(msg.contains("Settings"));
    }

    #[test]
    fn no_speech_error_gives_helpful_message() {
        let err = VaaniError::NoSpeechDetected;
        let msg = err.to_string();
        assert!(msg.contains("no speech"));
        assert!(msg.contains("louder"));
    }

    #[test]
    fn invalid_transition_explains_context() {
        let err = VaaniError::InvalidTransition {
            action: "start recording".to_string(),
            state: "already recording".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Cannot start recording while already recording"
        );
    }

    #[test]
    fn error_serializes_to_string() {
        let err = VaaniError::Config("bad value".to_string());
        let json = serde_json::to_string(&err).expect("serialization should succeed");
        assert_eq!(json, "\"Configuration error: bad value\"");
    }

    #[test]
    fn io_error_converts() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: VaaniError = io_err.into();
        assert!(err.to_string().contains("file missing"));
    }
}
