//! Miniti Linux — native desktop meeting assistant (Tauri 2 + Rust).
//!
//! Module map (PLAN.md): `audio` (capture, PCM, dual-source mixer), `deepgram`
//! (live transcription, speaker identity/segmentation, echo reconciliation),
//! `db` (SQLite), `prefs`, `device_id`, `coaching` (local metrics), `api`
//! (backend client), `insights` (engine, providers, MCP), `webhook`, `gates`,
//! `call_sensor`, `smart` (Smart meetings), `shell` (tray / presence /
//! notifications / deep links), `integrations` (Calendar, CRM), `export`,
//! `import`, and `state` (recording engine + Tauri commands).

use std::sync::{Arc, Mutex};
use tauri::Manager;

pub mod api;
pub mod audio;
pub mod call_sensor;
pub mod coaching;
pub mod db;
pub mod deepgram;
pub mod device_id;
pub mod export;
pub mod gates;
pub mod import;
pub mod integrations;
pub mod insights;
pub mod prefs;
pub mod shell;
pub mod smart;
pub mod state;
pub mod webhook;

use state::{
    accept_terms, catch_up, coaching_overview, coaching_report, complete_onboarding,
    delete_meeting, delete_segment, environment_health, export_markdown, get_device_id,
    get_levels, get_meeting, get_meeting_detail, get_prefs, get_segments, get_usage,
    import_granola_csv, insights_finishing, investigate, launch_gate, list_meetings,
    lookup_doc_topic, mark_as_you, meeting_markdown, pick_folder, portal_url, probe_docs_mcp,
    recording_status, regenerate_insights, restore_license, search_meetings, set_meeting_title,
    set_notes, set_pinned, set_prefs, set_sales_enabled, set_speaker_name, start_recording,
    stop_recording, subscribe_url, trim_transcript, AppState, Levels, RecordingSession,
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
        activity: Arc::new(state::ActivityTrack::default()),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tauri::Builder::default()
        .manage(shell::TraySlot::new(None))
        .manage(smart::MonitorSlot::new(smart::MonitorState::default()))
        .manage(integrations::CalendarSlot::new(integrations::CalendarState::default()))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(build_state())
        .setup(|app| {
            let handle = app.handle().clone();
            let show_tray = handle
                .state::<AppState>()
                .prefs
                .lock()
                .map(|p| p.show_tray)
                .unwrap_or(true);
            shell::setup_tray(&handle, show_tray);
            shell::setup_deep_links(&handle);
            state::spawn_shell_ticker(handle.clone());
            state::spawn_smart_monitor(handle.clone());
            state::spawn_calendar_refresher(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Close-to-tray: Miniti stays reachable after the main window closes.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == shell::MAIN_LABEL {
                    let keep_alive = window
                        .app_handle()
                        .try_state::<shell::TraySlot>()
                        .and_then(|s| s.lock().ok().map(|g| g.is_some()))
                        .unwrap_or(false);
                    if keep_alive {
                        let _ = window.hide();
                        api.prevent_close();
                    }
                }
            }
        })
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
            meeting_markdown,
            export_markdown,
            import_granola_csv,
            delete_segment,
            trim_transcript,
            state::notify,
            state::show_main_window,
            state::smart_decision,
            state::google_status,
            state::google_connect,
            state::google_disconnect,
            state::calendar_events,
            state::get_prep_notes,
            state::set_prep_notes,
            state::start_meeting_from_event,
            state::crm_status,
            state::crm_connect,
            state::crm_search,
            state::crm_preview,
            state::crm_send,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
