//! Application state + recording engine wiring the audio, Deepgram, and
//! persistence layers together, exposed to the UI as Tauri commands.
//!
//! Live transcription runs only when a credential is available (BYOK key today;
//! managed session token once the backend is reachable). Metering, meeting
//! lifecycle, and persistence work without any credential.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::audio;
use crate::coaching::{self, SpeechTurn};
use crate::db::{self, Meeting, TranscriptSegment};
use crate::deepgram::{self, Auth, DeepgramConfig, SegmentSource, TranscriptEvent};
use crate::prefs::{AppMode, Prefs};

/// Lock-free mic/system RMS levels for the waveform meters.
#[derive(Default)]
pub struct Levels {
    mic: AtomicU32,
    system: AtomicU32,
}

impl Levels {
    fn set(slot: &AtomicU32, v: f32) {
        slot.store(v.to_bits(), Ordering::Relaxed);
    }
    fn get(slot: &AtomicU32) -> f32 {
        f32::from_bits(slot.load(Ordering::Relaxed))
    }
    pub fn mic(&self) -> f32 {
        Self::get(&self.mic)
    }
    pub fn system(&self) -> f32 {
        Self::get(&self.system)
    }
    fn reset(&self) {
        Self::set(&self.mic, 0.0);
        Self::set(&self.system, 0.0);
    }
}

fn source_str(s: SegmentSource) -> &'static str {
    match s {
        SegmentSource::Microphone => "microphone",
        SegmentSource::System => "system",
        SegmentSource::Mono => "mono",
    }
}

#[derive(Default)]
pub struct RecordingSession {
    pub running: bool,
    pub meeting: Option<Meeting>,
    mic: Option<audio::CaptureHandle>,
    system: Option<audio::CaptureHandle>,
    readers: Vec<JoinHandle<()>>,
    dg_stop: Arc<AtomicBool>,
}

impl RecordingSession {
    fn start(
        &mut self,
        app: AppHandle,
        db: Arc<Mutex<Connection>>,
        levels: Arc<Levels>,
        prefs: &Prefs,
        title: String,
    ) -> Result<String, String> {
        let meeting = Meeting::new(title, prefs.language.clone());
        {
            let conn = db.lock().map_err(|_| "db poisoned")?;
            db::upsert_meeting(&conn, &meeting).map_err(|e| e.to_string())?;
        }
        levels.reset();
        self.dg_stop = Arc::new(AtomicBool::new(false));

        // Optional live transcription (needs a credential).
        let mut dg_pcm_tx: Option<tokio::sync::mpsc::Sender<Vec<u8>>> = None;
        if let Some(auth) = resolve_auth(prefs) {
            let (pcm_tx, pcm_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(128);
            let (ev_tx, mut ev_rx) = tokio::sync::mpsc::channel::<TranscriptEvent>(128);
            let cfg = DeepgramConfig {
                language: prefs.language.clone(),
                multichannel: false,
                keyterms: Vec::new(),
            };
            tauri::async_runtime::spawn(async move {
                if let Err(e) =
                    deepgram::live::connect_and_stream(&cfg, &auth, pcm_rx, ev_tx).await
                {
                    tracing::warn!("deepgram stream ended: {e}");
                }
            });

            let persist_db = db.clone();
            let persist_app = app.clone();
            let meeting_id = meeting.id.clone();
            tauri::async_runtime::spawn(async move {
                while let Some(ev) = ev_rx.recv().await {
                    if ev.is_final {
                        let seg = TranscriptSegment::new(
                            &meeting_id,
                            ev.speaker_id,
                            ev.text.clone(),
                            ev.start,
                            ev.end,
                            source_str(ev.source),
                        );
                        if let Ok(conn) = persist_db.lock() {
                            let _ = db::add_segment(&conn, &seg);
                        }
                    }
                    let _ = persist_app.emit("transcript", TranscriptPayload::from(&ev));
                }
            });
            dg_pcm_tx = Some(pcm_tx);
        }

        // Microphone: meter always; forward PCM to Deepgram when active.
        let (mic_tx, mic_rx) = audio::frame_channel();
        match audio::start_microphone(mic_tx) {
            Ok(handle) => {
                self.mic = Some(handle);
                let levels_mic = levels.clone();
                let forward = dg_pcm_tx.clone();
                let join = std::thread::spawn(move || {
                    while let Ok(frame) = mic_rx.recv() {
                        Levels::set(&levels_mic.mic, frame.level);
                        if let Some(tx) = &forward {
                            let bytes = audio::pcm::pcm16_to_le_bytes(&frame.samples);
                            let _ = tx.blocking_send(bytes);
                        }
                    }
                });
                self.readers.push(join);
            }
            Err(e) => tracing::warn!("microphone unavailable: {e}"),
        }

        // System audio: metering only for now (multichannel interleave is TODO).
        if prefs.capture_system_audio {
            let (sys_tx, sys_rx) = audio::frame_channel();
            match audio::start_system(sys_tx) {
                Ok(handle) => {
                    self.system = Some(handle);
                    let levels_sys = levels.clone();
                    let join = std::thread::spawn(move || {
                        while let Ok(frame) = sys_rx.recv() {
                            Levels::set(&levels_sys.system, frame.level);
                        }
                    });
                    self.readers.push(join);
                }
                Err(e) => tracing::info!("system audio unavailable: {e}"),
            }
        }

        let id = meeting.id.clone();
        self.meeting = Some(meeting);
        self.running = true;
        Ok(id)
    }

    fn stop(&mut self, db: Arc<Mutex<Connection>>) -> Result<Option<String>, String> {
        if !self.running {
            return Ok(None);
        }
        self.dg_stop.store(true, Ordering::SeqCst);
        // Dropping capture handles closes the frame channels, ending readers.
        if let Some(h) = self.mic.take() {
            h.stop();
        }
        if let Some(h) = self.system.take() {
            h.stop();
        }
        for join in self.readers.drain(..) {
            let _ = join.join();
        }

        let mut id = None;
        if let Some(mut meeting) = self.meeting.take() {
            meeting.ended_at = Some(chrono::Utc::now().timestamp());
            let conn = db.lock().map_err(|_| "db poisoned")?;
            db::upsert_meeting(&conn, &meeting).map_err(|e| e.to_string())?;
            id = Some(meeting.id);
        }
        self.running = false;
        Ok(id)
    }
}

/// Pick an auth scheme: BYOK Deepgram key today. Managed mode needs a session
/// token from `/api/session`, which requires the backend + app secret.
fn resolve_auth(prefs: &Prefs) -> Option<Auth> {
    match prefs.app_mode {
        AppMode::Byok => prefs
            .byok_deepgram_key
            .as_deref()
            .filter(|k| !k.trim().is_empty())
            .map(|k| Auth::Token(k.to_string())),
        AppMode::Managed => None,
    }
}

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub prefs: Mutex<Prefs>,
    pub device_id: String,
    pub levels: Arc<Levels>,
    pub session: Mutex<RecordingSession>,
}

// ---- Serializable payloads ------------------------------------------------

#[derive(Serialize, Clone)]
pub struct TranscriptPayload {
    pub text: String,
    pub speaker_id: i64,
    pub start: f64,
    pub end: f64,
    pub is_final: bool,
    pub source: String,
}

impl From<&TranscriptEvent> for TranscriptPayload {
    fn from(ev: &TranscriptEvent) -> Self {
        Self {
            text: ev.text.clone(),
            speaker_id: ev.speaker_id,
            start: ev.start,
            end: ev.end,
            is_final: ev.is_final,
            source: source_str(ev.source).to_string(),
        }
    }
}

#[derive(Serialize)]
pub struct EnvHealth {
    pub app_name: String,
    pub app_version: String,
    pub platform: String,
    pub pcm_contract: String,
    pub os: String,
    pub tauri_bridge: bool,
    pub microphone_available: bool,
    pub system_audio_available: bool,
}

#[derive(Serialize)]
pub struct LevelsPayload {
    pub mic: f32,
    pub system: f32,
    pub recording: bool,
}

// ---- Commands -------------------------------------------------------------

#[tauri::command]
pub fn environment_health(state: State<AppState>) -> EnvHealth {
    let _ = state; // reserved for future capability reporting
    EnvHealth {
        app_name: "Miniti Linux".to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        platform: crate::api::PLATFORM.to_string(),
        pcm_contract: "16 kHz PCM16 LE • mono or stereo mic/system".to_string(),
        os: std::env::consts::OS.to_string(),
        tauri_bridge: true,
        microphone_available: audio::microphone_available(),
        system_audio_available: audio::system_audio_available(),
    }
}

#[tauri::command]
pub fn get_prefs(state: State<AppState>) -> Prefs {
    state.prefs.lock().map(|p| p.clone()).unwrap_or_default()
}

#[tauri::command]
pub fn set_prefs(state: State<AppState>, prefs: Prefs) -> Result<(), String> {
    prefs.save().map_err(|e| e.to_string())?;
    *state.prefs.lock().map_err(|_| "prefs poisoned")? = prefs;
    Ok(())
}

#[tauri::command]
pub fn get_device_id(state: State<AppState>) -> String {
    state.device_id.clone()
}

#[tauri::command]
pub fn start_recording(
    app: AppHandle,
    state: State<AppState>,
    title: Option<String>,
) -> Result<String, String> {
    let prefs = state.prefs.lock().map_err(|_| "prefs poisoned")?.clone();
    let db = state.db.clone();
    let levels = state.levels.clone();
    let mut session = state.session.lock().map_err(|_| "session poisoned")?;
    if session.running {
        return Err("already recording".into());
    }
    let title = title.unwrap_or_else(|| "New meeting".to_string());
    session.start(app, db, levels, &prefs, title)
}

#[tauri::command]
pub fn stop_recording(state: State<AppState>) -> Result<Option<String>, String> {
    let db = state.db.clone();
    let mut session = state.session.lock().map_err(|_| "session poisoned")?;
    session.stop(db)
}

#[tauri::command]
pub fn get_levels(state: State<AppState>) -> LevelsPayload {
    let recording = state
        .session
        .lock()
        .map(|s| s.running)
        .unwrap_or(false);
    LevelsPayload {
        mic: state.levels.mic(),
        system: state.levels.system(),
        recording,
    }
}

#[tauri::command]
pub fn list_meetings(state: State<AppState>, limit: Option<i64>) -> Result<Vec<Meeting>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::list_meetings(&conn, limit.unwrap_or(100)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search_meetings(state: State<AppState>, query: String) -> Result<Vec<Meeting>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::search_meetings(&conn, &query, 100).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_meeting(state: State<AppState>, id: String) -> Result<Option<Meeting>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::get_meeting(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_segments(state: State<AppState>, meeting_id: String) -> Result<Vec<TranscriptSegment>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::list_segments(&conn, &meeting_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_pinned(state: State<AppState>, id: String, pinned: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_pinned(&conn, &id, pinned).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_meeting(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::delete_meeting(&conn, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn coaching_overview(
    state: State<AppState>,
    meeting_id: String,
) -> Result<coaching::CoachingMetrics, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let segments = db::list_segments(&conn, &meeting_id).map_err(|e| e.to_string())?;
    let language = state
        .prefs
        .lock()
        .map(|p| p.language.clone())
        .unwrap_or_else(|_| "en".to_string());

    let turns: Vec<SpeechTurn> = segments
        .iter()
        .map(|s| SpeechTurn {
            speaker: s.speaker,
            text: s.text.clone(),
            start_s: s.start_s,
            end_s: s.end_s,
        })
        .collect();

    // "Self" = mic-space speakers (>= offset); fall back to speaker 0 in mono.
    let mut self_speakers: Vec<i64> = segments
        .iter()
        .map(|s| s.speaker)
        .filter(|s| *s >= deepgram::MIC_SPEAKER_OFFSET)
        .collect();
    self_speakers.sort_unstable();
    self_speakers.dedup();
    if self_speakers.is_empty() {
        self_speakers.push(0);
    }

    let fillers = coaching::default_fillers(&language);
    Ok(coaching::compute(&turns, &self_speakers, &fillers))
}
