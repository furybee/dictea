//! Audio capture module
//!
//! Handles microphone capture in a dedicated thread.

mod microphone;

pub use microphone::{AudioConfig, AudioHandle, MicrophoneError};
