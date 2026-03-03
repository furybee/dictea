//! Groq Whisper API implementation for STT
//!
//! OpenAI-compatible API hosted on Groq's ultra-fast hardware.
//! Accumulates all audio, then sends in a single call on flush (stop).

use super::engine::{Language, SttEngine, SttError, SttEvent};
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// STT engine based on the Groq Whisper API
pub struct GroqEngine {
    api_key: String,
    language: Language,
    /// Accumulates all audio until flush
    audio_buffer: Vec<f32>,
    /// Events ready to be consumed
    shared_events: Arc<Mutex<VecDeque<SttEvent>>>,
    /// Flag indicating a request is in progress
    pending: Arc<AtomicBool>,
    http_client: reqwest::Client,
}

impl GroqEngine {
    pub fn with_api_key(api_key: String) -> Self {
        Self {
            api_key,
            language: Language::Auto,
            audio_buffer: Vec::new(),
            shared_events: Arc::new(Mutex::new(VecDeque::new())),
            pending: Arc::new(AtomicBool::new(false)),
            http_client: reqwest::Client::new(),
        }
    }

    /// Convert f32 samples to WAV bytes
    fn samples_to_wav(samples: &[f32]) -> Result<Vec<u8>, SttError> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec)
                .map_err(|e| SttError::InferenceError(format!("WAV error: {}", e)))?;

            for &sample in samples {
                let sample_i16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                writer
                    .write_sample(sample_i16)
                    .map_err(|e| SttError::InferenceError(format!("WAV write error: {}", e)))?;
            }

            writer
                .finalize()
                .map_err(|e| SttError::InferenceError(format!("WAV finalize error: {}", e)))?;
        }

        let data = cursor.into_inner();
        tracing::debug!("WAV header: {:?}", &data[..4.min(data.len())]);
        Ok(data)
    }

    /// Groq Whisper API call
    async fn transcribe_async(
        client: reqwest::Client,
        api_key: String,
        audio_data: Vec<f32>,
        language: Option<String>,
    ) -> Result<String, SttError> {
        let wav_data = Self::samples_to_wav(&audio_data)?;

        let duration_secs = audio_data.len() as f32 / 16000.0;
        tracing::info!(
            "Sending to Groq: {:.1}s audio, {} bytes WAV",
            duration_secs,
            wav_data.len()
        );

        let file_part = reqwest::multipart::Part::bytes(wav_data)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| SttError::InferenceError(e.to_string()))?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", "whisper-large-v3-turbo");

        if let Some(lang) = language {
            form = form.text("language", lang);
        }

        let response = client
            .post("https://api.groq.com/openai/v1/audio/transcriptions")
            .header("Authorization", format!("Bearer {}", api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| SttError::InferenceError(format!("Network error: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(SttError::InferenceError(format!(
                "Groq API error {}: {}",
                status, error_text
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| SttError::InferenceError(format!("JSON error: {}", e)))?;

        let text = json["text"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(text)
    }

    /// Send all accumulated audio buffer to the API
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
        let client = self.http_client.clone();
        let api_key = self.api_key.clone();
        let language = match &self.language {
            Language::Auto => None,
            lang => Some(lang.code().to_string()),
        };
        let shared_events = Arc::clone(&self.shared_events);
        let pending = Arc::clone(&self.pending);

        pending.store(true, Ordering::SeqCst);

        let duration = audio_data.len() as f32 / 16000.0;
        tracing::info!("Groq transcription of {:.1}s audio...", duration);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                match Self::transcribe_async(client, api_key, audio_data, language).await {
                    Ok(text) => {
                        if !text.is_empty() {
                            tracing::info!("Groq result: {}", text);
                            if let Ok(mut events) = shared_events.lock() {
                                events.push_back(SttEvent::Final(text));
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Groq error: {}", e);
                    }
                }
                pending.store(false, Ordering::SeqCst);
            });
        });
    }

    /// Wait for the current request to complete (max 30s)
    fn wait_for_pending(&self) {
        let start = std::time::Instant::now();
        while self.pending.load(Ordering::SeqCst) {
            if start.elapsed() > std::time::Duration::from_secs(30) {
                tracing::warn!("Timeout waiting for Groq response");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

impl SttEngine for GroqEngine {
    fn load(api_key: &str) -> Result<Self, SttError> {
        if api_key.is_empty() {
            return Err(SttError::ModelNotFound(
                "Groq API key required".to_string(),
            ));
        }

        tracing::info!("Initializing Groq Whisper API");
        Ok(Self::with_api_key(api_key.to_string()))
    }

    fn set_language(&mut self, language: Language) {
        self.language = language.clone();
        tracing::debug!("Groq language set: {:?}", language);
    }

    fn language(&self) -> &Language {
        &self.language
    }

    fn push_audio(&mut self, pcm: &[f32]) {
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
            "Flush Groq: {} samples ({:.1}s)",
            self.audio_buffer.len(),
            self.audio_buffer.len() as f32 / 16000.0
        );
        self.send_full_audio();
        self.wait_for_pending();
    }

    fn reset(&mut self) {
        self.audio_buffer.clear();
        if let Ok(mut events) = self.shared_events.lock() {
            events.clear();
        }
        tracing::debug!("Groq engine reset");
    }

    fn name(&self) -> &str {
        "Groq Whisper"
    }

    fn is_ready(&self) -> bool {
        true
    }
}
