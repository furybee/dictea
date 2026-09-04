//! Gemini implementation for STT
//!
//! Accumulates all audio, then sends in a single call on flush (stop).
//! Uses gemini-3.5-transcribe, the dedicated speech model, through the
//! /v1beta/interactions endpoint with base64-encoded audio.

use super::engine::{Language, SttEngine, SttError, SttEvent};
use base64::Engine as _;
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// STT engine based on Gemini (Google AI)
pub struct GeminiEngine {
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

impl GeminiEngine {
    /// Create a new instance with an API key
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

        Ok(cursor.into_inner())
    }

    /// Run inference via the Gemini transcription API
    ///
    /// gemini-3.5-transcribe is a dedicated speech model and does not answer on
    /// generateContent like the general-purpose ones: it lives behind
    /// /v1beta/interactions, takes the audio as an input item rather than a
    /// content part, and replaces the "transcribe this" prompt with a
    /// transcription_config. Audio is still sent inline, so no upload through
    /// the Files API is needed for dictation-sized clips.
    async fn transcribe_async(
        client: reqwest::Client,
        api_key: String,
        audio_data: Vec<f32>,
        language: Option<String>,
    ) -> Result<String, SttError> {
        let wav_data = Self::samples_to_wav(&audio_data)?;
        let audio_base64 = base64::engine::general_purpose::STANDARD.encode(&wav_data);

        let duration_secs = audio_data.len() as f32 / 16000.0;
        tracing::info!(
            "Sending to Gemini: {:.1}s audio, {} bytes WAV",
            duration_secs,
            wav_data.len()
        );

        // An empty list means auto-detection, which is what Auto maps to
        let language_codes: Vec<String> = language.into_iter().collect();

        let body = serde_json::json!({
            "model": "gemini-3.5-transcribe",
            "input": [{
                "type": "audio",
                "data": audio_base64,
                "mime_type": "audio/wav"
            }],
            "generation_config": {
                "transcription_config": {
                    "language_codes": language_codes
                }
            }
        });

        let response = client
            .post("https://generativelanguage.googleapis.com/v1beta/interactions")
            .header("x-goog-api-key", &api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| SttError::InferenceError(format!("Network error: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(SttError::InferenceError(format!(
                "Gemini API error {}: {}",
                status, error_text
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| SttError::InferenceError(format!("JSON parse error: {}", e)))?;

        // Anything other than "completed" means there is no transcript to read,
        // and silently returning an empty string would look like silence
        if let Some(status) = json["status"].as_str() {
            if status != "completed" {
                return Err(SttError::InferenceError(format!(
                    "Gemini transcription {}: {}",
                    status, json
                )));
            }
        }

        // The transcript is spread over the text parts of the model_output
        // steps — there is no top-level field holding it
        let text = json["steps"]
            .as_array()
            .map(|steps| {
                steps
                    .iter()
                    .filter_map(|step| step["content"].as_array())
                    .flatten()
                    .filter(|part| part["type"] == "text")
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default()
            .trim()
            .to_string();

        Ok(text)
    }

    /// Send all accumulated audio buffer to the API
    fn send_full_audio(&mut self) {
        // Ignore if less than 1 second of audio (an empty buffer included:
        // a tap on the shortcut should say so rather than do nothing)
        if self.audio_buffer.len() < 16000 {
            tracing::debug!(
                "Audio too short ({} samples), skipped",
                self.audio_buffer.len()
            );
            self.audio_buffer.clear();
            // Tell the user, otherwise the dictation just vanishes
            if let Ok(mut events) = self.shared_events.lock() {
                events.push_back(SttEvent::Error(
                    "Recording too short (less than 1 second)".to_string(),
                ));
            }
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
        tracing::info!("Gemini transcription of {:.1}s audio...", duration);

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                match Self::transcribe_async(client, api_key, audio_data, language).await {
                    Ok(text) => {
                        if !text.is_empty() {
                            tracing::info!("Gemini result: {}", text);
                            if let Ok(mut events) = shared_events.lock() {
                                events.push_back(SttEvent::Final(text));
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Gemini error: {}", e);
                        if let Ok(mut events) = shared_events.lock() {
                            events.push_back(SttEvent::Error(format!("Gemini: {}", e)));
                        }
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
                tracing::warn!("Timeout waiting for Gemini response");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

impl SttEngine for GeminiEngine {
    fn load(api_key_or_path: &str) -> Result<Self, SttError> {
        if api_key_or_path.is_empty() {
            return Err(SttError::ModelNotFound(
                "Gemini API key required".to_string(),
            ));
        }

        tracing::info!("Initializing Gemini with API key");
        Ok(Self::with_api_key(api_key_or_path.to_string()))
    }

    fn set_language(&mut self, language: Language) {
        self.language = language.clone();
        tracing::debug!("Gemini language set: {:?}", language);
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
            "Flush Gemini: {} samples ({:.1}s)",
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
        tracing::debug!("Gemini engine reset");
    }

    fn name(&self) -> &str {
        "Gemini"
    }

    fn is_ready(&self) -> bool {
        true
    }
}

impl Default for GeminiEngine {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            language: Language::Auto,
            audio_buffer: Vec::new(),
            shared_events: Arc::new(Mutex::new(VecDeque::new())),
            pending: Arc::new(AtomicBool::new(false)),
            http_client: reqwest::Client::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hits the real API when DICTEA_GEMINI_KEY is set, and is skipped
    /// otherwise (CI has no key).
    ///
    /// Worth the round trip: the transcription endpoint has nothing in common
    /// with generateContent, and the transcript is buried in the steps rather
    /// than exposed at the top level. Only a live call proves the request and
    /// the parser still agree with the API.
    ///
    /// Generate the fixture with:
    ///   say -v Thomas -o /tmp/g.aiff "Bonjour, ceci est un test"
    ///   afconvert -f WAVE -d LEI16@16000 -c 1 /tmp/g.aiff /tmp/g.wav
    #[tokio::test]
    async fn transcribes_a_wav_through_the_live_api() {
        let Ok(key) = std::env::var("DICTEA_GEMINI_KEY") else {
            eprintln!("DICTEA_GEMINI_KEY not set, skipping");
            return;
        };
        let wav_path =
            std::env::var("DICTEA_TEST_WAV").unwrap_or_else(|_| "/tmp/g.wav".to_string());
        let Ok(mut reader) = hound::WavReader::open(&wav_path) else {
            eprintln!("{} not found, skipping", wav_path);
            return;
        };

        let samples: Vec<f32> = reader
            .samples::<i16>()
            .filter_map(Result::ok)
            .map(|s| s as f32 / 32768.0)
            .collect();
        assert!(!samples.is_empty(), "fixture has no audio");

        let text = GeminiEngine::transcribe_async(reqwest::Client::new(), key, samples, None)
            .await
            .expect("transcription should succeed");

        eprintln!("transcript: {}", text);
        assert!(!text.trim().is_empty(), "transcript is empty");
    }
}
