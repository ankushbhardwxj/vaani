//! Pipeline orchestrator for the Vaani voice-to-text workflow.
//!
//! Coordinates the full flow: record → process → transcribe → paste.
//! This module ties together audio capture, transcription, and output.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::audio::capture::AudioBuffer;
use crate::audio::processing::{encode_wav, normalize_gain};
use crate::config::VaaniConfig;
use crate::enhance::enhance_streaming;
use crate::error::VaaniError;
use crate::keychain::create_secret_storage;
use crate::output::paste::{paste_text, type_text};
use crate::prompts::build_system_prompt;
use crate::state::StateMachine;
use crate::transcribe::transcribe;

/// Shared application state accessible from Tauri commands and the pipeline.
///
/// Note: `AudioRecorder` holds a cpal `Stream` which is not `Send`.
/// We only store the `AudioBuffer` (which IS Send+Sync) here.
/// The actual `AudioRecorder` is created and owned on the thread that starts recording.
pub struct VaaniApp {
    pub state: Arc<Mutex<StateMachine>>,
    pub config: Arc<Mutex<VaaniConfig>>,
    pub audio_buffer: AudioBuffer,
    pub http_client: reqwest::Client,
}

impl VaaniApp {
    /// Creates a new VaaniApp with the given config.
    pub fn new(config: VaaniConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .pool_max_idle_per_host(2)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            state: Arc::new(Mutex::new(StateMachine::new())),
            config: Arc::new(Mutex::new(config)),
            audio_buffer: AudioBuffer::new(),
            http_client,
        }
    }

    /// Process captured audio samples: normalize → encode → transcribe → paste.
    ///
    /// Called after recording stops. Takes the samples from the audio buffer.
    pub async fn process_and_paste(&self) -> Result<String, VaaniError> {
        let samples = self.audio_buffer.take_samples();
        let config = self
            .config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        // Process audio
        let result = self.process_audio(samples, &config).await;

        // Always transition back to idle
        if let Err(e) = self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .finish_processing()
        {
            tracing::error!("Failed to transition to idle: {e}");
        }

        result
    }

    /// Internal: process audio samples through the pipeline.
    async fn process_audio(
        &self,
        samples: Vec<f32>,
        config: &VaaniConfig,
    ) -> Result<String, VaaniError> {
        if samples.is_empty() {
            return Err(VaaniError::NoSpeechDetected);
        }

        tracing::info!(
            sample_count = samples.len(),
            "Processing audio ({:.1}s)",
            samples.len() as f32 / config.sample_rate as f32
        );

        // Normalize audio gain
        let normalized = normalize_gain(&samples, -20.0);

        // Encode to WAV
        let wav_bytes = encode_wav(&normalized, config.sample_rate)?;

        // Transcribe via Whisper API
        let api_key = resolve_api_key(
            "openai_api_key",
            &["VAANI_OPENAI_API_KEY", "OPENAI_API_KEY"],
        )
        .ok_or_else(|| VaaniError::MissingApiKey("OpenAI".to_string()))?;

        let text = transcribe(&self.http_client, &api_key, &wav_bytes, &config.stt_model).await?;

        tracing::info!(chars = text.len(), "Transcription complete");

        // Enhance via Claude with streaming paste
        let enhanced = self.enhance_and_paste(&text, config).await?;

        Ok(enhanced)
    }

    /// Enhance transcribed text via Claude and stream it to the cursor.
    ///
    /// If the Anthropic API key is missing, falls back to pasting the raw
    /// transcription via clipboard paste instead.
    async fn enhance_and_paste(
        &self,
        text: &str,
        config: &VaaniConfig,
    ) -> Result<String, VaaniError> {
        let anthropic_key = resolve_api_key(
            "anthropic_api_key",
            &["VAANI_ANTHROPIC_API_KEY", "ANTHROPIC_API_KEY"],
        );

        match anthropic_key {
            Some(key) => {
                let system_prompt = build_system_prompt(&config.active_mode);
                tracing::info!(mode = %config.active_mode, "Enhancing with streaming");

                let enhanced = enhance_streaming(
                    &self.http_client,
                    &key,
                    text,
                    &config.llm_model,
                    &system_prompt,
                    |tokens| {
                        if let Err(e) = type_text(tokens) {
                            tracing::warn!("Failed to type streamed tokens: {e}");
                        }
                    },
                )
                .await?;

                tracing::info!(
                    original_len = text.len(),
                    enhanced_len = enhanced.len(),
                    "Enhancement complete"
                );
                Ok(enhanced)
            }
            _ => {
                tracing::info!("No Anthropic API key, pasting raw transcription");
                paste_text(text, config.paste_restore_delay_ms as u64)?;
                Ok(text.to_string())
            }
        }
    }

    /// Returns the current app state.
    pub fn current_state(&self) -> crate::state::AppState {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .current()
    }

    /// Returns the current audio input level (0.0 to 1.0).
    pub fn current_mic_level(&self) -> f32 {
        self.audio_buffer.current_level()
    }
}

/// Look up an API key: keychain first, then environment variables.
///
/// Returns `None` if the key is not found in any source.
fn resolve_api_key(keychain_key: &str, env_vars: &[&str]) -> Option<String> {
    // 1. Try keychain
    let storage = create_secret_storage();
    if let Ok(Some(key)) = storage.get(keychain_key) {
        if !key.is_empty() {
            return Some(key);
        }
    }

    // 2. Fall back to environment variables
    for var in env_vars {
        if let Ok(key) = std::env::var(var) {
            if !key.is_empty() {
                return Some(key);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    fn default_app() -> VaaniApp {
        VaaniApp::new(VaaniConfig::default())
    }

    #[test]
    fn new_app_starts_idle() {
        let app = default_app();
        assert_eq!(app.current_state(), AppState::Idle);
    }

    #[test]
    fn mic_level_when_not_recording_is_zero() {
        let app = default_app();
        assert_eq!(app.current_mic_level(), 0.0);
    }

    #[test]
    fn http_client_is_configured() {
        let app = default_app();
        // Just verify the client was created (no panic)
        let _client = &app.http_client;
    }
}
