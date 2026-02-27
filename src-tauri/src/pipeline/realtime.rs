//! Real-time transcription pipeline
//!
//! Orchestrates audio capture and the STT engine.

use crate::audio::{AudioConfig, AudioHandle, MicrophoneError};
use crate::stt::{Language, SttEngine, SttError, SttEvent};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};

/// Pipeline configuration
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Language for transcription
    pub language: Language,
    /// Audio configuration
    pub audio_config: AudioConfig,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            language: Language::Auto,
            audio_config: AudioConfig::default(),
        }
    }
}

/// Pipeline errors
#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("Audio error: {0}")]
    AudioError(#[from] MicrophoneError),

    #[error("STT error: {0}")]
    SttError(#[from] SttError),

    #[error("Pipeline already running")]
    AlreadyRunning,

    #[error("Pipeline not started")]
    NotRunning,
}

/// Pipeline state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error(String),
}

/// Real-time transcription pipeline
pub struct RealtimePipeline<E: SttEngine> {
    config: PipelineConfig,
    stt_engine: Arc<Mutex<E>>,
    status: Arc<RwLock<PipelineStatus>>,
    event_tx: broadcast::Sender<SttEvent>,
    audio_handle: Option<AudioHandle>,
    stop_tx: Option<mpsc::Sender<()>>,
}

impl<E: SttEngine + 'static> RealtimePipeline<E> {
    /// Create a new pipeline with the specified STT engine
    pub fn new(stt_engine: E, config: PipelineConfig) -> Self {
        let (event_tx, _) = broadcast::channel(100);

        Self {
            config,
            stt_engine: Arc::new(Mutex::new(stt_engine)),
            status: Arc::new(RwLock::new(PipelineStatus::Stopped)),
            event_tx,
            audio_handle: None,
            stop_tx: None,
        }
    }

    /// Start the pipeline
    pub async fn start(&mut self) -> Result<(), PipelineError> {
        {
            let status = self.status.read().await;
            if *status == PipelineStatus::Running {
                return Err(PipelineError::AlreadyRunning);
            }
        }

        {
            let mut status = self.status.write().await;
            *status = PipelineStatus::Starting;
        }

        // Configure the language
        {
            let mut engine = self.stt_engine.lock().await;
            engine.set_language(self.config.language.clone());
        }

        // Channel to receive audio samples
        let (audio_tx, mut audio_rx) = mpsc::unbounded_channel::<Vec<f32>>();

        // Channel to stop the processing task
        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
        self.stop_tx = Some(stop_tx);

        // Start audio capture
        let audio_handle =
            AudioHandle::start(self.config.audio_config.clone(), move |samples| {
                let _ = audio_tx.send(samples);
            })?;
        self.audio_handle = Some(audio_handle);

        // Launch STT processing task
        let stt_engine = Arc::clone(&self.stt_engine);
        let event_tx = self.event_tx.clone();
        let status = Arc::clone(&self.status);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(samples) = audio_rx.recv() => {
                        // Process samples
                        let mut engine = stt_engine.lock().await;
                        engine.push_audio(&samples);

                        // Retrieve events
                        while let Some(event) = engine.poll() {
                            let _ = event_tx.send(event);
                        }
                    }
                    _ = stop_rx.recv() => {
                        tracing::debug!("Stop signal received");
                        break;
                    }
                }
            }

            // Update status
            let mut s = status.write().await;
            *s = PipelineStatus::Stopped;
        });

        {
            let mut status = self.status.write().await;
            *status = PipelineStatus::Running;
        }

        tracing::info!("Pipeline started");
        Ok(())
    }

    /// Stop the pipeline
    pub async fn stop(&mut self) -> Result<(), PipelineError> {
        {
            let status = self.status.read().await;
            if *status == PipelineStatus::Stopped {
                return Ok(());
            }
        }

        {
            let mut status = self.status.write().await;
            *status = PipelineStatus::Stopping;
        }

        // Send stop signal to the processing task
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(()).await;
        }

        // Stop audio capture
        if let Some(mut handle) = self.audio_handle.take() {
            handle.stop();
        }

        // Flush the STT engine
        {
            let mut engine = self.stt_engine.lock().await;
            engine.flush();

            while let Some(event) = engine.poll() {
                let _ = self.event_tx.send(event);
            }
        }

        {
            let mut status = self.status.write().await;
            *status = PipelineStatus::Stopped;
        }

        tracing::info!("Pipeline stopped");
        Ok(())
    }

    /// Return the current pipeline status
    pub async fn status(&self) -> PipelineStatus {
        self.status.read().await.clone()
    }

    /// Subscribe to transcription events
    pub fn subscribe(&self) -> broadcast::Receiver<SttEvent> {
        self.event_tx.subscribe()
    }

    /// Change the transcription language
    pub async fn set_language(&mut self, language: Language) {
        self.config.language = language.clone();
        let mut engine = self.stt_engine.lock().await;
        engine.set_language(language);
    }

    /// Return the current configuration
    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }
}
