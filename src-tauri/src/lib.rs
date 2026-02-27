//! Dictea - STT application with floating overlay
//!
//! Global shortcut to activate voice dictation.

mod audio;
mod pipeline;
mod stt;

use audio::{AudioConfig, AudioHandle};
use stt::{Language, GeminiEngine, OpenAiEngine, VoxtralEngine, SttEngine, SttEvent};
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
    /// STT engine: "openai", "voxtral", or "gemini"
    #[serde(default = "default_stt_engine")]
    pub stt_engine: String,
    /// Mistral API key (used when stt_engine == "voxtral")
    #[serde(default)]
    pub mistral_api_key: String,
    /// Gemini API key (used when stt_engine == "gemini")
    #[serde(default)]
    pub gemini_api_key: String,
}

fn default_stt_engine() -> String {
    "openai".to_string()
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

    /// Stop the pipeline and return remaining events
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
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(AppConfig::default())),
            pipeline: Arc::new(Mutex::new(None)),
            transcription: Arc::new(RwLock::new(TranscriptionState::default())),
            stopping: Arc::new(AtomicBool::new(false)),
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

    let mut current = state.config.write().await;
    *current = config;

    // Reset pipeline to use the new engine/model
    let mut pipeline = state.pipeline.lock().await;
    if let Some(ref mut p) = *pipeline {
        p.stop();
    }
    *pipeline = None;

    Ok(())
}

/// Create the STT engine based on config
fn create_engine(config: &AppConfig) -> Result<Box<dyn SttEngine>, String> {
    match config.stt_engine.as_str() {
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

/// Process text via chat API: reformulate and/or translate in a single call
async fn process_text(text: &str, reformulate: bool, output_language: &str, config: &AppConfig) -> String {
    // Determine API endpoint, model, and key based on engine
    let (api_url, model, api_key) = match config.stt_engine.as_str() {
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

    let system_prompt = match (reformulate, needs_translation) {
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
        _ => unreachable!(),
    };

    let mode_label = match (reformulate, needs_translation) {
        (true, true) => "reformulate+translate",
        (true, false) => "reformulate",
        (false, true) => "translate",
        _ => unreachable!(),
    };

    tracing::info!("Processing text ({}, model: {}): '{}'", mode_label, model, text);

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

    // Create pipeline if needed
    {
        let mut pipeline_guard = state.pipeline.lock().await;
        if pipeline_guard.is_none() {
            let engine = create_engine(&config)?;
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

    // Start the pipeline
    {
        let mut pipeline_guard = state.pipeline.lock().await;
        if let Some(ref mut pipeline) = *pipeline_guard {
            pipeline.start(lang.clone())?;

            let mut receiver = pipeline.subscribe();
            let app_handle = app.clone();
            let transcription = state.transcription.clone();

            tokio::spawn(async move {
                while let Ok(event) = receiver.recv().await {
                    let mut trans = transcription.write().await;
                    match event {
                        SttEvent::Partial(text) => {
                            trans.partial_text = text.clone();
                            let _ = app_handle.emit("stt_partial", text);
                        }
                        SttEvent::Final(text) => {
                            if !trans.final_text.is_empty() {
                                trans.final_text.push(' ');
                            }
                            trans.final_text.push_str(&text);
                            trans.partial_text.clear();
                            let _ = app_handle.emit("stt_final", text);
                        }
                    }
                }
            });

            let (audio_tx, mut audio_rx) = mpsc::unbounded_channel::<Vec<f32>>();

            let audio_handle = AudioHandle::start(AudioConfig::default(), move |samples| {
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
    }
    tracing::info!("Recording started (OpenAI)");
    Ok(())
}

/// Stop recording (internal, without hiding overlay)
async fn stop_recording_internal(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let remaining_events = {
        let mut pipeline_guard = state.pipeline.lock().await;
        if let Some(ref mut pipeline) = *pipeline_guard {
            pipeline.stop()
        } else {
            Vec::new()
        }
    };

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
        tracing::info!("No text to paste");
        hide_overlay_and_refocus(&app);
        return Ok(());
    }

    // Signal to the frontend that we're entering processing mode
    let _ = app.emit("processing_started", ());

    let config = state.config.read().await.clone();

    // Reformulate and/or translate in a single chat API call
    let final_text = process_text(
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
                    tracing::info!("Text is in clipboard, paste with Cmd+V");
                }
            }
            Err(e) => {
                tracing::error!("osascript launch error: {}", e);
                tracing::info!("Text is in clipboard, paste with Cmd+V");
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
                tracing::info!("Text is in clipboard, paste with Ctrl+V");
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
                    tracing::info!("Text is in clipboard, paste with Ctrl+V");
                }
            }
            Err(e) => {
                tracing::error!("xdotool launch error: {}", e);
                tracing::info!("Text is in clipboard, paste with Ctrl+V");
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

    // Stop pipeline without processing text
    {
        let mut pipeline_guard = state.pipeline.lock().await;
        if let Some(ref mut pipeline) = *pipeline_guard {
            pipeline.stop();
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
            start_recording,
            stop_recording,
            stop_and_paste,
            get_transcription_state,
            toggle_overlay,
            cancel_recording,
        ])
        .setup(|app| {
            use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

            // Load saved config
            let saved_config = AppConfig::load(app.handle());
            let state = app.state::<AppState>();
            let config = state.config.clone();
            tauri::async_runtime::block_on(async {
                let mut c = config.write().await;
                *c = saved_config;
            });

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
