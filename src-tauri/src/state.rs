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
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::{self, ApiClient, ApiError, HeaderContext, Usage, VersionInfo};
use crate::audio;
use crate::coaching::{self, CoachingReport, CoachingSnapshot, TrainingMetrics};
use crate::db::{self, Meeting, TranscriptSegment};
use crate::deepgram::live::{AuthProvider, StreamStatus};
use crate::deepgram::{self, Auth, DeepgramConfig, Processor, SegmentSource, TranscriptEvent};
use crate::gates::{self, Gate, GateInputs};
use crate::insights::engine::{self as insights_engine, EngineConfig, Finishing, LiveEngine};
use crate::insights::provider::Provider;
use crate::insights::InvestigationScope;
use crate::prefs::{AppMode, Prefs, CURRENT_TERMS_VERSION};

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How long stop waits for Deepgram to flush finals after `CloseStream`.
const STOP_DRAIN_BUDGET: Duration = Duration::from_secs(4);

/// RMS above this counts as speech-like audio activity (Smart meetings quiet rules).
const SPEECH_LIKE_LEVEL: f32 = 0.01;

/// Lock-free mic/system RMS levels for the waveform meters, plus the last
/// time either source carried speech-like energy.
#[derive(Default)]
pub struct Levels {
    mic: AtomicU32,
    system: AtomicU32,
    last_activity: Mutex<Option<Instant>>,
}

impl Levels {
    fn set(slot: &AtomicU32, v: f32) {
        slot.store(v.to_bits(), Ordering::Relaxed);
    }
    fn note_activity(&self, level: f32) {
        if level >= SPEECH_LIKE_LEVEL {
            if let Ok(mut t) = self.last_activity.lock() {
                *t = Some(Instant::now());
            }
        }
    }
    pub fn audio_gap(&self) -> f64 {
        self.last_activity.lock().ok().and_then(|t| *t).map(|t| t.elapsed().as_secs_f64()).unwrap_or(f64::INFINITY)
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
        if let Ok(mut t) = self.last_activity.lock() {
            *t = None;
        }
    }
}

/// Transcript activity for Smart meetings (updated by the persist consumer).
#[derive(Default)]
pub struct ActivityTrack {
    pub last_final_at: Mutex<Option<Instant>>,
    pub meaningful_finals: std::sync::atomic::AtomicUsize,
}

impl ActivityTrack {
    fn reset(&self) {
        if let Ok(mut t) = self.last_final_at.lock() {
            *t = None;
        }
        self.meaningful_finals.store(0, Ordering::Relaxed);
    }
    fn note_final(&self, text: &str) {
        if let Ok(mut t) = self.last_final_at.lock() {
            *t = Some(Instant::now());
        }
        if text.split_whitespace().count() >= 3 {
            self.meaningful_finals.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn transcript_gap(&self) -> f64 {
        self.last_final_at.lock().ok().and_then(|t| *t).map(|t| t.elapsed().as_secs_f64()).unwrap_or(f64::INFINITY)
    }
}

/// How a meeting is started (Home button, tray, calendar event, call prompt).
#[derive(Debug, Clone, Default)]
pub struct StartOptions {
    pub title: Option<String>,
    pub calendar_event_id: Option<String>,
    pub attendees_json: Option<String>,
    pub notes: Option<String>,
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
    engine: Option<(Arc<std::sync::atomic::AtomicBool>, tauri::async_runtime::JoinHandle<()>)>,
    engine_cfg: Option<EngineConfig>,
}

/// Everything `stop` needs after releasing the session lock.
struct StopParts {
    meeting: Meeting,
    started_at: Option<Instant>,
    dg_task: Option<tauri::async_runtime::JoinHandle<()>>,
    consumer_tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
    credential: Option<Credential>,
    engine: Option<(Arc<std::sync::atomic::AtomicBool>, tauri::async_runtime::JoinHandle<()>)>,
    engine_cfg: Option<EngineConfig>,
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
        opts: StartOptions,
        credential: Credential,
        engine_cfg: Option<EngineConfig>,
        activity: Arc<ActivityTrack>,
    ) -> Result<String, String> {
        let title = opts.title.clone().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| "New meeting".to_string());
        // Microphone first: without it there is nothing to transcribe.
        let (mic_tx, mic_rx) = audio::frame_channel();
        let mic = audio::start_microphone(mic_tx)
            .map_err(|e| format!("microphone unavailable: {e}"))?;

        // System audio decides the Deepgram channel layout, so start it before
        // the socket. Failure degrades to mono mic with a log line.
        let mut system_capture = None;
        let mut system_rx = None;
        if prefs.capture_system_audio {
            let (sys_tx, sys_rx) = audio::frame_channel();
            match audio::start_system(sys_tx) {
                Ok(handle) => {
                    system_capture = Some(handle);
                    system_rx = Some(sys_rx);
                }
                Err(e) => tracing::info!("system audio unavailable, recording mic only: {e}"),
            }
        }
        let dual = system_capture.is_some();

        let mut meeting = Meeting::new(title, prefs.language.clone());
        meeting.sales_enabled = prefs.sales_insights_default;
        meeting.calendar_event_id = opts.calendar_event_id.clone();
        if let Some(a) = opts.attendees_json.clone() {
            meeting.attendees = a;
        }
        if let Some(n) = opts.notes.clone().filter(|n| !n.trim().is_empty()) {
            meeting.notes = n;
        }
        if opts.title.is_some() {
            // Calendar / user-supplied titles are not overwritten by suggestions.
            meeting.title_auto = false;
        }
        if let Credential::Managed { session_id, .. } = &credential {
            meeting.managed_session_id = session_id.clone();
        }
        activity.reset();
        {
            let conn = db.lock().map_err(|_| "db poisoned")?;
            db::upsert_meeting(&conn, &meeting).map_err(|e| e.to_string())?;
        }
        levels.reset();
        if let Ok(mut s) = last_status.lock() {
            *s = Some(StreamStatus::Connecting);
        }
        let started_at = Instant::now();

        // ---- Deepgram stream: stereo interleave (ch0 mic, ch1 system) when both run.
        let cfg = DeepgramConfig {
            language: prefs.language.clone(),
            multichannel: dual,
            keyterms: deepgram_keyterms(prefs),
        };
        let processor = Processor::new(dual, SegmentSource::Microphone);
        let (pcm_tx, pcm_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);
        let (ev_tx, mut ev_rx) = tokio::sync::mpsc::channel::<TranscriptEvent>(512);
        let (st_tx, mut st_rx) = tokio::sync::mpsc::channel::<StreamStatus>(32);
        let mixer: Arc<Mutex<audio::dual::DualMixer>> = Arc::new(Mutex::new(audio::dual::DualMixer::new()));

        let auth_provider = credential.clone().into_auth_provider();
        self.dg_task = Some(tauri::async_runtime::spawn(async move {
            deepgram::live::run_stream(cfg, auth_provider, pcm_rx, ev_tx, st_tx, processor, started_at)
                .await;
        }));

        // ---- Persist finals + emit every event to the UI (dual: via the echo
        // reconciler + 350 ms ordering buffer; mono: straight through).
        let persist_db = db.clone();
        let persist_app = app.clone();
        let meeting_id = meeting.id.clone();
        let sales_default = prefs.sales_insights_default;
        let mixer_for_consumer = mixer.clone();
        let activity_for_consumer = activity.clone();
        self.consumer_tasks.push(tauri::async_runtime::spawn(async move {
            let mut finals = 0usize;
            let mut sales_suggested = sales_default;
            let mut reconciler = deepgram::echo::DualChannelReconciler::new();
            let dominant = move |start: f64, end: f64| -> audio::dual::Source {
                mixer_for_consumer.lock().map(|m| m.log.dominant_source(start, end)).unwrap_or(audio::dual::Source::Unknown)
            };
            let mut persist_final = |ev: &TranscriptEvent| -> Option<String> {
                finals += 1;
                activity_for_consumer.note_final(&ev.text);
                if let Some(focus) = insights_engine::detect_investigation_moment(&ev.text) {
                    let _ = persist_app.emit("investigation_suggested", serde_json::json!({
                        "meeting_id": meeting_id, "focus": focus
                    }));
                }
                let seg = TranscriptSegment::new(&meeting_id, ev.speaker_id, ev.text.clone(), ev.start, ev.end, ev.source.as_str());
                if let Ok(conn) = persist_db.lock() {
                    if let Err(e) = db::add_segment(&conn, &seg) {
                        tracing::warn!("failed to persist segment: {e}");
                    }
                    if !sales_suggested && finals % 10 == 0 && finals <= 60 {
                        if let Ok(segs) = db::list_segments(&conn, &meeting_id) {
                            let text: String = segs.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
                            if insights_engine::sounds_commercial(&text) {
                                sales_suggested = true;
                                let _ = persist_app.emit("sales_suggested", &meeting_id);
                            }
                        }
                    }
                }
                let _ = persist_app.emit("transcript", TranscriptPayload::new(&meeting_id, Some(seg.id.clone()), ev));
                Some(seg.id)
            };
            let emit_interim = |ev: &TranscriptEvent| {
                let _ = persist_app.emit("transcript", TranscriptPayload::new(&meeting_id, None, ev));
            };
            if !dual {
                while let Some(ev) = ev_rx.recv().await {
                    if ev.is_final { persist_final(&ev); } else { emit_interim(&ev); }
                }
                return;
            }
            let mut flush_tick = tokio::time::interval(Duration::from_millis(100));
            flush_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    maybe = ev_rx.recv() => {
                        match maybe {
                            Some(ev) => {
                                let (interims, committed) = reconciler.ingest(vec![ev], Instant::now(), &dominant);
                                for i in &interims { emit_interim(i); }
                                for f in &committed { persist_final(f); }
                            }
                            None => {
                                for f in &reconciler.flush_all(&dominant) { persist_final(f); }
                                if reconciler.suppressed > 0 {
                                    tracing::info!("echo reconciliation suppressed {} mic segments", reconciler.suppressed);
                                }
                                return;
                            }
                        }
                    }
                    _ = flush_tick.tick() => {
                        for f in &reconciler.flush_due(Instant::now(), &dominant) { persist_final(f); }
                    }
                }
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

        // ---- Mic reader: meter + forward PCM (interleaved with system when dual).
        // Owns the only pcm sender, so when capture stops the channel closes and
        // Deepgram drains gracefully.
        self.mic = Some(mic);
        let levels_mic = levels.clone();
        let mixer_for_mic = mixer.clone();
        self.readers.push(std::thread::spawn(move || {
            while let Ok(frame) = mic_rx.recv() {
                Levels::set(&levels_mic.mic, frame.level);
                levels_mic.note_activity(frame.level);
                let bytes = if dual {
                    let interleaved = match mixer_for_mic.lock() {
                        Ok(mut m) => m.interleave_mic(&frame.samples),
                        Err(_) => audio::pcm::interleave_stereo_pcm16(&frame.samples, &[]),
                    };
                    audio::pcm::pcm16_to_le_bytes(&interleaved)
                } else {
                    audio::pcm::pcm16_to_le_bytes(&frame.samples)
                };
                if pcm_tx.blocking_send(bytes).is_err() {
                    break;
                }
            }
        }));

        // ---- System reader: meter + ring buffer for the mic thread to drain.
        if let (Some(handle), Some(sys_rx)) = (system_capture, system_rx) {
            self.system = Some(handle);
            let levels_sys = levels.clone();
            let mixer_for_sys = mixer.clone();
            self.readers.push(std::thread::spawn(move || {
                while let Ok(frame) = sys_rx.recv() {
                    Levels::set(&levels_sys.system, frame.level);
                    levels_sys.note_activity(frame.level);
                    if let Ok(mut m) = mixer_for_sys.lock() {
                        m.push_system(&frame.samples);
                    }
                }
            }));
        }

        // ---- Live insights engine (cadence + apply live in the DB)
        if let Some(cfg) = engine_cfg.clone() {
            self.engine = Some(LiveEngine::spawn(app.clone(), db.clone(), cfg));
        }
        self.engine_cfg = engine_cfg;

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
        if let Some((stop, _)) = &self.engine {
            stop.store(true, Ordering::SeqCst);
        }
        Some(StopParts {
            meeting: self.meeting.take()?,
            started_at: self.started_at.take(),
            dg_task: self.dg_task.take(),
            consumer_tasks: std::mem::take(&mut self.consumer_tasks),
            credential: self.credential.take(),
            engine: self.engine.take(),
            engine_cfg: self.engine_cfg.take(),
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
    /// Meetings whose final insights are still being generated.
    pub finishing: Finishing,
    /// Transcript activity for Smart meetings.
    pub activity: Arc<ActivityTrack>,
}

/// Personal dictionary + system terms as Deepgram keyterms (capped in the URL builder).
fn deepgram_keyterms(prefs: &Prefs) -> Vec<String> {
    let mut terms: Vec<String> = vec!["Miniti".into()];
    for t in &prefs.personal_dictionary {
        let t = t.trim();
        if !t.is_empty() && !terms.iter().any(|x| x.eq_ignore_ascii_case(t)) {
            terms.push(t.to_string());
        }
    }
    terms
}

impl AppState {
    /// Insights provider for the current mode, or a user-facing reason why not.
    fn insights_provider(&self, prefs: &Prefs) -> Result<Provider, String> {
        match prefs.app_mode {
            AppMode::Managed => self.api_client(prefs).map(Provider::Managed).map_err(|e| e.to_string()),
            AppMode::Byok => prefs
                .byok_openai_key
                .as_deref()
                .map(str::trim)
                .filter(|k| !k.is_empty())
                .map(|k| Provider::byok(k.to_string()))
                .ok_or_else(|| "Add your OpenAI API key in Settings (BYOK) for live insights.".to_string()),
        }
    }

    /// Engine config for a meeting; `None` when insights are unavailable in
    /// this mode (the reason is emitted once so the UI can say why).
    async fn engine_config(&self, app: &AppHandle, prefs: &Prefs, meeting_id: &str, live: bool) -> Option<EngineConfig> {
        let provider = match self.insights_provider(prefs) {
            Ok(p) => p,
            Err(reason) => {
                let _ = app.emit("insights_status", insights_engine::InsightsEvent {
                    meeting_id: meeting_id.to_string(),
                    mode: "standard".into(),
                    state: "error",
                    message: Some(reason),
                });
                return None;
            }
        };
        // Automatic Playbook lookups for BYOK and Pro; managed-free is metered → manual.
        let auto_docs_lookup = match &provider {
            Provider::Byok { .. } => true,
            Provider::Managed(client) => match client.get_usage().await {
                Ok(u) => u.tier.as_deref() == Some("pro"),
                Err(_) => false,
            },
        };
        Some(EngineConfig {
            meeting_id: meeting_id.to_string(),
            language: prefs.language.clone(),
            provider,
            docs_mcp_url: prefs.docs_mcp_url.as_deref().map(str::trim).filter(|u| !u.is_empty()).map(String::from),
            auto_docs_lookup,
            live_enabled: live && prefs.live_insights_enabled,
        })
    }

    fn engine_config_blocking(&self, prefs: &Prefs, meeting_id: &str) -> Result<EngineConfig, String> {
        let provider = self.insights_provider(prefs)?;
        let auto_docs_lookup = matches!(provider, Provider::Byok { .. });
        Ok(EngineConfig {
            meeting_id: meeting_id.to_string(),
            language: prefs.language.clone(),
            provider,
            docs_mcp_url: prefs.docs_mcp_url.as_deref().map(str::trim).filter(|u| !u.is_empty()).map(String::from),
            auto_docs_lookup,
            live_enabled: false,
        })
    }

    /// Background final pass + `meeting.updated` webhook afterwards.
    fn spawn_finalize(&self, app: AppHandle, prefs: &Prefs, cfg: EngineConfig) {
        let db = self.db.clone();
        let finishing = self.finishing.clone();
        let webhook_url = prefs.webhook_url.as_deref().map(str::trim).filter(|u| !u.is_empty()).map(String::from);
        let fillers = self.fillers(prefs);
        tauri::async_runtime::spawn(async move {
            let meeting_id = cfg.meeting_id.clone();
            insights_engine::finalize_meeting(app.clone(), db.clone(), cfg, finishing).await;
            let _ = app.emit("meeting_saved", &meeting_id);
            if let Some(url) = webhook_url {
                let payload = {
                    let Ok(conn) = db.lock() else { return };
                    let Ok(Some(meeting)) = db::get_meeting(&conn, &meeting_id) else { return };
                    let segments = db::list_segments(&conn, &meeting_id).unwrap_or_default();
                    crate::webhook::payload_from_meeting("meeting.updated", &meeting, &segments, &fillers)
                };
                crate::webhook::send(&url, &payload).await;
            }
        });
    }

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
        pcm_contract: "16 kHz PCM16 LE • mono mic, or stereo mic+system (multichannel)".to_string(),
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

/// Start a meeting (shared by the Home button, tray, calendar and call prompts).
pub async fn start_meeting(app: AppHandle, state: State<'_, AppState>, opts: StartOptions) -> Result<String, String> {
    let prefs = state.prefs_snapshot()?;
    if state.session.lock().map_err(|_| "session poisoned")?.running {
        return Err("already recording".into());
    }
    let credential = resolve_credential(&state, &prefs).await?;
    let engine_cfg = state.engine_config(&app, &prefs, "pending", true).await;
    let db = state.db.clone();
    let levels = state.levels.clone();
    let last_status = state.last_status.clone();
    let activity = state.activity.clone();
    let mut session = state.session.lock().map_err(|_| "session poisoned")?;
    if session.running {
        return Err("already recording".into());
    }
    let id = session.start(app.clone(), db.clone(), levels, last_status, &prefs, opts, credential, None, activity)?;
    if let Some(mut cfg) = engine_cfg {
        cfg.meeting_id = id.clone();
        session.engine = Some(LiveEngine::spawn(app, db, cfg.clone()));
        session.engine_cfg = Some(cfg);
    }
    Ok(id)
}

#[tauri::command]
pub async fn start_recording(
    app: AppHandle,
    state: State<'_, AppState>,
    title: Option<String>,
) -> Result<String, String> {
    start_meeting(app, state, StartOptions { title, ..Default::default() }).await
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
    if let Some((_, task)) = parts.engine {
        let _ = tokio::time::timeout(Duration::from_secs(6), task).await;
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

    // Final insights as meeting-scoped background work (macOS "Stopped session").
    if !segments.is_empty() {
        if let Some(mut cfg) = parts.engine_cfg {
            cfg.live_enabled = false;
            state.spawn_finalize(app.clone(), &prefs, cfg);
        }
    }
    Ok(Some(meeting.id))
}

// ---- Desktop shell glue -----------------------------------------------------

/// Tray "Start / Stop meeting": reuse the command paths with the managed state.
pub async fn toggle_recording_from_shell(app: AppHandle) {
    let state = app.state::<AppState>();
    let running = state.session.lock().map(|s| s.running).unwrap_or(false);
    let result = if running {
        stop_recording(app.clone(), state.clone()).await.map(|_| ())
    } else {
        start_meeting(app.clone(), state.clone(), StartOptions::default()).await.map(|id| {
            let _ = app.emit("navigate_meeting", &id);
        })
    };
    if let Err(e) = result {
        crate::shell::notify(&app, "Miniti", &e);
    }
}

/// One-second ticker: tray timer + floating surface visibility follow the
/// recording state (owned here so the shell never touches the session lock
/// from a UI callback).
pub fn spawn_shell_ticker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut was_recording = false;
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            let state = app.state::<AppState>();
            let (recording, elapsed) = state
                .session
                .lock()
                .map(|s| (s.running, s.started_at.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0)))
                .unwrap_or((false, 0.0));
            let stream = state.last_status.lock().ok().and_then(|s| s.clone());
            let stream_state = stream.as_ref().map(|s| match s {
                StreamStatus::Connecting => "connecting",
                StreamStatus::Connected { .. } => "connected",
                StreamStatus::Reconnecting { .. } => "reconnecting",
                StreamStatus::Ended => "ended",
                StreamStatus::Failed { .. } => "failed",
            });
            crate::shell::update_tray(&app, recording, elapsed, stream_state);
            if recording != was_recording {
                let surface = state.prefs.lock().map(|p| p.show_floating_indicator).unwrap_or(true);
                if recording && surface {
                    crate::shell::show_presence(&app);
                } else {
                    crate::shell::hide_presence(&app);
                }
                was_recording = recording;
            }
        }
    });
}

#[tauri::command]
pub fn notify(app: AppHandle, title: String, body: String) {
    crate::shell::notify(&app, &title, &body);
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    crate::shell::show_main(&app);
}

// ---- Smart meetings runtime ---------------------------------------------------

fn calendar_snapshot(app: &AppHandle) -> Vec<crate::integrations::CalendarEvent> {
    app.try_state::<crate::integrations::CalendarSlot>()
        .and_then(|c| c.lock().ok().map(|c| c.events.clone()))
        .unwrap_or_default()
}

async fn perform_smart_action(app: &AppHandle, action: crate::smart::Action) {
    use crate::smart::Action;
    let state = app.state::<AppState>();
    let result: Result<(), String> = match action {
        Action::Stop { reason } => {
            tracing::info!("smart: stopping ({reason})");
            let r = stop_recording(app.clone(), state.clone()).await.map(|_| ());
            if reason != "user" {
                crate::shell::notify(app, "Miniti", &format!("Recording saved ({reason})."));
            }
            r
        }
        Action::StartFromEvent(event) => start_from_event(app, &state, &event).await.map(|id| {
            let _ = app.emit("navigate_meeting", &id);
        }),
        Action::StartFromCall(_) => start_meeting(app.clone(), state.clone(), StartOptions::default()).await.map(|id| {
            let _ = app.emit("navigate_meeting", &id);
        }),
        Action::EndAndStartEvent(event) => {
            let stopped = stop_recording(app.clone(), state.clone()).await.map(|_| ());
            match stopped {
                Ok(()) => start_from_event(app, &state, &event).await.map(|id| {
                    let _ = app.emit("navigate_meeting", &id);
                }),
                Err(e) => Err(e),
            }
        }
        Action::EndAndStartNew => {
            let stopped = stop_recording(app.clone(), state.clone()).await.map(|_| ());
            match stopped {
                Ok(()) => start_meeting(app.clone(), state.clone(), StartOptions::default()).await.map(|id| {
                    let _ = app.emit("navigate_meeting", &id);
                }),
                Err(e) => Err(e),
            }
        }
    };
    if let Err(e) = result {
        tracing::warn!("smart action failed: {e}");
        crate::shell::notify(app, "Miniti", &e);
    }
}

async fn start_from_event(app: &AppHandle, state: &State<'_, AppState>, event: &crate::integrations::CalendarEvent) -> Result<String, String> {
    let notes = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        db::get_prep_notes(&conn, &event.id).unwrap_or_default()
    };
    start_meeting(
        app.clone(),
        state.clone(),
        StartOptions {
            title: Some(event.title.clone()).filter(|t| !t.trim().is_empty()),
            calendar_event_id: Some(event.id.clone()),
            attendees_json: Some(event.attendees_json()),
            notes: Some(notes),
        },
    )
    .await
}

/// 1 Hz Smart-meetings monitor + live guidance.
pub fn spawn_smart_monitor(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            let state = app.state::<AppState>();
            let Ok(prefs) = state.prefs_snapshot() else { continue };
            let (recording, elapsed, meeting_id, event_id) = state
                .session
                .lock()
                .map(|s| (
                    s.running,
                    s.started_at.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0),
                    s.meeting.as_ref().map(|m| m.id.clone()),
                    s.meeting.as_ref().and_then(|m| m.calendar_event_id.clone()),
                ))
                .unwrap_or((false, 0.0, None, None));
            let activity = crate::smart::Activity {
                recording,
                recording_duration: elapsed,
                transcript_gap: state.activity.transcript_gap(),
                audio_gap: state.levels.audio_gap(),
                meaningful_finals: state.activity.meaningful_finals.load(Ordering::Relaxed),
                now_unix: chrono::Utc::now().timestamp(),
            };
            let events = calendar_snapshot(&app);
            let sensor = if prefs.smart_meetings_enabled {
                tokio::task::spawn_blocking(crate::call_sensor::snapshot_capture_clients).await.ok().flatten()
            } else {
                None
            };
            let actions = {
                let Some(slot) = crate::smart::slot(&app) else { continue };
                let Ok(mut m) = slot.lock() else { continue };
                m.current_meeting = meeting_id.clone();
                m.tick(&app, &prefs, activity, event_id.as_deref(), &events, sensor, Instant::now())
            };
            for a in actions {
                perform_smart_action(&app, a).await;
            }
            if recording && prefs.live_guidance_enabled {
                if let Some(mid) = meeting_id {
                    evaluate_live_guidance(&app, &prefs, &mid, elapsed);
                }
            }
        }
    });
}

fn evaluate_live_guidance(app: &AppHandle, prefs: &Prefs, meeting_id: &str, elapsed: f64) {
    use crate::smart::YouSegment;
    let state = app.state::<AppState>();
    let fillers = state.fillers(prefs);
    let filler_tokens: Vec<Vec<String>> = fillers.iter().map(|f| coaching::tokenize(f)).filter(|t| !t.is_empty()).collect();
    let (meeting, segments) = {
        let Ok(conn) = state.db.lock() else { return };
        let Ok(Some(meeting)) = db::get_meeting(&conn, meeting_id) else { return };
        let Ok(segments) = db::list_segments_since(&conn, meeting_id, (elapsed - 400.0).max(0.0)) else { return };
        (meeting, segments)
    };
    let self_ids = meeting.self_speaker_ids().unwrap_or_else(|| vec![deepgram::MIC_SPEAKER_ID]);
    let you: Vec<(usize, YouSegment)> = segments
        .iter()
        .enumerate()
        .filter(|(_, s)| self_ids.contains(&s.speaker))
        .map(|(i, s)| {
            let toks = coaching::tokenize(&s.text);
            let fillers = filler_tokens.iter().map(|p| coaching::count_phrase_occurrences(p, &toks)).sum();
            (i, YouSegment { start: s.start_s, end: s.end_s, words: toks.len(), fillers })
        })
        .collect();
    // Trailing uninterrupted "You" run.
    let mut run: Vec<YouSegment> = Vec::new();
    for (i, s) in segments.iter().enumerate().rev() {
        if self_ids.contains(&s.speaker) {
            if let Some((_, y)) = you.iter().find(|(j, _)| *j == i) {
                run.push(y.clone());
            }
        } else {
            break;
        }
    }
    run.reverse();
    let recent: Vec<YouSegment> = you.iter().map(|(_, y)| y.clone()).collect();
    let high: Vec<String> = serde_json::from_str::<Vec<serde_json::Value>>(&meeting.suggested_questions)
        .unwrap_or_default()
        .into_iter()
        .filter(|q| q.get("priority").and_then(|p| p.as_str()) == Some("high"))
        .filter_map(|q| q.get("question").and_then(|s| s.as_str()).map(String::from))
        .collect();
    let transcript_end = segments.last().map(|s| s.end_s).unwrap_or(elapsed);
    if let Some(slot) = crate::smart::slot(app) {
        if let Ok(mut m) = slot.lock() {
            m.evaluate_nudges(app, prefs, &recent, &run, &high, transcript_end, Instant::now());
        }
    }
}

#[tauri::command]
pub async fn smart_decision(app: AppHandle, prompt_id: String, choice: String) -> Result<(), String> {
    let actions = {
        let slot = crate::smart::slot(&app).ok_or("smart monitor unavailable")?;
        let mut m = slot.lock().map_err(|_| "monitor poisoned")?;
        m.decide(&app, &prompt_id, &choice, Instant::now())
    };
    for a in actions {
        perform_smart_action(&app, a).await;
    }
    Ok(())
}

// ---- Calendar + CRM ---------------------------------------------------------------

async fn refresh_calendar(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(prefs) = state.prefs_snapshot() else { return };
    let Ok(client) = state.api_client(&prefs) else { return };
    let status = client.google_status().await;
    let slot = app.state::<crate::integrations::CalendarSlot>();
    match status {
        Ok(st) if st.connected => {
            let now = chrono::Utc::now();
            let min = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            let max = (now + chrono::Duration::days(7)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            match client.google_events(&min, &max, 50).await {
                Ok(ev) => {
                    if let Ok(mut c) = slot.lock() {
                        c.connected = true;
                        c.email = st.email;
                        c.events = ev.events;
                        c.fetched_at = Some(Instant::now());
                        c.last_error = None;
                    }
                    let _ = app.emit("calendar_updated", ());
                }
                Err(e) => {
                    if let Ok(mut c) = slot.lock() {
                        c.connected = true;
                        c.last_error = Some(e.to_string());
                    }
                }
            }
        }
        Ok(_) => {
            if let Ok(mut c) = slot.lock() {
                c.connected = false;
                c.email = None;
                c.events.clear();
                c.last_error = None;
            }
        }
        Err(e) => {
            if let Ok(mut c) = slot.lock() {
                c.last_error = Some(e.to_string());
            }
        }
    }
}

/// Refresh calendar status + events every 60 s (macOS cadence) when a backend key exists.
pub fn spawn_calendar_refresher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            if app.state::<AppState>().api_key.is_none() {
                continue;
            }
            refresh_calendar(&app).await;
            if let Ok(conn) = app.state::<AppState>().db.lock() {
                let _ = db::prune_prep_notes(&conn);
            }
        }
    });
}

#[derive(Serialize)]
pub struct CalendarView {
    pub connected: bool,
    pub email: Option<String>,
    pub events: Vec<crate::integrations::CalendarEvent>,
    pub upcoming: Vec<crate::integrations::CalendarEvent>,
    pub error: Option<String>,
    pub available: bool,
}

#[tauri::command]
pub async fn calendar_events(app: AppHandle, state: State<'_, AppState>, refresh: Option<bool>) -> Result<CalendarView, String> {
    if refresh.unwrap_or(false) && state.api_key.is_some() {
        refresh_calendar(&app).await;
    }
    let slot = app.state::<crate::integrations::CalendarSlot>();
    let c = slot.lock().map_err(|_| "calendar poisoned")?;
    let now = chrono::Utc::now().timestamp();
    Ok(CalendarView {
        connected: c.connected,
        email: c.email.clone(),
        events: c.events.clone(),
        upcoming: crate::integrations::upcoming(&c.events, now, 5),
        error: c.last_error.clone(),
        available: state.api_key.is_some(),
    })
}

#[tauri::command]
pub async fn google_status(state: State<'_, AppState>) -> Result<crate::integrations::GoogleStatus, String> {
    let prefs = state.prefs_snapshot()?;
    let client = state.api_client(&prefs).map_err(|e| e.to_string())?;
    client.google_status().await.map_err(|e| e.to_string())
}

/// Returns the authorize URL; the UI opens it in the browser. The return
/// arrives as a `miniti-google://oauth-callback` deep link.
#[tauri::command]
pub async fn google_connect(state: State<'_, AppState>) -> Result<String, String> {
    let prefs = state.prefs_snapshot()?;
    let client = state.api_client(&prefs).map_err(|e| e.to_string())?;
    client.google_connect_start().await.map(|c| c.auth_url).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn google_disconnect(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let prefs = state.prefs_snapshot()?;
    let client = state.api_client(&prefs).map_err(|e| e.to_string())?;
    client.google_disconnect().await.map_err(|e| e.to_string())?;
    refresh_calendar(&app).await;
    Ok(())
}

#[tauri::command]
pub fn get_prep_notes(state: State<AppState>, event_id: String) -> Result<String, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::get_prep_notes(&conn, &event_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_prep_notes(state: State<AppState>, event_id: String, notes: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_prep_notes(&conn, &event_id, &notes).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_meeting_from_event(app: AppHandle, state: State<'_, AppState>, event_id: String) -> Result<String, String> {
    let event = calendar_snapshot(&app).into_iter().find(|e| e.id == event_id).ok_or("event not found")?;
    if let Some(slot) = crate::smart::slot(&app) {
        if let Ok(mut m) = slot.lock() {
            m.clear_prompt(&app);
        }
    }
    start_from_event(&app, &state, &event).await
}

#[tauri::command]
pub async fn crm_status(state: State<'_, AppState>, provider: crate::integrations::CrmProvider) -> Result<crate::integrations::CrmStatus, String> {
    let prefs = state.prefs_snapshot()?;
    let client = state.api_client(&prefs).map_err(|e| e.to_string())?;
    client.crm_status(provider).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn crm_connect(state: State<'_, AppState>, provider: crate::integrations::CrmProvider) -> Result<String, String> {
    let prefs = state.prefs_snapshot()?;
    let client = state.api_client(&prefs).map_err(|e| e.to_string())?;
    client.crm_connect_start(provider).await.map(|c| c.auth_url).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn crm_search(state: State<'_, AppState>, provider: crate::integrations::CrmProvider, query: String) -> Result<Vec<crate::integrations::CrmRecord>, String> {
    let prefs = state.prefs_snapshot()?;
    let client = state.api_client(&prefs).map_err(|e| e.to_string())?;
    if query.trim().chars().count() < 2 {
        return Ok(Vec::new());
    }
    client.crm_search(provider, query.trim(), provider.objects()).await.map(|r| r.data).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct CrmPreview {
    pub payload: serde_json::Value,
    pub tasks: Vec<crate::integrations::CrmTask>,
}

#[tauri::command]
pub fn crm_preview(state: State<AppState>, meeting_id: String) -> Result<CrmPreview, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let meeting = db::get_meeting(&conn, &meeting_id).map_err(|e| e.to_string())?.ok_or("meeting not found")?;
    let segments = db::list_segments(&conn, &meeting_id).map_err(|e| e.to_string())?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    Ok(CrmPreview {
        payload: crate::integrations::crm_meeting_payload(&meeting, &segments),
        tasks: crate::integrations::default_tasks(&meeting, &today),
    })
}

#[tauri::command]
pub async fn crm_send(
    state: State<'_, AppState>,
    provider: crate::integrations::CrmProvider,
    meeting_id: String,
    target_object: String,
    target_record_id: String,
    tasks: Vec<crate::integrations::CrmTask>,
) -> Result<serde_json::Value, String> {
    let prefs = state.prefs_snapshot()?;
    let client = state.api_client(&prefs).map_err(|e| e.to_string())?;
    let payload = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        let meeting = db::get_meeting(&conn, &meeting_id).map_err(|e| e.to_string())?.ok_or("meeting not found")?;
        let segments = db::list_segments(&conn, &meeting_id).map_err(|e| e.to_string())?;
        crate::integrations::crm_meeting_payload(&meeting, &segments)
    };
    let body = serde_json::json!({
        "target_object": target_object,
        "target_record_id": target_record_id,
        "meeting": payload,
        "tasks": tasks,
    });
    client.crm_send(provider, &body).await.map_err(|e| e.to_string())
}

// ---- Insights commands -----------------------------------------------------

#[tauri::command]
pub fn insights_finishing(state: State<AppState>) -> Vec<String> {
    state.finishing.lock().map(|f| f.iter().cloned().collect()).unwrap_or_default()
}

#[tauri::command]
pub fn set_sales_enabled(state: State<AppState>, meeting_id: String, enabled: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_sales_enabled(&conn, &meeting_id, enabled).map_err(|e| e.to_string())
}

/// Re-run the final pass for a saved meeting (after trim, or on demand).
#[tauri::command]
pub fn regenerate_insights(app: AppHandle, state: State<AppState>, meeting_id: String) -> Result<(), String> {
    let prefs = state.prefs_snapshot()?;
    let cfg = state.engine_config_blocking(&prefs, &meeting_id)?;
    if state.finishing.lock().map(|f| f.contains(&meeting_id)).unwrap_or(false) {
        return Err("Insights are already being generated for this meeting.".into());
    }
    state.spawn_finalize(app, &prefs, cfg);
    Ok(())
}

#[tauri::command]
pub async fn catch_up(state: State<'_, AppState>, meeting_id: String) -> Result<serde_json::Value, String> {
    let prefs = state.prefs_snapshot()?;
    let cfg = state.engine_config_blocking(&prefs, &meeting_id)?;
    let (meeting, segments) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        let meeting = db::get_meeting(&conn, &meeting_id).map_err(|e| e.to_string())?.ok_or("meeting not found")?;
        let segments = db::list_segments(&conn, &meeting_id).map_err(|e| e.to_string())?;
        (meeting, segments)
    };
    insights_engine::catch_up(&cfg, &meeting, &segments).await
}

#[tauri::command]
pub async fn investigate(
    state: State<'_, AppState>,
    meeting_id: String,
    scope: InvestigationScope,
    focus: String,
) -> Result<serde_json::Value, String> {
    let prefs = state.prefs_snapshot()?;
    let cfg = state.engine_config_blocking(&prefs, &meeting_id)?;
    let (meeting, segments) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        let meeting = db::get_meeting(&conn, &meeting_id).map_err(|e| e.to_string())?.ok_or("meeting not found")?;
        let segments = db::list_segments(&conn, &meeting_id).map_err(|e| e.to_string())?;
        (meeting, segments)
    };
    let codebase = if scope == InvestigationScope::Codebase {
        let root = prefs.codebase_root.clone().ok_or("Choose a codebase folder in Settings first.")?;
        let focus_c = focus.clone();
        let (ctx, files) = tokio::task::spawn_blocking(move || insights_engine::build_codebase_snapshot(std::path::Path::new(&root), &focus_c))
            .await
            .map_err(|e| e.to_string())?;
        if ctx.is_empty() {
            return Err("No files in the codebase folder matched this question.".into());
        }
        Some((ctx, files))
    } else {
        None
    };
    let result = insights_engine::investigate(&cfg, &meeting, &segments, scope, &focus, codebase).await?;
    // Persist for the saved-meeting view.
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let mut list: Vec<serde_json::Value> = serde_json::from_str(&meeting.investigations).unwrap_or_default();
    list.push(serde_json::json!({
        "focus": focus,
        "scope": scope,
        "answer": result.get("answer").cloned().unwrap_or(serde_json::Value::String(String::new())),
        "sources": result.get("sources").cloned().unwrap_or(serde_json::Value::Array(vec![])),
        "referenced_files": result.get("referenced_files").cloned().unwrap_or(serde_json::Value::Array(vec![])),
        "at": chrono::Utc::now().timestamp(),
    }));
    let _ = db::set_investigations(&conn, &meeting_id, &serde_json::to_string(&list).unwrap_or_default());
    Ok(result)
}

/// Manual Playbook topic lookup (managed-free is metered by the backend).
#[tauri::command]
pub async fn lookup_doc_topic(app: AppHandle, state: State<'_, AppState>, meeting_id: String, label: String) -> Result<(), String> {
    let prefs = state.prefs_snapshot()?;
    let cfg = state.engine_config_blocking(&prefs, &meeting_id)?;
    if cfg.docs_mcp_url.is_none() {
        return Err("Set a Docs MCP URL in Settings first.".into());
    }
    let db = state.db.clone();
    insights_engine::lookup_topic(app, db, cfg, label).await;
    Ok(())
}

#[tauri::command]
pub async fn probe_docs_mcp(url: String) -> Result<serde_json::Value, String> {
    let (search_tool, tools) = crate::insights::mcp::probe(&url).await?;
    Ok(serde_json::json!({ "search_tool": search_tool, "tools": tools }))
}

// ---- Export / import / trim -----------------------------------------------------

#[tauri::command]
pub fn meeting_markdown(state: State<AppState>, meeting_id: String) -> Result<String, String> {
    let prefs = state.prefs_snapshot()?;
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let meeting = db::get_meeting(&conn, &meeting_id).map_err(|e| e.to_string())?.ok_or("meeting not found")?;
    let segments = db::list_segments(&conn, &meeting_id).map_err(|e| e.to_string())?;
    Ok(crate::export::full_meeting_markdown(&meeting, &segments, &state.fillers(&prefs)))
}

/// Save the full meeting as Markdown via the native save dialog. Returns the
/// written path, or None if the user cancelled.
#[tauri::command]
pub async fn export_markdown(app: AppHandle, state: State<'_, AppState>, meeting_id: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let prefs = state.prefs_snapshot()?;
    let (markdown, file_name) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        let meeting = db::get_meeting(&conn, &meeting_id).map_err(|e| e.to_string())?.ok_or("meeting not found")?;
        let segments = db::list_segments(&conn, &meeting_id).map_err(|e| e.to_string())?;
        (
            crate::export::full_meeting_markdown(&meeting, &segments, &state.fillers(&prefs)),
            crate::export::export_file_name(&meeting),
        )
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app.dialog().file().set_file_name(&file_name).add_filter("Markdown", &["md"]);
    if let Some(dir) = prefs.export_folder.as_deref().filter(|d| !d.is_empty()) {
        dialog = dialog.set_directory(dir);
    }
    dialog.save_file(move |p| {
        let _ = tx.send(p.map(|p| p.to_string()));
    });
    let Some(path) = rx.await.map_err(|_| "dialog closed".to_string())? else {
        return Ok(None);
    };
    tokio::fs::write(&path, markdown).await.map_err(|e| format!("could not write {path}: {e}"))?;
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let mut p = state.prefs.lock().map_err(|_| "prefs poisoned")?;
        p.export_folder = Some(parent.to_string_lossy().to_string());
        let _ = p.save();
    }
    Ok(Some(path))
}

/// Pick a Granola CSV export and import it with duplicate protection.
#[tauri::command]
pub async fn import_granola_csv(app: AppHandle, state: State<'_, AppState>) -> Result<Option<crate::import::granola::ImportSummary>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().add_filter("CSV", &["csv"]).pick_file(move |p| {
        let _ = tx.send(p.map(|p| p.to_string()));
    });
    let Some(path) = rx.await.map_err(|_| "dialog closed".to_string())? else {
        return Ok(None);
    };
    let bytes = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
    let text = String::from_utf8(bytes).map_err(|_| "The Granola export could not be read as a UTF-8 CSV file.".to_string())?;
    let parsed = crate::import::granola::parse(&text)?;
    let prefs = state.prefs_snapshot()?;
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let existing: std::collections::HashSet<String> = db::find_by_import_source(&conn, crate::import::granola::SOURCE)
        .map_err(|e| e.to_string())?
        .into_iter()
        .collect();
    let mut imported = 0;
    let mut duplicates = 0;
    for rec in &parsed.meetings {
        let (meeting, segments) = crate::import::granola::make_meeting(rec, &prefs.language);
        if existing.contains(meeting.import_source.as_deref().unwrap_or_default()) {
            duplicates += 1;
            continue;
        }
        db::upsert_meeting(&conn, &meeting).map_err(|e| e.to_string())?;
        db::replace_segments(&conn, &meeting.id, &segments).map_err(|e| e.to_string())?;
        imported += 1;
    }
    let summary = crate::import::granola::ImportSummary { imported, duplicates, skipped_rows: parsed.skipped_rows };
    let _ = app.emit("meeting_saved", "import");
    Ok(Some(summary))
}

#[tauri::command]
pub fn delete_segment(state: State<AppState>, meeting_id: String, segment_id: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::delete_segment(&conn, &meeting_id, &segment_id).map(|_| ()).map_err(|e| e.to_string())
}

/// Trim a saved transcript: drop everything before `before_s` and/or after `after_s`.
#[tauri::command]
pub fn trim_transcript(state: State<AppState>, meeting_id: String, before_s: Option<f64>, after_s: Option<f64>) -> Result<usize, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let mut removed = 0;
    if let Some(b) = before_s {
        removed += db::trim_segments_before(&conn, &meeting_id, b).map_err(|e| e.to_string())?;
    }
    if let Some(a) = after_s {
        removed += db::trim_segments_after(&conn, &meeting_id, a).map_err(|e| e.to_string())?;
    }
    Ok(removed)
}

/// Folder picker for codebase investigations (native dialog).
#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |p| {
        let _ = tx.send(p.map(|p| p.to_string()));
    });
    rx.await.map_err(|_| "dialog closed".to_string())
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
    let mut manual: Vec<i64> = serde_json::from_str(&meeting.manual_speaker_ids).unwrap_or_default();
    if !manual.contains(&speaker_id) {
        manual.push(speaker_id);
        let _ = db::set_manual_speaker_ids(&conn, &meeting_id, &serde_json::to_string(&manual).unwrap_or_default());
    }
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
