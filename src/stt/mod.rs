//! Module STT (Speech-to-Text)
//!
//! Fournit les traits et implémentations pour la transcription vocale.

mod engine;
mod voxtral;
mod whisper;

pub use engine::{SttEngine, SttEvent, SttError, Language};
pub use voxtral::VoxtralEngine;
pub use whisper::WhisperEngine;
