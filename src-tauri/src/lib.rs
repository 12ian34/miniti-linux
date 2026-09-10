//! miniti Linux — native desktop meeting assistant (Tauri 2 + Rust).
//!
//! Module map (PLAN.md): `audio` (capture, PCM, dual-source mixer), `deepgram`
//! (live transcription, speaker identity/segmentation, echo reconciliation),
//! `db` (SQLite), `prefs`, `device_id`, `coaching` (local metrics), `api`
//! (backend client), `insights` (engine, providers, MCP), `webhook`, `gates`,
//! `call_sensor`, `smart` (Smart meetings), `shell` (tray / presence /
//! notifications / deep links / autostart), `integrations` (Calendar, CRM),
//! `export`, `import`, `state` (recording engine + Tauri commands), `ipc`
//! (control socket, state file, D-Bus, idle inhibit) and `cli` (the `miniti`
//! command line, same binary).

use std::sync::{Arc, Mutex};
use tauri::Manager;

pub mod api;
pub mod audio;
pub mod auth;
pub mod call_sensor;
pub mod cli;
pub mod coaching;
pub mod corrections;
pub mod db;
pub mod deepgram;
pub mod device_id;
pub mod export;
pub mod gates;
pub mod import;
pub mod insights;
pub mod integrations;
pub mod ipc;
pub mod prefs;
pub mod shell;
pub mod smart;
pub mod state;
pub mod webhook;

use state::{
    accept_terms, auth_create_account, auth_delete_account, auth_devices, auth_recovery_key,
    auth_remove_device, auth_restore_account, auth_rotate_recovery_key, auth_sign_out, auth_start_over,
    auth_status, add_correction, remove_correction, join_and_start_from_event, open_meeting_join_link, mark_all_mic_as_you,
    catch_up, coaching_overview, coaching_report, complete_onboarding, debug_log_clear,
    debug_log_export, debug_log_path, debug_log_reveal, debug_log_tail, delete_meeting,
    delete_segment, disable_nudge_kind, environment_health, export_markdown, frontend_ready,
    get_device_id, get_levels, get_meeting, get_meeting_detail, get_prefs, get_segments, get_usage,
    import_granola_csv, insights_finishing, investigate, launch_gate, list_meetings,
    list_templates, lookup_doc_topic, mark_as_you, meeting_markdown, pick_folder, portal_url,
    probe_docs_mcp, recording_presence, recording_status, regenerate_insights, restore_license,
    search_meetings, set_meeting_title, set_notes, set_pinned, set_prefs, set_sales_enabled,
    set_speaker_name, set_template, start_recording, stop_recording, subscribe_url,
    trim_transcript, AppState, Levels, RecordingSession,
};

/// Keeps the non-blocking log writer alive for the life of the process.
static LOG_GUARD: std::sync::OnceLock<tracing_appender::non_blocking::WorkerGuard> =
    std::sync::OnceLock::new();

/// Directory holding the daily rolling log files (`miniti.log.YYYY-MM-DD`):
/// `$XDG_STATE_HOME/miniti/logs` (`~/.local/state/miniti/logs`), the XDG home
/// for logs and other state that should survive a restart but is not user
/// data. Platforms without a state dir (macOS dev box) keep logs under data.
pub fn log_dir() -> std::path::PathBuf {
    dirs::state_dir()
        .map(|d| d.join("miniti"))
        .unwrap_or_else(device_id::data_dir)
        .join("logs")
}

fn init_tracing() {
    use tracing_subscriber::prelude::*;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let stderr = tracing_subscriber::fmt::layer();
    // Daily rolling file so a user can send the log from Settings → Privacy & Support.
    let dir = log_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file_layer = match tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("miniti.log")
        .max_log_files(7)
        .build(&dir)
    {
        Ok(appender) => {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let _ = LOG_GUARD.set(guard);
            Some(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(writer),
            )
        }
        Err(e) => {
            eprintln!("log file unavailable ({e}); logging to stderr only");
            None
        }
    };
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(stderr)
        .with(file_layer)
        .try_init();
    // A panic inside a command must show up in the log a user can send us, not vanish.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".into());
        tracing::error!(target: "panic", "panic at {location}: {info}");
        default_hook(info);
    }));
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
    let auth = Arc::new(auth::manager::AuthManager::load(
        api::DEFAULT_BASE_URL,
        device.clone(),
        state::APP_VERSION,
    ));
    if !auth.is_enrolled() {
        tracing::info!("device not enrolled: managed mode needs a recovery key (Settings → Account); BYOK works");
    }

    AppState {
        db: Arc::new(Mutex::new(conn)),
        corrector: std::sync::RwLock::new(corrections::Corrector::new(&prefs.dictionary_corrections)),
        prefs: Mutex::new(prefs),
        device_id: device,
        auth,
        levels: Arc::new(Levels::default()),
        session: Mutex::new(RecordingSession::default()),
        last_status: Arc::new(Mutex::new(None)),
        finishing: Arc::new(Mutex::new(std::collections::HashSet::new())),
        activity: Arc::new(state::ActivityTrack::default()),
        audio_health: Arc::new(Mutex::new(state::AudioHealth::default())),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// WebKitGTK's DMA-BUF renderer produces a blank window on many NVIDIA setups
/// (the proprietary driver has no usable DMA-BUF export path). Disable it there
/// unless the user has decided otherwise; other GPUs keep the faster path.
fn apply_webkit_workarounds() {
    #[cfg(target_os = "linux")]
    {
        let nvidia = std::path::Path::new("/proc/driver/nvidia").exists();
        if nvidia && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            tracing::info!("NVIDIA driver detected: WEBKIT_DISABLE_DMABUF_RENDERER=1 (set it to 0 to override)");
        }
    }
}

pub fn run() {
    // Subcommands (`miniti status`, `miniti start`, …) never start the desktop
    // app: they talk to the running one over the control socket and exit.
    let launch = match cli::run() {
        Ok(launch) => launch,
        Err(code) => std::process::exit(code),
    };
    init_tracing();
    // Single instance: if a miniti already answers on the socket, hand it our
    // job (raise the window, deliver deep links) and leave. Without this an
    // OAuth return through the desktop entry started a second copy.
    if let Ok(mut client) = ipc::client::connect() {
        use ipc::protocol::Request;
        let mut delivered = true;
        for url in &launch.urls {
            delivered &= client
                .call(&Request::OpenUrl { url: url.clone() })
                .map(|r| r.ok)
                .unwrap_or(false);
        }
        if launch.start_meeting {
            let _ = client.call(&Request::Start { title: launch.title.clone() });
        }
        if !launch.hidden {
            let _ = client.call(&Request::Show);
        }
        tracing::info!("miniti is already running; forwarded {} link(s)", launch.urls.len());
        if !delivered {
            eprintln!("miniti: the running instance did not accept one of the links");
        }
        return;
    }
    apply_webkit_workarounds();
    #[cfg(all(not(debug_assertions), not(feature = "custom-protocol")))]
    tracing::error!(
        "release binary built without the `custom-protocol` feature: the window will try to load the Vite dev server. \
         Build with `cargo build --release --features custom-protocol` (or `pnpm tauri build`)."
    );
    tauri::Builder::default()
        .manage(shell::TraySlot::new(None))
        .manage(smart::MonitorSlot::new(smart::MonitorState::default()))
        .manage(integrations::CalendarSlot::new(
            integrations::CalendarState::default(),
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(build_state())
        .manage(ipc::snapshot::HubSlot::new(ipc::snapshot::Hub::new()))
        .manage(state::PendingLinks::new(launch.urls.clone()))
        .setup(move |app| {
            let handle = app.handle().clone();
            let show_tray = handle
                .state::<AppState>()
                .prefs
                .lock()
                .map(|p| p.show_tray)
                .unwrap_or(true);
            shell::setup_tray(&handle, show_tray);
            shell::setup_deep_links(&handle);
            ipc::snapshot::listen(&handle);
            ipc::server::spawn(handle.clone());
            ipc::dbus::spawn(handle.clone());
            state::spawn_shell_ticker(handle.clone());
            state::spawn_smart_monitor(handle.clone());
            state::spawn_calendar_refresher(handle.clone());
            // The window is created the ordinary way (a window created
            // hidden and shown later came up without a usable close button
            // under Hyprland until it was resized); `--hidden` at login hides
            // it again at once, which is a brief flash at most.
            if launch.hidden && show_tray {
                if let Some(w) = handle.get_webview_window(shell::MAIN_LABEL) {
                    let _ = w.hide();
                }
            }
            if launch.start_meeting {
                let title = launch.title.clone();
                tauri::async_runtime::spawn(async move {
                    state::start_from_shell(handle, title).await;
                });
            }
            state::spawn_signal_handler(app.handle().clone());
            state::spawn_keyring_retry(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Close-to-tray: miniti stays reachable after the main window closes.
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
            recording_presence,
            frontend_ready,
            disable_nudge_kind,
            debug_log_path,
            debug_log_tail,
            debug_log_reveal,
            debug_log_export,
            debug_log_clear,
            list_templates,
            set_template,
            auth_status,
            auth_create_account,
            auth_restore_account,
            auth_recovery_key,
            auth_rotate_recovery_key,
            auth_devices,
            auth_remove_device,
            auth_sign_out,
            auth_start_over,
            auth_delete_account,
            add_correction,
            remove_correction,
            join_and_start_from_event,
            open_meeting_join_link,
            mark_all_mic_as_you,
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
            shell::resize_presence,
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
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                ipc::snapshot::cleanup();
            }
        });
}
