//! Module de capture audio
//!
//! Gère la capture du microphone et le buffering temps réel.

mod microphone;

pub use microphone::{AudioConfig, AudioStream, MicrophoneError};
