//! Live insights engine — a port of the cadence, apply-safety and background
//! behaviour in the macOS `AppState` insights code:
//!
//! - Standard: first pass after 3 finals, then every ≥4 new finals and ≥60 s
//!   (or ≥120 s with any new content). Questions and MEDDPICC run in the
//!   background with their own thresholds, staggered after standard.
//! - Speaker naming: after 8 finals, then rarely (≥40 new finals, ≥120 s).
//! - Docs topics (Playbook): every ≥20 s while a Docs MCP URL is set.
//! - Incremental transport: delta since the last acked segment + a recent
//!   window + the rolling state; a degraded response never advances the ack.
//! - Stop: a background "final" pass (non-incremental) so the user can start
//!   another meeting while this one keeps updating in History.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::coaching;
use crate::db::{self, Meeting, TranscriptSegment};

use super::provider::Provider;
use super::{
    build_request, validate, Attendee, IncrementalPayload, InsightMode, InsightRequest,
    InvestigationScope,
};

const TICK: Duration = Duration::from_secs(5);
const RECENT_WINDOW_CHARS: usize = 10_000;
const FIRST_INSIGHT_THRESHOLD: usize = 3;
const FIRST_QUESTIONS_THRESHOLD: usize = 6;
const FIRST_MEDDPICC_THRESHOLD: usize = 6;
const FIRST_SPEAKER_NAMES_THRESHOLD: usize = 8;
const SPEAKER_NAMES_UPDATE_THRESHOLD: usize = 40;
const SPEAKER_NAMES_MIN_INTERVAL: Duration = Duration::from_secs(120);
const TITLE_UPDATE_THRESHOLD: usize = 15;
const WARMUP_RETRY_INTERVAL: Duration = Duration::from_secs(30);
const MEDDPICC_STAGGER: Duration = Duration::from_secs(15);
const QUESTIONS_STAGGER: Duration = Duration::from_secs(22);
const DOCS_TOPICS_MIN_INTERVAL: Duration = Duration::from_secs(20);
const DOCS_MIN_TRANSCRIPT_CHARS: usize = 40;
const MAX_CONCURRENT_DOCS_LOOKUPS: usize = 2;
pub const CATCHUP_RECENT_WINDOW_SECS: f64 = 180.0;
pub const CATCHUP_MIN_SEGMENTS: usize = 2;
pub const CATCHUP_FALLBACK_WINDOW_SEGMENTS: usize = 8;

#[derive(Clone, Copy)]
struct CadencePolicy {
    minimum_interval: Duration,
    minimum_segment_delta: usize,
    maximum_interval: Duration,
}

const STANDARD_POLICY: CadencePolicy = CadencePolicy {
    minimum_interval: Duration::from_secs(60),
    minimum_segment_delta: 4,
    maximum_interval: Duration::from_secs(120),
};
const MEDDPICC_POLICY: CadencePolicy = CadencePolicy {
    minimum_interval: Duration::from_secs(90),
    minimum_segment_delta: 8,
    maximum_interval: Duration::from_secs(180),
};
const QUESTIONS_POLICY: CadencePolicy = CadencePolicy {
    minimum_interval: Duration::from_secs(60),
    minimum_segment_delta: 6,
    maximum_interval: Duration::from_secs(120),
};

/// Per-mode tracking (port of the `*CadenceAnchor` / `*AckedSegmentCount` fields).
#[derive(Default)]
struct ModeState {
    anchor: Option<Instant>,
    last_attempt: Option<Instant>,
    last_fired_segments: usize,
    acked_segments: usize,
    rolling_state: Value,
    request_seq: i64,
    last_applied_seq: i64,
    success_count: u32,
    in_flight: Arc<AtomicBool>,
    last_summary: String,
}

impl ModeState {
    fn new() -> Self {
        Self {
            last_applied_seq: -1,
            rolling_state: Value::Null,
            ..Default::default()
        }
    }

    /// Port of the cadence decision: first pass on threshold, then either the
    /// minimum interval with enough new finals, or the maximum interval with
    /// any new final. A warm-up retry covers the very first success.
    fn due(
        &self,
        policy: CadencePolicy,
        first_threshold: usize,
        segments: usize,
        now: Instant,
    ) -> bool {
        if self.in_flight.load(Ordering::Relaxed) || segments == 0 {
            return false;
        }
        if self.success_count == 0 {
            if segments < first_threshold {
                return false;
            }
            return self
                .last_attempt
                .map(|t| now.duration_since(t) >= WARMUP_RETRY_INTERVAL)
                .unwrap_or(true);
        }
        let delta = segments.saturating_sub(self.last_fired_segments);
        if delta == 0 {
            return false;
        }
        let since = self
            .anchor
            .map(|a| now.duration_since(a))
            .unwrap_or(Duration::MAX);
        (since >= policy.minimum_interval && delta >= policy.minimum_segment_delta)
            || since >= policy.maximum_interval
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InsightsEvent {
    pub meeting_id: String,
    pub mode: String,
    /// "running" | "applied" | "degraded" | "error" | "finished"
    pub state: &'static str,
    pub message: Option<String>,
}

fn emit(
    app: &AppHandle,
    meeting_id: &str,
    mode: &str,
    state: &'static str,
    message: Option<String>,
) {
    let _ = app.emit(
        "insights_status",
        InsightsEvent {
            meeting_id: meeting_id.into(),
            mode: mode.into(),
            state,
            message,
        },
    );
    if state == "applied" || state == "finished" {
        let _ = app.emit("insights_updated", meeting_id);
    }
}

/// Everything the engine needs for one meeting.
#[derive(Clone)]
pub struct EngineConfig {
    pub meeting_id: String,
    pub language: String,
    pub provider: Provider,
    pub docs_mcp_url: Option<String>,
    /// Automatic topic lookups (Pro / BYOK); managed-free looks up on demand.
    pub auto_docs_lookup: bool,
    pub live_enabled: bool,
}

/// Shared "insights still running" registry so the sidebar can show a
/// `finishing insights…` label after stop.
pub type Finishing = Arc<Mutex<HashSet<String>>>;

// ---- Transcript text (ports of transcriptText / transcriptTextWithSpeakerIDs)

pub fn labels_for(meeting: &Meeting, segments: &[TranscriptSegment]) -> HashMap<i64, String> {
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
                id,
                coaching::resolved_speaker_label(id, names_opt, self_ids.as_deref()),
            )
        })
        .collect()
}

pub fn transcript_text(segments: &[TranscriptSegment], labels: &HashMap<i64, String>) -> String {
    segments
        .iter()
        .map(|s| {
            format!(
                "[{}] {}",
                labels
                    .get(&s.speaker)
                    .cloned()
                    .unwrap_or_else(|| coaching::resolved_speaker_label(s.speaker, None, None)),
                s.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn transcript_with_speaker_ids(segments: &[TranscriptSegment]) -> String {
    segments
        .iter()
        .map(|s| format!("[SpeakerID:{}] {}", s.speaker, s.text))
        .collect::<Vec<_>>()
        .join("\n")
}

fn tail_chars(s: &str, n: usize) -> String {
    let count = s.chars().count();
    s.chars().skip(count.saturating_sub(n)).collect()
}

fn attendees_of(meeting: &Meeting) -> Vec<Attendee> {
    let raw: Vec<Value> = serde_json::from_str(&meeting.attendees).unwrap_or_default();
    raw.into_iter()
        .filter_map(|a| {
            let email = a
                .get("email")
                .and_then(|e| e.as_str())
                .unwrap_or_default()
                .to_string();
            let name = a
                .get("name")
                .or(a.get("displayName"))
                .and_then(|n| n.as_str())
                .map(String::from)
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| email.split('@').next().unwrap_or_default().to_string());
            let domain = a
                .get("domain")
                .and_then(|d| d.as_str())
                .map(String::from)
                .unwrap_or_else(|| email.split('@').nth(1).unwrap_or_default().to_string());
            if name.is_empty() && domain.is_empty() {
                None
            } else {
                Some(Attendee {
                    name,
                    domain,
                    role: None,
                })
            }
        })
        .collect()
}

fn incremental_payload(
    state: &ModeState,
    segments: &[TranscriptSegment],
    labels: &HashMap<i64, String>,
    full: &str,
) -> Option<IncrementalPayload> {
    if state.success_count == 0 || state.rolling_state.is_null() {
        return None;
    }
    let acked = state.acked_segments.min(segments.len());
    let delta_segments = &segments[acked..];
    let recent = tail_chars(full, RECENT_WINDOW_CHARS);
    let recent_count = segments
        .iter()
        .rev()
        .scan(0usize, |acc, s| {
            *acc += s.text.chars().count() + 1;
            Some(*acc)
        })
        .take_while(|n| *n <= RECENT_WINDOW_CHARS)
        .count()
        .max(1);
    Some(IncrementalPayload {
        strategy: IncrementalPayload::STRATEGY.into(),
        full_segment_count: segments.len(),
        acked_segment_count: acked,
        delta_segment_count: delta_segments.len(),
        recent_segment_count: recent_count,
        transcript_delta: transcript_text(delta_segments, labels),
        recent_transcript: recent,
        rolling_state: state.rolling_state.clone(),
    })
}

fn string_array(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn opt_string(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|s| s.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.to_lowercase() != "null")
}

/// Strip leading bullet characters the model may add despite instructions.
fn strip_bullets(s: &str) -> String {
    s.lines()
        .map(|l| l.trim().trim_start_matches(['-', '•', '*']).trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub const MEDDPICC_KEYS: [&str; 8] = [
    "metrics",
    "economic_buyer",
    "decision_criteria",
    "decision_process",
    "paper_process",
    "identified_pain",
    "champion",
    "competition",
];

// ---- Apply (port of applyInsights per mode) ------------------------------------

fn apply_standard(
    conn: &Connection,
    meeting_id: &str,
    v: &Value,
    state: &mut ModeState,
    segments: usize,
    update_title: bool,
) -> Result<(), String> {
    let summary = opt_string(v, "summary").unwrap_or_default();
    let action_items = string_array(v, "action_items");
    let topics = string_array(v, "topics");
    let flow = string_array(v, "discussion_flow");
    let decisions = string_array(v, "key_decisions");
    let decisions_json = serde_json::to_string(&decisions).unwrap_or_default();
    db::set_standard_insights(
        conn,
        meeting_id,
        &summary,
        &serde_json::to_string(&action_items).unwrap_or_default(),
        &serde_json::to_string(&topics).unwrap_or_default(),
        &serde_json::to_string(&flow).unwrap_or_default(),
        if decisions.is_empty() {
            None
        } else {
            Some(decisions_json.as_str())
        },
    )
    .map_err(|e| e.to_string())?;
    let mut suggested_title = opt_string(v, "title");
    if update_title {
        if let Some(t) = &suggested_title {
            let _ = db::set_auto_title(conn, meeting_id, t);
        }
    } else {
        suggested_title = state
            .rolling_state
            .get("suggested_title")
            .and_then(|t| t.as_str())
            .map(String::from);
    }
    state.last_summary = summary.clone();
    state.rolling_state = json!({
        "summary": summary,
        "discussion_flow": flow,
        "action_items": action_items,
        "topics": topics,
        "suggested_title": suggested_title,
    });
    state.acked_segments = segments;
    Ok(())
}

fn apply_meddpicc(
    conn: &Connection,
    meeting_id: &str,
    v: &Value,
    state: &mut ModeState,
    segments: usize,
) -> Result<(), String> {
    let mut fields = serde_json::Map::new();
    for k in MEDDPICC_KEYS {
        fields.insert(
            k.into(),
            opt_string(v, k)
                .map(|s| Value::String(strip_bullets(&s)))
                .unwrap_or(Value::Null),
        );
    }
    db::set_meddpicc(conn, meeting_id, &Value::Object(fields.clone()).to_string())
        .map_err(|e| e.to_string())?;
    state.last_summary = opt_string(v, "summary").unwrap_or_default();
    state.rolling_state = json!({
        "summary": state.last_summary,
        "discussion_flow": string_array(v, "discussion_flow"),
        "action_items": string_array(v, "action_items"),
        "topics": string_array(v, "topics"),
        "meddpicc": Value::Object(fields),
    });
    state.acked_segments = segments;
    Ok(())
}

fn apply_questions(
    conn: &Connection,
    meeting_id: &str,
    v: &Value,
    state: &mut ModeState,
    segments: usize,
) -> Result<(), String> {
    let questions: Vec<Value> = v
        .get("questions")
        .and_then(|q| q.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|q| {
            q.get("question")
                .and_then(|s| s.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
        })
        .collect();
    db::set_questions(
        conn,
        meeting_id,
        &Value::Array(questions.clone()).to_string(),
    )
    .map_err(|e| e.to_string())?;
    state.rolling_state = json!({ "questions": questions });
    state.acked_segments = segments;
    Ok(())
}

/// Merge inferred names, never overwriting manual renames (port of the
/// inferred/override split).
fn apply_speaker_names(conn: &Connection, meeting: &Meeting, v: &Value) -> Result<bool, String> {
    let inferred = v
        .get("speakers")
        .and_then(|s| s.as_object())
        .cloned()
        .unwrap_or_default();
    if inferred.is_empty() {
        return Ok(false);
    }
    let manual: Vec<i64> = serde_json::from_str(&meeting.manual_speaker_ids).unwrap_or_default();
    let mut names: HashMap<String, String> =
        serde_json::from_str(&meeting.speaker_names).unwrap_or_default();
    let mut changed = false;
    for (id, name) in inferred {
        let Some(name) = name.as_str().map(str::trim).filter(|n| !n.is_empty()) else {
            continue;
        };
        let Ok(id_num) = id.parse::<i64>() else {
            continue;
        };
        if manual.contains(&id_num) {
            continue;
        }
        let lower = name.to_lowercase();
        if lower == "unknown" || lower.starts_with("speaker") || lower.starts_with("person") {
            continue;
        }
        if names.get(&id).map(|n| n != name).unwrap_or(true) {
            names.insert(id, name.to_string());
            changed = true;
        }
    }
    if changed {
        db::set_speaker_names(
            conn,
            &meeting.id,
            &serde_json::to_string(&names).unwrap_or_default(),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(changed)
}

/// Playbook topic entry (port of `DocTopic`).
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct DocTopic {
    pub id: String,
    pub label: String,
    /// pending | looking_up | answered | no_match | busy | failed
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn topic_slug(raw: &str) -> String {
    raw.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn merge_doc_topics(existing: &[DocTopic], labels: &[String]) -> Vec<DocTopic> {
    let mut out = existing.to_vec();
    for label in labels {
        let id = topic_slug(label);
        if id.is_empty() || out.iter().any(|t| t.id == id) {
            continue;
        }
        out.push(DocTopic {
            id,
            label: label.trim().to_string(),
            state: "pending".into(),
            error: None,
        });
    }
    out
}

// ---- The engine ------------------------------------------------------------------

pub struct LiveEngine {
    app: AppHandle,
    db: Arc<Mutex<Connection>>,
    cfg: EngineConfig,
    standard: ModeState,
    meddpicc: ModeState,
    questions: ModeState,
    speaker_names: ModeState,
    docs_topics: ModeState,
    docs_in_flight: Arc<Mutex<HashSet<String>>>,
    last_title_update_count: usize,
    stop: Arc<AtomicBool>,
}

struct Snapshot {
    meeting: Meeting,
    segments: Vec<TranscriptSegment>,
    labels: HashMap<i64, String>,
    transcript: String,
}

fn load_snapshot(db: &Arc<Mutex<Connection>>, meeting_id: &str) -> Option<Snapshot> {
    let conn = db.lock().ok()?;
    let meeting = db::get_meeting(&conn, meeting_id).ok().flatten()?;
    let segments = db::list_segments(&conn, meeting_id).ok()?;
    let labels = labels_for(&meeting, &segments);
    let transcript = transcript_text(&segments, &labels);
    Some(Snapshot {
        meeting,
        segments,
        labels,
        transcript,
    })
}

impl LiveEngine {
    pub fn spawn(
        app: AppHandle,
        db: Arc<Mutex<Connection>>,
        cfg: EngineConfig,
    ) -> (Arc<AtomicBool>, tauri::async_runtime::JoinHandle<()>) {
        let stop = Arc::new(AtomicBool::new(false));
        let mut engine = LiveEngine {
            app,
            db,
            cfg,
            standard: ModeState::new(),
            meddpicc: ModeState::new(),
            questions: ModeState::new(),
            speaker_names: ModeState::new(),
            docs_topics: ModeState::new(),
            docs_in_flight: Arc::new(Mutex::new(HashSet::new())),
            last_title_update_count: 0,
            stop: stop.clone(),
        };
        let handle = tauri::async_runtime::spawn(async move { engine.run().await });
        (stop, handle)
    }

    async fn run(&mut self) {
        let mut ticker = tokio::time::interval(TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            if self.stop.load(Ordering::Relaxed) {
                break;
            }
            self.tick().await;
        }
    }

    async fn tick(&mut self) {
        let Some(snap) = load_snapshot(&self.db, &self.cfg.meeting_id) else {
            return;
        };
        let now = Instant::now();
        let n = snap.segments.len();
        if n == 0 {
            return;
        }
        if self.cfg.live_enabled {
            if self
                .standard
                .due(STANDARD_POLICY, FIRST_INSIGHT_THRESHOLD, n, now)
            {
                self.standard.last_attempt = Some(now);
                self.standard.last_fired_segments = n;
                self.run_standard(&snap, false).await;
            }
            let standard_anchor = self.standard.anchor;
            let after = |stagger: Duration| {
                standard_anchor
                    .map(|a| now.duration_since(a) >= stagger)
                    .unwrap_or(true)
            };

            if snap.meeting.sales_enabled
                && self
                    .meddpicc
                    .due(MEDDPICC_POLICY, FIRST_MEDDPICC_THRESHOLD, n, now)
                && (self.meddpicc.success_count == 0 || after(MEDDPICC_STAGGER))
            {
                self.meddpicc.last_attempt = Some(now);
                self.meddpicc.last_fired_segments = n;
                self.run_meddpicc(&snap, false).await;
            }
            if self
                .questions
                .due(QUESTIONS_POLICY, FIRST_QUESTIONS_THRESHOLD, n, now)
                && (self.questions.success_count == 0 || after(QUESTIONS_STAGGER))
            {
                self.questions.last_attempt = Some(now);
                self.questions.last_fired_segments = n;
                self.run_questions(&snap, false).await;
            }
            let names_due = !self.speaker_names.in_flight.load(Ordering::Relaxed)
                && n >= FIRST_SPEAKER_NAMES_THRESHOLD
                && n >= self.speaker_names.last_fired_segments
                    + if self.speaker_names.last_fired_segments == 0 {
                        0
                    } else {
                        SPEAKER_NAMES_UPDATE_THRESHOLD
                    }
                && self
                    .speaker_names
                    .last_attempt
                    .map(|t| now.duration_since(t) >= SPEAKER_NAMES_MIN_INTERVAL)
                    .unwrap_or(true);
            if names_due {
                self.speaker_names.last_attempt = Some(now);
                self.speaker_names.last_fired_segments = n;
                self.run_speaker_names(&snap).await;
            }
        }
        if self.cfg.docs_mcp_url.is_some() {
            let due = !self.docs_topics.in_flight.load(Ordering::Relaxed)
                && snap.transcript.chars().count() >= DOCS_MIN_TRANSCRIPT_CHARS
                && n > self.docs_topics.last_fired_segments
                && self
                    .docs_topics
                    .last_attempt
                    .map(|t| now.duration_since(t) >= DOCS_TOPICS_MIN_INTERVAL)
                    .unwrap_or(true);
            if due {
                self.docs_topics.last_attempt = Some(now);
                self.docs_topics.last_fired_segments = n;
                self.run_docs_topics(&snap).await;
            }
            if self.cfg.auto_docs_lookup {
                self.auto_lookup_topics(&snap).await;
            }
        }
    }

    fn should_update_title(&mut self, n: usize) -> bool {
        if n >= self.last_title_update_count + TITLE_UPDATE_THRESHOLD {
            self.last_title_update_count = n;
            true
        } else {
            false
        }
    }

    async fn run_standard(&mut self, snap: &Snapshot, final_pass: bool) {
        let update_title = final_pass || self.should_update_title(snap.segments.len());
        let incremental = if final_pass {
            None
        } else {
            incremental_payload(
                &self.standard,
                &snap.segments,
                &snap.labels,
                &snap.transcript,
            )
        };
        self.standard.request_seq += 1;
        let mut req = build_request(
            InsightMode::Standard,
            self.cfg.provider.app_mode(),
            &snap.transcript,
            &self.cfg.language,
            &attendees_of(&snap.meeting),
            incremental,
            Some(self.standard.request_seq),
        );
        req.existing_summary = Some(self.standard.last_summary.clone())
            .filter(|s| !s.is_empty())
            .or_else(|| Some(snap.meeting.summary.clone()).filter(|s| !s.is_empty()));
        if !update_title || !snap.meeting.title_auto {
            req.existing_title = Some(snap.meeting.display_title());
        }
        let n = snap.segments.len();
        if let Some(v) = run_mode(
            &self.app,
            &self.cfg,
            &mut self.standard,
            "standard",
            req,
            InsightMode::Standard,
        )
        .await
        {
            if let Ok(conn) = self.db.lock() {
                if let Err(e) = apply_standard(
                    &conn,
                    &self.cfg.meeting_id,
                    &v,
                    &mut self.standard,
                    n,
                    update_title && snap.meeting.title_auto,
                ) {
                    emit(
                        &self.app,
                        &self.cfg.meeting_id,
                        "standard",
                        "error",
                        Some(e),
                    );
                    return;
                }
            }
            emit(&self.app, &self.cfg.meeting_id, "standard", "applied", None);
        }
    }

    async fn run_meddpicc(&mut self, snap: &Snapshot, final_pass: bool) {
        let incremental = if final_pass {
            None
        } else {
            incremental_payload(
                &self.meddpicc,
                &snap.segments,
                &snap.labels,
                &snap.transcript,
            )
        };
        self.meddpicc.request_seq += 1;
        let mut req = build_request(
            InsightMode::Meddpicc,
            self.cfg.provider.app_mode(),
            &snap.transcript,
            &self.cfg.language,
            &attendees_of(&snap.meeting),
            incremental,
            Some(self.meddpicc.request_seq),
        );
        req.existing_summary = Some(self.meddpicc.last_summary.clone()).filter(|s| !s.is_empty());
        req.existing_title = Some(snap.meeting.display_title());
        let n = snap.segments.len();
        if let Some(v) = run_mode(
            &self.app,
            &self.cfg,
            &mut self.meddpicc,
            "meddpicc",
            req,
            InsightMode::Meddpicc,
        )
        .await
        {
            if let Ok(conn) = self.db.lock() {
                if let Err(e) =
                    apply_meddpicc(&conn, &self.cfg.meeting_id, &v, &mut self.meddpicc, n)
                {
                    emit(
                        &self.app,
                        &self.cfg.meeting_id,
                        "meddpicc",
                        "error",
                        Some(e),
                    );
                    return;
                }
            }
            emit(&self.app, &self.cfg.meeting_id, "meddpicc", "applied", None);
        }
    }

    async fn run_questions(&mut self, snap: &Snapshot, final_pass: bool) {
        let incremental = if final_pass {
            None
        } else {
            incremental_payload(
                &self.questions,
                &snap.segments,
                &snap.labels,
                &snap.transcript,
            )
        };
        self.questions.request_seq += 1;
        let mut req = build_request(
            InsightMode::Questions,
            self.cfg.provider.app_mode(),
            &snap.transcript,
            &self.cfg.language,
            &attendees_of(&snap.meeting),
            incremental,
            Some(self.questions.request_seq),
        );
        req.existing_title = Some(snap.meeting.display_title());
        let n = snap.segments.len();
        if let Some(v) = run_mode(
            &self.app,
            &self.cfg,
            &mut self.questions,
            "questions",
            req,
            InsightMode::Questions,
        )
        .await
        {
            if let Ok(conn) = self.db.lock() {
                if let Err(e) =
                    apply_questions(&conn, &self.cfg.meeting_id, &v, &mut self.questions, n)
                {
                    emit(
                        &self.app,
                        &self.cfg.meeting_id,
                        "questions",
                        "error",
                        Some(e),
                    );
                    return;
                }
            }
            emit(
                &self.app,
                &self.cfg.meeting_id,
                "questions",
                "applied",
                None,
            );
        }
    }

    async fn run_speaker_names(&mut self, snap: &Snapshot) {
        let transcript = transcript_with_speaker_ids(&snap.segments);
        let candidates: Vec<String> = attendees_of(&snap.meeting)
            .into_iter()
            .map(|a| a.name)
            .filter(|n| !n.is_empty())
            .collect();
        let mut req = build_request(
            InsightMode::SpeakerNames,
            self.cfg.provider.app_mode(),
            &transcript,
            &self.cfg.language,
            &[],
            None,
            None,
        );
        req.candidates = candidates;
        if let Some(v) = run_mode(
            &self.app,
            &self.cfg,
            &mut self.speaker_names,
            "speaker_names",
            req,
            InsightMode::SpeakerNames,
        )
        .await
        {
            let changed = match self.db.lock() {
                Ok(conn) => apply_speaker_names(&conn, &snap.meeting, &v).unwrap_or(false),
                Err(_) => false,
            };
            if changed {
                emit(
                    &self.app,
                    &self.cfg.meeting_id,
                    "speaker_names",
                    "applied",
                    None,
                );
            }
        }
    }

    async fn run_docs_topics(&mut self, snap: &Snapshot) {
        let req = build_request(
            InsightMode::DocsTopics,
            self.cfg.provider.app_mode(),
            &snap.transcript,
            &self.cfg.language,
            &[],
            None,
            None,
        );
        if let Some(v) = run_mode(
            &self.app,
            &self.cfg,
            &mut self.docs_topics,
            "docs_topics",
            req,
            InsightMode::DocsTopics,
        )
        .await
        {
            let labels = string_array(&v, "topics");
            if labels.is_empty() {
                return;
            }
            let changed = {
                let Ok(conn) = self.db.lock() else { return };
                let existing: Vec<DocTopic> =
                    serde_json::from_str(&snap.meeting.doc_topics).unwrap_or_default();
                let merged = merge_doc_topics(&existing, &labels);
                if merged.len() != existing.len() {
                    let _ = db::set_docs(
                        &conn,
                        &self.cfg.meeting_id,
                        &snap.meeting.docs,
                        &serde_json::to_string(&merged).unwrap_or_default(),
                    );
                    true
                } else {
                    false
                }
            };
            if changed {
                emit(
                    &self.app,
                    &self.cfg.meeting_id,
                    "docs_topics",
                    "applied",
                    None,
                );
            }
        }
    }

    async fn auto_lookup_topics(&mut self, snap: &Snapshot) {
        let topics: Vec<DocTopic> =
            serde_json::from_str(&snap.meeting.doc_topics).unwrap_or_default();
        let in_flight = self.docs_in_flight.lock().map(|s| s.len()).unwrap_or(0);
        if in_flight >= MAX_CONCURRENT_DOCS_LOOKUPS {
            return;
        }
        for t in topics
            .iter()
            .filter(|t| t.state == "pending" || t.state == "busy")
        {
            let already = self
                .docs_in_flight
                .lock()
                .map(|s| s.contains(&t.id))
                .unwrap_or(true);
            if already {
                continue;
            }
            if let Ok(mut s) = self.docs_in_flight.lock() {
                if s.len() >= MAX_CONCURRENT_DOCS_LOOKUPS {
                    break;
                }
                s.insert(t.id.clone());
            }
            let app = self.app.clone();
            let db = self.db.clone();
            let cfg = self.cfg.clone();
            let topic = t.clone();
            let set = self.docs_in_flight.clone();
            tauri::async_runtime::spawn(async move {
                lookup_topic(app, db, cfg, topic.label).await;
                if let Ok(mut s) = set.lock() {
                    s.remove(&topic.id);
                }
            });
        }
    }
}

/// Run one request with the shared in-flight / stale / degraded rules.
/// Returns the value to apply, or None (error / stale / degraded — already reported).
async fn run_mode(
    app: &AppHandle,
    cfg: &EngineConfig,
    state: &mut ModeState,
    name: &str,
    req: InsightRequest,
    mode: InsightMode,
) -> Option<Value> {
    if let Err(e) = validate(&req) {
        emit(app, &cfg.meeting_id, name, "error", Some(e));
        return None;
    }
    state.in_flight.store(true, Ordering::Relaxed);
    emit(app, &cfg.meeting_id, name, "running", None);
    let result = cfg.provider.run(&req, mode, None).await;
    state.in_flight.store(false, Ordering::Relaxed);
    match result {
        Ok(r) => {
            if r.meta.stale {
                tracing::info!("{name}: stale response dropped");
                return None;
            }
            if let Some(seq) = r.meta.request_seq {
                if seq < state.last_applied_seq {
                    tracing::info!(
                        "{name}: out-of-order response {seq} < {} dropped",
                        state.last_applied_seq
                    );
                    return None;
                }
                state.last_applied_seq = seq;
            }
            if r.meta.degraded {
                tracing::info!(
                    "{name}: degraded response — not advancing cursor ({:?})",
                    r.meta.fallback_reason
                );
                emit(
                    app,
                    &cfg.meeting_id,
                    name,
                    "degraded",
                    r.meta.fallback_reason,
                );
                // Still apply what came back (server preserved prior state), but keep the cursor.
                return Some(r.value);
            }
            state.anchor = Some(Instant::now());
            state.success_count += 1;
            Some(r.value)
        }
        Err(e) => {
            tracing::warn!("{name} insights failed: {e}");
            emit(app, &cfg.meeting_id, name, "error", Some(e));
            None
        }
    }
}

/// Look one topic up (Playbook). Managed-free: metered by the backend.
pub async fn lookup_topic(
    app: AppHandle,
    db: Arc<Mutex<Connection>>,
    cfg: EngineConfig,
    label: String,
) {
    let id = topic_slug(&label);
    let set_state =
        |db: &Arc<Mutex<Connection>>, state: &str, error: Option<String>, card: Option<Value>| {
            let Ok(conn) = db.lock() else { return };
            let Ok(Some(m)) = db::get_meeting(&conn, &cfg.meeting_id) else {
                return;
            };
            let mut topics: Vec<DocTopic> = serde_json::from_str(&m.doc_topics).unwrap_or_default();
            if let Some(t) = topics.iter_mut().find(|t| t.id == id) {
                t.state = state.into();
                t.error = error;
            }
            let mut docs: Vec<Value> = serde_json::from_str(&m.docs).unwrap_or_default();
            if let Some(card) = card {
                docs.retain(|d| {
                    d.get("topic").and_then(|t| t.as_str()).map(topic_slug) != Some(id.clone())
                });
                docs.push(card);
            }
            let _ = db::set_docs(
                &conn,
                &cfg.meeting_id,
                &serde_json::to_string(&docs).unwrap_or_default(),
                &serde_json::to_string(&topics).unwrap_or_default(),
            );
        };
    set_state(&db, "looking_up", None, None);
    emit(
        &app,
        &cfg.meeting_id,
        "docs",
        "running",
        Some(label.clone()),
    );

    let Some(snap) = load_snapshot(&db, &cfg.meeting_id) else {
        return;
    };
    let mut req = build_request(
        InsightMode::Docs,
        cfg.provider.app_mode(),
        &snap.transcript,
        &cfg.language,
        &[],
        None,
        None,
    );
    req.docs_mcp_url = cfg.docs_mcp_url.clone();
    req.topic = Some(label.clone());
    match cfg.provider.docs(&req).await {
        Ok(r) => {
            let cards = r
                .value
                .get("docs")
                .and_then(|d| d.as_array())
                .cloned()
                .unwrap_or_default();
            if r.meta.degraded {
                set_state(&db, "busy", r.meta.fallback_reason, None);
                emit(&app, &cfg.meeting_id, "docs", "degraded", None);
            } else if let Some(card) = cards.into_iter().next() {
                set_state(&db, "answered", None, Some(card));
                emit(&app, &cfg.meeting_id, "docs", "applied", None);
            } else {
                set_state(&db, "no_match", None, None);
                emit(&app, &cfg.meeting_id, "docs", "applied", None);
            }
        }
        Err(e) => {
            let limit = e.contains("docs lookups") || e.contains("limit");
            set_state(
                &db,
                if limit { "pending" } else { "failed" },
                Some(e.clone()),
                None,
            );
            emit(&app, &cfg.meeting_id, "docs", "error", Some(e));
        }
    }
}

/// Background final pass after stop (or regenerate): standard, questions,
/// MEDDPICC when enabled, speaker names. Never blocks the UI.
pub async fn finalize_meeting(
    app: AppHandle,
    db: Arc<Mutex<Connection>>,
    cfg: EngineConfig,
    finishing: Finishing,
) {
    if let Ok(mut f) = finishing.lock() {
        f.insert(cfg.meeting_id.clone());
    }
    let _ = app.emit("insights_finishing", &cfg.meeting_id);
    let mut engine = LiveEngine {
        app: app.clone(),
        db: db.clone(),
        cfg: cfg.clone(),
        standard: ModeState::new(),
        meddpicc: ModeState::new(),
        questions: ModeState::new(),
        speaker_names: ModeState::new(),
        docs_topics: ModeState::new(),
        docs_in_flight: Arc::new(Mutex::new(HashSet::new())),
        last_title_update_count: 0,
        stop: Arc::new(AtomicBool::new(false)),
    };
    if let Some(snap) = load_snapshot(&db, &cfg.meeting_id) {
        if !snap.segments.is_empty() {
            engine.run_standard(&snap, true).await;
            if let Some(snap) = load_snapshot(&db, &cfg.meeting_id) {
                engine.run_questions(&snap, true).await;
                if snap.meeting.sales_enabled {
                    engine.run_meddpicc(&snap, true).await;
                }
                if snap.segments.len() >= FIRST_SPEAKER_NAMES_THRESHOLD {
                    engine.run_speaker_names(&snap).await;
                }
            }
        }
    }
    if let Ok(mut f) = finishing.lock() {
        f.remove(&cfg.meeting_id);
    }
    emit(&app, &cfg.meeting_id, "final", "finished", None);
}

/// "I zoned out": recent window (180 s, at least 8 trailing segments) + full transcript as background.
pub async fn catch_up(
    cfg: &EngineConfig,
    meeting: &Meeting,
    segments: &[TranscriptSegment],
) -> Result<Value, String> {
    if segments.len() < CATCHUP_MIN_SEGMENTS {
        return Err("Not enough transcript yet to catch up on.".into());
    }
    let labels = labels_for(meeting, segments);
    let full = transcript_text(segments, &labels);
    let last_end = segments.last().map(|s| s.end_s).unwrap_or(0.0);
    let mut recent: Vec<&TranscriptSegment> = segments
        .iter()
        .filter(|s| s.end_s >= last_end - CATCHUP_RECENT_WINDOW_SECS)
        .collect();
    if recent.len() < CATCHUP_FALLBACK_WINDOW_SEGMENTS {
        recent = segments
            .iter()
            .rev()
            .take(CATCHUP_FALLBACK_WINDOW_SEGMENTS)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }
    let recent_text = recent
        .iter()
        .map(|s| {
            format!(
                "[{}] {}",
                labels.get(&s.speaker).cloned().unwrap_or_default(),
                s.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut req = build_request(
        InsightMode::Catchup,
        cfg.provider.app_mode(),
        &recent_text,
        &cfg.language,
        &[],
        None,
        None,
    );
    req.full_transcript = Some(super::trim_transcript(&full));
    let r = cfg.provider.run(&req, InsightMode::Catchup, None).await?;
    Ok(r.value)
}

/// Explicit investigation (never automatic). Port of `investigationMeetingContext`.
pub fn investigation_context(meeting: &Meeting, segments: &[TranscriptSegment]) -> String {
    const MAX: usize = 11_500;
    let budget = (MAX - 1_000).max(1_000);
    let per_turn = (budget / 2).clamp(500, 2_500);
    let labels = labels_for(meeting, segments);
    let mut lines: Vec<String> = Vec::new();
    let mut count = 0usize;
    for s in segments.iter().rev() {
        let line: String = format!(
            "{}: {}",
            labels.get(&s.speaker).cloned().unwrap_or_default(),
            s.text
        )
        .chars()
        .take(per_turn)
        .collect();
        if !lines.is_empty() && count + line.chars().count() > budget {
            break;
        }
        count += line.chars().count() + 1;
        lines.push(line);
        if lines.len() >= 14 {
            break;
        }
    }
    let mut ctx = format!("Meeting title\n{}", meeting.display_title());
    let notes = meeting.notes.trim();
    if !notes.is_empty() {
        ctx.push_str(&format!(
            "\n\nMeeting prep notes\n{}",
            notes.chars().take(2_000).collect::<String>()
        ));
    }
    if !lines.is_empty() {
        lines.reverse();
        ctx.push_str(&format!("\n\nRecent conversation\n{}", lines.join("\n")));
    }
    ctx.chars().take(MAX).collect()
}

pub async fn investigate(
    cfg: &EngineConfig,
    meeting: &Meeting,
    segments: &[TranscriptSegment],
    scope: InvestigationScope,
    focus: &str,
    codebase: Option<(String, Vec<String>)>,
) -> Result<Value, String> {
    let context = investigation_context(meeting, segments);
    let mut req = build_request(
        InsightMode::Investigation,
        cfg.provider.app_mode(),
        &context,
        &cfg.language,
        &[],
        None,
        None,
    );
    req.investigation_scope = Some(scope);
    req.focus = Some(focus.trim().chars().take(super::FOCUS_MAX_CHARS).collect());
    if let Some((ctx, files)) = codebase {
        req.codebase_context = Some(ctx);
        req.referenced_files = files
            .into_iter()
            .take(super::REFERENCED_FILES_MAX)
            .collect();
    }
    validate(&req)?;
    let r = cfg
        .provider
        .run(&req, InsightMode::Investigation, None)
        .await?;
    Ok(r.value)
}

/// Local heuristic (port of the macOS conversation-moment detector): does this
/// final segment sound like something worth investigating? Never runs an
/// investigation itself.
pub fn detect_investigation_moment(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.split_whitespace().count() < 4 {
        return None;
    }
    let n = trimmed.to_lowercase();
    const EXPLICIT: &[&str] = &[
        "investigate",
        "research",
        "look into",
        "find out",
        "check whether",
        "figure out",
        "verify whether",
        "is it possible",
        "would it be possible",
        "what would it take",
        "would be cool if",
    ];
    const CAPABILITY: &[&str] = &[
        "can we build",
        "could we build",
        "can we implement",
        "could we implement",
        "can we integrate",
        "could we integrate",
        "can we automate",
        "could we automate",
        "can we support",
        "could we support",
        "can we fix",
        "could we fix",
        "is there a way",
        "is there some way",
    ];
    const DIAGNOSTIC: &[&str] = &[
        "why does",
        "why is",
        "how could",
        "how can",
        "what causes",
        "feasible",
        "feasibility",
        "possible",
        "tradeoff",
        "cost",
        "impact",
    ];
    let explicit = EXPLICIT.iter().chain(CAPABILITY).any(|p| n.contains(p));
    let diagnostic = (n.contains('?') || n.starts_with("why") || n.starts_with("how"))
        && DIAGNOSTIC.iter().any(|p| n.contains(p));
    if explicit || diagnostic {
        Some(trimmed.chars().take(280).collect())
    } else {
        None
    }
}

/// Local default-on Sales suggestion: enough distinct commercial vocabulary in
/// the early transcript. Never enables Sales by itself.
pub fn sounds_commercial(transcript: &str) -> bool {
    const TERMS: &[&str] = &[
        "pricing",
        "price",
        "budget",
        "contract",
        "procurement",
        "renewal",
        "quote",
        "proposal",
        "discount",
        "vendor",
        "stakeholder",
        "decision maker",
        "purchase",
        "buy",
        "trial",
        "pilot",
        "seats",
        "licence",
        "license",
        "roi",
        "invoice",
        "onboarding",
        "competitor",
        "evaluation",
    ];
    let t = transcript.to_lowercase();
    TERMS.iter().filter(|w| t.contains(*w)).count() >= 4
}

/// Port of `buildCodebaseSnapshot`: pick up to 8 files under `root` that best
/// match the focus, bounded to ~40k characters of excerpts.
pub fn build_codebase_snapshot(root: &std::path::Path, focus: &str) -> (String, Vec<String>) {
    const ALLOWED: &[&str] = &[
        "swift", "m", "mm", "h", "c", "cc", "cpp", "hpp", "ts", "tsx", "js", "jsx", "py", "go",
        "rs", "java", "kt", "kts", "rb", "php", "cs", "sql", "graphql", "json", "yaml", "yml",
        "toml", "md",
    ];
    const EXCLUDED: &[&str] = &[
        ".git",
        ".build",
        ".swiftpm",
        "DerivedData",
        "node_modules",
        "Pods",
        "vendor",
        "dist",
        "build",
        "target",
    ];
    const STOP: &[&str] = &[
        "could",
        "would",
        "should",
        "there",
        "their",
        "about",
        "which",
        "this",
        "that",
        "with",
        "from",
        "have",
        "what",
        "when",
        "where",
        "into",
        "some",
        "investigate",
        "the",
        "and",
        "for",
        "why",
        "does",
        "how",
        "can",
        "are",
        "was",
        "not",
        "but",
        "you",
        "our",
        "its",
        "fail",
        "work",
    ];
    const MAX_CHARS: usize = 40_000;
    const MAX_FILES: usize = 8;
    let tokens: Vec<String> = focus
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3 && !STOP.contains(t))
        .map(String::from)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if tokens.is_empty() {
        return (String::new(), Vec::new());
    }
    let mut candidates: Vec<(i64, String, String)> = Vec::new();
    let mut inspected = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                if !EXCLUDED.contains(&name.as_str()) {
                    stack.push(path);
                }
                continue;
            }
            if !meta.is_file() || meta.len() > 300_000 {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_lowercase();
            if !ALLOWED.contains(&ext.as_str()) {
                continue;
            }
            inspected += 1;
            if inspected > 1_200 {
                break;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string_lossy().to_string());
            let lower_path = rel.to_lowercase();
            let lower = content.to_lowercase();
            let mut score = 0i64;
            for t in &tokens {
                if lower_path.contains(t.as_str()) {
                    score += 12;
                }
                score += (lower.matches(t.as_str()).count().min(20)) as i64;
            }
            if score > 0 {
                candidates.push((score, rel, content));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut out = String::new();
    let mut files = Vec::new();
    for (_, rel, content) in candidates.into_iter().take(MAX_FILES) {
        let lower = content.to_lowercase();
        let first = tokens
            .iter()
            .filter_map(|t| lower.find(t.as_str()))
            .min()
            .unwrap_or(0);
        let start = content[..first]
            .char_indices()
            .rev()
            .nth(1_500)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let excerpt: String = content[start..].chars().take(5_000).collect();
        let block = format!("\n\n=== {rel} ===\n{excerpt}");
        if out.chars().count() + block.chars().count() > MAX_CHARS {
            break;
        }
        out.push_str(&block);
        files.push(rel);
    }
    (out.trim().to_string(), files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(speaker: i64, text: &str, start: f64) -> TranscriptSegment {
        TranscriptSegment::new("m", speaker, text, start, start + 1.0, "microphone")
    }

    #[test]
    fn cadence_matches_macos_policy() {
        let mut s = ModeState::new();
        let t0 = Instant::now();
        assert!(!s.due(STANDARD_POLICY, 3, 2, t0), "below first threshold");
        assert!(s.due(STANDARD_POLICY, 3, 3, t0), "first pass at threshold");
        s.last_attempt = Some(t0);
        assert!(
            !s.due(STANDARD_POLICY, 3, 5, t0 + Duration::from_secs(10)),
            "warm-up retry waits 30s"
        );
        assert!(s.due(STANDARD_POLICY, 3, 5, t0 + Duration::from_secs(31)));

        s.success_count = 1;
        s.anchor = Some(t0);
        s.last_fired_segments = 5;
        assert!(
            !s.due(STANDARD_POLICY, 3, 5, t0 + Duration::from_secs(200)),
            "no new finals → never"
        );
        assert!(
            !s.due(STANDARD_POLICY, 3, 7, t0 + Duration::from_secs(70)),
            "min interval needs ≥4 new"
        );
        assert!(s.due(STANDARD_POLICY, 3, 9, t0 + Duration::from_secs(70)));
        assert!(
            s.due(STANDARD_POLICY, 3, 6, t0 + Duration::from_secs(121)),
            "max interval with any new content"
        );
        s.in_flight.store(true, Ordering::Relaxed);
        assert!(
            !s.due(STANDARD_POLICY, 3, 20, t0 + Duration::from_secs(300)),
            "never overlap requests"
        );
    }

    #[test]
    fn transcript_formats_match_backend_expectations() {
        let m = Meeting::new("t", "en");
        let segs = vec![seg(1000, "hi", 0.0), seg(0, "hello", 1.0)];
        let labels = labels_for(&m, &segs);
        assert_eq!(
            transcript_text(&segs, &labels),
            "[You] hi\n[Speaker 1] hello"
        );
        assert_eq!(
            transcript_with_speaker_ids(&segs),
            "[SpeakerID:1000] hi\n[SpeakerID:0] hello"
        );
    }

    #[test]
    fn incremental_payload_uses_ack_cursor_and_recent_window() {
        let mut s = ModeState::new();
        let segs: Vec<_> = (0..6)
            .map(|i| seg(1000, &format!("line {i}"), i as f64))
            .collect();
        let labels = labels_for(&Meeting::new("t", "en"), &segs);
        let full = transcript_text(&segs, &labels);
        assert!(
            incremental_payload(&s, &segs, &labels, &full).is_none(),
            "first pass is never incremental"
        );
        s.success_count = 1;
        s.rolling_state = json!({"summary": "s"});
        s.acked_segments = 4;
        let p = incremental_payload(&s, &segs, &labels, &full).unwrap();
        assert_eq!(p.acked_segment_count, 4);
        assert_eq!(p.delta_segment_count, 2);
        assert_eq!(p.transcript_delta, "[You] line 4\n[You] line 5");
        assert_eq!(p.full_segment_count, 6);
        assert_eq!(p.strategy, "delta_recent_window_v1");
    }

    #[test]
    fn apply_standard_persists_and_updates_rolling_state() {
        let conn = crate::db::open_in_memory().unwrap();
        let m = Meeting::new("New meeting", "en");
        crate::db::upsert_meeting(&conn, &m).unwrap();
        let mut s = ModeState::new();
        let v = json!({"title": "Roadmap sync", "summary": "We planned.", "action_items": ["ship"], "topics": ["roadmap"], "discussion_flow": ["a", "b"]});
        apply_standard(&conn, &m.id, &v, &mut s, 7, true).unwrap();
        let got = crate::db::get_meeting(&conn, &m.id).unwrap().unwrap();
        assert_eq!(got.summary, "We planned.");
        assert_eq!(got.title, "Roadmap sync");
        assert_eq!(got.action_items, r#"["ship"]"#);
        assert_eq!(s.acked_segments, 7);
        assert_eq!(s.rolling_state["suggested_title"], "Roadmap sync");
        assert!(got.insights_updated_at.is_some());
    }

    #[test]
    fn apply_meddpicc_strips_bullets_and_keeps_nulls() {
        let conn = crate::db::open_in_memory().unwrap();
        let m = Meeting::new("t", "en");
        crate::db::upsert_meeting(&conn, &m).unwrap();
        let mut s = ModeState::new();
        let v = json!({"summary": "q", "metrics": "- 20% faster\n• saves 3h", "champion": null, "competition": "null"});
        apply_meddpicc(&conn, &m.id, &v, &mut s, 3).unwrap();
        let got = crate::db::get_meeting(&conn, &m.id).unwrap().unwrap();
        let med: Value = serde_json::from_str(&got.meddpicc).unwrap();
        assert_eq!(med["metrics"], "20% faster\nsaves 3h");
        assert!(med["champion"].is_null());
        assert!(
            med["competition"].is_null(),
            "literal 'null' strings are nulls"
        );
        assert!(med.get("paper_process").is_some());
    }

    #[test]
    fn speaker_names_respect_manual_renames_and_placeholders() {
        let conn = crate::db::open_in_memory().unwrap();
        let mut m = Meeting::new("t", "en");
        m.speaker_names = r#"{"0":"Manual Name"}"#.into();
        m.manual_speaker_ids = "[0]".into();
        crate::db::upsert_meeting(&conn, &m).unwrap();
        let v = json!({"speakers": {"0": "Alex", "1000": "Ian", "1": "Speaker 2", "2": "Unknown"}});
        assert!(apply_speaker_names(&conn, &m, &v).unwrap());
        let got = crate::db::get_meeting(&conn, &m.id).unwrap().unwrap();
        let names: HashMap<String, String> = serde_json::from_str(&got.speaker_names).unwrap();
        assert_eq!(names["0"], "Manual Name");
        assert_eq!(names["1000"], "Ian");
        assert!(!names.contains_key("1"));
        assert!(!names.contains_key("2"));
        assert!(!apply_speaker_names(&conn, &m, &json!({"speakers": {}})).unwrap());
    }

    #[test]
    fn doc_topics_merge_preserves_state_and_dedupes() {
        let existing = vec![DocTopic {
            id: "sso / saml".into(),
            label: "SSO / SAML".into(),
            state: "answered".into(),
            error: None,
        }];
        let merged = merge_doc_topics(&existing, &["sso /  saml".into(), "Data retention".into()]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].state, "answered");
        assert_eq!(merged[1].state, "pending");
        assert_eq!(merged[1].id, "data retention");
    }

    #[test]
    fn investigation_moment_heuristic() {
        assert!(
            detect_investigation_moment("Can we integrate this with Salesforce next quarter?")
                .is_some()
        );
        assert!(
            detect_investigation_moment("Why does the export take so long, what causes it?")
                .is_some()
        );
        assert!(detect_investigation_moment("ok sounds good").is_none());
        assert!(detect_investigation_moment("we should look into it").is_some());
        assert!(
            detect_investigation_moment("How was your weekend?").is_none(),
            "question without diagnostic term"
        );
    }

    #[test]
    fn investigation_context_is_bounded_and_recent_first() {
        let mut m = Meeting::new("Roadmap", "en");
        m.notes = "prep".into();
        let segs: Vec<_> = (0..30)
            .map(|i| seg(1000, &format!("turn {i}"), i as f64))
            .collect();
        let ctx = investigation_context(&m, &segs);
        assert!(ctx.starts_with("Meeting title\nRoadmap"));
        assert!(ctx.contains("Meeting prep notes\nprep"));
        assert!(ctx.contains("turn 29"));
        assert!(!ctx.contains("turn 10"), "only the last 14 turns");
        assert!(ctx.chars().count() <= 11_500);
    }

    #[test]
    fn commercial_heuristic_needs_several_terms() {
        assert!(!sounds_commercial("let's talk about the budget"));
        assert!(sounds_commercial(
            "pricing and budget for the contract, procurement wants a quote and a proposal"
        ));
    }

    #[test]
    fn codebase_snapshot_scores_matching_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules/x")).unwrap();
        std::fs::write(
            dir.path().join("src/billing.rs"),
            "fn invoice() { /* stripe webhook */ }",
        )
        .unwrap();
        std::fs::write(dir.path().join("src/other.rs"), "fn nothing() {}").unwrap();
        std::fs::write(
            dir.path().join("node_modules/x/stripe.js"),
            "stripe stripe stripe",
        )
        .unwrap();
        let (ctx, files) = build_codebase_snapshot(dir.path(), "why does the stripe webhook fail?");
        assert_eq!(files, vec!["src/billing.rs"]);
        assert!(ctx.contains("=== src/billing.rs ==="));
        assert!(!ctx.contains("node_modules"));
    }
}
