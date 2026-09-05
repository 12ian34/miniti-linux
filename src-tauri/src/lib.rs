use serde::Serialize;

/// Minimal environment/health snapshot surfaced by the Phase 0 skeleton so the
/// UI can prove the Rust <-> WebView bridge and the dev toolchain are wired up.
/// Product contracts (Deepgram PCM, X-Platform header) are echoed here as
/// constants from PLAN.md — real capture/session logic lands in later phases.
#[derive(Serialize)]
struct EnvHealth {
    app_name: String,
    app_version: String,
    /// `X-Platform` header value sent to the Miniti backend (see PLAN.md §5).
    platform: String,
    /// Deepgram audio contract from PLAN.md §6 (16 kHz PCM16 LE).
    pcm_contract: String,
    /// Human-readable OS string.
    os: String,
    tauri_bridge: bool,
}

#[tauri::command]
fn environment_health() -> EnvHealth {
    EnvHealth {
        app_name: "Miniti Linux".to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        platform: "linux".to_string(),
        pcm_contract: "16 kHz PCM16 LE • mono or stereo mic/system".to_string(),
        os: std::env::consts::OS.to_string(),
        tauri_bridge: true,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![environment_health])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
