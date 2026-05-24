//! Voice Activity Detection (VAD) using Silero VAD via ONNX Runtime.
//!
//! Classifies audio chunks as speech or silence. The primary implementation
//! (`SileroVad`) runs inference on the Silero ONNX model; a `MockVad` is
//! provided for testing without a model file.

use std::path::Path;

use ndarray::Array3;
use ort::value::Tensor;
use tracing::{debug, warn};

use crate::error::VaaniError;

// ── Constants ────────────────────────────────────────────────────────────────

/// Number of samples per VAD chunk at 16 kHz.
const CHUNK_SIZE: usize = 512;

/// Number of padding chunks to keep before and after speech regions.
const PADDING_CHUNKS: usize = 3;

// ── Trait ────────────────────────────────────────────────────────────────────

/// Abstraction over voice activity detection for testability and mocking.
pub trait VoiceActivityDetector: Send + Sync {
    /// Process a chunk of audio samples and return speech probability (0.0 to 1.0).
    fn speech_probability(&mut self, samples: &[f32], sample_rate: u32) -> Result<f32, VaaniError>;

    /// Reset internal state (call between recordings).
    fn reset(&mut self);
}

// ── SileroVad ────────────────────────────────────────────────────────────────

/// Real VAD implementation backed by the Silero ONNX model.
pub struct SileroVad {
    session: ort::session::Session,
    h_state: Array3<f32>,
    c_state: Array3<f32>,
}

impl SileroVad {
    /// Load the Silero VAD ONNX model from disk and initialise hidden states.
    pub fn new(model_path: &Path) -> Result<Self, VaaniError> {
        let session = ort::session::Session::builder()
            .and_then(|builder| builder.commit_from_file(model_path))
            .map_err(|e| {
                warn!(
                    "Failed to load Silero VAD model from {}: {e}",
                    model_path.display()
                );
                VaaniError::Vad(format!(
                    "Failed to load voice detection model from {}: {e}",
                    model_path.display()
                ))
            })?;

        debug!(path = %model_path.display(), "Silero VAD model loaded");

        Ok(Self {
            session,
            h_state: Array3::<f32>::zeros((2, 1, 64)),
            c_state: Array3::<f32>::zeros((2, 1, 64)),
        })
    }
}

impl VoiceActivityDetector for SileroVad {
    fn speech_probability(&mut self, samples: &[f32], sample_rate: u32) -> Result<f32, VaaniError> {
        let chunk_len = samples.len();

        // Build input tensors — convert ndarray to ort Tensor values
        let input_arr = ndarray::Array2::from_shape_vec((1, chunk_len), samples.to_vec())
            .map_err(|e| VaaniError::Vad(format!("Failed to create input tensor: {e}")))?;
        let input_tensor = Tensor::from_array(input_arr)
            .map_err(|e| VaaniError::Vad(format!("Failed to create input tensor: {e}")))?;

        let sr_arr = ndarray::Array1::from_vec(vec![sample_rate as i64]);
        let sr_tensor = Tensor::from_array(sr_arr)
            .map_err(|e| VaaniError::Vad(format!("Failed to create sr tensor: {e}")))?;

        let h_tensor = Tensor::from_array(self.h_state.clone())
            .map_err(|e| VaaniError::Vad(format!("Failed to create h_state tensor: {e}")))?;
        let c_tensor = Tensor::from_array(self.c_state.clone())
            .map_err(|e| VaaniError::Vad(format!("Failed to create c_state tensor: {e}")))?;

        let outputs = self
            .session
            .run(ort::inputs![input_tensor, sr_tensor, h_tensor, c_tensor])
            .map_err(|e| VaaniError::Vad(format!("VAD inference failed: {e}")))?;

        // Extract speech probability from the output tensor (returns (&Shape, &[f32]))
        let (_, prob_data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| VaaniError::Vad(format!("Failed to extract probability: {e}")))?;
        let probability = prob_data
            .first()
            .copied()
            .ok_or_else(|| VaaniError::Vad("VAD output tensor is empty".to_string()))?;

        // Update hidden/cell states
        if let Ok((_, h_data)) = outputs[1].try_extract_tensor::<f32>() {
            if let Ok(arr) = Array3::from_shape_vec((2, 1, 64), h_data.to_vec()) {
                self.h_state = arr;
            }
        }
        if let Ok((_, c_data)) = outputs[2].try_extract_tensor::<f32>() {
            if let Ok(arr) = Array3::from_shape_vec((2, 1, 64), c_data.to_vec()) {
                self.c_state = arr;
            }
        }

        Ok(probability)
    }

    fn reset(&mut self) {
        self.h_state = Array3::<f32>::zeros((2, 1, 64));
        self.c_state = Array3::<f32>::zeros((2, 1, 64));
        debug!("Silero VAD state reset");
    }
}

// ── MockVad ──────────────────────────────────────────────────────────────────

/// Test double that always returns a configurable speech probability.
pub struct MockVad {
    pub probability: f32,
}

impl VoiceActivityDetector for MockVad {
    fn speech_probability(
        &mut self,
        _samples: &[f32],
        _sample_rate: u32,
    ) -> Result<f32, VaaniError> {
        Ok(self.probability)
    }

    fn reset(&mut self) {
        // No-op for mock
    }
}

// ── trim_silence ─────────────────────────────────────────────────────────────

/// Remove silence from audio, keeping speech regions with padding for natural transitions.
///
/// Processes the audio in fixed-size chunks (`CHUNK_SIZE` = 512 samples) and uses
/// the provided VAD to classify each chunk. Chunks at or above `threshold` are kept,
/// along with up to `PADDING_CHUNKS` chunks on either side for smooth transitions.
///
/// Returns an empty `Vec` if no speech is detected.
pub fn trim_silence(
    samples: &[f32],
    sample_rate: u32,
    threshold: f32,
    vad: &mut dyn VoiceActivityDetector,
) -> Result<Vec<f32>, VaaniError> {
    if samples.is_empty() {
        return Ok(Vec::new());
    }

    // Reset VAD state before processing a new recording
    vad.reset();

    // Split into chunks and classify each
    let chunks: Vec<&[f32]> = samples.chunks(CHUNK_SIZE).collect();
    let num_chunks = chunks.len();

    let mut is_speech = vec![false; num_chunks];
    for (i, chunk) in chunks.iter().enumerate() {
        let prob = vad.speech_probability(chunk, sample_rate)?;
        is_speech[i] = prob >= threshold;
    }

    // If no speech at all, return empty
    if !is_speech.iter().any(|&s| s) {
        debug!("No speech detected in {} chunks", num_chunks);
        return Ok(Vec::new());
    }

    // Mark chunks to keep: speech chunks plus padding before and after
    let mut keep = vec![false; num_chunks];
    for (i, &speech) in is_speech.iter().enumerate() {
        if speech {
            // Mark the speech chunk itself and surrounding padding
            let pad_start = i.saturating_sub(PADDING_CHUNKS);
            let pad_end = (i + PADDING_CHUNKS + 1).min(num_chunks);
            for slot in &mut keep[pad_start..pad_end] {
                *slot = true;
            }
        }
    }

    // Collect kept chunks into output
    let result: Vec<f32> = chunks
        .iter()
        .zip(keep.iter())
        .filter(|(_, &k)| k)
        .flat_map(|(chunk, _)| chunk.iter().copied())
        .collect();

    debug!(
        total_chunks = num_chunks,
        speech_chunks = is_speech.iter().filter(|&&s| s).count(),
        kept_chunks = keep.iter().filter(|&&k| k).count(),
        "Trimmed silence"
    );

    Ok(result)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn mock_vad_returns_configured_probability() {
        let mut vad = MockVad { probability: 0.8 };
        let samples = vec![0.0_f32; CHUNK_SIZE];
        let prob = vad
            .speech_probability(&samples, 16000)
            .expect("mock should not fail");
        assert!((prob - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn mock_vad_reset_is_noop() {
        let mut vad = MockVad { probability: 0.5 };
        vad.reset(); // Should not panic
        let prob = vad
            .speech_probability(&[0.0; CHUNK_SIZE], 16000)
            .expect("mock should not fail");
        assert!((prob - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn trim_silence_removes_silent_chunks() {
        let mut vad = MockVad { probability: 0.0 };
        let samples = vec![0.1_f32; CHUNK_SIZE * 10];
        let result = trim_silence(&samples, 16000, 0.5, &mut vad).expect("trim should not fail");
        assert!(
            result.is_empty(),
            "All-silent input should produce empty output"
        );
    }

    #[test]
    fn trim_silence_keeps_speech_chunks() {
        let mut vad = MockVad { probability: 0.9 };
        let samples = vec![0.5_f32; CHUNK_SIZE * 5];
        let result = trim_silence(&samples, 16000, 0.5, &mut vad).expect("trim should not fail");
        assert_eq!(
            result.len(),
            samples.len(),
            "All-speech input should retain all samples"
        );
    }

    #[test]
    fn trim_silence_empty_input_returns_empty() {
        let mut vad = MockVad { probability: 0.9 };
        let result = trim_silence(&[], 16000, 0.5, &mut vad).expect("trim should not fail");
        assert!(result.is_empty());
    }

    #[test]
    fn trim_silence_adds_padding_around_speech() {
        // Build a mock that returns speech for only one specific chunk (index 5),
        // silence for all others. We'll have 12 chunks total.
        struct PatternVad {
            speech_index: usize,
            call_count: usize,
        }

        impl VoiceActivityDetector for PatternVad {
            fn speech_probability(
                &mut self,
                _samples: &[f32],
                _sample_rate: u32,
            ) -> Result<f32, VaaniError> {
                let is_speech = self.call_count == self.speech_index;
                self.call_count += 1;
                Ok(if is_speech { 1.0 } else { 0.0 })
            }

            fn reset(&mut self) {
                self.call_count = 0;
            }
        }

        let num_chunks = 12;
        let speech_at = 5;
        let mut vad = PatternVad {
            speech_index: speech_at,
            call_count: 0,
        };

        let samples = vec![0.1_f32; CHUNK_SIZE * num_chunks];
        let result = trim_silence(&samples, 16000, 0.5, &mut vad).expect("trim should not fail");

        // Speech at index 5 -> padding keeps indices 2..=8 (5-3 to 5+3), i.e. 7 chunks
        let expected_start = speech_at.saturating_sub(PADDING_CHUNKS); // 2
        let expected_end = (speech_at + PADDING_CHUNKS + 1).min(num_chunks); // 9
        let expected_kept = expected_end - expected_start; // 7

        assert_eq!(
            result.len(),
            expected_kept * CHUNK_SIZE,
            "Expected {expected_kept} chunks ({} samples), got {} samples",
            expected_kept * CHUNK_SIZE,
            result.len()
        );
    }

    #[test]
    fn trim_silence_resets_vad_before_processing() {
        /// A mock that tracks how many times `reset()` is called.
        struct TrackingVad {
            probability: f32,
            reset_count: Arc<AtomicU32>,
        }

        impl VoiceActivityDetector for TrackingVad {
            fn speech_probability(
                &mut self,
                _samples: &[f32],
                _sample_rate: u32,
            ) -> Result<f32, VaaniError> {
                Ok(self.probability)
            }

            fn reset(&mut self) {
                self.reset_count.fetch_add(1, Ordering::SeqCst);
            }
        }

        let reset_count = Arc::new(AtomicU32::new(0));
        let mut vad = TrackingVad {
            probability: 0.9,
            reset_count: Arc::clone(&reset_count),
        };

        let samples = vec![0.1_f32; CHUNK_SIZE * 3];
        let _ = trim_silence(&samples, 16000, 0.5, &mut vad).expect("trim should not fail");

        assert_eq!(
            reset_count.load(Ordering::SeqCst),
            1,
            "VAD should be reset exactly once before processing"
        );
    }
}
