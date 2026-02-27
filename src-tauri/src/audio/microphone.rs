//! Audio capture from microphone
//!
//! Uses cpal for cross-platform capture.
//! Audio is captured in a dedicated thread and resampled to 16kHz for Whisper.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use thiserror::Error;

/// Audio configuration for capture
#[derive(Debug, Clone)]
pub struct AudioConfig {
    /// Target sample rate (16kHz for STT)
    pub target_sample_rate: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            target_sample_rate: 16000,
        }
    }
}

/// Microphone capture errors
#[derive(Error, Debug)]
pub enum MicrophoneError {
    #[error("No audio device found")]
    NoDevice,

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Stream error: {0}")]
    StreamError(String),

    #[error("Audio thread not started")]
    NotStarted,
}

/// Commands to control the audio thread
enum AudioCommand {
    Stop,
}

/// Handle to control audio capture
pub struct AudioHandle {
    command_tx: mpsc::Sender<AudioCommand>,
    thread_handle: Option<JoinHandle<()>>,
}

impl AudioHandle {
    /// Start audio capture in a dedicated thread
    pub fn start<F>(config: AudioConfig, sample_callback: F) -> Result<Self, MicrophoneError>
    where
        F: Fn(Vec<f32>) + Send + 'static,
    {
        let (command_tx, command_rx) = mpsc::channel();

        let thread_handle = thread::spawn(move || {
            if let Err(e) = run_audio_capture(config, sample_callback, command_rx) {
                tracing::error!("Audio capture error: {}", e);
            }
        });

        Ok(Self {
            command_tx,
            thread_handle: Some(thread_handle),
        })
    }

    /// Stop audio capture
    pub fn stop(&mut self) {
        let _ = self.command_tx.send(AudioCommand::Stop);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    /// List available input devices
    pub fn list_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default()
    }
}

impl Drop for AudioHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Simple linear resample from source_rate to target_rate
fn resample(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if source_rate == target_rate {
        return samples.to_vec();
    }

    let ratio = source_rate as f64 / target_rate as f64;
    let output_len = (samples.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 * ratio;
        let idx_floor = src_idx.floor() as usize;
        let idx_ceil = (idx_floor + 1).min(samples.len() - 1);
        let frac = src_idx - idx_floor as f64;

        let sample = samples[idx_floor] * (1.0 - frac as f32) + samples[idx_ceil] * frac as f32;
        output.push(sample);
    }

    output
}

/// Convert stereo to mono
fn stereo_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels == 1 {
        return samples.to_vec();
    }

    samples
        .chunks(channels as usize)
        .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Run audio capture (in a dedicated thread)
fn run_audio_capture<F>(
    config: AudioConfig,
    sample_callback: F,
    command_rx: mpsc::Receiver<AudioCommand>,
) -> Result<(), MicrophoneError>
where
    F: Fn(Vec<f32>) + Send + 'static,
{
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(MicrophoneError::NoDevice)?;

    tracing::info!("Audio device: {:?}", device.name());

    // Use the device's default configuration
    let supported_config = device
        .default_input_config()
        .map_err(|e| MicrophoneError::ConfigError(e.to_string()))?;

    let source_sample_rate = supported_config.sample_rate().0;
    let source_channels = supported_config.channels();
    let target_rate = config.target_sample_rate;

    tracing::info!(
        "Audio config: {}Hz {}ch -> {}Hz mono",
        source_sample_rate,
        source_channels,
        target_rate
    );

    let stream_config = supported_config.into();

    let stream = device
        .build_input_stream(
            &stream_config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Convert to mono if needed
                let mono = stereo_to_mono(data, source_channels);

                // Resample to 16kHz
                let resampled = resample(&mono, source_sample_rate, target_rate);

                if !resampled.is_empty() {
                    sample_callback(resampled);
                }
            },
            |err| {
                tracing::error!("Audio stream error: {}", err);
            },
            None,
        )
        .map_err(|e| MicrophoneError::StreamError(e.to_string()))?;

    stream
        .play()
        .map_err(|e| MicrophoneError::StreamError(e.to_string()))?;

    tracing::info!("Audio capture started");

    // Wait for stop signal
    loop {
        match command_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(AudioCommand::Stop) => {
                tracing::info!("Audio capture stopped");
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(())
}
