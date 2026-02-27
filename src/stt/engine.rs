//! Trait principal pour les moteurs STT

use thiserror::Error;

/// Événements émis par le moteur STT
#[derive(Debug, Clone)]
pub enum SttEvent {
    /// Transcription partielle (peut être réécrite)
    Partial(String),
    /// Transcription finale (définitive)
    Final(String),
}

/// Langues supportées pour la transcription
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Language {
    /// Détection automatique
    Auto,
    /// Français
    French,
    /// Anglais
    English,
    /// Espagnol
    Spanish,
    /// Allemand
    German,
    /// Italien
    Italian,
    /// Portugais
    Portuguese,
    /// Autre langue (code ISO 639-1)
    Other(String),
}

impl Language {
    /// Retourne le code ISO 639-1 de la langue
    pub fn code(&self) -> &str {
        match self {
            Language::Auto => "auto",
            Language::French => "fr",
            Language::English => "en",
            Language::Spanish => "es",
            Language::German => "de",
            Language::Italian => "it",
            Language::Portuguese => "pt",
            Language::Other(code) => code,
        }
    }

    /// Crée une langue depuis un code ISO 639-1
    pub fn from_code(code: &str) -> Self {
        match code.to_lowercase().as_str() {
            "auto" => Language::Auto,
            "fr" | "french" => Language::French,
            "en" | "english" => Language::English,
            "es" | "spanish" => Language::Spanish,
            "de" | "german" => Language::German,
            "it" | "italian" => Language::Italian,
            "pt" | "portuguese" => Language::Portuguese,
            other => Language::Other(other.to_string()),
        }
    }
}

/// Erreurs du moteur STT
#[derive(Error, Debug)]
pub enum SttError {
    #[error("Erreur de chargement du modèle: {0}")]
    ModelLoadError(String),

    #[error("Modèle non trouvé: {0}")]
    ModelNotFound(String),

    #[error("Erreur d'inférence: {0}")]
    InferenceError(String),

    #[error("Format audio invalide: {0}")]
    InvalidAudioFormat(String),

    #[error("Moteur non initialisé")]
    NotInitialized,
}

/// Trait principal pour les moteurs STT
///
/// Chaque implémentation (Voxtral, Whisper) doit implémenter ce trait.
pub trait SttEngine: Send + Sync {
    /// Charge le modèle depuis le chemin spécifié
    fn load(model_path: &str) -> Result<Self, SttError>
    where
        Self: Sized;

    /// Définit la langue pour la transcription
    fn set_language(&mut self, language: Language);

    /// Retourne la langue actuelle
    fn language(&self) -> &Language;

    /// Envoie des échantillons audio au moteur
    ///
    /// Les échantillons doivent être en PCM float32, mono, 16kHz.
    fn push_audio(&mut self, pcm: &[f32]);

    /// Récupère le prochain événement de transcription
    ///
    /// Retourne `None` si aucun événement n'est disponible.
    fn poll(&mut self) -> Option<SttEvent>;

    /// Vide le buffer audio et force la transcription finale
    fn flush(&mut self);

    /// Réinitialise l'état du moteur
    fn reset(&mut self);

    /// Retourne le nom du moteur
    fn name(&self) -> &str;

    /// Vérifie si le moteur est prêt
    fn is_ready(&self) -> bool;
}
