//! STT (Speech-to-Text) module
//!
//! Provides traits and implementations for voice transcription.

mod engine;
mod gemini;
mod openai;
mod voxtral;
pub use engine::{SttEngine, SttEvent, SttError, Language};
pub use gemini::GeminiEngine;
pub use openai::OpenAiEngine;
pub use voxtral::VoxtralEngine;
