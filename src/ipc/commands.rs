//! Commandes Tauri pour l'IPC
//!
//! Ces commandes seront exposées à l'UI via Tauri.

use crate::pipeline::{PipelineStatus, RealtimePipeline};
use crate::stt::{Language, VoxtralEngine};
use std::sync::Arc;
use tokio::sync::Mutex;

/// État global de l'application (partagé avec Tauri)
pub struct AppState {
    pub pipeline: Arc<Mutex<Option<RealtimePipeline<VoxtralEngine>>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            pipeline: Arc::new(Mutex::new(None)),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Réponse de statut pour l'UI
#[derive(Debug, Clone)]
pub struct StatusResponse {
    pub status: String,
    pub language: String,
    pub is_listening: bool,
}

/// Démarre l'écoute et la transcription
///
/// # Arguments
/// * `language` - Code langue optionnel (ex: "fr", "en", "auto")
///
/// # Tauri Command
/// ```ignore
/// #[tauri::command]
/// async fn start_listening(
///     state: State<'_, AppState>,
///     language: Option<String>
/// ) -> Result<(), String>
/// ```
pub async fn start_listening(
    state: &AppState,
    language: Option<String>,
) -> Result<(), String> {
    let lang = language
        .map(|l| Language::from_code(&l))
        .unwrap_or(Language::Auto);

    let mut pipeline_guard = state.pipeline.lock().await;

    // Créer le pipeline s'il n'existe pas
    if pipeline_guard.is_none() {
        let engine = VoxtralEngine::new();
        let config = crate::pipeline::PipelineConfig {
            language: lang.clone(),
            ..Default::default()
        };
        *pipeline_guard = Some(RealtimePipeline::new(engine, config));
    }

    // Démarrer le pipeline
    if let Some(ref mut pipeline) = *pipeline_guard {
        pipeline.set_language(lang).await;
        pipeline.start().await.map_err(|e| e.to_string())?;
    }

    tracing::info!("Écoute démarrée");
    Ok(())
}

/// Arrête l'écoute et la transcription
///
/// # Tauri Command
/// ```ignore
/// #[tauri::command]
/// async fn stop_listening(state: State<'_, AppState>) -> Result<(), String>
/// ```
pub async fn stop_listening(state: &AppState) -> Result<(), String> {
    let mut pipeline_guard = state.pipeline.lock().await;

    if let Some(ref mut pipeline) = *pipeline_guard {
        pipeline.stop().await.map_err(|e| e.to_string())?;
    }

    tracing::info!("Écoute arrêtée");
    Ok(())
}

/// Retourne le statut actuel de l'application
///
/// # Tauri Command
/// ```ignore
/// #[tauri::command]
/// async fn get_status(state: State<'_, AppState>) -> Result<StatusResponse, String>
/// ```
pub async fn get_status(state: &AppState) -> Result<StatusResponse, String> {
    let pipeline_guard = state.pipeline.lock().await;

    let (status, language, is_listening) = if let Some(ref pipeline) = *pipeline_guard {
        let status = pipeline.status().await;
        let is_listening = status == PipelineStatus::Running;
        let status_str = match status {
            PipelineStatus::Stopped => "stopped",
            PipelineStatus::Starting => "starting",
            PipelineStatus::Running => "running",
            PipelineStatus::Stopping => "stopping",
            PipelineStatus::Error(_) => "error",
        };
        (
            status_str.to_string(),
            pipeline.config().language.code().to_string(),
            is_listening,
        )
    } else {
        ("not_initialized".to_string(), "auto".to_string(), false)
    };

    Ok(StatusResponse {
        status,
        language,
        is_listening,
    })
}

// Note: Les événements Tauri seront émis depuis le pipeline:
// - "stt_partial" : transcription partielle
// - "stt_final"   : transcription finale
// - "stt_error"   : erreur de transcription
