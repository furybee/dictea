//! Local model download manager (Parakeet)
//!
//! Downloads the int8 quantized ONNX export of parakeet-tdt-0.6b-v3
//! (~670 MB total) from HuggingFace into the app data directory.

use futures_util::StreamExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

const HF_BASE: &str = "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main";

/// (remote file on HuggingFace, local name expected by parakeet-rs)
/// We download the int8 quantized variants (652 MB vs 2.5 GB for fp32).
const DOWNLOADS: [(&str, &str); 3] = [
    ("vocab.txt", "vocab.txt"),
    ("decoder_joint-model.int8.onnx", "decoder_joint-model.onnx"),
    ("encoder-model.int8.onnx", "encoder-model.onnx"),
];

/// Guard against concurrent downloads
static DOWNLOADING: AtomicBool = AtomicBool::new(false);

/// Set when the user aborts the download in progress
static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

/// How a download run ended
enum Outcome {
    Completed,
    Cancelled,
}

/// Directory where the Parakeet model files are stored
pub fn parakeet_model_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("models")
        .join("parakeet-tdt-0.6b-v3")
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ParakeetModelStatus {
    pub downloaded: bool,
    pub downloading: bool,
    pub path: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DownloadProgress {
    file: String,
    file_index: usize,
    file_count: usize,
    downloaded: u64,
    total: u64,
}

/// Get the Parakeet model status
#[tauri::command]
pub fn parakeet_model_status(app: AppHandle) -> ParakeetModelStatus {
    let dir = parakeet_model_dir(&app);
    ParakeetModelStatus {
        downloaded: crate::stt::parakeet::is_model_downloaded(&dir),
        downloading: DOWNLOADING.load(Ordering::SeqCst),
        path: dir.display().to_string(),
    }
}

/// Download the Parakeet model files (with progress events)
#[tauri::command]
pub async fn download_parakeet_model(app: AppHandle) -> Result<(), String> {
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("Download already in progress".to_string());
    }
    CANCEL_REQUESTED.store(false, Ordering::SeqCst);

    let result = do_download(&app).await;
    DOWNLOADING.store(false, Ordering::SeqCst);
    CANCEL_REQUESTED.store(false, Ordering::SeqCst);

    match &result {
        Ok(Outcome::Completed) => {
            tracing::info!("Parakeet model download complete");
            let _ = app.emit("parakeet_download_done", ());
            // The model is usable right away: start loading it in the background
            crate::warm_local_engine(&app);
        }
        Ok(Outcome::Cancelled) => {
            tracing::info!("Parakeet model download cancelled");
            let _ = app.emit("parakeet_download_cancelled", ());
        }
        Err(e) => {
            tracing::error!("Parakeet model download failed: {}", e);
            let _ = app.emit("parakeet_download_error", e.clone());
        }
    }
    result.map(|_| ())
}

/// Abort the download in progress (the partial file is discarded)
#[tauri::command]
pub fn cancel_parakeet_download() {
    if DOWNLOADING.load(Ordering::SeqCst) {
        CANCEL_REQUESTED.store(true, Ordering::SeqCst);
        tracing::info!("Parakeet download cancellation requested");
    }
}

async fn do_download(app: &AppHandle) -> Result<Outcome, String> {
    let dir = parakeet_model_dir(app);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create model dir: {}", e))?;

    let client = reqwest::Client::new();

    // Only count the files actually missing, so the progress counter matches
    // what the user sees happening (a resumed download no longer starts at 3/3)
    let missing: Vec<&(&str, &str)> = DOWNLOADS
        .iter()
        .filter(|(_, local)| !dir.join(local).is_file())
        .collect();
    let file_count = missing.len();

    for (i, (remote, local)) in missing.iter().enumerate() {
        if CANCEL_REQUESTED.load(Ordering::SeqCst) {
            return Ok(Outcome::Cancelled);
        }

        let dest = dir.join(local);
        let part_path = dir.join(format!("{}.part", local));

        let result = download_file(app, &client, remote, local, &part_path, i, file_count).await;

        match result {
            Ok(Outcome::Completed) => {
                tokio::fs::rename(&part_path, &dest)
                    .await
                    .map_err(|e| format!("Rename error: {}", e))?;
                tracing::info!("{} downloaded", local);
            }
            // Cancelled or failed: never leave a half-written file behind
            other => {
                let _ = tokio::fs::remove_file(&part_path).await;
                return other;
            }
        }
    }

    Ok(Outcome::Completed)
}

/// Stream one file to `part_path`, emitting throttled progress events
async fn download_file(
    app: &AppHandle,
    client: &reqwest::Client,
    remote: &str,
    local: &str,
    part_path: &std::path::Path,
    index: usize,
    file_count: usize,
) -> Result<Outcome, String> {
    let url = format!("{}/{}", HF_BASE, remote);
    tracing::info!("Downloading {} -> {}", url, part_path.display());

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {} for {}", response.status(), remote));
    }

    let total = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(part_path)
        .await
        .map_err(|e| format!("Cannot create {}: {}", part_path.display(), e))?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        if CANCEL_REQUESTED.load(Ordering::SeqCst) {
            return Ok(Outcome::Cancelled);
        }

        let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write error: {}", e))?;
        downloaded += chunk.len() as u64;

        // Throttle progress events (~5/s)
        if last_emit.elapsed().as_millis() > 200 {
            last_emit = std::time::Instant::now();
            let _ = app.emit(
                "parakeet_download_progress",
                DownloadProgress {
                    file: local.to_string(),
                    file_index: index + 1,
                    file_count,
                    downloaded,
                    total,
                },
            );
        }
    }

    file.flush().await.map_err(|e| format!("Flush error: {}", e))?;
    drop(file);

    // A truncated body still comes back as HTTP 200: refuse to install a file
    // that would only fail later, deep inside the ONNX loader
    if total > 0 && downloaded != total {
        return Err(format!(
            "Incomplete download for {} ({} of {} bytes)",
            local, downloaded, total
        ));
    }

    Ok(Outcome::Completed)
}

/// Delete the downloaded Parakeet model
#[tauri::command]
pub fn delete_parakeet_model(app: AppHandle) -> Result<(), String> {
    if DOWNLOADING.load(Ordering::SeqCst) {
        return Err("Download in progress".to_string());
    }
    let dir = parakeet_model_dir(&app);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("Delete error: {}", e))?;
        tracing::info!("Parakeet model deleted ({})", dir.display());
        // Drop the engine still holding the deleted model
        crate::reset_engine(&app);
    }
    Ok(())
}
