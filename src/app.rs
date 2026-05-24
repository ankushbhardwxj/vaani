//! Pipeline orchestrator for the Vaani voice-to-text workflow.
//!
//! Coordinates the full flow: record → process → transcribe → paste.
//! This module ties together audio capture, transcription, and output.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::audio::capture::AudioBuffer;
use crate::audio::processing::{encode_wav, normalize_gain, resample};
use crate::config::VaaniConfig;
use crate::enhance::enhance_streaming;
use crate::error::VaaniError;
use crate::output::paste::{paste_text, type_text};
use crate::prompts::build_system_prompt;
use crate::sounds::{play_sound_if_enabled, SoundEffect};
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
    /// Signal to stop the recording thread.
    pub stop_recording: Arc<AtomicBool>,
    /// Signal to stop mic test thread.
    pub mic_test_active: Arc<AtomicBool>,
    /// Shared buffer for mic test level monitoring.
    pub mic_test_buffer: AudioBuffer,
    /// Actual sample rate used by the device during capture.
    /// May differ from config.sample_rate if the device doesn't support it.
    pub capture_sample_rate: Arc<AtomicU32>,
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
            stop_recording: Arc::new(AtomicBool::new(false)),
            mic_test_active: Arc::new(AtomicBool::new(false)),
            mic_test_buffer: AudioBuffer::new(),
            capture_sample_rate: Arc::new(AtomicU32::new(16_000)),
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

        // Resample if the device captured at a different rate than the target
        let capture_rate = self.capture_sample_rate.load(Ordering::SeqCst);
        let target_rate = config.sample_rate;
        let samples = if capture_rate != target_rate {
            tracing::info!(
                capture_rate,
                target_rate,
                "Resampling audio from device rate to target rate"
            );
            resample(&samples, capture_rate, target_rate)
        } else {
            samples
        };

        tracing::info!(
            sample_count = samples.len(),
            "Processing audio ({:.1}s)",
            samples.len() as f32 / target_rate as f32
        );

        // Normalize audio gain
        let normalized = normalize_gain(&samples, -20.0);

        // Encode to WAV at the target rate (16kHz for Whisper)
        let wav_bytes = encode_wav(&normalized, target_rate)?;

        // Transcribe via Whisper API
        let api_key = resolve_api_key(
            config.openai_api_key.as_deref(),
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
            config.anthropic_api_key.as_deref(),
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

    /// Toggle recording: if idle, start recording; if recording, stop and process.
    pub fn toggle_recording(self: &Arc<Self>) {
        let current = self.current_state();
        match current {
            crate::state::AppState::Idle => {
                if let Err(e) = self.begin_recording() {
                    tracing::error!("Failed to start recording: {e}");
                }
            }
            crate::state::AppState::Recording => {
                self.end_recording();
            }
            crate::state::AppState::Processing => {
                tracing::info!("Already processing, ignoring toggle");
            }
        }
    }

    /// Start recording audio on a dedicated thread.
    ///
    /// Uses a channel to wait for the recording thread to confirm that the
    /// audio stream started successfully. If the thread fails, the state
    /// machine is rolled back to Idle.
    fn begin_recording(self: &Arc<Self>) -> Result<(), VaaniError> {
        // Transition state
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .start_recording()?;

        let config = self
            .config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        // Play start sound
        let _ = play_sound_if_enabled(SoundEffect::RecordStart, config.sounds_enabled);

        // Clear stop signal
        self.stop_recording.store(false, Ordering::SeqCst);

        // Clone what we need for the recording thread
        let buffer = self.audio_buffer.clone();
        let stop_signal = Arc::clone(&self.stop_recording);
        let sample_rate = config.sample_rate;
        let device_index = config.microphone_device;
        let capture_rate_ref = Arc::clone(&self.capture_sample_rate);

        // Channel to receive recorder start result from the thread
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::Builder::new()
            .name("vaani-recorder".into())
            .spawn(move || {
                use crate::audio::capture::AudioRecorder;

                let mut recorder =
                    match AudioRecorder::with_buffer(buffer, device_index, sample_rate) {
                        Ok(r) => r,
                        Err(e) => {
                            let _ = tx.send(Err(e));
                            return;
                        }
                    };

                if let Err(e) = recorder.start(device_index) {
                    let _ = tx.send(Err(e));
                    return;
                }

                // Store the actual device sample rate so process_audio can resample
                capture_rate_ref.store(recorder.actual_sample_rate(), Ordering::SeqCst);

                // Signal success to the calling thread
                let _ = tx.send(Ok(()));

                tracing::info!("Recording thread started");

                // Wait for stop signal, checking every 50ms
                while !stop_signal.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(50));
                }

                // Stop recording — samples are already in the shared buffer
                recorder.stop();
                tracing::info!("Recording thread stopped");
            })
            .map_err(|e| {
                // Roll back state if we can't even spawn the thread
                let _ = self
                    .state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .cancel_recording();
                VaaniError::Audio(format!("Failed to spawn recording thread: {e}"))
            })?;

        // Wait for the thread to confirm recording started (with timeout)
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {
                tracing::info!("Recording started");
                Ok(())
            }
            Ok(Err(e)) => {
                // Recording failed — roll back to idle
                let _ = self
                    .state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .cancel_recording();
                Err(e)
            }
            Err(_) => {
                let _ = self
                    .state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .cancel_recording();
                Err(VaaniError::Audio(
                    "Recording thread did not respond in time".to_string(),
                ))
            }
        }
    }

    /// Stop recording and kick off async processing.
    fn end_recording(self: &Arc<Self>) {
        let config = self
            .config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();

        // Play stop sound
        let _ = play_sound_if_enabled(SoundEffect::RecordStop, config.sounds_enabled);

        // Signal the recording thread to stop
        self.stop_recording.store(true, Ordering::SeqCst);

        // Give the recording thread a moment to stop and flush samples
        std::thread::sleep(Duration::from_millis(100));

        // Transition to processing
        if let Err(e) = self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stop_recording()
        {
            tracing::error!("Failed to transition to processing: {e}");
            return;
        }

        // Spawn async processing
        let app = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            match app.process_and_paste().await {
                Ok(text) => {
                    tracing::info!(len = text.len(), "Pipeline complete");
                }
                Err(e) => {
                    tracing::error!("Pipeline error: {e}");
                    // Make sure we get back to idle
                    if let Err(e2) = app
                        .state
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .finish_processing()
                    {
                        tracing::error!("Failed to recover to idle: {e2}");
                    }
                }
            }
        });
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

/// Look up an API key: config value first, then environment variables.
///
/// Returns `None` if the key is not found in any source.
fn resolve_api_key(cfg_value: Option<&str>, env_vars: &[&str]) -> Option<String> {
    if let Some(key) = cfg_value {
        if !key.is_empty() {
            return Some(key.to_string());
        }
    }

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
