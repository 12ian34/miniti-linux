//! Application state + recording engine wiring the audio, Deepgram, backend,
//! persistence and webhook layers together, exposed to the UI as Tauri commands.
//!
//! Credential resolution: BYOK uses the user's Deepgram key (`Token`); managed
//! mode calls `POST /api/session` for a grant JWT (`Bearer`) and refreshes it
//! before reconnects. Recording refuses to start without a usable credential
//! rather than silently producing an empty meeting.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::api::{self, ApiClient, ApiError, HeaderContext, Usage, VersionInfo};
use crate::audio;
use crate::coaching::{self, CoachingReport, CoachingSnapshot, TrainingMetrics};
use crate::db::{self, Meeting, TranscriptSegment};
use crate::deepgram::live::{AuthProvider, StreamStatus};
use crate::deepgram::{self, Auth, DeepgramConfig, Processor, SegmentSource, TranscriptEvent};
use crate::gates::{self, Gate, GateInputs};
use crate::prefs::{AppMode, Prefs, CURRENT_TERMS_VERSION};

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How long stop waits for Deepgram to flush finals after `CloseStream`.
const STOP_DRAIN_BUDGET: Duration = Duration::from_secs(4);

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

/// What the current meeting is authenticated with.
#[derive(Clone)]
enum Credential {
    Byok(String),
    Managed {
        client: ApiClient,
        language: String,
        session_id: Option<String>,
        token: String,
        expiry_at: Option<i64>,
    },
}

impl Credential {
    fn into_auth_provider(self) -> AuthProvider {
        match self {
            Credential::Byok(key) => Box::new(move || {
                let key = key.clone();
                Box::pin(async move { Ok(Auth::Token(key)) })
            }),
            Credential::Managed {
                client,
                language,
                token,
                expiry_at,
                ..
            } => {
                let cache = Arc::new(tokio::sync::Mutex::new((token, expiry_at)));
                Box::new(move || {
                    let client = client.clone();
                    let language = language.clone();
                    let cache = cache.clone();
                    Box::pin(async move {
                        let mut guard = cache.lock().await;
                        let now = chrono::Utc::now().timestamp();
                        let stale = guard
                            .1
                            .map(|exp| api::needs_refresh(exp, now))
                            .unwrap_or(false);
                        if stale {
                            tracing::info!("managed grant near expiry; refreshing session");
                            let s = client
                                .create_session(&language)
                                .await
                                .map_err(|e| e.to_string())?;
                            let token = s
                                .token()
                                .ok_or_else(|| "session response had no token".to_string())?
                                .to_string();
                            *guard = (token, s.expiry_at(now));
                        }
                        Ok(Auth::Bearer(guard.0.clone()))
                    })
                })
            }
        }
    }
}

#[derive(Default)]
pub struct RecordingSession {
    pub running: bool,
    pub meeting: Option<Meeting>,
    started_at: Option<Instant>,
    mic: Option<audio::CaptureHandle>,
    system: Option<audio::CaptureHandle>,
    readers: Vec<JoinHandle<()>>,
    dg_task: Option<tauri::async_runtime::JoinHandle<()>>,
    consumer_tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
    credential: Option<Credential>,
}

/// Everything `stop` needs after releasing the session lock.
struct StopParts {
    meeting: Meeting,
    started_at: Option<Instant>,
    dg_task: Option<tauri::async_runtime::JoinHandle<()>>,
    consumer_tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
    credential: Option<Credential>,
}

impl RecordingSession {
    #[allow(clippy::too_many_arguments)]
    fn start(
        &mut self,
        app: AppHandle,
        db: Arc<Mutex<Connection>>,
        levels: Arc<Levels>,
        last_status: Arc<Mutex<Option<StreamStatus>>>,
        prefs: &Prefs,
        title: String,
        credential: Credential,
    ) -> Result<String, String> {
        // Microphone first: without it there is nothing to transcribe.
        let (mic_tx, mic_rx) = audio::frame_channel();
        let mic = audio::start_microphone(mic_tx)
            .map_err(|e| format!("microphone unavailable: {e}"))?;

        let mut meeting = Meeting::new(title, prefs.language.clone());
        if let Credential::Managed { session_id, .. } = &credential {
            meeting.managed_session_id = session_id.clone();
        }
        {
            let conn = db.lock().map_err(|_| "db poisoned")?;
            db::upsert_meeting(&conn, &meeting).map_err(|e| e.to_string())?;
        }
        levels.reset();
        if let Ok(mut s) = last_status.lock() {
            *s = Some(StreamStatus::Connecting);
        }
        let started_at = Instant::now();

        // ---- Deepgram stream (mono mic today; multichannel once system PCM is interleaved)
        let cfg = DeepgramConfig {
            language: prefs.language.clone(),
            multichannel: false,
            keyterms: Vec::new(),
        };
        let processor = Processor::new(false, SegmentSource::Microphone);
        let (pcm_tx, pcm_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);
        let (ev_tx, mut ev_rx) = tokio::sync::mpsc::channel::<TranscriptEvent>(512);
        let (st_tx, mut st_rx) = tokio::sync::mpsc::channel::<StreamStatus>(32);

        let auth_provider = credential.clone().into_auth_provider();
        self.dg_task = Some(tauri::async_runtime::spawn(async move {
            deepgram::live::run_stream(cfg, auth_provider, pcm_rx, ev_tx, st_tx, processor, started_at)
                .await;
        }));

        // ---- Persist finals + emit every event to the UI
        let persist_db = db.clone();
        let persist_app = app.clone();
        let meeting_id = meeting.id.clone();
        self.consumer_tasks.push(tauri::async_runtime::spawn(async move {
            while let Some(ev) = ev_rx.recv().await {
                let mut segment_id = None;
                if ev.is_final {
                    let seg = TranscriptSegment::new(
                        &meeting_id,
                        ev.speaker_id,
                        ev.text.clone(),
                        ev.start,
                        ev.end,
                        ev.source.as_str(),
                    );
                    if let Ok(conn) = persist_db.lock() {
                        if let Err(e) = db::add_segment(&conn, &seg) {
                            tracing::warn!("failed to persist segment: {e}");
                        }
                    }
                    segment_id = Some(seg.id);
                }
                let _ = persist_app.emit(
                    "transcript",
                    TranscriptPayload::new(&meeting_id, segment_id, &ev),
                );
            }
        }));

        // ---- Connection status → UI
        let status_app = app.clone();
        let status_slot = last_status.clone();
        self.consumer_tasks.push(tauri::async_runtime::spawn(async move {
            while let Some(st) = st_rx.recv().await {
                if let Ok(mut s) = status_slot.lock() {
                    *s = Some(st.clone());
                }
                let _ = status_app.emit("transcription_status", &st);
            }
        }));

        // ---- Mic reader: meter + forward PCM. Owns the only pcm sender, so
        // when capture stops the channel closes and Deepgram drains gracefully.
        self.mic = Some(mic);
        let levels_mic = levels.clone();
        self.readers.push(std::thread::spawn(move || {
            while let Ok(frame) = mic_rx.recv() {
                Levels::set(&levels_mic.mic, frame.level);
                let bytes = audio::pcm::pcm16_to_le_bytes(&frame.samples);
                if pcm_tx.blocking_send(bytes).is_err() {
                    break;
                }
            }
        }));

        // ---- System audio: metering only for now (interleave engine is TODO).
        if prefs.capture_system_audio {
            let (sys_tx, sys_rx) = audio::frame_channel();
            match audio::start_system(sys_tx) {
                Ok(handle) => {
                    self.system = Some(handle);
                    let levels_sys = levels.clone();
                    self.readers.push(std::thread::spawn(move || {
                        while let Ok(frame) = sys_rx.recv() {
                            Levels::set(&levels_sys.system, frame.level);
                        }
                    }));
                }
                Err(e) => tracing::info!("system audio unavailable: {e}"),
            }
        }

        let id = meeting.id.clone();
        self.meeting = Some(meeting);
        self.started_at = Some(started_at);
        self.credential = Some(credential);
        self.running = true;
        Ok(id)
    }

    /// Synchronous half of stop: tear down capture (closing the PCM channel)
    /// and hand back what the async half needs. Returns `None` if not running.
    fn begin_stop(&mut self) -> Option<StopParts> {
        if !self.running {
            return None;
        }
        if let Some(h) = self.mic.take() {
            h.stop();
        }
        if let Some(h) = self.system.take() {
            h.stop();
        }
        for join in self.readers.drain(..) {
            let _ = join.join();
        }
        self.running = false;
        Some(StopParts {
            meeting: self.meeting.take()?,
            started_at: self.started_at.take(),
            dg_task: self.dg_task.take(),
            consumer_tasks: std::mem::take(&mut self.consumer_tasks),
            credential: self.credential.take(),
        })
    }
}

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub prefs: Mutex<Prefs>,
    pub device_id: String,
    /// Shared backend secret compiled into this build (None ⇒ BYOK only).
    pub api_key: Option<String>,
    pub levels: Arc<Levels>,
    pub session: Mutex<RecordingSession>,
    pub last_status: Arc<Mutex<Option<StreamStatus>>>,
}

impl AppState {
    fn api_client(&self, prefs: &Prefs) -> Result<ApiClient, ApiError> {
        let api_key = self.api_key.clone().ok_or(ApiError::NoApiKey)?;
        Ok(ApiClient::new(
            api::DEFAULT_BASE_URL,
            HeaderContext {
                api_key,
                device_id: self.device_id.clone(),
                app_version: APP_VERSION.to_string(),
                app_mode: prefs.app_mode,
            },
        ))
    }

    fn prefs_snapshot(&self) -> Result<Prefs, String> {
        Ok(self.prefs.lock().map_err(|_| "prefs poisoned")?.clone())
    }

    fn fillers(&self, prefs: &Prefs) -> Vec<String> {
        coaching::effective_fillers(&prefs.language, &prefs.filler_overrides)
    }
}

// ---- Serializable payloads ------------------------------------------------

#[derive(Serialize, Clone)]
pub struct TranscriptPayload {
    pub meeting_id: String,
    /// Persisted segment id for finals (None for interims) — lets the UI dedupe
    /// against segments it already loaded from the database.
    pub segment_id: Option<String>,
    pub text: String,
    pub speaker_id: i64,
    /// Default display label (before names / mark-as-you): "You", "Speaker N".
    pub speaker_label: String,
    pub start: f64,
    pub end: f64,
    pub is_final: bool,
    pub confidence: f64,
    pub source: String,
    pub channel_index: Option<u32>,
}

impl TranscriptPayload {
    fn new(meeting_id: &str, segment_id: Option<String>, ev: &TranscriptEvent) -> Self {
        Self {
            meeting_id: meeting_id.to_string(),
            segment_id,
            text: ev.text.clone(),
            speaker_id: ev.speaker_id,
            speaker_label: coaching::resolved_speaker_label(ev.speaker_id, None, None),
            start: ev.start,
            end: ev.end,
            is_final: ev.is_final,
            confidence: ev.confidence,
            source: ev.source.as_str().to_string(),
            channel_index: ev.channel_index,
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
    /// Whether this build can talk to the Miniti backend (managed mode).
    pub backend_key_present: bool,
    pub device_id: String,
}

#[derive(Serialize)]
pub struct LevelsPayload {
    pub mic: f32,
    pub system: f32,
    pub recording: bool,
}

#[derive(Serialize)]
pub struct RecordingStatus {
    pub recording: bool,
    pub meeting_id: Option<String>,
    pub elapsed_seconds: f64,
    pub stream: Option<StreamStatus>,
}

#[derive(Serialize)]
pub struct LaunchGate {
    pub gate: Gate,
    pub current_version: String,
    pub min_version: Option<String>,
    pub latest_version: Option<String>,
    pub download_url: Option<String>,
    pub terms_version: String,
    pub backend_reachable: bool,
    pub backend_error: Option<String>,
}

#[derive(Serialize)]
pub struct MeetingDetail {
    pub meeting: Meeting,
    pub segments: Vec<TranscriptSegment>,
    /// Resolved labels per speaker id (names + mark-as-you applied).
    pub speaker_labels: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct CoachingOverview {
    pub report: Option<CoachingReport>,
    pub snapshots: Vec<CoachingSnapshot>,
}

fn speaker_labels_for(meeting: &Meeting, segments: &[TranscriptSegment]) -> HashMap<String, String> {
    let names: HashMap<String, String> =
        serde_json::from_str(&meeting.speaker_names).unwrap_or_default();
    let names_opt = if names.is_empty() { None } else { Some(&names) };
    let self_ids = meeting.self_speaker_ids();
    let mut ids: Vec<i64> = segments.iter().map(|s| s.speaker).collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .map(|id| {
            (
                id.to_string(),
                coaching::resolved_speaker_label(id, names_opt, self_ids.as_deref()),
            )
        })
        .collect()
}

fn metrics_for(meeting: &Meeting, segments: &[TranscriptSegment], fillers: &[String]) -> TrainingMetrics {
    let names: HashMap<String, String> =
        serde_json::from_str(&meeting.speaker_names).unwrap_or_default();
    let names_opt = if names.is_empty() { None } else { Some(&names) };
    let self_ids = meeting.self_speaker_ids();
    let turns: Vec<coaching::Segment> = segments
        .iter()
        .map(|s| coaching::Segment {
            text: s.text.clone(),
            speaker: s.speaker,
            is_final: true,
            timestamp: s.start_s,
        })
        .collect();
    coaching::compute(
        &turns,
        meeting.duration_seconds() as f64,
        fillers,
        names_opt,
        self_ids.as_deref(),
    )
}

// ---- Commands -------------------------------------------------------------

#[tauri::command]
pub fn environment_health(state: State<AppState>) -> EnvHealth {
    EnvHealth {
        app_name: "Miniti Linux".to_string(),
        app_version: APP_VERSION.to_string(),
        platform: api::PLATFORM.to_string(),
        pcm_contract: "16 kHz PCM16 LE • mono mic (mic+system interleave pending)".to_string(),
        os: std::env::consts::OS.to_string(),
        tauri_bridge: true,
        microphone_available: audio::microphone_available(),
        system_audio_available: audio::system_audio_available(),
        backend_key_present: state.api_key.is_some(),
        device_id: state.device_id.clone(),
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

/// Launch gate: force-update (via `/api/version`) → terms → onboarding → main.
/// Backend failures never block launch; they are reported for the UI.
#[tauri::command]
pub async fn launch_gate(state: State<'_, AppState>) -> Result<LaunchGate, String> {
    let prefs = state.prefs_snapshot()?;
    let (version, backend_error): (Option<VersionInfo>, Option<String>) =
        match state.api_client(&prefs) {
            Ok(client) => match client.get_version().await {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e.to_string())),
            },
            Err(e) => (None, Some(e.to_string())),
        };
    let gate = gates::route(&GateInputs {
        current_version: APP_VERSION.to_string(),
        min_version: version.as_ref().map(|v| v.min_version.clone()).unwrap_or_default(),
        accepted_terms_version: prefs.accepted_terms_version.clone(),
        current_terms_version: CURRENT_TERMS_VERSION.to_string(),
        first_launch: !prefs.onboarding_complete,
    });
    Ok(LaunchGate {
        gate,
        current_version: APP_VERSION.to_string(),
        min_version: version.as_ref().map(|v| v.min_version.clone()),
        latest_version: version.as_ref().map(|v| v.latest_version.clone()),
        download_url: version.as_ref().and_then(|v| v.download_url.clone()),
        terms_version: CURRENT_TERMS_VERSION.to_string(),
        backend_reachable: version.is_some(),
        backend_error,
    })
}

#[tauri::command]
pub fn accept_terms(state: State<AppState>) -> Result<Prefs, String> {
    let mut prefs = state.prefs.lock().map_err(|_| "prefs poisoned")?;
    prefs.accepted_terms_version = Some(CURRENT_TERMS_VERSION.to_string());
    prefs.save().map_err(|e| e.to_string())?;
    Ok(prefs.clone())
}

#[tauri::command]
pub fn complete_onboarding(state: State<AppState>) -> Result<Prefs, String> {
    let mut prefs = state.prefs.lock().map_err(|_| "prefs poisoned")?;
    prefs.onboarding_complete = true;
    prefs.save().map_err(|e| e.to_string())?;
    Ok(prefs.clone())
}

/// Managed usage (minutes / tier). Errors are user-facing strings.
#[tauri::command]
pub async fn get_usage(state: State<'_, AppState>) -> Result<Usage, String> {
    let prefs = state.prefs_snapshot()?;
    let client = state.api_client(&prefs).map_err(|e| e.to_string())?;
    client.get_usage().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn subscribe_url(state: State<'_, AppState>) -> Result<String, String> {
    let prefs = state.prefs_snapshot()?;
    let client = state.api_client(&prefs).map_err(|e| e.to_string())?;
    client.subscribe_url().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn portal_url(state: State<'_, AppState>) -> Result<String, String> {
    let prefs = state.prefs_snapshot()?;
    let client = state.api_client(&prefs).map_err(|e| e.to_string())?;
    client.portal_url().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn restore_license(state: State<'_, AppState>, license_key: String) -> Result<serde_json::Value, String> {
    let prefs = state.prefs_snapshot()?;
    let client = state.api_client(&prefs).map_err(|e| e.to_string())?;
    client.restore(license_key.trim()).await.map_err(|e| e.to_string())
}

async fn resolve_credential(state: &AppState, prefs: &Prefs) -> Result<Credential, String> {
    match prefs.app_mode {
        AppMode::Byok => prefs
            .byok_deepgram_key()
            .map(|k| Credential::Byok(k.to_string()))
            .ok_or_else(|| "Add your Deepgram API key in Settings (BYOK) to transcribe.".to_string()),
        AppMode::Managed => {
            let client = state.api_client(prefs).map_err(|e| e.to_string())?;
            let session = client
                .create_session(&prefs.language)
                .await
                .map_err(|e| format!("Could not start a managed session: {e}"))?;
            let token = session
                .token()
                .ok_or_else(|| "Backend session response had no access token.".to_string())?
                .to_string();
            let now = chrono::Utc::now().timestamp();
            Ok(Credential::Managed {
                client,
                language: prefs.language.clone(),
                session_id: session.session_id.clone(),
                token,
                expiry_at: session.expiry_at(now),
            })
        }
    }
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    title: Option<String>,
) -> Result<String, String> {
    let prefs = state.prefs_snapshot()?;
    if state.session.lock().map_err(|_| "session poisoned")?.running {
        return Err("already recording".into());
    }
    let credential = resolve_credential(&state, &prefs).await?;

    let title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "New meeting".to_string());
    let db = state.db.clone();
    let levels = state.levels.clone();
    let last_status = state.last_status.clone();
    let mut session = state.session.lock().map_err(|_| "session poisoned")?;
    if session.running {
        return Err("already recording".into());
    }
    session.start(app, db, levels, last_status, &prefs, title, credential)
}

#[tauri::command]
pub async fn stop_recording(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let parts = {
        let mut session = state.session.lock().map_err(|_| "session poisoned")?;
        session.begin_stop()
    };
    let Some(parts) = parts else {
        return Ok(None);
    };
    state.levels.reset();

    // Let Deepgram flush its finals (CloseStream → drain), then the consumers.
    if let Some(task) = parts.dg_task {
        if tokio::time::timeout(STOP_DRAIN_BUDGET, task).await.is_err() {
            tracing::warn!("deepgram stream did not finish within {STOP_DRAIN_BUDGET:?}");
        }
    }
    for task in parts.consumer_tasks {
        let _ = tokio::time::timeout(Duration::from_secs(2), task).await;
    }
    if let Ok(mut s) = state.last_status.lock() {
        *s = None;
    }

    let meeting_id = parts.meeting.id.clone();
    let ended_at = chrono::Utc::now().timestamp();
    let prefs = state.prefs_snapshot()?;
    // Re-read the row: notes, title, names and mark-as-you may have been edited
    // during the recording and must not be clobbered by the in-memory copy.
    let (meeting, segments) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        db::set_ended_at(&conn, &meeting_id, ended_at).map_err(|e| e.to_string())?;
        let Some(meeting) = db::get_meeting(&conn, &meeting_id).map_err(|e| e.to_string())? else {
            // Deleted mid-recording: honour the deletion.
            return Ok(None);
        };
        let segments = db::list_segments(&conn, &meeting_id).map_err(|e| e.to_string())?;
        (meeting, segments)
    };

    // Report managed usage (idempotent server-side; failures are logged).
    if let Some(Credential::Managed {
        client,
        session_id: Some(session_id),
        ..
    }) = parts.credential
    {
        let minutes = parts
            .started_at
            .map(|s| s.elapsed().as_secs_f64() / 60.0)
            .unwrap_or(meeting.duration_seconds() as f64 / 60.0);
        match client.end_session(&session_id, minutes).await {
            Ok(r) => tracing::info!(
                "session {session_id} ended: {:.1} min used, finalized={}",
                r.minutes_used,
                r.session_finalized
            ),
            Err(e) => tracing::warn!("session end failed (will not retry): {e}"),
        }
    }

    // Webhook (fire-and-forget, same payload shape as macOS).
    if let Some(url) = prefs.webhook_url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        let payload = crate::webhook::payload_from_meeting(
            "meeting.saved",
            &meeting,
            &segments,
            &state.fillers(&prefs),
        );
        let url = url.to_string();
        tauri::async_runtime::spawn(async move {
            crate::webhook::send(&url, &payload).await;
        });
    }

    let _ = app.emit("meeting_saved", &meeting.id);
    Ok(Some(meeting.id))
}

#[tauri::command]
pub fn get_levels(state: State<AppState>) -> LevelsPayload {
    let recording = state.session.lock().map(|s| s.running).unwrap_or(false);
    LevelsPayload {
        mic: state.levels.mic(),
        system: state.levels.system(),
        recording,
    }
}

#[tauri::command]
pub fn recording_status(state: State<AppState>) -> RecordingStatus {
    let (recording, meeting_id, elapsed) = state
        .session
        .lock()
        .map(|s| {
            (
                s.running,
                s.meeting.as_ref().map(|m| m.id.clone()),
                s.started_at.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0),
            )
        })
        .unwrap_or((false, None, 0.0));
    RecordingStatus {
        recording,
        meeting_id,
        elapsed_seconds: elapsed,
        stream: state.last_status.lock().ok().and_then(|s| s.clone()),
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
pub fn get_meeting_detail(state: State<AppState>, id: String) -> Result<Option<MeetingDetail>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let Some(meeting) = db::get_meeting(&conn, &id).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let segments = db::list_segments(&conn, &id).map_err(|e| e.to_string())?;
    let speaker_labels = speaker_labels_for(&meeting, &segments);
    Ok(Some(MeetingDetail {
        meeting,
        segments,
        speaker_labels,
    }))
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
pub fn set_notes(state: State<AppState>, id: String, notes: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_notes(&conn, &id, &notes).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_meeting_title(state: State<AppState>, id: String, title: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_title(&conn, &id, title.trim()).map_err(|e| e.to_string())
}

/// Manual speaker rename (empty name clears).
#[tauri::command]
pub fn set_speaker_name(
    state: State<AppState>,
    meeting_id: String,
    speaker_id: i64,
    name: String,
) -> Result<HashMap<String, String>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let meeting = db::get_meeting(&conn, &meeting_id)
        .map_err(|e| e.to_string())?
        .ok_or("meeting not found")?;
    let mut names: HashMap<String, String> =
        serde_json::from_str(&meeting.speaker_names).unwrap_or_default();
    let name = name.trim();
    if name.is_empty() {
        names.remove(&speaker_id.to_string());
    } else {
        names.insert(speaker_id.to_string(), name.to_string());
    }
    let json = serde_json::to_string(&names).map_err(|e| e.to_string())?;
    db::set_speaker_names(&conn, &meeting_id, &json).map_err(|e| e.to_string())?;
    Ok(names)
}

/// "Mark as you": toggles a speaker id in the meeting's explicit self set.
#[tauri::command]
pub fn mark_as_you(
    state: State<AppState>,
    meeting_id: String,
    speaker_id: i64,
    is_you: bool,
) -> Result<Vec<i64>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let meeting = db::get_meeting(&conn, &meeting_id)
        .map_err(|e| e.to_string())?
        .ok_or("meeting not found")?;
    let mut ids: Vec<i64> = serde_json::from_str(&meeting.self_speaker_ids).unwrap_or_default();
    if ids.is_empty() {
        // Materialize the implicit default before editing it.
        ids.push(deepgram::MIC_SPEAKER_ID);
    }
    ids.retain(|&id| id != speaker_id);
    if is_you {
        ids.push(speaker_id);
    }
    ids.sort_unstable();
    let json = serde_json::to_string(&ids).map_err(|e| e.to_string())?;
    db::set_self_speaker_ids(&conn, &meeting_id, &json).map_err(|e| e.to_string())?;
    Ok(ids)
}

#[tauri::command]
pub fn delete_meeting(state: State<AppState>, id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::delete_meeting(&conn, &id).map_err(|e| e.to_string())
}

/// Per-meeting coaching metrics (port of `TrainingMetrics.compute`).
#[tauri::command]
pub fn coaching_overview(state: State<AppState>, meeting_id: String) -> Result<TrainingMetrics, String> {
    let prefs = state.prefs_snapshot()?;
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let meeting = db::get_meeting(&conn, &meeting_id)
        .map_err(|e| e.to_string())?
        .ok_or("meeting not found")?;
    let segments = db::list_segments(&conn, &meeting_id).map_err(|e| e.to_string())?;
    Ok(metrics_for(&meeting, &segments, &state.fillers(&prefs)))
}

/// Cross-meeting coaching report (port of `CoachingAdvisor.analyze`).
#[tauri::command]
pub fn coaching_report(state: State<AppState>, limit: Option<i64>) -> Result<CoachingOverview, String> {
    let prefs = state.prefs_snapshot()?;
    let fillers = state.fillers(&prefs);
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let meetings = db::list_meetings(&conn, limit.unwrap_or(30)).map_err(|e| e.to_string())?;
    let mut snapshots = Vec::new();
    for m in &meetings {
        let segments = db::list_segments(&conn, &m.id).map_err(|e| e.to_string())?;
        if segments.is_empty() {
            continue;
        }
        let metrics = metrics_for(m, &segments, &fillers);
        if let Some(snap) = CoachingSnapshot::from_metrics(
            &m.id,
            &m.display_title(),
            m.started_at.unwrap_or(m.created_at),
            &metrics,
        ) {
            snapshots.push(snap);
        }
    }
    Ok(CoachingOverview {
        report: coaching::analyze(&snapshots),
        snapshots,
    })
}
