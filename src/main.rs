//! Dictea - Application STT locale temps réel
//!
//! Application desktop cross-platform pour la transcription vocale
//! 100% locale, sans envoi réseau.

mod audio;
mod ipc;
mod pipeline;
mod stt;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    // Initialiser le logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "dictea=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Dictea v{}", env!("CARGO_PKG_VERSION"));

    // Lister les périphériques audio disponibles
    let devices = audio::AudioStream::list_devices();
    tracing::info!("Périphériques audio détectés: {:?}", devices);

    // TODO: Intégrer Tauri pour l'UI
    // Pour l'instant, on affiche un message de démarrage
    println!("🎤 Dictea - STT local temps réel");
    println!("   Version: {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Périphériques audio:");
    for device in &devices {
        println!("  - {}", device);
    }
    println!();
    println!("En attente de l'intégration Tauri...");
}
