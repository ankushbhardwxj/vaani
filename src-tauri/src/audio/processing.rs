//! Audio processing utilities: gain normalization and WAV encoding.

use std::io::Cursor;

use hound::{SampleFormat, WavSpec, WavWriter};
use tracing::{debug, warn};

use crate::error::VaaniError;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Compute the root mean square (RMS) of a slice of audio samples.
///
/// Returns `0.0` for an empty slice.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let mean_sq = sum_sq / samples.len() as f64;
    mean_sq.sqrt() as f32
}

// ── Gain normalisation ──────────────────────────────────────────────────────

/// Threshold below which audio is considered silent.
const SILENCE_THRESHOLD: f32 = 1e-10;

/// Normalize the gain of audio samples to a target level in dBFS.
///
/// # Arguments
///
/// * `samples` - Input audio samples (expected in the range `[-1.0, 1.0]`).
/// * `target_db` - Desired RMS level in dBFS (e.g., `-20.0`).
///
/// # Behaviour
///
/// - Empty input produces an empty `Vec`.
/// - Silent input (RMS < 1e-10) is returned unchanged to avoid division by zero.
/// - After gain is applied every sample is clamped to `[-1.0, 1.0]`.
pub fn normalize_gain(samples: &[f32], target_db: f32) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }

    let current_rms = rms(samples);

    if current_rms < SILENCE_THRESHOLD {
        debug!("Input is silent (RMS {current_rms:.2e}), returning unchanged");
        return samples.to_vec();
    }

    // Convert target dBFS to linear amplitude.
    // dBFS = 20 * log10(linear)  =>  linear = 10^(dBFS / 20)
    let target_linear = 10_f32.powf(target_db / 20.0);
    let gain = target_linear / current_rms;

    debug!(
        current_rms,
        target_db, target_linear, gain, "Applying gain normalization"
    );

    samples
        .iter()
        .map(|&s| (s * gain).clamp(-1.0, 1.0))
        .collect()
}

// ── WAV encoding ────────────────────────────────────────────────────────────

/// Encode float-32 audio samples to an in-memory WAV file (PCM 16-bit, mono).
///
/// Returns the complete WAV byte buffer including the RIFF header.
/// An empty `samples` slice produces a valid (but data-less) WAV file.
pub fn encode_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, VaaniError> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let mut buf = Vec::new();

    {
        let cursor = Cursor::new(&mut buf);
        let mut writer = WavWriter::new(cursor, spec).map_err(|e| {
            warn!("Failed to create WAV writer: {e}");
            VaaniError::Audio(format!("Failed to create WAV writer: {e}"))
        })?;

        for &sample in samples {
            // Convert f32 [-1.0, 1.0] -> i16 range.
            let clamped = sample.clamp(-1.0, 1.0);
            let as_i16 = (clamped * i16::MAX as f32) as i16;
            writer.write_sample(as_i16).map_err(|e| {
                warn!("Failed to write WAV sample: {e}");
                VaaniError::Audio(format!("Failed to write WAV sample: {e}"))
            })?;
        }

        writer.finalize().map_err(|e| {
            warn!("Failed to finalize WAV file: {e}");
            VaaniError::Audio(format!("Failed to finalize WAV file: {e}"))
        })?;
    }

    debug!(
        byte_len = buf.len(),
        sample_count = samples.len(),
        sample_rate,
        "Encoded WAV"
    );

    Ok(buf)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    // ── rms tests ───────────────────────────────────────────────────────

    #[test]
    fn rms_of_silence_is_zero() {
        let silence = vec![0.0_f32; 1024];
        assert!((rms(&silence) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn rms_of_known_signal() {
        // RMS of a constant signal c is |c|.
        let signal = vec![0.5_f32; 1000];
        let r = rms(&signal);
        assert!((r - 0.5).abs() < 1e-5, "Expected RMS ~0.5, got {r}");
    }

    #[test]
    fn rms_of_empty_slice_is_zero() {
        assert!((rms(&[]) - 0.0).abs() < f32::EPSILON);
    }

    // ── normalize_gain tests ────────────────────────────────────────────

    #[test]
    fn normalize_gain_amplifies_quiet_audio() {
        // Quiet signal: constant 0.001 => RMS = 0.001 => ~-60 dBFS
        let quiet: Vec<f32> = vec![0.001; 4096];
        let original_rms = rms(&quiet);

        // Target -20 dBFS => much louder than -60 dBFS
        let normalised = normalize_gain(&quiet, -20.0);
        let new_rms = rms(&normalised);

        assert!(
            new_rms > original_rms,
            "Expected RMS to increase: {original_rms} -> {new_rms}"
        );
    }

    #[test]
    fn normalize_gain_attenuates_loud_audio() {
        // Loud signal: constant 0.9 => RMS = 0.9 => ~-0.9 dBFS
        let loud: Vec<f32> = vec![0.9; 4096];
        let original_rms = rms(&loud);

        // Target -40 dBFS => much quieter
        let normalised = normalize_gain(&loud, -40.0);
        let new_rms = rms(&normalised);

        assert!(
            new_rms < original_rms,
            "Expected RMS to decrease: {original_rms} -> {new_rms}"
        );
    }

    #[test]
    fn normalize_gain_clamps_to_valid_range() {
        // Very quiet signal pushed to a high target should still be clamped.
        let quiet: Vec<f32> = vec![0.01; 4096];
        let normalised = normalize_gain(&quiet, -1.0); // near 0 dBFS

        for &s in &normalised {
            assert!(
                (-1.0..=1.0).contains(&s),
                "Sample {s} is out of [-1.0, 1.0]"
            );
        }
    }

    #[test]
    fn normalize_gain_empty_input_returns_empty() {
        let result = normalize_gain(&[], -20.0);
        assert!(result.is_empty());
    }

    #[test]
    fn normalize_gain_silent_input_returns_unchanged() {
        let silence = vec![0.0_f32; 512];
        let result = normalize_gain(&silence, -20.0);
        assert_eq!(result.len(), silence.len());
        for &s in &result {
            assert!((s - 0.0).abs() < f32::EPSILON);
        }
    }

    // ── encode_wav tests ────────────────────────────────────────────────

    #[test]
    fn encode_wav_produces_valid_riff_header() {
        let samples: Vec<f32> = vec![0.0; 100];
        let wav = encode_wav(&samples, 16000).expect("encoding should succeed");

        assert!(wav.len() >= 4, "WAV too short to contain RIFF header");
        assert_eq!(&wav[..4], b"RIFF", "WAV must start with RIFF");
    }

    #[test]
    fn encode_wav_roundtrip_with_hound() {
        // Create a simple 440 Hz sine wave, 0.1 s at 16 kHz.
        let sample_rate = 16_000_u32;
        let duration_samples = (sample_rate as f32 * 0.1) as usize;
        let samples: Vec<f32> = (0..duration_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5
            })
            .collect();

        let wav_bytes = encode_wav(&samples, sample_rate).expect("encoding should succeed");

        // Read back with hound and verify.
        let cursor = Cursor::new(wav_bytes);
        let mut reader = hound::WavReader::new(cursor).expect("hound should read the WAV");

        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, sample_rate);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, SampleFormat::Int);

        let decoded: Vec<i16> = reader
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .expect("samples should decode");
        assert_eq!(decoded.len(), samples.len());

        // Verify first sample is close to the original (within quantisation error).
        let original_i16 = (samples[0].clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        assert!(
            (decoded[0] - original_i16).abs() <= 1,
            "First sample mismatch: decoded={}, expected~={}",
            decoded[0],
            original_i16
        );
    }

    #[test]
    fn encode_wav_empty_samples_produces_valid_wav() {
        let wav_bytes = encode_wav(&[], 44100).expect("encoding empty samples should succeed");

        // Should still be a valid WAV file.
        assert!(
            wav_bytes.len() >= 44,
            "WAV header should be at least 44 bytes"
        );
        assert_eq!(&wav_bytes[..4], b"RIFF");

        // Hound should be able to read it with zero samples.
        let cursor = Cursor::new(wav_bytes);
        let reader = hound::WavReader::new(cursor).expect("hound should read empty WAV");
        assert_eq!(reader.len(), 0);
    }

    #[test]
    fn encode_wav_correct_sample_rate_in_header() {
        let rates = [8000_u32, 16000, 22050, 44100, 48000];

        for &rate in &rates {
            let wav_bytes = encode_wav(&[0.0; 10], rate).expect("encoding should succeed");
            let cursor = Cursor::new(wav_bytes);
            let reader = hound::WavReader::new(cursor).expect("hound should read WAV");
            assert_eq!(
                reader.spec().sample_rate,
                rate,
                "Sample rate mismatch for {rate}"
            );
        }
    }
}
