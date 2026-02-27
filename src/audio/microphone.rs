//! Capture audio depuis le microphone
//!
//! Utilise cpal pour la capture cross-platform et ringbuf pour le buffering.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleRate, Stream, StreamConfig};
use ringbuf::{HeapRb, traits::*};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

/// Configuration audio pour la capture
#[derive(Debug, Clone)]
pub struct AudioConfig {
    /// Taux d'échantillonnage (16kHz recommandé pour STT)
    pub sample_rate: u32,
    /// Nombre de canaux (1 = mono)
    pub channels: u16,
    /// Taille du buffer en échantillons
    pub buffer_size: usize,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            channels: 1,
            buffer_size: 16000, // 1 seconde de buffer
        }
    }
}

/// Erreurs liées à la capture microphone
#[derive(Error, Debug)]
pub enum MicrophoneError {
    #[error("Aucun périphérique audio trouvé")]
    NoDevice,

    #[error("Erreur de configuration: {0}")]
    ConfigError(String),

    #[error("Erreur de stream: {0}")]
    StreamError(String),

    #[error("Microphone non initialisé")]
    NotInitialized,
}

/// Stream audio depuis le microphone
pub struct AudioStream {
    stream: Option<Stream>,
    consumer: Arc<Mutex<ringbuf::HeapCons<f32>>>,
    config: AudioConfig,
}

impl AudioStream {
    /// Crée un nouveau stream audio avec la configuration par défaut
    pub fn new() -> Result<Self, MicrophoneError> {
        Self::with_config(AudioConfig::default())
    }

    /// Crée un nouveau stream audio avec une configuration personnalisée
    pub fn with_config(config: AudioConfig) -> Result<Self, MicrophoneError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(MicrophoneError::NoDevice)?;

        let stream_config = StreamConfig {
            channels: config.channels,
            sample_rate: SampleRate(config.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let rb = HeapRb::<f32>::new(config.buffer_size);
        let (mut producer, consumer) = rb.split();
        let consumer = Arc::new(Mutex::new(consumer));

        let stream = device
            .build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    // Push audio data into ring buffer
                    for &sample in data {
                        let _ = producer.try_push(sample);
                    }
                },
                |err| {
                    tracing::error!("Erreur stream audio: {}", err);
                },
                None,
            )
            .map_err(|e| MicrophoneError::StreamError(e.to_string()))?;

        Ok(Self {
            stream: Some(stream),
            consumer,
            config,
        })
    }

    /// Démarre la capture audio
    pub fn start(&self) -> Result<(), MicrophoneError> {
        if let Some(ref stream) = self.stream {
            stream
                .play()
                .map_err(|e| MicrophoneError::StreamError(e.to_string()))?;
            tracing::info!("Capture audio démarrée");
            Ok(())
        } else {
            Err(MicrophoneError::NotInitialized)
        }
    }

    /// Arrête la capture audio
    pub fn stop(&self) -> Result<(), MicrophoneError> {
        if let Some(ref stream) = self.stream {
            stream
                .pause()
                .map_err(|e| MicrophoneError::StreamError(e.to_string()))?;
            tracing::info!("Capture audio arrêtée");
            Ok(())
        } else {
            Err(MicrophoneError::NotInitialized)
        }
    }

    /// Lit les échantillons disponibles dans le buffer
    pub async fn read_samples(&self, buffer: &mut [f32]) -> usize {
        let mut consumer = self.consumer.lock().await;
        consumer.pop_slice(buffer)
    }

    /// Retourne la configuration audio
    pub fn config(&self) -> &AudioConfig {
        &self.config
    }

    /// Liste les périphériques d'entrée disponibles
    pub fn list_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|devices| {
                devices
                    .filter_map(|d| d.name().ok())
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Default for AudioStream {
    fn default() -> Self {
        Self::new().expect("Impossible d'initialiser le stream audio")
    }
}
