//! Dictea - STT application with floating overlay
//!
//! Global shortcut to activate voice dictation.

mod apple_intelligence;
mod audio;
mod models;
mod pipeline;
mod stt;

use audio::{AudioConfig, AudioHandle};
use stt::{Language, GeminiEngine, GroqEngine, OpenAiEngine, ParakeetEngine, VoxtralEngine, SttEngine, SttEvent};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Application configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    pub global_shortcut: String,
    pub openai_api_key: String,
    pub output_language: String,
    /// Reformulate text via GPT before pasting
    #[serde(default)]
    pub reformulate: bool,
    /// STT engine: "openai", "voxtral", "gemini", or "groq"
    #[serde(default = "default_stt_engine")]
    pub stt_engine: String,
    /// Mistral API key (used when stt_engine == "voxtral")
    #[serde(default)]
    pub mistral_api_key: String,
    /// Gemini API key (used when stt_engine == "gemini")
    #[serde(default)]
    pub gemini_api_key: String,
    /// Groq API key (used when stt_engine == "groq")
    #[serde(default)]
    pub groq_api_key: String,
    /// Selected audio input device name (empty = system default)
    #[serde(default)]
    pub audio_device: String,
    /// Provider that reformulates and translates: "auto" follows the STT
    /// engine, or name one explicitly ("apple", "openai", "groq", "voxtral",
    /// "gemini").
    ///
    /// Renamed from parakeet_reformulation_provider, which only existed
    /// because the local engine forced the question. The alias keeps configs
    /// written by earlier versions working — without it every user would
    /// silently fall back to the default and lose their choice.
    #[serde(
        default = "default_reformulation_provider",
        alias = "parakeet_reformulation_provider"
    )]
    pub reformulation_provider: String,
}

fn default_stt_engine() -> String {
    "openai".to_string()
}

fn default_reformulation_provider() -> String {
    "auto".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            global_shortcut: "CmdOrCtrl+Shift+Space".to_string(),
            openai_api_key: String::new(),
            output_language: "auto".to_string(),
            reformulate: false,
            stt_engine: "openai".to_string(),
            mistral_api_key: String::new(),
            gemini_api_key: String::new(),
            groq_api_key: String::new(),
            audio_device: String::new(),
            reformulation_provider: "auto".to_string(),
        }
    }
}

impl AppConfig {
    /// Config file path
    fn config_path(app: &AppHandle) -> PathBuf {
        app.path()
            .app_data_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("config.json")
    }

    /// Load config from disk, or return defaults
    fn load(app: &AppHandle) -> Self {
        let path = Self::config_path(app);
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(config) => {
                        tracing::info!("Config loaded from {}", path.display());
                        return config;
                    }
                    Err(e) => tracing::warn!("Invalid config, using defaults: {}", e),
                },
                Err(e) => tracing::warn!("Cannot read config: {}", e),
            }
        }
        Self::default()
    }

    /// Save config to disk
    fn save(&self, app: &AppHandle) {
        let path = Self::config_path(app);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    tracing::error!("Config save error: {}", e);
                } else {
                    tracing::info!("Config saved to {}", path.display());
                }
            }
            Err(e) => tracing::error!("Config serialization error: {}", e),
        }
    }
}

/// Current transcription state
#[derive(Debug, Clone, serde::Serialize)]
pub struct TranscriptionState {
    pub is_recording: bool,
    pub partial_text: String,
    pub final_text: String,
}

impl Default for TranscriptionState {
    fn default() -> Self {
        Self {
            is_recording: false,
            partial_text: String::new(),
            final_text: String::new(),
        }
    }
}

/// Simplified transcription pipeline
struct TranscriptionPipeline {
    engine: Box<dyn SttEngine>,
    audio_handle: Option<AudioHandle>,
    event_tx: broadcast::Sender<SttEvent>,
    is_running: bool,
}

impl TranscriptionPipeline {
    fn new(engine: Box<dyn SttEngine>) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            engine,
            audio_handle: None,
            event_tx,
            is_running: false,
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<SttEvent> {
        self.event_tx.subscribe()
    }

    fn start(&mut self, language: Language) -> Result<(), String> {
        if self.is_running {
            return Ok(());
        }

        self.engine.set_language(language);
        self.is_running = true;
        tracing::info!("Transcription started");
        Ok(())
    }

    /// Stop without transcribing: the buffered audio is dropped.
    /// Returns immediately — no inference, no API call.
    fn cancel(&mut self) {
        if let Some(mut handle) = self.audio_handle.take() {
            handle.stop();
        }
        if self.is_running {
            self.engine.reset();
            self.is_running = false;
            tracing::info!("Transcription cancelled, buffered audio dropped");
        }
    }

    /// Stop the pipeline and return remaining events
    ///
    /// Blocking: the engine flush waits for the transcription to complete.
    /// Callers on the async runtime must go through `spawn_blocking`.
    fn stop(&mut self) -> Vec<SttEvent> {
        let mut remaining = Vec::new();
        if !self.is_running {
            return remaining;
        }

        if let Some(mut handle) = self.audio_handle.take() {
            handle.stop();
        }

        self.engine.flush();
        while let Some(event) = self.engine.poll() {
            remaining.push(event);
        }

        self.is_running = false;
        tracing::info!("Transcription stopped, {} remaining events", remaining.len());
        remaining
    }

    fn process_audio(&mut self, samples: Vec<f32>) {
        self.engine.push_audio(&samples);
        while let Some(event) = self.engine.poll() {
            let _ = self.event_tx.send(event);
        }
    }
}

/// Global application state
pub struct AppState {
    config: Arc<RwLock<AppConfig>>,
    pipeline: Arc<Mutex<Option<TranscriptionPipeline>>>,
    transcription: Arc<RwLock<TranscriptionState>>,
    /// Guard against double calls to stop_and_paste
    stopping: Arc<AtomicBool>,
    /// Mic preview handle for settings UI
    mic_preview: Arc<Mutex<Option<AudioHandle>>>,
    /// Last STT error of the current dictation (cleared on each start)
    last_error: Arc<Mutex<Option<String>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(AppConfig::default())),
            pipeline: Arc::new(Mutex::new(None)),
            transcription: Arc::new(RwLock::new(TranscriptionState::default())),
            stopping: Arc::new(AtomicBool::new(false)),
            mic_preview: Arc::new(Mutex::new(None)),
            last_error: Arc::new(Mutex::new(None)),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Hide overlay and refocus the previous app
fn hide_overlay_and_refocus(app: &AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.hide();
    }

    // On macOS, hide the Tauri app to refocus the previous app
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg("tell application \"System Events\" to set frontmost of process \"dictea\" to false")
            .output();
    }

    // On Windows, minimizing the main window gives focus back to the previous app
    #[cfg(target_os = "windows")]
    {
        if let Some(main_win) = app.get_webview_window("main") {
            let _ = main_win.minimize();
        }
    }

    // On Linux, xdotool can refocus the previous window
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdotool")
            .args(["getactivewindow", "windowfocus"])
            .output();
    }
}

/// List available audio input devices
#[tauri::command]
fn list_audio_devices() -> Vec<String> {
    AudioHandle::list_devices()
}

/// Stop mic preview (internal helper)
async fn stop_mic_preview_internal(state: &AppState) {
    let mut preview = state.mic_preview.lock().await;
    if let Some(mut handle) = preview.take() {
        handle.stop();
    }
}

/// Start mic level preview for settings UI
#[tauri::command]
async fn start_mic_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    device_name: String,
) -> Result<(), String> {
    stop_mic_preview_internal(&state).await;

    let audio_config = AudioConfig {
        target_sample_rate: 16000,
        device_name: if device_name.is_empty() { None } else { Some(device_name) },
    };

    let app_handle = app.clone();
    let last_send = std::sync::Arc::new(std::sync::Mutex::new(std::time::Instant::now()));
    let audio_handle = AudioHandle::start(audio_config, move |samples| {
        let mut last = last_send.lock().unwrap();
        if last.elapsed().as_millis() < 50 {
            return;
        }
        *last = std::time::Instant::now();
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        let level = (rms * 50.0).min(1.0);
        let _ = app_handle.emit("mic_preview_level", level);
    })
    .map_err(|e| e.to_string())?;

    let mut preview = state.mic_preview.lock().await;
    *preview = Some(audio_handle);
    Ok(())
}

/// Stop mic level preview
#[tauri::command]
async fn stop_mic_preview(state: State<'_, AppState>) -> Result<(), String> {
    stop_mic_preview_internal(&state).await;
    Ok(())
}

/// Get configuration
#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let config = state.config.read().await;
    Ok(config.clone())
}

/// Update configuration
#[tauri::command]
async fn set_config(app: AppHandle, state: State<'_, AppState>, config: AppConfig) -> Result<(), String> {
    // Save to disk
    config.save(&app);

    // Only the settings that feed create_engine warrant dropping the engine.
    // The UI saves the whole config on mount, and rebuilding blindly made the
    // local model load twice at every startup.
    let engine_changed = {
        let current = state.config.read().await;
        current.stt_engine != config.stt_engine
            || current.openai_api_key != config.openai_api_key
            || current.mistral_api_key != config.mistral_api_key
            || current.gemini_api_key != config.gemini_api_key
            || current.groq_api_key != config.groq_api_key
    };

    {
        let mut current = state.config.write().await;
        *current = config;
    }

    if !engine_changed {
        return Ok(());
    }

    // Reset pipeline to use the new engine/model. The events of a flush would
    // go nowhere since the pipeline is dropped right after, so cancel instead.
    let mut pipeline = state.pipeline.lock().await;
    if let Some(ref mut p) = *pipeline {
        p.cancel();
    }
    *pipeline = None;
    drop(pipeline);

    // Start loading the local model right away if it was just selected
    warm_local_engine(&app);

    Ok(())
}

/// Create the STT engine based on config
fn create_engine(config: &AppConfig, app: &AppHandle) -> Result<Box<dyn SttEngine>, String> {
    match config.stt_engine.as_str() {
        "parakeet" => {
            let model_dir = models::parakeet_model_dir(app);
            if !stt::parakeet::is_model_downloaded(&model_dir) {
                return Err("Parakeet model not downloaded. Configure it in Settings > Engine.".to_string());
            }
            let engine = ParakeetEngine::load(&model_dir.to_string_lossy())
                .map_err(|e| format!("Parakeet error: {}", e))?;
            tracing::info!("Parakeet local STT engine initialized");
            Ok(Box::new(engine))
        }
        "gemini" => {
            if config.gemini_api_key.is_empty() {
                return Err("Gemini API key required".to_string());
            }
            let engine = GeminiEngine::load(&config.gemini_api_key)
                .map_err(|e| format!("Gemini error: {}", e))?;
            tracing::info!("Gemini STT engine initialized");
            Ok(Box::new(engine))
        }
        "voxtral" => {
            if config.mistral_api_key.is_empty() {
                return Err("Mistral API key required".to_string());
            }
            let engine = VoxtralEngine::load(&config.mistral_api_key)
                .map_err(|e| format!("Voxtral error: {}", e))?;
            tracing::info!("Voxtral STT engine initialized");
            Ok(Box::new(engine))
        }
        "groq" => {
            if config.groq_api_key.is_empty() {
                return Err("Groq API key required".to_string());
            }
            let engine = GroqEngine::load(&config.groq_api_key)
                .map_err(|e| format!("Groq error: {}", e))?;
            tracing::info!("Groq Whisper API engine initialized");
            Ok(Box::new(engine))
        }
        _ => {
            if config.openai_api_key.is_empty() {
                return Err("OpenAI API key required".to_string());
            }
            let engine = OpenAiEngine::load(&config.openai_api_key)
                .map_err(|e| format!("OpenAI error: {}", e))?;
            tracing::info!("OpenAI Whisper API engine initialized");
            Ok(Box::new(engine))
        }
    }
}

/// Pre-create the local engine in the background so the first dictation doesn't
/// pay for the ONNX model load (a few seconds). No-op for the API engines,
/// which have nothing to load.
pub(crate) fn warm_local_engine(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let config = state.config.read().await.clone();
        if config.stt_engine != "parakeet" {
            return;
        }

        let mut pipeline = state.pipeline.lock().await;
        if pipeline.is_some() {
            return;
        }
        match create_engine(&config, &app) {
            // ParakeetEngine::load returns immediately, the ONNX session
            // finishes loading on its own thread.
            Ok(engine) => {
                *pipeline = Some(TranscriptionPipeline::new(engine));
                tracing::info!("Parakeet engine pre-loading in background");
            }
            Err(e) => tracing::warn!("Parakeet pre-load skipped: {}", e),
        }
    });
}

/// Drop the current engine, e.g. after the local model has been deleted
pub(crate) fn reset_engine(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let mut pipeline = state.pipeline.lock().await;
        if let Some(ref mut p) = *pipeline {
            p.cancel();
        }
        *pipeline = None;
    });
}

/// Build the instructions sent to whichever model does the rewriting.
///
/// The "SAME LANGUAGE" clause is not decoration: the Apple on-device model
/// answers in English by default and silently translated a French dictation
/// without it. Cloud models infer the intent, this one does not.
fn build_system_prompt(
    reformulate: bool,
    needs_translation: bool,
    lang_name: &str,
    on_device: bool,
) -> String {
    let prompt = match (reformulate, needs_translation) {
        (true, true) => format!(
            "Reformulate the following spoken text into clean written text, then translate it to {}. \
            Fix grammar, punctuation, remove hesitations, repetitions and filler words. \
            Keep the meaning and tone. Output ONLY the final translated result in {}. \
            Do NOT include any preamble, explanation, label or prefix. \
            Do NOT write \"Here's the translation\" or similar. Just the text.",
            lang_name, lang_name
        ),
        (true, false) => "Reformulate the following spoken text into clean written text. \
            Fix grammar, punctuation, remove hesitations, repetitions and filler words. \
            Keep the meaning and tone. Preserve English words used intentionally \
            (franglais, technical terms, dev/tech jargon like push, pull, merge, deploy, commit, build, etc.). \
            Do not translate them. Output ONLY the reformulated text. \
            Do NOT include any preamble, explanation or prefix.".to_string(),
        (false, true) => format!(
            "Translate the following text to {}. Output ONLY the translated text. \
            Do NOT include any preamble, explanation, label or prefix like \"Here's the translation\". Just the text.",
            lang_name
        ),
        (false, false) => String::new(),
    };

    if !on_device || prompt.is_empty() {
        return prompt;
    }

    // Measured against the on-device model, not guessed. Two failure modes it
    // has and the cloud models do not:
    //   - it answers in English unless told otherwise, and the instruction only
    //     holds when it comes LAST and names a concrete case;
    //   - it summarises instead of cleaning up, dropping half a dictation.
    // Left out of the cloud prompts on purpose: no evidence they need it.
    let reinforcement = if needs_translation {
        format!(
            "Do NOT summarise, shorten or omit anything: every idea present in the input \
             must still be present in the output. \
             CRITICAL: your answer MUST be written in {}.",
            lang_name
        )
    } else {
        "Do NOT summarise, shorten or omit anything: every idea present in the input must \
         still be present in the output. You are cleaning up the wording, not rewriting the content. \
         CRITICAL: your answer MUST be written in the same language as the input. If the input \
         is French, answer in French. Never translate the text into another language."
            .to_string()
    };

    format!("{} {}", prompt, reinforcement)
}

/// Which provider actually rewrites the text.
///
/// "auto" reproduces the behaviour from before this was configurable: the STT
/// engine also rewrites its own output. Parakeet cannot rewrite anything, so
/// it keeps the OpenAI default it always had.
fn resolve_reformulation_provider(config: &AppConfig) -> &str {
    match config.reformulation_provider.as_str() {
        "auto" if config.stt_engine == "parakeet" => "openai",
        "auto" => config.stt_engine.as_str(),
        explicit => explicit,
    }
}

/// Best guess at the language of a text, whatever the confidence.
///
/// `is_reliable()` is deliberately ignored: it rejects correct calls on short
/// text ("Je pense qu'il faudrait déployer avant vendredi." comes back French
/// at 34% and unreliable), and dictations are short. The guesses are only ever
/// compared with one another, never trusted as absolute labels — on four words
/// the detector happily calls English "Romanian", but it calls it that
/// consistently, which is all the comparison needs.
fn guess_language(text: &str) -> Option<whatlang::Lang> {
    if text.split_whitespace().count() < 2 {
        return None;
    }
    whatlang::detect_lang(&strip_accents(text))
}

/// Fold accented Latin letters onto their base letter.
///
/// Reformulating restores the accents the transcription missed, and on a short
/// phrase that alone flips the verdict: "Le build est casse" reads as Catalan,
/// "Le build est cassé." as French. Comparing both sides after folding leaves
/// only the real difference — the wording — which is what the check is about.
fn strip_accents(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'A',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'È' | 'É' | 'Ê' | 'Ë' => 'E',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'Ò' | 'Ó' | 'Ô' | 'Ö' | 'Õ' => 'O',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
            'ç' => 'c',
            'Ç' => 'C',
            'ñ' => 'n',
            'Ñ' => 'N',
            other => other,
        })
        .collect()
}

/// Whether two texts read as the same language.
///
/// Undetectable on either side means no opinion, and no opinion accepts.
fn same_language(a: &str, b: &str) -> bool {
    match (guess_language(a), guess_language(b)) {
        (Some(x), Some(y)) => x == y,
        _ => true,
    }
}

/// What the answer has to satisfy for the model to have done its job
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LanguageCheck {
    /// Reformulating must not change the language
    SameAsInput,
    /// Translating must change it
    DifferentFromInput,
}

impl LanguageCheck {
    fn accepts(&self, input: &str, answer: &str) -> bool {
        match self {
            LanguageCheck::SameAsInput => same_language(input, answer),
            LanguageCheck::DifferentFromInput => !same_language(input, answer),
        }
    }
}

/// Report whether the Apple on-device model can be used/// Report whether the Apple on-device model can be used
#[tauri::command]
fn apple_intelligence_status() -> serde_json::Value {
    let availability = apple_intelligence::availability();
    serde_json::json!({
        "availability": availability,
        "message": availability.message(),
    })
}

/// How many times to ask before giving up and keeping the transcription.
/// The model is stochastic: a rejected answer is often right on the next try.
const ATTEMPTS: u32 = 3;

/// Run the reformulation on Apple's on-device model.
///
/// This never falls back to a cloud provider. Choosing the on-device model is a
/// statement about where the text is allowed to go, and quietly shipping it to
/// OpenAI because the local one was unavailable would break exactly that. When
/// it cannot run, the raw transcription is pasted and the reason is shown.
async fn process_on_device(
    app: &AppHandle,
    system_prompt: &str,
    text: &str,
    mode_label: &str,
    check: LanguageCheck,
    language_name: Option<&str>,
) -> String {
    let availability = apple_intelligence::availability();
    if availability != apple_intelligence::Availability::Available {
        tracing::warn!("Apple Intelligence unavailable: {:?}", availability);
        let _ = app.emit("config_error", availability.message().to_string());
        return text.to_string();
    }

    // Naming the language beats asking for "the same language as the input",
    // and the answer is checked against it below. Two attempts: the model is
    // stochastic, so a rejected answer is worth asking again before giving up.
    let instructions = match language_name {
        Some(language) => format!(
            "{} The answer MUST be written in {}. Output nothing that is not {}.",
            system_prompt, language, language
        ),
        None => system_prompt.to_string(),
    };

    for attempt in 1..=ATTEMPTS {
        let instructions = instructions.clone();
        let prompt = text.to_string();
        // The FFI call parks its thread until the model answers
        let result = tokio::task::spawn_blocking(move || {
            apple_intelligence::respond(&instructions, &prompt)
        })
        .await;

        let answer = match result {
            Ok(Ok(answer)) => answer.trim().to_string(),
            Ok(Err(e)) => {
                tracing::error!("Apple Intelligence error: {}", e);
                let _ = app.emit("config_error", format!("Apple Intelligence: {}", e));
                return text.to_string();
            }
            Err(e) => {
                tracing::error!("Apple Intelligence task panicked: {}", e);
                return text.to_string();
            }
        };

        if answer.is_empty() {
            tracing::error!("Apple Intelligence returned an empty answer");
            return text.to_string();
        }

        // A 3B model can wander off and start commenting on the text instead of
        // rewriting it. Rewriting stays close to the original length, so treat
        // a blow-up as a failure and keep the dictation.
        if answer.chars().count() > text.chars().count() * 3 + 200 {
            tracing::warn!(
                "Apple Intelligence answer looks runaway ({} chars for {} in), keeping the transcription",
                answer.chars().count(),
                text.chars().count()
            );
            return text.to_string();
        }

        // The reason this check exists: asked to clean up a French dictation,
        // the model answered in English 22 times out of 24. Pasting the right
        // words in the wrong language is worse than pasting the raw dictation.
        if !check.accepts(text, &answer) {
            tracing::warn!(
                "Apple Intelligence answer rejected on attempt {} ({:?}): {:?}",
                attempt,
                check,
                answer
            );
            continue;
        }

        tracing::info!(
            "Processed on device ({}): '{}' -> '{}'",
            mode_label,
            text,
            answer
        );
        return answer;
    }

    tracing::warn!("Apple Intelligence kept answering in the wrong language, keeping the transcription");
    text.to_string()
}

/// Process text: reformulate and/or translate in a single call, either on the
/// Apple on-device model or through a chat API
async fn process_text(
    app: &AppHandle,
    text: &str,
    reformulate: bool,
    output_language: &str,
    config: &AppConfig,
) -> String {
    let provider = resolve_reformulation_provider(config);

    let on_device = provider == "apple";

    let (api_url, model, api_key) = match provider {
        // Nothing to configure: the model lives in the OS
        "apple" => ("", "apple-on-device", "on-device"),
        "gemini" => (
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
            "gemini-2.5-flash-lite",
            config.gemini_api_key.as_str(),
        ),
        "voxtral" => (
            "https://api.mistral.ai/v1/chat/completions",
            "mistral-small-latest",
            config.mistral_api_key.as_str(),
        ),
        "groq" => (
            "https://api.groq.com/openai/v1/chat/completions",
            "llama-3.3-70b-versatile",
            config.groq_api_key.as_str(),
        ),
        // Text passes through unchanged if no key is set for the provider
        _ => (
            "https://api.openai.com/v1/chat/completions",
            "gpt-4o-mini",
            config.openai_api_key.as_str(),
        ),
    };

    if text.is_empty() || api_key.is_empty() {
        return text.to_string();
    }

    let needs_translation = output_language != "auto";

    if !reformulate && !needs_translation {
        return text.to_string();
    }

    let lang_name = match output_language {
        "fr" => "French",
        "en" => "English",
        "es" => "Spanish",
        "de" => "German",
        "it" => "Italian",
        "pt" => "Portuguese",
        other => other,
    };

    let system_prompt = build_system_prompt(reformulate, needs_translation, lang_name, on_device);

    let mode_label = match (reformulate, needs_translation) {
        (true, true) => "reformulate+translate",
        (true, false) => "reformulate",
        (false, true) => "translate",
        _ => unreachable!(),
    };

    tracing::info!("Processing text ({}, model: {}): '{}'", mode_label, model, text);

    if on_device {
        // Reformulating must not change the language; translating must
        let (check, language_name) = if needs_translation {
            (LanguageCheck::DifferentFromInput, Some(lang_name))
        } else {
            (LanguageCheck::SameAsInput, None)
        };
        return process_on_device(app, &system_prompt, text, mode_label, check, language_name).await;
    }

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": text}
        ],
        "temperature": 0.3
    });

    match client
        .post(api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
    {
        Ok(response) => {
            if let Ok(json) = response.json::<serde_json::Value>().await {
                if let Some(result) = json["choices"][0]["message"]["content"].as_str() {
                    let result = result.trim().to_string();
                    tracing::info!("Processed ({}): '{}' -> '{}'", mode_label, text, result);
                    return result;
                }
            }
            tracing::error!("Error parsing chat response ({})", mode_label);
            text.to_string()
        }
        Err(e) => {
            tracing::error!("Chat {} error: {}", mode_label, e);
            text.to_string()
        }
    }
}

/// The automatic paste failed. The text is safe in the clipboard, but nothing
/// appeared where the user was typing and nothing tells them why.
///
/// On macOS this is nearly always TCC: the app is ad-hoc signed, so its code
/// identity changes with every build, and an update invalidates the
/// Accessibility grant. The microphone re-prompts, Accessibility never does —
/// it just silently denies. So bring the window up and explain.
fn report_paste_failure(app: &AppHandle) {
    tracing::warn!("Automatic paste failed, text left in clipboard");
    let _ = app.emit("paste_failed", ());
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
}

/// Surface a crash from the webview, which has no console anyone can read
#[tauri::command]
fn log_frontend_error(message: String) {
    tracing::error!("Frontend error: {}", message);
}

/// Open the OS pane where the paste permission is granted
#[tauri::command]
fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

/// Record an STT error and surface it to the user (toast + overlay)
async fn report_stt_error(
    app: &AppHandle,
    last_error: &Arc<Mutex<Option<String>>>,
    message: String,
) {
    tracing::error!("STT error: {}", message);
    {
        let mut guard = last_error.lock().await;
        *guard = Some(message.clone());
    }
    let _ = app.emit("stt_error", message);
}

/// Start recording
#[tauri::command]
async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    language: Option<String>,
) -> Result<(), String> {
    let lang = language
        .map(|l| Language::from_code(&l))
        .unwrap_or(Language::Auto);

    let config = state.config.read().await.clone();

    // Stop mic preview to avoid concurrent streams
    stop_mic_preview_internal(&state).await;

    // Create pipeline if needed
    {
        let mut pipeline_guard = state.pipeline.lock().await;
        if pipeline_guard.is_none() {
            let engine = create_engine(&config, &app)?;
            *pipeline_guard = Some(TranscriptionPipeline::new(engine));
        }
    }

    // Reset transcription state
    {
        let mut trans = state.transcription.write().await;
        trans.is_recording = true;
        trans.partial_text.clear();
        trans.final_text.clear();
    }
    {
        let mut last_error = state.last_error.lock().await;
        *last_error = None;
    }

    // Start the pipeline
    {
        let mut pipeline_guard = state.pipeline.lock().await;
        if let Some(ref mut pipeline) = *pipeline_guard {
            pipeline.start(lang.clone())?;

            let mut receiver = pipeline.subscribe();
            let app_handle = app.clone();
            let transcription = state.transcription.clone();
            let last_error = state.last_error.clone();

            tokio::spawn(async move {
                while let Ok(event) = receiver.recv().await {
                    match event {
                        SttEvent::Partial(text) => {
                            let mut trans = transcription.write().await;
                            trans.partial_text = text.clone();
                            let _ = app_handle.emit("stt_partial", text);
                        }
                        SttEvent::Final(text) => {
                            let mut trans = transcription.write().await;
                            if !trans.final_text.is_empty() {
                                trans.final_text.push(' ');
                            }
                            trans.final_text.push_str(&text);
                            trans.partial_text.clear();
                            let _ = app_handle.emit("stt_final", text);
                        }
                        SttEvent::Error(message) => {
                            report_stt_error(&app_handle, &last_error, message).await;
                        }
                    }
                }
            });

            let (audio_tx, mut audio_rx) = mpsc::unbounded_channel::<Vec<f32>>();

            let audio_config = AudioConfig {
                target_sample_rate: 16000,
                device_name: if config.audio_device.is_empty() {
                    None
                } else {
                    Some(config.audio_device.clone())
                },
            };
            let audio_handle = AudioHandle::start(audio_config, move |samples| {
                let _ = audio_tx.send(samples);
            })
            .map_err(|e| e.to_string())?;

            pipeline.audio_handle = Some(audio_handle);

            let pipeline_arc = state.pipeline.clone();
            let app_for_level = app.clone();
            tokio::spawn(async move {
                let mut last_level_send = std::time::Instant::now();
                while let Some(samples) = audio_rx.recv().await {
                    // Send audio level to overlay (~20fps)
                    if last_level_send.elapsed().as_millis() > 50 {
                        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
                        let level = (rms * 50.0).min(1.0); // normalize (mic levels are very low)
                        if let Some(overlay) = app_for_level.get_webview_window("overlay") {
                            let _ = overlay.eval(&format!(
                                "window.__overlaySetLevel && window.__overlaySetLevel({})",
                                level
                            ));
                        }
                        last_level_send = std::time::Instant::now();
                    }

                    let mut guard = pipeline_arc.lock().await;
                    if let Some(ref mut p) = *guard {
                        if p.is_running {
                            p.process_audio(samples);
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            });
        }
    }

    // Show overlay on the screen where the mouse cursor is
    if let Some(overlay) = app.get_webview_window("overlay") {
        let monitor = overlay.cursor_position()
            .ok()
            .and_then(|cursor| overlay.monitor_from_point(cursor.x, cursor.y).ok().flatten())
            .or_else(|| overlay.current_monitor().ok().flatten());

        if let Some(monitor) = monitor {
            let scale = monitor.scale_factor();
            let pos = monitor.position();
            let size = monitor.size();
            let screen_x = pos.x as f64 / scale;
            let screen_y = pos.y as f64 / scale;
            let screen_width = size.width as f64 / scale;
            let screen_height = size.height as f64 / scale;
            let window_width = 140.0;
            let x = (screen_x + (screen_width - window_width) / 2.0) as i32;
            let y = (screen_y + screen_height * 0.15) as i32;
            let _ = overlay.set_position(tauri::LogicalPosition::new(x, y));
        }
        let _ = overlay.show();
    }

    let _ = app.emit("recording_started", ());
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.eval("window.__overlaySetProcessing && window.__overlaySetProcessing(false)");
        let _ = overlay.eval("window.__overlaySetError && window.__overlaySetError(false)");
    }
    tracing::info!("Recording started ({})", config.stt_engine);
    Ok(())
}

/// Stop recording (internal, without hiding overlay)
async fn stop_recording_internal(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    // The flush blocks until the transcription completes: keep it off the
    // async runtime so the rest of the app stays responsive.
    let pipeline_arc = state.pipeline.clone();
    let remaining_events = tokio::task::spawn_blocking(move || {
        let mut pipeline_guard = pipeline_arc.blocking_lock();
        match *pipeline_guard {
            Some(ref mut pipeline) => pipeline.stop(),
            None => Vec::new(),
        }
    })
    .await
    .unwrap_or_else(|e| {
        tracing::error!("Flush task panicked: {}", e);
        Vec::new()
    });

    let mut errors = Vec::new();

    let final_text = {
        let mut trans = state.transcription.write().await;
        trans.is_recording = false;

        for event in remaining_events {
            match event {
                SttEvent::Partial(text) => {
                    trans.partial_text = text;
                }
                SttEvent::Final(text) => {
                    if !trans.final_text.is_empty() {
                        trans.final_text.push(' ');
                    }
                    trans.final_text.push_str(&text);
                    trans.partial_text.clear();
                }
                SttEvent::Error(message) => errors.push(message),
            }
        }

        let mut text = trans.final_text.clone();
        if !trans.partial_text.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&trans.partial_text);
        }
        text.trim().to_string()
    };

    for message in errors {
        report_stt_error(&app, &state.last_error, message).await;
    }

    let _ = app.emit("recording_stopped", final_text.clone());
    tracing::info!("Recording stopped, text: {}", final_text);

    Ok(final_text)
}

/// Stop recording and return the text
#[tauri::command]
async fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let text = stop_recording_internal(app.clone(), state).await?;
    hide_overlay_and_refocus(&app);
    Ok(text)
}

/// Stop and paste text into the active application
#[tauri::command]
async fn stop_and_paste(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    // Guard against double call
    if state.stopping.swap(true, Ordering::SeqCst) {
        tracing::warn!("stop_and_paste already in progress, skipped");
        return Ok(());
    }

    let result = do_stop_and_paste(app, state.clone()).await;

    state.stopping.store(false, Ordering::SeqCst);
    result
}

async fn do_stop_and_paste(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    // Switch immediately to processing mode
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.eval("window.__overlaySetProcessing && window.__overlaySetProcessing(true)");
    }

    // Stop recording WITHOUT hiding the overlay
    let text = stop_recording_internal(app.clone(), state.clone()).await?;

    if text.is_empty() {
        let error = state.last_error.lock().await.clone();
        match error {
            // Transcription failed: flash the pill red so the failure isn't silent
            Some(message) => {
                tracing::warn!("Dictation failed without text: {}", message);
                if let Some(overlay) = app.get_webview_window("overlay") {
                    let _ = overlay.eval("window.__overlaySetError && window.__overlaySetError(true)");
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
                if let Some(overlay) = app.get_webview_window("overlay") {
                    let _ = overlay.eval("window.__overlaySetError && window.__overlaySetError(false)");
                }
            }
            None => tracing::info!("No text to paste"),
        }
        hide_overlay_and_refocus(&app);
        return Ok(());
    }

    // Signal to the frontend that we're entering processing mode
    let _ = app.emit("processing_started", ());

    let config = state.config.read().await.clone();

    // Reformulate and/or translate in a single chat API call
    let final_text = process_text(
        &app,
        &text,
        config.reformulate,
        &config.output_language,
        &config,
    ).await;

    // Now hide the overlay
    hide_overlay_and_refocus(&app);

    tracing::info!("Copying text to clipboard: {}", final_text);

    // Copy to clipboard
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => {
            if let Err(e) = clipboard.set_text(&final_text) {
                tracing::error!("Clipboard copy error: {}", e);
                return Err(format!("Copy error: {}", e));
            }
            tracing::info!("Text copied to clipboard");
        }
        Err(e) => {
            tracing::error!("Clipboard creation error: {}", e);
            return Err(format!("Clipboard error: {}", e));
        }
    }

    // Wait for focus to return to the previous app
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    // Simulate Cmd+V to paste
    tracing::info!("Simulating Cmd+V...");

    #[cfg(target_os = "macos")]
    {
        // Use osascript to paste - more reliable than enigo and no Accessibility permissions needed
        let status = std::process::Command::new("osascript")
            .arg("-e")
            .arg("tell application \"System Events\" to keystroke \"v\" using command down")
            .output();

        match status {
            Ok(output) => {
                if output.status.success() {
                    tracing::info!("Cmd+V simulated via osascript");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::error!("osascript error: {}", stderr);
                    report_paste_failure(&app);
                }
            }
            Err(e) => {
                tracing::error!("osascript launch error: {}", e);
                report_paste_failure(&app);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        use enigo::{Enigo, Key, Keyboard, Settings};
        match Enigo::new(&Settings::default()) {
            Ok(mut enigo) => {
                enigo.key(Key::Control, enigo::Direction::Press).ok();
                enigo.key(Key::Unicode('v'), enigo::Direction::Click).ok();
                enigo.key(Key::Control, enigo::Direction::Release).ok();
                tracing::info!("Ctrl+V simulated via enigo");
            }
            Err(e) => {
                tracing::error!("enigo error: {}", e);
                report_paste_failure(&app);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let status = std::process::Command::new("xdotool")
            .args(["key", "ctrl+v"])
            .output();

        match status {
            Ok(output) => {
                if output.status.success() {
                    tracing::info!("Ctrl+V simulated via xdotool");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::error!("xdotool error: {}", stderr);
                    report_paste_failure(&app);
                }
            }
            Err(e) => {
                tracing::error!("xdotool launch error: {}", e);
                report_paste_failure(&app);
            }
        }
    }

    tracing::info!("Text pasted: {}", final_text);
    Ok(())
}

/// Get transcription state
#[tauri::command]
async fn get_transcription_state(
    state: State<'_, AppState>,
) -> Result<TranscriptionState, String> {
    let trans = state.transcription.read().await;
    Ok(trans.clone())
}

/// Toggle overlay (global shortcut)
#[tauri::command]
async fn toggle_overlay(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let is_recording = {
        let trans = state.transcription.read().await;
        trans.is_recording
    };

    if is_recording {
        stop_and_paste(app, state).await
    } else {
        let result = start_recording(app.clone(), state, None).await;
        if let Err(ref e) = result {
            tracing::error!("start_recording failed: {}", e);
            let _ = app.emit("config_error", e.clone());
        }
        result
    }
}

/// Cancel current recording (no paste)
#[tauri::command]
async fn cancel_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let is_recording = {
        let trans = state.transcription.read().await;
        trans.is_recording
    };

    if !is_recording {
        return Ok(());
    }

    // Drop the buffered audio instead of transcribing it: cancelling must be
    // instant, and must not burn an API call for a result nobody wants.
    {
        let mut pipeline_guard = state.pipeline.lock().await;
        if let Some(ref mut pipeline) = *pipeline_guard {
            pipeline.cancel();
        }
    }

    {
        let mut trans = state.transcription.write().await;
        trans.is_recording = false;
        trans.partial_text.clear();
        trans.final_text.clear();
    }

    hide_overlay_and_refocus(&app);
    let _ = app.emit("recording_cancelled", ());
    tracing::info!("Recording cancelled");

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "dictea=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Dictea started");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_config,
            set_config,
            list_audio_devices,
            start_mic_preview,
            stop_mic_preview,
            start_recording,
            stop_recording,
            stop_and_paste,
            get_transcription_state,
            toggle_overlay,
            cancel_recording,
            open_accessibility_settings,
            log_frontend_error,
            apple_intelligence_status,
            models::parakeet_model_status,
            models::download_parakeet_model,
            models::cancel_parakeet_download,
            models::delete_parakeet_model,
        ])
        .setup(|app| {
            use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

            // Load saved config
            let saved_config = AppConfig::load(app.handle());
            // Which provider ends up rewriting the text is worth stating: the
            // setting is indirect ("auto"), and configs written by earlier
            // versions reach it through a serde alias
            tracing::info!(
                "STT engine: {}, reformulation: {} (setting: {})",
                saved_config.stt_engine,
                resolve_reformulation_provider(&saved_config),
                saved_config.reformulation_provider
            );
            let state = app.state::<AppState>();
            let config = state.config.clone();
            tauri::async_runtime::block_on(async {
                let mut c = config.write().await;
                *c = saved_config;
            });

            // Local engine: start loading the model now, not on first dictation
            warm_local_engine(app.handle());

            let app_handle = app.handle().clone();

            let toggle_shortcut: Shortcut = "CmdOrCtrl+Shift+Space"
                .parse()
                .expect("Invalid shortcut");
            let cancel_shortcut: Shortcut = "CmdOrCtrl+Shift+C"
                .parse()
                .expect("Invalid shortcut");

            let toggle_sc = toggle_shortcut.clone();
            let cancel_sc = cancel_shortcut.clone();

            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |_app, shortcut, event| {
                        tracing::trace!("Shortcut event: {:?} ({:?})", shortcut, event.state);
                        if event.state == ShortcutState::Pressed {
                            let handle = app_handle.clone();
                            if shortcut == &toggle_sc {
                                tauri::async_runtime::spawn(async move {
                                    let state = handle.state::<AppState>();
                                    let _ = toggle_overlay(handle.clone(), state).await;
                                });
                            } else if shortcut == &cancel_sc {
                                tauri::async_runtime::spawn(async move {
                                    let state = handle.state::<AppState>();
                                    let _ = cancel_recording(handle.clone(), state).await;
                                });
                            }
                        }
                    })
                    .build(),
            )?;

            app.global_shortcut().register(toggle_shortcut)?;
            app.global_shortcut().register(cancel_shortcut)?;
            tracing::info!("Global shortcuts registered: Cmd+Shift+Space (toggle), Cmd+Shift+C (cancel)");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Error launching application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// Engine that records what the pipeline asks of it
    #[derive(Default)]
    struct FakeEngine {
        flushed: Arc<AtomicUsize>,
        reset: Arc<AtomicUsize>,
        buffered: usize,
    }

    impl SttEngine for FakeEngine {
        fn load(_model_path: &str) -> Result<Self, stt::SttError> {
            Ok(Self::default())
        }
        fn set_language(&mut self, _language: Language) {}
        fn language(&self) -> &Language {
            &Language::Auto
        }
        fn push_audio(&mut self, pcm: &[f32]) {
            self.buffered += pcm.len();
        }
        fn poll(&mut self) -> Option<SttEvent> {
            None
        }
        fn flush(&mut self) {
            // Real engines block here until the transcription completes
            std::thread::sleep(std::time::Duration::from_millis(50));
            self.flushed.fetch_add(1, Ordering::SeqCst);
        }
        fn reset(&mut self) {
            self.buffered = 0;
            self.reset.fetch_add(1, Ordering::SeqCst);
        }
        fn name(&self) -> &str {
            "fake"
        }
        fn is_ready(&self) -> bool {
            true
        }
    }

    /// The on-device model answers in English unless told otherwise: on a
    /// French dictation it drifted 22 times out of 24 with the plain prompt.
    /// The reinforcement cuts that to roughly one in twelve, and the check in
    /// process_on_device turns what remains into a retry.
    ///
    /// Asserted as a rate, not a single pass: the model returns different
    /// answers to identical calls, so a one-shot assertion is a coin flip. The
    /// earlier version of this test was exactly that, and flaked.
    ///
    /// Skipped where Apple Intelligence is off (CI runners, older macOS).
    #[test]
    fn on_device_reformulation_mostly_keeps_the_language() {
        if apple_intelligence::availability() != apple_intelligence::Availability::Available {
            eprintln!("Apple Intelligence unavailable, skipping");
            return;
        }

        let prompt = build_system_prompt(true, false, "", true);
        let dictation = "alors euh je voulais te dire que le le build il est casse \
                         sur linux euh faut qu'on regarde ca demain matin";

        const SAMPLES: usize = 6;
        let drifted = (0..SAMPLES)
            .filter_map(|_| apple_intelligence::respond(&prompt, dictation).ok())
            .filter(|answer| !same_language(dictation, answer))
            .count();

        // Generous on purpose: this guards against the prompt regressing to the
        // 90% drift measured without it, not against the model's variance.
        assert!(
            drifted <= SAMPLES / 2,
            "the language reinforcement stopped working: {}/{} answers drifted",
            drifted,
            SAMPLES
        );
    }

    /// Cancelling must drop the buffered audio, never transcribe it:
    /// a flush here would burn an API call for a result nobody wants.
    #[test]
    fn cancel_resets_the_engine_without_flushing() {
        let flushed = Arc::new(AtomicUsize::new(0));
        let reset = Arc::new(AtomicUsize::new(0));
        let engine = FakeEngine {
            flushed: flushed.clone(),
            reset: reset.clone(),
            buffered: 0,
        };

        let mut pipeline = TranscriptionPipeline::new(Box::new(engine));
        pipeline.start(Language::Auto).unwrap();
        pipeline.process_audio(vec![0.0; 16000]);
        pipeline.cancel();

        assert_eq!(flushed.load(Ordering::SeqCst), 0, "cancel must not transcribe");
        assert_eq!(reset.load(Ordering::SeqCst), 1, "cancel must drop the audio");
        assert!(!pipeline.is_running);
    }

    /// The stop path locks the pipeline from inside spawn_blocking:
    /// tokio::sync::Mutex::blocking_lock panics if it ever runs in an async
    /// context, so pin the pattern used by stop_recording_internal.
    #[tokio::test(flavor = "multi_thread")]
    async fn stop_flushes_from_a_blocking_task() {
        let flushed = Arc::new(AtomicUsize::new(0));
        let engine = FakeEngine {
            flushed: flushed.clone(),
            reset: Arc::new(AtomicUsize::new(0)),
            buffered: 0,
        };

        let mut pipeline = TranscriptionPipeline::new(Box::new(engine));
        pipeline.start(Language::Auto).unwrap();
        let pipeline = Arc::new(Mutex::new(Some(pipeline)));

        let events = tokio::task::spawn_blocking(move || {
            let mut guard = pipeline.blocking_lock();
            match *guard {
                Some(ref mut p) => p.stop(),
                None => Vec::new(),
            }
        })
        .await
        .expect("flush task must not panic");

        assert_eq!(flushed.load(Ordering::SeqCst), 1);
        assert!(events.is_empty());
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    /// A config written by an earlier version stores the choice under the old
    /// key. Without the serde alias every user would silently fall back to the
    /// default and lose it — the kind of regression nobody reports.
    #[test]
    fn an_older_config_keeps_its_reformulation_provider() {
        let older = r#"{
            "global_shortcut": "CmdOrCtrl+Shift+Space",
            "openai_api_key": "",
            "output_language": "auto",
            "stt_engine": "parakeet",
            "parakeet_reformulation_provider": "groq"
        }"#;

        let config: AppConfig = serde_json::from_str(older).expect("older config should parse");
        assert_eq!(config.reformulation_provider, "groq");
        assert_eq!(resolve_reformulation_provider(&config), "groq");
    }

    /// Predates the setting entirely: nothing to migrate, follow the engine.
    #[test]
    fn a_config_without_the_field_follows_the_engine() {
        let bare = r#"{
            "global_shortcut": "CmdOrCtrl+Shift+Space",
            "openai_api_key": "",
            "output_language": "auto",
            "stt_engine": "groq"
        }"#;

        let config: AppConfig = serde_json::from_str(bare).expect("bare config should parse");
        assert_eq!(config.reformulation_provider, "auto");
        assert_eq!(resolve_reformulation_provider(&config), "groq");
    }

    /// Parakeet cannot rewrite its own output, so auto has to name someone
    #[test]
    fn auto_on_the_local_engine_keeps_the_previous_default() {
        let config = AppConfig {
            stt_engine: "parakeet".to_string(),
            ..AppConfig::default()
        };
        assert_eq!(config.reformulation_provider, "auto");
        assert_eq!(resolve_reformulation_provider(&config), "openai");
    }

    /// The combination this whole change exists for: cloud transcription,
    /// on-device rewriting.
    #[test]
    fn an_api_engine_can_rewrite_on_device() {
        let config = AppConfig {
            stt_engine: "openai".to_string(),
            reformulation_provider: "apple".to_string(),
            ..AppConfig::default()
        };
        assert_eq!(resolve_reformulation_provider(&config), "apple");
    }
}

#[cfg(test)]
mod prompt_measurement {
    use super::*;

    /// The comparison is what carries the guarantee, so pin it. Absolute labels
    /// are wrong on short text — the detector calls "Hello, how are you?"
    /// Romanian — but it is wrong consistently, which is enough to tell two
    /// texts apart.
    /// The comparison carries the guarantee, so pin it. Absolute labels are
    /// wrong on short text — the detector calls "Hi, how are you?" Romanian —
    /// but it is wrong consistently, which is all a comparison needs.
    #[test]
    fn comparison_separates_languages() {
        // Accent restoration must not read as a language change
        assert!(same_language("Le build est casse", "Le build est cassé."));
        assert!(same_language("salut euh comment ca va", "Salut, comment ça va ?"));
        assert!(same_language(
            "Le build est casse sur linux",
            "Le build est cassé sur Linux."
        ));

        // Real drift must be caught, including on short phrases
        assert!(!same_language(
            "alors euh le build il est casse sur linux",
            "The build is broken on Linux."
        ));
        assert!(!same_language("Salut, comment ça va ?", "Hi, how are you?"));

        assert!(LanguageCheck::SameAsInput.accepts("Le build est casse", "Le build est cassé."));
        assert!(!LanguageCheck::SameAsInput.accepts("Le build est casse", "The build is broken."));
        assert!(LanguageCheck::DifferentFromInput
            .accepts("Le build est casse", "The build is broken."));
        assert!(!LanguageCheck::DifferentFromInput
            .accepts("Salut, comment ça va ?", "Salut, comment ça va ?"));
    }

    /// End-to-end rate of what the user gets, reported as a ratio because the
    /// model varies between identical calls.
    #[test]
    fn measure_end_to_end() {
        if apple_intelligence::availability() != apple_intelligence::Availability::Available {
            eprintln!("Apple Intelligence unavailable, skipping");
            return;
        }

        let cases: Vec<(&str, String, LanguageCheck, Vec<&str>)> = vec![
            (
                "reformulation FR",
                build_system_prompt(true, false, "", true),
                LanguageCheck::SameAsInput,
                vec![
                    "alors euh je voulais te dire que le build il est casse sur linux faut qu'on regarde ca demain",
                    "bon euh du coup je pense qu'il faudrait qu'on deploy avant vendredi non ?",
                    "salut euh comment ca va ?",
                ],
            ),
            (
                "traduction FR->EN",
                format!(
                    "{} The answer MUST be written in English. Output nothing that is not English.",
                    build_system_prompt(false, true, "English", true)
                ),
                LanguageCheck::DifferentFromInput,
                vec![
                    "Salut, comment ça va ?",
                    "le build est casse sur linux",
                    "merci beaucoup a demain",
                ],
            ),
        ];

        const REPS: usize = 6;
        for (label, prompt, check, inputs) in cases {
            let (mut failed, mut total, mut second_try) = (0, 0, 0);
            for input in inputs {
                for _ in 0..REPS {
                    total += 1;
                    let mut ok = false;
                    for attempt in 1..=ATTEMPTS {
                        if let Ok(out) = apple_intelligence::respond(&prompt, input) {
                            if check.accepts(input, &out) {
                                if attempt > 1 {
                                    second_try += 1;
                                }
                                ok = true;
                                break;
                            }
                        }
                    }
                    if !ok {
                        failed += 1;
                    }
                }
            }
            eprintln!(
                "=== {} : {}/{} replis sur la transcription brute ({} rattrapes au 2e essai) ===",
                label, failed, total, second_try
            );
        }
    }
}
