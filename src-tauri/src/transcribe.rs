//! Sends audio to the OpenAI Whisper API for transcription.
//!
//! The primary entry point is [`transcribe()`], which posts WAV audio to the
//! Whisper endpoint and returns the recognized text. A lower-level
//! [`transcribe_with_url()`] variant accepts a custom base URL for testing.

use crate::error::VaaniError;
use reqwest::multipart;

/// Default OpenAI Whisper transcription endpoint.
const DEFAULT_WHISPER_URL: &str = "https://api.openai.com/v1/audio/transcriptions";

/// Typed representation of the Whisper API JSON response.
#[derive(serde::Deserialize)]
struct WhisperResponse {
    text: String,
}

/// Transcribe audio using the default OpenAI Whisper endpoint.
///
/// # Arguments
///
/// * `client` - A reusable `reqwest::Client` (connection pooling, timeouts, etc.).
/// * `api_key` - OpenAI API key. Must not be empty.
/// * `audio_wav` - Raw WAV-encoded audio bytes.
/// * `model` - Whisper model identifier (e.g. `"whisper-1"`).
///
/// # Errors
///
/// Returns [`VaaniError::MissingApiKey`] if the key is empty,
/// [`VaaniError::NoSpeechDetected`] if Whisper returns empty text, or
/// [`VaaniError::Transcribe`] on any HTTP / parsing failure.
pub async fn transcribe(
    client: &reqwest::Client,
    api_key: &str,
    audio_wav: &[u8],
    model: &str,
) -> Result<String, VaaniError> {
    transcribe_with_url(client, api_key, audio_wav, model, DEFAULT_WHISPER_URL).await
}

/// Transcribe audio, allowing the caller to override the endpoint URL.
///
/// This is the implementation behind [`transcribe()`]. Accepting a custom URL
/// makes it possible to point at a local mock server in integration tests.
pub async fn transcribe_with_url(
    client: &reqwest::Client,
    api_key: &str,
    audio_wav: &[u8],
    model: &str,
    base_url: &str,
) -> Result<String, VaaniError> {
    if api_key.is_empty() {
        return Err(VaaniError::MissingApiKey("OpenAI".into()));
    }

    tracing::debug!(
        url = base_url,
        model = model,
        audio_bytes = audio_wav.len(),
        "sending transcription request"
    );

    let file_part = multipart::Part::bytes(audio_wav.to_vec())
        .file_name("recording.wav")
        .mime_str("audio/wav")
        .map_err(|e| VaaniError::Transcribe(format!("failed to build mime type: {e}")))?;

    let form = multipart::Form::new()
        .part("file", file_part)
        .text("model", model.to_owned());

    let response = client
        .post(base_url)
        .header("Authorization", format!("Bearer {api_key}"))
        .multipart(form)
        .send()
        .await
        .map_err(|e| VaaniError::Transcribe(format!("request failed: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".into());
        return Err(VaaniError::Transcribe(format!("HTTP {status}: {body}")));
    }

    let whisper: WhisperResponse = response
        .json()
        .await
        .map_err(|e| VaaniError::Transcribe(format!("failed to parse response: {e}")))?;

    if whisper.text.is_empty() {
        return Err(VaaniError::NoSpeechDetected);
    }

    tracing::debug!(chars = whisper.text.len(), "transcription complete");

    Ok(whisper.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- API-key validation ----

    #[tokio::test]
    async fn empty_api_key_returns_missing_api_key_error() {
        let client = reqwest::Client::new();
        let result = transcribe(&client, "", b"fake-wav-data", "whisper-1").await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            VaaniError::MissingApiKey(provider) => {
                assert_eq!(provider, "OpenAI");
            }
            other => panic!("expected MissingApiKey, got: {other:?}"),
        }
    }

    // ---- Error variant names compile and are matchable ----

    #[test]
    fn error_variants_are_constructible() {
        // Verify that the specific variants we rely on exist and can be constructed.
        let _transcribe = VaaniError::Transcribe("test".into());
        let _missing = VaaniError::MissingApiKey("OpenAI".into());
        let _no_speech = VaaniError::NoSpeechDetected;
    }

    // ---- WhisperResponse deserialization ----

    #[test]
    fn whisper_response_deserializes_from_json() {
        let json = r#"{"text": "Hello, world!"}"#;
        let resp: WhisperResponse =
            serde_json::from_str(json).expect("deserialization should succeed");
        assert_eq!(resp.text, "Hello, world!");
    }

    #[test]
    fn whisper_response_deserializes_empty_text() {
        let json = r#"{"text": ""}"#;
        let resp: WhisperResponse =
            serde_json::from_str(json).expect("deserialization should succeed");
        assert!(resp.text.is_empty());
    }
}
