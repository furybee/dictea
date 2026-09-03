//! STT (Speech-to-Text) module
//!
//! Provides traits and implementations for voice transcription.

mod engine;
mod gemini;
mod groq;
mod openai;
pub mod parakeet;
mod voxtral;
pub use engine::{SttEngine, SttEvent, SttError, Language};
pub use gemini::GeminiEngine;
pub use groq::GroqEngine;
pub use openai::OpenAiEngine;
pub use parakeet::ParakeetEngine;
pub use voxtral::VoxtralEngine;
