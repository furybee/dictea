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

    let result = do_download(&app).await;
    DOWNLOADING.store(false, Ordering::SeqCst);

    match &result {
        Ok(_) => {
            tracing::info!("Parakeet model download complete");
            let _ = app.emit("parakeet_download_done", ());
        }
        Err(e) => {
            tracing::error!("Parakeet model download failed: {}", e);
            let _ = app.emit("parakeet_download_error", e.clone());
        }
    }
    result
}

async fn do_download(app: &AppHandle) -> Result<(), String> {
    let dir = parakeet_model_dir(app);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create model dir: {}", e))?;

    let client = reqwest::Client::new();
    let file_count = DOWNLOADS.len();

    for (i, (remote, local)) in DOWNLOADS.iter().enumerate() {
        let dest = dir.join(local);
        if dest.is_file() {
            tracing::info!("{} already downloaded, skipped", local);
            continue;
        }

        let url = format!("{}/{}", HF_BASE, remote);
        tracing::info!("Downloading {} -> {}", url, dest.display());

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("HTTP {} for {}", response.status(), remote));
        }

        let total = response.content_length().unwrap_or(0);
        let part_path = dir.join(format!("{}.part", local));
        let mut file = tokio::fs::File::create(&part_path)
            .await
            .map_err(|e| format!("Cannot create {}: {}", part_path.display(), e))?;

        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;
        let mut last_emit = std::time::Instant::now();

        while let Some(chunk) = stream.next().await {
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
                        file_index: i + 1,
                        file_count,
                        downloaded,
                        total,
                    },
                );
            }
        }

        file.flush().await.map_err(|e| format!("Flush error: {}", e))?;
        drop(file);

        tokio::fs::rename(&part_path, &dest)
            .await
            .map_err(|e| format!("Rename error: {}", e))?;

        tracing::info!("{} downloaded ({} bytes)", local, downloaded);
    }

    Ok(())
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
    }
    Ok(())
}
