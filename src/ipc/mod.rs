//! Module IPC pour Tauri
//!
//! Définit les commandes et événements pour la communication UI ↔ Core.

mod commands;

pub use commands::{
    get_status, start_listening, stop_listening,
    AppState, StatusResponse,
};
