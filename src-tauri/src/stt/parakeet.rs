//! NVIDIA Parakeet local STT engine (ONNX Runtime)
//!
//! 100% local inference, no data leaves the machine.
//! Uses parakeet-tdt-0.6b-v3 (multilingual, 25 languages, auto-detection).
//! Like the API engines: accumulates all audio, transcribes on flush (stop).

use super::engine::{Language, SttEngine, SttError, SttEvent};
use parakeet_rs::{ParakeetTDT, Transcriber};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Files required in the model directory (parakeet-rs expects these exact names)
pub const REQUIRED_FILES: [&str; 3] = [
    "encoder-model.onnx",
    "decoder_joint-model.onnx",
    "vocab.txt",
];

/// Check that all model files are present in the directory
pub fn is_model_downloaded(dir: &Path) -> bool {
    REQUIRED_FILES.iter().all(|f| dir.join(f).is_file())
}

/// Local STT engine based on NVIDIA Parakeet TDT
pub struct ParakeetEngine {
    language: Language,
    /// Accumulates all audio until flush
    audio_buffer: Vec<f32>,
    /// Events ready to be consumed
    shared_events: Arc<Mutex<VecDeque<SttEvent>>>,
    /// Flag indicating a transcription is in progress
    pending: Arc<AtomicBool>,
    /// Model loaded in background (loading takes a few seconds)
    model: Arc<Mutex<Option<ParakeetTDT>>>,
    /// Set if the background load failed
    load_failed: Arc<AtomicBool>,
}

impl ParakeetEngine {
    /// Wait until the model is loaded (or failed), then transcribe
    fn transcribe_blocking(
        model: &Arc<Mutex<Option<ParakeetTDT>>>,
        load_failed: &Arc<AtomicBool>,
        audio: Vec<f32>,
    ) -> Result<String, SttError> {
        // Wait for the background load to finish (max 120s)
        let start = std::time::Instant::now();
        loop {
            if load_failed.load(Ordering::SeqCst) {
                return Err(SttError::ModelLoadError(
                    "Parakeet model failed to load".to_string(),
                ));
            }
            {
                let guard = model
                    .lock()
                    .map_err(|_| SttError::InferenceError("Model lock poisoned".to_string()))?;
                if guard.is_some() {
                    break;
                }
            }
            if start.elapsed() > std::time::Duration::from_secs(120) {
                return Err(SttError::InferenceError(
                    "Timeout waiting for Parakeet model load".to_string(),
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        let mut guard = model
            .lock()
            .map_err(|_| SttError::InferenceError("Model lock poisoned".to_string()))?;
        let parakeet = guard.as_mut().ok_or(SttError::NotInitialized)?;

        let result = parakeet
            .transcribe_samples(audio, 16000, 1, None)
            .map_err(|e| SttError::InferenceError(format!("Parakeet inference: {}", e)))?;

        Ok(result.text.trim().to_string())
    }

    /// Transcribe the full accumulated buffer in a background thread
    fn send_full_audio(&mut self) {
        if self.audio_buffer.is_empty() {
            return;
        }

        // Ignore if less than 1 second of audio
        if self.audio_buffer.len() < 16000 {
            tracing::debug!(
                "Audio too short ({} samples), skipped",
                self.audio_buffer.len()
            );
            self.audio_buffer.clear();
            return;
        }

        let audio_data = std::mem::take(&mut self.audio_buffer);
        let shared_events = Arc::clone(&self.shared_events);
        let pending = Arc::clone(&self.pending);
        let model = Arc::clone(&self.model);
        let load_failed = Arc::clone(&self.load_failed);

        pending.store(true, Ordering::SeqCst);

        let duration = audio_data.len() as f32 / 16000.0;
        tracing::info!("Parakeet local transcription of {:.1}s audio...", duration);

        std::thread::spawn(move || {
            match Self::transcribe_blocking(&model, &load_failed, audio_data) {
                Ok(text) => {
                    if !text.is_empty() {
                        tracing::info!("Parakeet result: {}", text);
                        if let Ok(mut events) = shared_events.lock() {
                            events.push_back(SttEvent::Final(text));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Parakeet error: {}", e);
                    if let Ok(mut events) = shared_events.lock() {
                        events.push_back(SttEvent::Error(format!("Parakeet: {}", e)));
                    }
                }
            }
            pending.store(false, Ordering::SeqCst);
        });
    }

    /// Wait for the current transcription to complete (max 120s: first
    /// flush may also wait for the background model load)
    fn wait_for_pending(&self) {
        let start = std::time::Instant::now();
        while self.pending.load(Ordering::SeqCst) {
            if start.elapsed() > std::time::Duration::from_secs(120) {
                tracing::warn!("Timeout waiting for Parakeet transcription");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

impl SttEngine for ParakeetEngine {
    fn load(model_path: &str) -> Result<Self, SttError> {
        let dir = Path::new(model_path);
        if !is_model_downloaded(dir) {
            return Err(SttError::ModelNotFound(format!(
                "Parakeet model files missing in {}",
                dir.display()
            )));
        }

        let model: Arc<Mutex<Option<ParakeetTDT>>> = Arc::new(Mutex::new(None));
        let load_failed = Arc::new(AtomicBool::new(false));

        // Load the model in the background: it takes a few seconds and
        // start_recording must stay responsive.
        let model_clone = Arc::clone(&model);
        let load_failed_clone = Arc::clone(&load_failed);
        let dir_owned = dir.to_path_buf();
        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            match ParakeetTDT::from_pretrained(&dir_owned, None) {
                Ok(parakeet) => {
                    if let Ok(mut guard) = model_clone.lock() {
                        *guard = Some(parakeet);
                    }
                    tracing::info!(
                        "Parakeet model loaded in {:.1}s",
                        start.elapsed().as_secs_f32()
                    );
                }
                Err(e) => {
                    load_failed_clone.store(true, Ordering::SeqCst);
                    tracing::error!("Parakeet model load error: {}", e);
                }
            }
        });

        tracing::info!("Initializing Parakeet local engine ({})", model_path);
        Ok(Self {
            language: Language::Auto,
            audio_buffer: Vec::new(),
            shared_events: Arc::new(Mutex::new(VecDeque::new())),
            pending: Arc::new(AtomicBool::new(false)),
            model,
            load_failed,
        })
    }

    fn set_language(&mut self, language: Language) {
        // Parakeet TDT v3 auto-detects the language; stored for reference only
        self.language = language.clone();
        tracing::debug!("Parakeet language set: {:?} (auto-detected at inference)", language);
    }

    fn language(&self) -> &Language {
        &self.language
    }

    fn push_audio(&mut self, pcm: &[f32]) {
        // Just accumulate - we'll transcribe everything on flush
        self.audio_buffer.extend_from_slice(pcm);
    }

    fn poll(&mut self) -> Option<SttEvent> {
        if let Ok(mut events) = self.shared_events.lock() {
            events.pop_front()
        } else {
            None
        }
    }

    fn flush(&mut self) {
        tracing::info!(
            "Flush Parakeet: {} samples ({:.1}s)",
            self.audio_buffer.len(),
            self.audio_buffer.len() as f32 / 16000.0
        );
        // Transcribe all accumulated audio locally
        self.send_full_audio();
        // Wait for the result
        self.wait_for_pending();
    }

    fn reset(&mut self) {
        self.audio_buffer.clear();
        if let Ok(mut events) = self.shared_events.lock() {
            events.clear();
        }
        tracing::debug!("Parakeet engine reset");
    }

    fn name(&self) -> &str {
        "Parakeet (local)"
    }

    fn is_ready(&self) -> bool {
        !self.load_failed.load(Ordering::SeqCst)
    }
}
