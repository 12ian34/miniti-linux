//! Miniti Linux — native desktop meeting assistant (Tauri 2 + Rust).
//!
//! Module map (PLAN.md): `audio` (capture + PCM), `deepgram` (live transcription),
//! `db` (SQLite), `prefs`, `device_id`, `coaching` (local metrics), `api`
//! (backend client), `insights`, `webhook`, `gates`, `call_sensor`, and `state`
//! (recording engine + Tauri commands).

use std::sync::{Arc, Mutex};

pub mod api;
pub mod audio;
pub mod call_sensor;
pub mod coaching;
pub mod db;
pub mod deepgram;
pub mod device_id;
pub mod gates;
pub mod insights;
pub mod prefs;
pub mod state;
pub mod webhook;

use state::{
    coaching_overview, delete_meeting, environment_health, get_device_id, get_levels, get_meeting,
    get_prefs, get_segments, list_meetings, search_meetings, set_pinned, set_prefs,
    start_recording, stop_recording, AppState, Levels, RecordingSession,
};

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

fn build_state() -> AppState {
    let data_dir = device_id::data_dir();
    let _ = std::fs::create_dir_all(&data_dir);
    let db_path = data_dir.join("miniti.db");
    let conn = db::open(&db_path).expect("failed to open database");
    let prefs = prefs::Prefs::load();
    let device = device_id::get_or_create().unwrap_or_else(|_| "unknown-device".to_string());

    AppState {
        db: Arc::new(Mutex::new(conn)),
        prefs: Mutex::new(prefs),
        device_id: device,
        levels: Arc::new(Levels::default()),
        session: Mutex::new(RecordingSession::default()),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(build_state())
        .invoke_handler(tauri::generate_handler![
            environment_health,
            get_prefs,
            set_prefs,
            get_device_id,
            start_recording,
            stop_recording,
            get_levels,
            list_meetings,
            search_meetings,
            get_meeting,
            get_segments,
            set_pinned,
            delete_meeting,
            coaching_overview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
