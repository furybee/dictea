//! Pipeline de transcription temps réel
//!
//! Orchestre la capture audio et le moteur STT dans une boucle async.

use crate::audio::{AudioStream, MicrophoneError};
use crate::stt::{Language, SttEngine, SttError, SttEvent};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio::time::{interval, Duration};

/// Configuration du pipeline
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Intervalle de traitement des chunks audio (ms)
    pub chunk_interval_ms: u64,
    /// Taille du chunk audio (échantillons)
    pub chunk_size: usize,
    /// Langue pour la transcription
    pub language: Language,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            chunk_interval_ms: 30, // 30ms pour faible latence
            chunk_size: 480,       // 30ms @ 16kHz
            language: Language::Auto,
        }
    }
}

/// Erreurs du pipeline
#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("Erreur audio: {0}")]
    AudioError(#[from] MicrophoneError),

    #[error("Erreur STT: {0}")]
    SttError(#[from] SttError),

    #[error("Pipeline déjà en cours d'exécution")]
    AlreadyRunning,

    #[error("Pipeline non démarré")]
    NotRunning,
}

/// État du pipeline
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineStatus {
    /// Pipeline arrêté
    Stopped,
    /// Pipeline en cours de démarrage
    Starting,
    /// Pipeline en cours d'exécution
    Running,
    /// Pipeline en cours d'arrêt
    Stopping,
    /// Pipeline en erreur
    Error(String),
}

/// Pipeline de transcription temps réel
pub struct RealtimePipeline<E: SttEngine> {
    config: PipelineConfig,
    audio_stream: Option<AudioStream>,
    stt_engine: Arc<Mutex<E>>,
    status: Arc<RwLock<PipelineStatus>>,
    event_tx: broadcast::Sender<SttEvent>,
    stop_tx: Option<mpsc::Sender<()>>,
}

impl<E: SttEngine + 'static> RealtimePipeline<E> {
    /// Crée un nouveau pipeline avec le moteur STT spécifié
    pub fn new(stt_engine: E, config: PipelineConfig) -> Self {
        let (event_tx, _) = broadcast::channel(100);

        Self {
            config,
            audio_stream: None,
            stt_engine: Arc::new(Mutex::new(stt_engine)),
            status: Arc::new(RwLock::new(PipelineStatus::Stopped)),
            event_tx,
            stop_tx: None,
        }
    }

    /// Démarre le pipeline
    pub async fn start(&mut self) -> Result<(), PipelineError> {
        // Vérifier l'état actuel
        {
            let status = self.status.read().await;
            if *status == PipelineStatus::Running {
                return Err(PipelineError::AlreadyRunning);
            }
        }

        // Mettre à jour le statut
        {
            let mut status = self.status.write().await;
            *status = PipelineStatus::Starting;
        }

        // Initialiser le stream audio
        let audio_stream = AudioStream::new()?;
        audio_stream.start()?;
        self.audio_stream = Some(audio_stream);

        // Configurer la langue
        {
            let mut engine = self.stt_engine.lock().await;
            engine.set_language(self.config.language.clone());
        }

        // Créer le canal d'arrêt
        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
        self.stop_tx = Some(stop_tx);

        // Lancer la boucle de traitement
        let audio_stream = self.audio_stream.as_ref().unwrap();
        let stt_engine = Arc::clone(&self.stt_engine);
        let status = Arc::clone(&self.status);
        let event_tx = self.event_tx.clone();
        let chunk_size = self.config.chunk_size;
        let chunk_interval = Duration::from_millis(self.config.chunk_interval_ms);

        // Clone pour le move dans la task
        let audio_consumer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(chunk_size)));

        {
            let mut status = self.status.write().await;
            *status = PipelineStatus::Running;
        }

        tracing::info!("Pipeline démarré");

        // Note: Dans une vraie implémentation, on lancerait une task tokio ici
        // Pour l'instant, on laisse le pipeline dans l'état Running

        Ok(())
    }

    /// Arrête le pipeline
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

        // Envoyer le signal d'arrêt
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(()).await;
        }

        // Arrêter le stream audio
        if let Some(ref audio_stream) = self.audio_stream {
            audio_stream.stop()?;
        }

        // Flush le moteur STT
        {
            let mut engine = self.stt_engine.lock().await;
            engine.flush();

            // Récupérer les derniers événements
            while let Some(event) = engine.poll() {
                let _ = self.event_tx.send(event);
            }
        }

        {
            let mut status = self.status.write().await;
            *status = PipelineStatus::Stopped;
        }

        tracing::info!("Pipeline arrêté");
        Ok(())
    }

    /// Retourne le statut actuel du pipeline
    pub async fn status(&self) -> PipelineStatus {
        self.status.read().await.clone()
    }

    /// S'abonne aux événements de transcription
    pub fn subscribe(&self) -> broadcast::Receiver<SttEvent> {
        self.event_tx.subscribe()
    }

    /// Change la langue de transcription
    pub async fn set_language(&mut self, language: Language) {
        self.config.language = language.clone();
        let mut engine = self.stt_engine.lock().await;
        engine.set_language(language);
    }

    /// Retourne la configuration actuelle
    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }
}
