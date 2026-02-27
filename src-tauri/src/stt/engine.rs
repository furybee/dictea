//! Main trait for STT engines

use thiserror::Error;

/// Events emitted by the STT engine
#[derive(Debug, Clone)]
pub enum SttEvent {
    /// Partial transcription (may be rewritten)
    Partial(String),
    /// Final transcription (definitive)
    Final(String),
}

/// Supported languages for transcription
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Language {
    /// Automatic detection
    Auto,
    /// French
    French,
    /// English
    English,
    /// Spanish
    Spanish,
    /// German
    German,
    /// Italian
    Italian,
    /// Portuguese
    Portuguese,
    /// Other language (ISO 639-1 code)
    Other(String),
}

impl Language {
    /// Return the ISO 639-1 language code
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

    /// Create a language from an ISO 639-1 code
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

/// STT engine errors
#[derive(Error, Debug)]
pub enum SttError {
    #[error("Model load error: {0}")]
    ModelLoadError(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Inference error: {0}")]
    InferenceError(String),

    #[error("Invalid audio format: {0}")]
    InvalidAudioFormat(String),

    #[error("Engine not initialized")]
    NotInitialized,
}

/// Main trait for STT engines
///
/// Each implementation (Voxtral, OpenAI) must implement this trait.
pub trait SttEngine: Send + Sync {
    /// Load the model from the specified path
    fn load(model_path: &str) -> Result<Self, SttError>
    where
        Self: Sized;

    /// Set the language for transcription
    fn set_language(&mut self, language: Language);

    /// Return the current language
    fn language(&self) -> &Language;

    /// Push audio samples to the engine
    ///
    /// Samples must be PCM float32, mono, 16kHz.
    fn push_audio(&mut self, pcm: &[f32]);

    /// Retrieve the next transcription event
    ///
    /// Returns `None` if no event is available.
    fn poll(&mut self) -> Option<SttEvent>;

    /// Flush the audio buffer and force final transcription
    fn flush(&mut self);

    /// Reset the engine state
    fn reset(&mut self);

    /// Return the engine name
    fn name(&self) -> &str;

    /// Check if the engine is ready
    fn is_ready(&self) -> bool;
}
