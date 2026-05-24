//! Audio capture using cpal.
//!
//! Provides `AudioRecorder` for recording audio from an input device.
//! Audio samples are accumulated in a thread-safe buffer via cpal's callback.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleRate, Stream, StreamConfig};

use crate::error::VaaniError;

/// Lists available audio input devices with their names and indices.
pub fn list_input_devices() -> Result<Vec<(u32, String)>, VaaniError> {
    let host = cpal::default_host();
    let devices: Vec<(u32, String)> = host
        .input_devices()
        .map_err(|e| VaaniError::Audio(format!("Failed to enumerate input devices: {e}")))?
        .enumerate()
        .map(|(i, d)| {
            let name = d.name().unwrap_or_else(|_| format!("Device {i}"));
            (i as u32, name)
        })
        .collect();
    Ok(devices)
}

/// Returns the default input device, or an error if none is available.
fn get_device(device_index: Option<u32>) -> Result<Device, VaaniError> {
    let host = cpal::default_host();

    match device_index {
        Some(idx) => {
            let devices: Vec<Device> = host
                .input_devices()
                .map_err(|e| VaaniError::Audio(format!("Failed to enumerate input devices: {e}")))?
                .collect();
            devices
                .into_iter()
                .nth(idx as usize)
                .ok_or_else(|| VaaniError::Audio(format!("No input device found at index {idx}")))
        }
        None => host
            .default_input_device()
            .ok_or_else(|| VaaniError::Audio("No default input device found".to_string())),
    }
}

/// Thread-safe buffer that accumulates audio samples from the cpal callback.
#[derive(Clone)]
pub struct AudioBuffer {
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<Mutex<f32>>,
}

impl Default for AudioBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioBuffer {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            level: Arc::new(Mutex::new(0.0)),
        }
    }

    /// Returns the current RMS level (0.0 to 1.0) of the most recent chunk.
    pub fn current_level(&self) -> f32 {
        *self.level.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Drains and returns all accumulated samples, clearing the buffer.
    pub fn take_samples(&self) -> Vec<f32> {
        let mut buf = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *buf)
    }

    /// Appends samples and updates the RMS level.
    fn push_samples(&self, data: &[f32]) {
        // Update RMS level
        if !data.is_empty() {
            let sum_sq: f64 = data.iter().map(|&s| (s as f64) * (s as f64)).sum();
            let rms = (sum_sq / data.len() as f64).sqrt() as f32;
            if let Ok(mut level) = self.level.lock() {
                *level = rms;
            }
        }

        // Accumulate samples
        if let Ok(mut buf) = self.samples.lock() {
            buf.extend_from_slice(data);
        }
    }
}

/// Records audio from an input device using cpal.
pub struct AudioRecorder {
    stream: Option<Stream>,
    buffer: AudioBuffer,
    /// Target sample rate requested by the caller (e.g. 16kHz for Whisper).
    target_sample_rate: u32,
    /// Actual sample rate of the device stream. Set after `start()`.
    actual_sample_rate: u32,
    /// Number of channels the device stream is using. Set after `start()`.
    actual_channels: u16,
}

impl AudioRecorder {
    /// Creates a new recorder targeting the specified device and sample rate.
    ///
    /// Does not start recording — call `start()` to begin.
    pub fn new(device_index: Option<u32>, sample_rate: u32) -> Result<Self, VaaniError> {
        let _device = get_device(device_index)?; // Validate device exists
        Ok(Self {
            stream: None,
            buffer: AudioBuffer::new(),
            target_sample_rate: sample_rate,
            actual_sample_rate: sample_rate,
            actual_channels: 1,
        })
    }

    /// Creates a new recorder that writes samples into the provided shared buffer.
    ///
    /// This allows the caller (e.g., `VaaniApp`) to own the buffer and read
    /// samples from it after recording stops.
    pub fn with_buffer(
        buffer: AudioBuffer,
        device_index: Option<u32>,
        sample_rate: u32,
    ) -> Result<Self, VaaniError> {
        let _device = get_device(device_index)?; // Validate device exists
        Ok(Self {
            stream: None,
            buffer,
            target_sample_rate: sample_rate,
            actual_sample_rate: sample_rate,
            actual_channels: 1,
        })
    }

    /// Starts recording. Audio samples accumulate in the internal buffer.
    ///
    /// Tries the target sample rate first. If the device doesn't support it,
    /// falls back to the device's default config and records at the native rate.
    /// Callers should use `actual_sample_rate()` to get the real capture rate
    /// and resample if needed before sending to Whisper.
    pub fn start(&mut self, device_index: Option<u32>) -> Result<(), VaaniError> {
        let device = get_device(device_index)?;

        // Try the requested config first (mono, target sample rate)
        let requested = StreamConfig {
            channels: 1,
            sample_rate: SampleRate(self.target_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let (stream_config, channels) = match device.build_input_stream(
            &requested,
            |_: &[f32], _: &cpal::InputCallbackInfo| {},
            |_| {},
            None,
        ) {
            Ok(_test_stream) => {
                // Requested config works — drop the test stream and use it
                drop(_test_stream);
                (requested, 1u16)
            }
            Err(_) => {
                // Fall back to device's default config
                let default_config = device.default_input_config().map_err(|e| {
                    VaaniError::Audio(format!("Failed to get default input config: {e}"))
                })?;

                tracing::info!(
                    device_rate = default_config.sample_rate().0,
                    device_channels = default_config.channels(),
                    target_rate = self.target_sample_rate,
                    "Device doesn't support requested config, using device defaults"
                );

                let ch = default_config.channels();
                let config = StreamConfig {
                    channels: ch,
                    sample_rate: default_config.sample_rate(),
                    buffer_size: cpal::BufferSize::Default,
                };
                (config, ch)
            }
        };

        self.actual_sample_rate = stream_config.sample_rate.0;
        self.actual_channels = channels;

        let buffer = self.buffer.clone();
        let num_channels = channels;
        let err_fn = |err: cpal::StreamError| {
            tracing::error!("Audio stream error: {err}");
        };

        let stream = device
            .build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if num_channels == 1 {
                        buffer.push_samples(data);
                    } else {
                        // Downmix to mono by averaging channels
                        let mono: Vec<f32> = data
                            .chunks(num_channels as usize)
                            .map(|frame| frame.iter().sum::<f32>() / num_channels as f32)
                            .collect();
                        buffer.push_samples(&mono);
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| VaaniError::Audio(format!("Failed to build audio stream: {e}")))?;

        stream
            .play()
            .map_err(|e| VaaniError::Audio(format!("Failed to start audio stream: {e}")))?;

        tracing::info!(
            target_rate = self.target_sample_rate,
            actual_rate = self.actual_sample_rate,
            channels = self.actual_channels,
            "Recording started"
        );
        self.stream = Some(stream);
        Ok(())
    }

    /// Stops recording and returns all captured audio samples.
    pub fn stop(&mut self) -> Vec<f32> {
        if let Some(stream) = self.stream.take() {
            drop(stream); // Stops the stream
            tracing::info!("Recording stopped");
        }
        self.buffer.take_samples()
    }

    /// Returns the current audio input level (0.0 to 1.0).
    pub fn current_level(&self) -> f32 {
        self.buffer.current_level()
    }

    /// Returns true if currently recording.
    pub fn is_recording(&self) -> bool {
        self.stream.is_some()
    }

    /// Returns the actual sample rate the device is recording at.
    ///
    /// This may differ from the requested rate if the device doesn't support it.
    /// Before `start()` is called, returns the target rate.
    pub fn actual_sample_rate(&self) -> u32 {
        self.actual_sample_rate
    }

    /// Returns the target sample rate originally requested.
    pub fn target_sample_rate(&self) -> u32 {
        self.target_sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_buffer_starts_empty() {
        let buf = AudioBuffer::new();
        assert_eq!(buf.current_level(), 0.0);
        assert!(buf.take_samples().is_empty());
    }

    #[test]
    fn audio_buffer_accumulates_samples() {
        let buf = AudioBuffer::new();
        buf.push_samples(&[0.1, 0.2, 0.3]);
        buf.push_samples(&[0.4, 0.5]);
        let samples = buf.take_samples();
        assert_eq!(samples.len(), 5);
        assert!((samples[0] - 0.1).abs() < f32::EPSILON);
        assert!((samples[4] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn audio_buffer_take_clears_buffer() {
        let buf = AudioBuffer::new();
        buf.push_samples(&[0.1, 0.2]);
        let first = buf.take_samples();
        assert_eq!(first.len(), 2);
        let second = buf.take_samples();
        assert!(second.is_empty());
    }

    #[test]
    fn audio_buffer_updates_level() {
        let buf = AudioBuffer::new();
        // Push a constant signal of 0.5 — RMS should be 0.5
        buf.push_samples(&[0.5; 100]);
        let level = buf.current_level();
        assert!((level - 0.5).abs() < 0.01, "Expected ~0.5, got {level}");
    }

    #[test]
    fn audio_buffer_level_of_silence_is_zero() {
        let buf = AudioBuffer::new();
        buf.push_samples(&[0.0; 100]);
        assert_eq!(buf.current_level(), 0.0);
    }

    #[test]
    fn list_input_devices_does_not_panic() {
        // This test just verifies the function doesn't panic.
        // It may return an empty list in CI environments without audio hardware.
        let result = list_input_devices();
        assert!(result.is_ok());
    }
}
