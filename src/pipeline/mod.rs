//! Module pipeline de traitement temps réel
//!
//! Gère le flux audio → STT → événements de transcription.

mod realtime;

pub use realtime::{RealtimePipeline, PipelineConfig, PipelineError, PipelineStatus};
