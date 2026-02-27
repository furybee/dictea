//! Implémentation Voxtral (Mistral) pour le STT

use super::engine::{Language, SttEngine, SttError, SttEvent};
use std::collections::VecDeque;

/// Moteur STT basé sur Voxtral (Mistral)
pub struct VoxtralEngine {
    model_path: String,
    language: Language,
    audio_buffer: Vec<f32>,
    events: VecDeque<SttEvent>,
    is_ready: bool,
}

impl VoxtralEngine {
    /// Taille minimale du buffer pour lancer l'inférence (en échantillons)
    /// 16000 * 0.5 = 500ms d'audio
    const MIN_BUFFER_SIZE: usize = 8000;

    /// Crée une nouvelle instance (sans charger le modèle)
    pub fn new() -> Self {
        Self {
            model_path: String::new(),
            language: Language::Auto,
            audio_buffer: Vec::with_capacity(Self::MIN_BUFFER_SIZE * 2),
            events: VecDeque::new(),
            is_ready: false,
        }
    }

    /// Effectue l'inférence sur le buffer audio
    fn run_inference(&mut self) {
        if self.audio_buffer.len() < Self::MIN_BUFFER_SIZE {
            return;
        }

        // TODO: Implémenter l'inférence Voxtral réelle
        // Pour l'instant, on simule une transcription
        tracing::debug!(
            "Inférence Voxtral sur {} échantillons",
            self.audio_buffer.len()
        );

        // Placeholder: émettre un événement partial
        self.events.push_back(SttEvent::Partial("[transcription...]".to_string()));

        // Vider le buffer après traitement
        self.audio_buffer.clear();
    }
}

impl SttEngine for VoxtralEngine {
    fn load(model_path: &str) -> Result<Self, SttError> {
        // Vérifier que le fichier existe
        if !std::path::Path::new(model_path).exists() {
            return Err(SttError::ModelNotFound(model_path.to_string()));
        }

        tracing::info!("Chargement du modèle Voxtral: {}", model_path);

        // TODO: Charger le modèle Voxtral réel
        Ok(Self {
            model_path: model_path.to_string(),
            language: Language::Auto,
            audio_buffer: Vec::with_capacity(Self::MIN_BUFFER_SIZE * 2),
            events: VecDeque::new(),
            is_ready: true,
        })
    }

    fn set_language(&mut self, language: Language) {
        self.language = language;
        tracing::debug!("Langue définie: {:?}", self.language);
    }

    fn language(&self) -> &Language {
        &self.language
    }

    fn push_audio(&mut self, pcm: &[f32]) {
        self.audio_buffer.extend_from_slice(pcm);

        // Lancer l'inférence si assez de données
        if self.audio_buffer.len() >= Self::MIN_BUFFER_SIZE {
            self.run_inference();
        }
    }

    fn poll(&mut self) -> Option<SttEvent> {
        self.events.pop_front()
    }

    fn flush(&mut self) {
        if !self.audio_buffer.is_empty() {
            // Forcer l'inférence sur les données restantes
            tracing::debug!("Flush: {} échantillons restants", self.audio_buffer.len());

            // TODO: Implémenter l'inférence finale
            self.events.push_back(SttEvent::Final("[fin de transcription]".to_string()));
            self.audio_buffer.clear();
        }
    }

    fn reset(&mut self) {
        self.audio_buffer.clear();
        self.events.clear();
        tracing::debug!("Moteur Voxtral réinitialisé");
    }

    fn name(&self) -> &str {
        "Voxtral"
    }

    fn is_ready(&self) -> bool {
        self.is_ready
    }
}

impl Default for VoxtralEngine {
    fn default() -> Self {
        Self::new()
    }
}
