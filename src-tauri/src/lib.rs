//! Miniti Linux — native desktop meeting assistant (Tauri 2 + Rust).
//!
//! Module map (PLAN.md): `audio` (capture + PCM), `deepgram` (live transcription
//! + speaker identity/segmentation), `db` (SQLite), `prefs`, `device_id`,
//! `coaching` (local metrics), `api` (backend client), `insights` (request
//! contract), `webhook`, `gates`, `call_sensor` (sensor + policy), and `state`
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
    accept_terms, catch_up, coaching_overview, coaching_report, complete_onboarding,
    delete_meeting, environment_health, get_device_id, get_levels, get_meeting,
    get_meeting_detail, get_prefs, get_segments, get_usage, insights_finishing, investigate,
    launch_gate, list_meetings, lookup_doc_topic, mark_as_you, pick_folder, portal_url,
    probe_docs_mcp, recording_status, regenerate_insights, restore_license, search_meetings,
    set_meeting_title, set_notes, set_pinned, set_prefs, set_sales_enabled, set_speaker_name,
    start_recording, stop_recording, subscribe_url, AppState, Levels, RecordingSession,
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
    let device = device_id::get_or_create().unwrap_or_else(|e| {
        tracing::error!("device id unavailable ({e}); using an ephemeral id");
        uuid::Uuid::new_v4().to_string()
    });
    let api_key = api::embedded_api_key();
    if api_key.is_none() {
        tracing::info!("no backend key in this build: managed mode unavailable, BYOK only");
    }

    AppState {
        db: Arc::new(Mutex::new(conn)),
        prefs: Mutex::new(prefs),
        device_id: device,
        api_key,
        levels: Arc::new(Levels::default()),
        session: Mutex::new(RecordingSession::default()),
        last_status: Arc::new(Mutex::new(None)),
        finishing: Arc::new(Mutex::new(std::collections::HashSet::new())),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_deep_link::init())
        .manage(build_state())
        .invoke_handler(tauri::generate_handler![
            environment_health,
            get_prefs,
            set_prefs,
            get_device_id,
            launch_gate,
            accept_terms,
            complete_onboarding,
            get_usage,
            subscribe_url,
            portal_url,
            restore_license,
            start_recording,
            stop_recording,
            get_levels,
            recording_status,
            list_meetings,
            search_meetings,
            get_meeting,
            get_meeting_detail,
            get_segments,
            set_pinned,
            set_meeting_title,
            set_notes,
            set_speaker_name,
            mark_as_you,
            delete_meeting,
            coaching_overview,
            coaching_report,
            insights_finishing,
            set_sales_enabled,
            regenerate_insights,
            catch_up,
            investigate,
            lookup_doc_topic,
            probe_docs_mcp,
            pick_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
