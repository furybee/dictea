//! Real-time processing pipeline module

#[allow(dead_code)]
mod realtime;

#[allow(unused_imports)]
pub use realtime::{PipelineConfig, PipelineError, PipelineStatus, RealtimePipeline};
