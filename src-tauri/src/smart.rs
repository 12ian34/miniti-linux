//! Smart meetings: the call-lifecycle engine (port of macOS
//! `CallLifecycleEngine`), quiet-ended prompts, calendar transition / handoff,
//! calendar auto-start, silence auto-stop, ending grace, and live guidance
//! nudges. Pure policy lives in small tested functions; `Monitor` is the 1 Hz
//! runtime that turns them into `smart_prompt` / `recording_nudge` events and
//! acts on the user's decisions.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::call_sensor::{classify, CallAppKind, CaptureClient};
use crate::integrations::CalendarEvent;

// ---- Policy constants (macOS) ----------------------------------------------------

pub const START_DEBOUNCE: Duration = Duration::from_secs(2);
pub const END_DEBOUNCE: Duration = Duration::from_secs(4);
pub const END_QUIET_REQUIREMENT: Duration = Duration::from_secs(5);
pub const BROWSER_END_DEBOUNCE: Duration = Duration::from_secs(8);
pub const BROWSER_END_QUIET_REQUIREMENT: Duration = Duration::from_secs(8);
pub const START_RESET_GAP: Duration = Duration::from_secs(3);
pub const SHORT_SESSION_MIN_DURATION: Duration = Duration::from_secs(60);
pub const ENDING_GRACE: Duration = Duration::from_secs(10);
pub const KEEP_RECORDING_SUPPRESSION: Duration = Duration::from_secs(300);
pub const CALENDAR_SNOOZE: Duration = Duration::from_secs(120);
pub const HANDOFF_COUNTDOWN: u64 = 15;
pub const QUESTION_NUDGE_MIN_INTERVAL: Duration = Duration::from_secs(120);
pub const MONOLOGUE_NUDGE_MIN_INTERVAL: Duration = Duration::from_secs(180);
pub const FILLER_NUDGE_MIN_INTERVAL: Duration = Duration::from_secs(180);
pub const MONOLOGUE_MIN_SECONDS: f64 = 60.0;
pub const MONOLOGUE_MIN_WORDS: usize = 180;
pub const FILLER_WINDOW_SECONDS: f64 = 60.0;
pub const FILLER_NUDGE_MIN_PER_MINUTE: f64 = 8.0;
pub const FILLER_WINDOW_MIN_YOU_WORDS: usize = 20;

/// Smart prompt after quiet, keyed on the silence auto-stop setting (minutes).
pub fn smart_quiet_threshold_secs(auto_stop_minutes: i64) -> f64 {
    match auto_stop_minutes {
        0 => 5.0 * 60.0,
        3 => 2.0 * 60.0,
        5 => 3.0 * 60.0,
        _ => 5.0 * 60.0,
    }
}

/// Port of `quietCrossedCommonMeetingBoundary`: after ≥15 min of recording and
/// ≥2 min of quiet, a :00 / :30 wall-clock boundary inside the quiet window
/// makes a likely-ended prompt reasonable earlier.
pub fn quiet_crossed_common_boundary(quiet_started_unix: i64, now_unix: i64, recording_secs: f64) -> bool {
    if recording_secs < 15.0 * 60.0 || (now_unix - quiet_started_unix) < 120 {
        return false;
    }
    // Next half-hour boundary strictly after quiet start (local time).
    let local = |t: i64| chrono::DateTime::<chrono::Utc>::from_timestamp(t, 0).map(|d| d.with_timezone(&chrono::Local));
    let Some(start) = local(quiet_started_unix) else { return false };
    let offset_secs = start.offset().local_minus_utc() as i64;
    let local_secs = quiet_started_unix + offset_secs;
    let mut boundary = local_secs - local_secs.rem_euclid(1800) + 1800;
    while boundary - offset_secs <= now_unix {
        if boundary - offset_secs > quiet_started_unix {
            return true;
        }
        boundary += 1800;
    }
    false
}

/// Port of `shouldOfferSmartQuietPrompt`.
pub fn should_offer_quiet_prompt(has_meaningful: bool, transcript_gap: f64, audio_gap: f64, auto_stop_minutes: i64, crossed_boundary: bool, already_prompted: bool, suppressed: bool) -> bool {
    if !has_meaningful || already_prompted || suppressed {
        return false;
    }
    let threshold = if crossed_boundary { 120.0 } else { smart_quiet_threshold_secs(auto_stop_minutes) };
    transcript_gap >= threshold && audio_gap >= threshold
}

/// Port of `canAutomaticallyHandoffCalendarMeeting`.
#[allow(clippy::too_many_arguments)]
pub fn can_auto_handoff(current_end: Option<i64>, next_start: Option<i64>, now: i64, transcript_gap: f64, audio_gap: f64, auto_start: bool, calendar_auto_stop: bool, overlap: bool) -> bool {
    let (Some(ce), Some(ns)) = (current_end, next_start) else { return false };
    auto_start && calendar_auto_stop && !overlap && ce <= now && ns >= ce && ns - ce <= 15 * 60 && transcript_gap >= 120.0 && audio_gap >= 120.0
}

// ---- Call lifecycle engine (port) ---------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallConfidence {
    Native,
    Browser,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveCall {
    pub id: String,
    pub name: String,
    pub confidence: CallConfidence,
}

#[derive(Debug, Clone)]
pub struct CallSnapshot {
    pub active: Vec<ActiveCall>,
    pub reliable: bool,
    pub at: Instant,
}

impl CallSnapshot {
    /// From the PipeWire/Pulse capture clients (None = sensor unavailable).
    pub fn from_clients(clients: Option<Vec<CaptureClient>>, at: Instant) -> Self {
        let Some(clients) = clients else {
            return Self { active: Vec::new(), reliable: false, at };
        };
        let mut active: Vec<ActiveCall> = clients
            .into_iter()
            .filter(|c| c.is_capturing)
            .filter_map(|c| match classify(&c.app_name) {
                CallAppKind::Strong => Some(ActiveCall { id: c.app_name.to_lowercase(), name: c.app_name.clone(), confidence: CallConfidence::Native }),
                CallAppKind::Weak => Some(ActiveCall { id: c.app_name.to_lowercase(), name: c.app_name.clone(), confidence: CallConfidence::Browser }),
                CallAppKind::None => None,
            })
            .collect();
        active.sort_by(|a, b| (a.confidence == CallConfidence::Browser).cmp(&(b.confidence == CallConfidence::Browser)).then(a.id.cmp(&b.id)));
        active.dedup_by(|a, b| a.id == b.id);
        Self { active, reliable: true, at }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleEvent {
    OfferStartPrompt(ActiveCall),
    ClearStartPrompt,
    BeginEndingGrace { app: ActiveCall, ask_only: bool },
    CancelEndCandidate,
    ReactivateDuringGrace,
    OfferTransition(ActiveCall),
}

#[derive(Debug, Clone, Copy)]
pub struct LifecycleContext {
    pub is_recording: bool,
    pub smart_enabled: bool,
    pub has_meaningful_content: bool,
    pub recording_duration: Duration,
    pub transcript_gap: Duration,
    pub audio_gap: Duration,
    pub in_ending_grace: bool,
}

#[derive(Debug, Default)]
pub struct CallLifecycleEngine {
    start_candidate: Option<(ActiveCall, Instant)>,
    prompted_start_id: Option<String>,
    suppressed_start_id: Option<String>,
    last_start_app_seen_at: Option<Instant>,
    start_prompt_visible: bool,
    pub associated_app: Option<ActiveCall>,
    has_observed_associated_active: bool,
    end_candidate_since: Option<Instant>,
    transition_candidates: HashMap<String, Instant>,
    offered_transition_ids: HashSet<String>,
}

impl CallLifecycleEngine {
    fn end_debounce(c: CallConfidence) -> Duration {
        if c == CallConfidence::Browser { BROWSER_END_DEBOUNCE } else { END_DEBOUNCE }
    }
    fn end_quiet(c: CallConfidence) -> Duration {
        if c == CallConfidence::Browser { BROWSER_END_QUIET_REQUIREMENT } else { END_QUIET_REQUIREMENT }
    }

    pub fn note_recording_started(&mut self, active: &[ActiveCall], from_prompt: Option<ActiveCall>) {
        self.end_candidate_since = None;
        self.has_observed_associated_active = false;
        self.transition_candidates.clear();
        self.offered_transition_ids.clear();
        self.associated_app = match from_prompt {
            Some(app) => Some(app),
            None if active.len() == 1 => Some(active[0].clone()),
            None => None,
        };
        self.start_candidate = None;
        self.prompted_start_id = None;
        self.suppressed_start_id = None;
        self.start_prompt_visible = false;
    }

    pub fn note_recording_ended(&mut self) {
        self.associated_app = None;
        self.has_observed_associated_active = false;
        self.end_candidate_since = None;
        self.transition_candidates.clear();
        self.offered_transition_ids.clear();
    }

    pub fn note_start_prompt_dismissed(&mut self, id: &str) {
        self.suppressed_start_id = Some(id.to_string());
        self.start_prompt_visible = false;
    }

    pub fn note_start_prompt_not_shown(&mut self, id: &str) {
        if self.prompted_start_id.as_deref() == Some(id) {
            self.prompted_start_id = None;
            self.start_prompt_visible = false;
        }
    }

    pub fn ingest(&mut self, snapshot: &CallSnapshot, ctx: LifecycleContext) -> Vec<LifecycleEvent> {
        if !ctx.smart_enabled {
            return Vec::new();
        }
        if !snapshot.reliable {
            self.end_candidate_since = None;
            self.start_candidate = None;
            return Vec::new();
        }
        if ctx.is_recording { self.ingest_recording(snapshot, ctx) } else { self.ingest_idle(snapshot) }
    }

    fn ingest_recording(&mut self, snapshot: &CallSnapshot, ctx: LifecycleContext) -> Vec<LifecycleEvent> {
        let mut events = Vec::new();
        let now = snapshot.at;
        self.start_candidate = None;
        if let Some(associated) = self.associated_app.clone() {
            if let Some(current) = snapshot.active.iter().find(|a| a.id == associated.id) {
                self.has_observed_associated_active = true;
                self.associated_app = Some(current.clone());
                if self.end_candidate_since.is_some() {
                    self.end_candidate_since = None;
                    events.push(if ctx.in_ending_grace { LifecycleEvent::ReactivateDuringGrace } else { LifecycleEvent::CancelEndCandidate });
                } else if ctx.in_ending_grace {
                    events.push(LifecycleEvent::ReactivateDuringGrace);
                }
            } else if self.has_observed_associated_active && !ctx.in_ending_grace {
                match self.end_candidate_since {
                    Some(since) => {
                        if now.duration_since(since) >= Self::end_debounce(associated.confidence) && ctx.transcript_gap >= Self::end_quiet(associated.confidence) {
                            let ask_only = !ctx.has_meaningful_content || ctx.recording_duration < SHORT_SESSION_MIN_DURATION;
                            events.push(LifecycleEvent::BeginEndingGrace { app: associated.clone(), ask_only });
                            self.end_candidate_since = None;
                        }
                    }
                    None => self.end_candidate_since = Some(now),
                }
            }
        }
        let active_ids: HashSet<&str> = snapshot.active.iter().map(|a| a.id.as_str()).collect();
        self.transition_candidates.retain(|id, _| active_ids.contains(id.as_str()));
        let associated_id = self.associated_app.as_ref().map(|a| a.id.clone());
        for app in snapshot.active.iter().filter(|a| Some(&a.id) != associated_id.as_ref()) {
            match self.transition_candidates.get(&app.id) {
                Some(since) => {
                    if now.duration_since(*since) >= START_DEBOUNCE && !self.offered_transition_ids.contains(&app.id) {
                        self.offered_transition_ids.insert(app.id.clone());
                        events.push(LifecycleEvent::OfferTransition(app.clone()));
                    }
                }
                None => {
                    self.transition_candidates.insert(app.id.clone(), now);
                }
            }
        }
        events
    }

    fn ingest_idle(&mut self, snapshot: &CallSnapshot) -> Vec<LifecycleEvent> {
        let mut events = Vec::new();
        let now = snapshot.at;
        self.end_candidate_since = None;
        if let Some(preferred) = snapshot.active.first() {
            self.last_start_app_seen_at = Some(now);
            if self.start_candidate.as_ref().map(|(a, _)| a.id != preferred.id).unwrap_or(true) {
                self.start_candidate = Some((preferred.clone(), now));
            } else if let Some((_, first_seen)) = &self.start_candidate {
                if now.duration_since(*first_seen) >= START_DEBOUNCE
                    && self.prompted_start_id.as_deref() != Some(&preferred.id)
                    && self.suppressed_start_id.as_deref() != Some(&preferred.id)
                {
                    self.prompted_start_id = Some(preferred.id.clone());
                    self.start_prompt_visible = true;
                    events.push(LifecycleEvent::OfferStartPrompt(preferred.clone()));
                }
            }
        } else {
            self.start_candidate = None;
            if let Some(last) = self.last_start_app_seen_at {
                if now.duration_since(last) >= START_RESET_GAP {
                    self.last_start_app_seen_at = None;
                    self.prompted_start_id = None;
                    self.suppressed_start_id = None;
                    if self.start_prompt_visible {
                        self.start_prompt_visible = false;
                        events.push(LifecycleEvent::ClearStartPrompt);
                    }
                }
            }
        }
        events
    }
}

// ---- Prompts + nudges --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SmartPrompt {
    pub id: String,
    /// start | quiet | ending | calendar | clear
    pub kind: String,
    pub message: String,
    pub primary: String,
    pub secondary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tertiary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub countdown: Option<u64>,
}

impl SmartPrompt {
    fn clear() -> Self {
        Self { id: "clear".into(), kind: "clear".into(), message: String::new(), primary: String::new(), secondary: String::new(), tertiary: None, event_id: None, countdown: None }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordingNudge {
    pub id: String,
    pub kind: &'static str,
    pub title: String,
    pub message: String,
}

/// Which prompt is pending and what its buttons do.
#[derive(Debug, Clone, PartialEq)]
enum Pending {
    CallStart(ActiveCall),
    Quiet,
    CallEndAsk(ActiveCall),
    EndingGrace { deadline: Instant },
    Transition(ActiveCall),
    CalendarTransition { event: CalendarEvent, handoff_deadline: Option<Instant> },
    CalendarStart { event: CalendarEvent, deadline: Option<Instant> },
}

/// Live activity the monitor needs from the recording engine.
#[derive(Debug, Clone, Copy)]
pub struct Activity {
    pub recording: bool,
    pub recording_duration: f64,
    /// Seconds since the last final transcript segment (∞ when none).
    pub transcript_gap: f64,
    /// Seconds since speech-like audio (∞ when none).
    pub audio_gap: f64,
    pub meaningful_finals: usize,
    pub now_unix: i64,
}

/// One "You" segment for nudge evaluation.
#[derive(Debug, Clone)]
pub struct YouSegment {
    pub start: f64,
    pub end: f64,
    pub words: usize,
    pub fillers: usize,
}

/// Monologue nudge: trailing uninterrupted "You" run ≥ 60 s and ≥ 180 words.
/// `run` = consecutive self segments ending at the transcript tail.
pub fn monologue_due(run: &[YouSegment]) -> bool {
    let (Some(first), Some(last)) = (run.first(), run.last()) else { return false };
    let words: usize = run.iter().map(|s| s.words).sum();
    last.end - first.start >= MONOLOGUE_MIN_SECONDS && words >= MONOLOGUE_MIN_WORDS
}

/// Filler nudge: ≥ 8 fillers/min over the last 60 s of "You" speech with ≥ 20 words.
pub fn filler_rate_due(recent: &[YouSegment], window_end: f64) -> Option<f64> {
    let window_start = (window_end - FILLER_WINDOW_SECONDS).max(0.0);
    let (words, fillers) = recent.iter().filter(|s| s.end >= window_start).fold((0usize, 0usize), |(w, f), s| (w + s.words, f + s.fillers));
    if words < FILLER_WINDOW_MIN_YOU_WORDS {
        return None;
    }
    let per_min = fillers as f64 / (FILLER_WINDOW_SECONDS / 60.0);
    (per_min >= FILLER_NUDGE_MIN_PER_MINUTE).then_some(per_min)
}

#[derive(Default)]
pub struct MonitorState {
    pub engine: CallLifecycleEngine,
    pending: Option<Pending>,
    prompt_id: u64,
    quiet_prompted_this_episode: bool,
    quiet_started_at: Option<i64>,
    suppress_until: Option<Instant>,
    snoozed_events: HashMap<String, Instant>,
    dismissed_events: HashSet<String>,
    last_question_nudge: Option<Instant>,
    notified_questions: HashSet<String>,
    last_monologue_nudge: Option<Instant>,
    monologue_nudged_run_start: Option<f64>,
    last_filler_nudge: Option<Instant>,
    last_sensor_scan: Option<Instant>,
    last_snapshot: Option<CallSnapshot>,
    was_recording: bool,
    /// Meeting id the current recording belongs to (for prompt scoping).
    pub current_meeting: Option<String>,
    pub start_from_prompt: Option<ActiveCall>,
}

pub type MonitorSlot = Mutex<MonitorState>;

/// What the monitor decided this tick; the runtime applies it.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Stop { reason: &'static str },
    StartFromEvent(CalendarEvent),
    StartFromCall(Option<ActiveCall>),
    EndAndStartEvent(CalendarEvent),
    EndAndStartNew,
}

fn emit_prompt(app: &AppHandle, prompt: &SmartPrompt, surface_enabled: bool, notifications: bool) {
    let _ = app.emit("smart_prompt", prompt);
    if prompt.kind == "clear" {
        crate::shell::set_tray_decision(app, None);
        crate::shell::shrink_presence(app);
        return;
    }
    crate::shell::set_tray_decision(app, Some((&prompt.message, &prompt.primary, &prompt.secondary)));
    if surface_enabled {
        crate::shell::raise_presence(app);
    }
    if notifications && !(surface_enabled && crate::shell::main_is_focused(app)) {
        crate::shell::notify(app, "Miniti", &format!("{} ({} / {})", prompt.message, prompt.primary, prompt.secondary));
    }
}

impl MonitorState {
    fn next_id(&mut self, kind: &str) -> String {
        self.prompt_id += 1;
        format!("{kind}-{}", self.prompt_id)
    }

    fn set_pending(&mut self, app: &AppHandle, pending: Pending, prompt: SmartPrompt, surface: bool, notifications: bool) {
        self.pending = Some(pending);
        emit_prompt(app, &prompt, surface, notifications);
    }

    pub fn clear_prompt(&mut self, app: &AppHandle) {
        if self.pending.take().is_some() {
            emit_prompt(app, &SmartPrompt::clear(), false, false);
        }
    }

    fn current_prompt_kind(&self) -> Option<&'static str> {
        Some(match self.pending.as_ref()? {
            Pending::CallStart(_) | Pending::CalendarStart { .. } => "start",
            Pending::Quiet => "quiet",
            Pending::CallEndAsk(_) | Pending::EndingGrace { .. } => "ending",
            Pending::Transition(_) | Pending::CalendarTransition { .. } => "calendar",
        })
    }

    /// One tick. Returns actions for the runtime to perform.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        app: &AppHandle,
        prefs: &crate::prefs::Prefs,
        activity: Activity,
        current_event_id: Option<&str>,
        events: &[CalendarEvent],
        sensor: Option<Vec<CaptureClient>>,
        now: Instant,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        let surface = prefs.show_floating_indicator;
        let notifications = prefs.notifications_enabled;
        let smart = prefs.smart_meetings_enabled;

        // Recording edge: reset episode state; associate call app.
        if activity.recording != self.was_recording {
            self.was_recording = activity.recording;
            self.quiet_prompted_this_episode = false;
            self.quiet_started_at = None;
            self.suppress_until = None;
            self.clear_prompt(app);
            if activity.recording {
                let active = self.last_snapshot.as_ref().map(|s| s.active.clone()).unwrap_or_default();
                let from_prompt = self.start_from_prompt.take();
                self.engine.note_recording_started(&active, from_prompt);
            } else {
                self.engine.note_recording_ended();
                self.monologue_nudged_run_start = None;
            }
        }

        // Speech resets the quiet episode and suppression.
        if activity.transcript_gap < 30.0 {
            self.quiet_prompted_this_episode = false;
            self.quiet_started_at = None;
            if matches!(self.pending, Some(Pending::Quiet)) {
                self.clear_prompt(app);
            }
        } else if self.quiet_started_at.is_none() {
            self.quiet_started_at = Some(activity.now_unix - activity.transcript_gap as i64);
        }
        let suppressed = self.suppress_until.map(|t| now < t).unwrap_or(false);

        // Countdowns.
        match self.pending.clone() {
            Some(Pending::EndingGrace { deadline }) => {
                let left = deadline.saturating_duration_since(now).as_secs();
                if now >= deadline {
                    self.pending = None;
                    emit_prompt(app, &SmartPrompt::clear(), false, false);
                    actions.push(Action::Stop { reason: "call ended" });
                } else {
                    let _ = app.emit("smart_prompt", SmartPrompt { id: "ending-grace".into(), kind: "ending".into(), message: format!("Call ended — saving in {left}s"), primary: "End now".into(), secondary: "Keep recording".into(), tertiary: None, event_id: None, countdown: Some(left) });
                }
            }
            Some(Pending::CalendarTransition { event, handoff_deadline: Some(deadline) }) => {
                if activity.transcript_gap < 5.0 {
                    // Meaningful speech resumed: cancel the automatic handoff.
                    self.pending = Some(Pending::CalendarTransition { event, handoff_deadline: None });
                } else if now >= deadline {
                    self.pending = None;
                    emit_prompt(app, &SmartPrompt::clear(), false, false);
                    actions.push(Action::EndAndStartEvent(event));
                } else {
                    let left = deadline.saturating_duration_since(now).as_secs();
                    let _ = app.emit("smart_prompt", SmartPrompt { id: format!("calendar-{}", event.id), kind: "calendar".into(), message: format!("Ending and starting “{}” in {left}s", event.title), primary: "End & start next".into(), secondary: "Keep recording".into(), tertiary: Some("Remind in 2 min".into()), event_id: Some(event.id.clone()), countdown: Some(left) });
                }
            }
            Some(Pending::CalendarStart { event, deadline: Some(deadline) }) => {
                if now >= deadline {
                    self.pending = None;
                    emit_prompt(app, &SmartPrompt::clear(), false, false);
                    self.dismissed_events.insert(event.id.clone());
                    actions.push(Action::StartFromEvent(event));
                } else {
                    let left = deadline.saturating_duration_since(now).as_secs();
                    let _ = app.emit("smart_prompt", SmartPrompt { id: format!("start-{}", event.id), kind: "start".into(), message: format!("Starting “{}” in {left}s", event.title), primary: "Start now".into(), secondary: "Don't start".into(), tertiary: None, event_id: Some(event.id.clone()), countdown: Some(left) });
                }
            }
            _ => {}
        }
        if !actions.is_empty() {
            return actions;
        }

        // Call sensor (every 3 s).
        let scan_due = self.last_sensor_scan.map(|t| now.duration_since(t) >= Duration::from_secs(3)).unwrap_or(true);
        if smart && scan_due {
            self.last_sensor_scan = Some(now);
            let snapshot = CallSnapshot::from_clients(sensor, now);
            let ctx = LifecycleContext {
                is_recording: activity.recording,
                smart_enabled: smart,
                has_meaningful_content: activity.meaningful_finals > 0,
                recording_duration: Duration::from_secs_f64(activity.recording_duration.max(0.0)),
                transcript_gap: Duration::from_secs_f64(activity.transcript_gap.min(1e9)),
                audio_gap: Duration::from_secs_f64(activity.audio_gap.min(1e9)),
                in_ending_grace: matches!(self.pending, Some(Pending::EndingGrace { .. })),
            };
            for ev in self.engine.ingest(&snapshot, ctx) {
                match ev {
                    LifecycleEvent::OfferStartPrompt(appc) => {
                        if self.pending.is_none() {
                            let id = self.next_id("start");
                            let name = appc.name.clone();
                            self.set_pending(app, Pending::CallStart(appc), SmartPrompt { id, kind: "start".into(), message: format!("{name} is using the microphone. Take notes?"), primary: "Start recording".into(), secondary: "Not now".into(), tertiary: None, event_id: None, countdown: None }, surface, notifications);
                        } else {
                            self.engine.note_start_prompt_not_shown(&appc.id);
                        }
                    }
                    LifecycleEvent::ClearStartPrompt => {
                        if matches!(self.pending, Some(Pending::CallStart(_))) {
                            self.clear_prompt(app);
                        }
                    }
                    LifecycleEvent::BeginEndingGrace { app: appc, ask_only } => {
                        if ask_only {
                            let id = self.next_id("ending");
                            let title = if appc.confidence == CallConfidence::Browser { "Browser call ended".to_string() } else { format!("{} call ended", appc.name) };
                            self.set_pending(app, Pending::CallEndAsk(appc), SmartPrompt { id, kind: "ending".into(), message: format!("{title}. Has this meeting finished?"), primary: "End meeting".into(), secondary: "Keep recording".into(), tertiary: None, event_id: None, countdown: None }, surface, notifications);
                        } else {
                            let deadline = now + ENDING_GRACE;
                            let id = self.next_id("ending");
                            self.set_pending(app, Pending::EndingGrace { deadline }, SmartPrompt { id, kind: "ending".into(), message: format!("{} call ended — saving in {}s", appc.name, ENDING_GRACE.as_secs()), primary: "End now".into(), secondary: "Keep recording".into(), tertiary: None, event_id: None, countdown: Some(ENDING_GRACE.as_secs()) }, surface, notifications);
                        }
                    }
                    LifecycleEvent::CancelEndCandidate => {}
                    LifecycleEvent::ReactivateDuringGrace => {
                        if matches!(self.pending, Some(Pending::EndingGrace { .. }) | Some(Pending::CallEndAsk(_))) {
                            self.clear_prompt(app);
                        }
                    }
                    LifecycleEvent::OfferTransition(appc) => {
                        if self.pending.is_none() {
                            let id = self.next_id("transition");
                            let name = appc.name.clone();
                            self.set_pending(app, Pending::Transition(appc), SmartPrompt { id, kind: "calendar".into(), message: format!("{name} call started. Switch to a new meeting?"), primary: "End & start new".into(), secondary: "Keep recording".into(), tertiary: None, event_id: None, countdown: None }, surface, notifications);
                        }
                    }
                }
            }
            self.last_snapshot = Some(snapshot);
        }

        if activity.recording {
            // Existing silence auto-stop (independent of Smart meetings).
            if prefs.auto_stop_minutes > 0 && !suppressed {
                let t = prefs.auto_stop_minutes as f64 * 60.0;
                if activity.meaningful_finals > 0 && activity.transcript_gap >= t && activity.audio_gap >= t {
                    self.clear_prompt(app);
                    actions.push(Action::Stop { reason: "silence" });
                    return actions;
                }
            }
            // Likely-ended prompt.
            if smart && self.pending.is_none() {
                let crossed = self.quiet_started_at.map(|q| quiet_crossed_common_boundary(q, activity.now_unix, activity.recording_duration)).unwrap_or(false);
                if should_offer_quiet_prompt(activity.meaningful_finals > 0, activity.transcript_gap, activity.audio_gap, prefs.auto_stop_minutes, crossed, self.quiet_prompted_this_episode, suppressed) {
                    self.quiet_prompted_this_episode = true;
                    let mins = (activity.transcript_gap / 60.0).floor() as i64;
                    let id = self.next_id("quiet");
                    self.set_pending(app, Pending::Quiet, SmartPrompt { id, kind: "quiet".into(), message: format!("No speech for {mins} minute{}. Has this meeting ended?", if mins == 1 { "" } else { "s" }), primary: "End meeting".into(), secondary: "Keep recording 5 min".into(), tertiary: None, event_id: None, countdown: None }, surface, notifications);
                }
            }
            // Calendar transition prompt (~1 min before the next event).
            if smart && (self.pending.is_none() || matches!(self.pending, Some(Pending::CalendarTransition { .. }))) {
                let next = events.iter().find(|e| {
                    Some(e.id.as_str()) != current_event_id
                        && !e.is_all_day
                        && !self.dismissed_events.contains(&e.id)
                        && self.snoozed_events.get(&e.id).map(|t| now >= *t).unwrap_or(true)
                        && e.start_ts().map(|s| { let d = s - activity.now_unix; (-120..=60).contains(&d) }).unwrap_or(false)
                });
                if let Some(event) = next.cloned() {
                    let already = matches!(&self.pending, Some(Pending::CalendarTransition { event: e, .. }) if e.id == event.id);
                    if !already {
                        let time = event.start_ts().and_then(|s| chrono::DateTime::<chrono::Utc>::from_timestamp(s, 0)).map(|d| d.with_timezone(&chrono::Local).format("%H:%M").to_string()).unwrap_or_else(|| "soon".into());
                        let id = self.next_id("calendar");
                        self.set_pending(app, Pending::CalendarTransition { event: event.clone(), handoff_deadline: None }, SmartPrompt { id, kind: "calendar".into(), message: format!("Next: {}. Starts at {time}. Is the current meeting over?", event.title), primary: "End & start next".into(), secondary: "Keep recording".into(), tertiary: Some("Remind in 2 min".into()), event_id: Some(event.id.clone()), countdown: None }, surface, notifications);
                    }
                    // Automatic handoff only when every opt-in and inactivity condition holds.
                    if let Some(Pending::CalendarTransition { event: e, handoff_deadline: None }) = self.pending.clone() {
                        let current = current_event_id.and_then(|id| events.iter().find(|x| x.id == id));
                        let starts_in = e.start_ts().map(|s| s - activity.now_unix).unwrap_or(999);
                        let overlap = match (current.and_then(|c| c.end_ts()), e.start_ts()) { (Some(ce), Some(ns)) => ns < ce, _ => true };
                        if starts_in <= HANDOFF_COUNTDOWN as i64
                            && can_auto_handoff(current.and_then(|c| c.end_ts()), e.start_ts(), activity.now_unix, activity.transcript_gap, activity.audio_gap, prefs.calendar_auto_start, prefs.calendar_auto_stop, overlap)
                        {
                            self.pending = Some(Pending::CalendarTransition { event: e, handoff_deadline: Some(now + Duration::from_secs(starts_in.clamp(1, HANDOFF_COUNTDOWN as i64) as u64)) });
                        }
                    }
                }
            }
        } else if prefs.calendar_auto_start || smart {
            // Idle: calendar auto-start (countdown ≤ 16 s, or immediate within 2 min after start).
            if self.pending.is_none() {
                for e in events.iter().filter(|e| !e.is_all_day && !self.dismissed_events.contains(&e.id)) {
                    let Some(s) = e.start_ts() else { continue };
                    let d = s - activity.now_unix;
                    if prefs.calendar_auto_start && d > 0 && d <= 16 {
                        let id = self.next_id("start");
                        self.set_pending(app, Pending::CalendarStart { event: e.clone(), deadline: Some(now + Duration::from_secs(d as u64)) }, SmartPrompt { id, kind: "start".into(), message: format!("Starting “{}” in {d}s", e.title), primary: "Start now".into(), secondary: "Don't start".into(), tertiary: None, event_id: Some(e.id.clone()), countdown: Some(d as u64) }, surface, notifications);
                        break;
                    }
                    if prefs.calendar_auto_start && (-120..=0).contains(&d) {
                        self.dismissed_events.insert(e.id.clone());
                        actions.push(Action::StartFromEvent(e.clone()));
                        return actions;
                    }
                    if !prefs.calendar_auto_start && (-120..=60).contains(&d) {
                        let id = self.next_id("start");
                        self.set_pending(app, Pending::CalendarStart { event: e.clone(), deadline: None }, SmartPrompt { id, kind: "start".into(), message: format!("“{}” is starting. Record it?", e.title), primary: "Start recording".into(), secondary: "Not this one".into(), tertiary: None, event_id: Some(e.id.clone()), countdown: None }, surface, notifications);
                        break;
                    }
                }
            }
        }
        actions
    }

    /// Live guidance evaluation (called by the runtime with fresh transcript data).
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_nudges(&mut self, app: &AppHandle, prefs: &crate::prefs::Prefs, you_recent: &[YouSegment], you_run: &[YouSegment], high_questions: &[String], transcript_end: f64, now: Instant) {
        if !prefs.live_guidance_enabled {
            return;
        }
        let deliver = |n: RecordingNudge| {
            let _ = app.emit("recording_nudge", &n);
            crate::shell::deliver_guidance(app, &n.title, &n.message, prefs.show_floating_indicator && prefs.notifications_enabled);
        };
        if let Some(q) = high_questions.iter().find(|q| !self.notified_questions.contains(*q)) {
            if self.last_question_nudge.map(|t| now.duration_since(t) >= QUESTION_NUDGE_MIN_INTERVAL).unwrap_or(true) {
                self.notified_questions.insert(q.clone());
                self.last_question_nudge = Some(now);
                deliver(RecordingNudge { id: format!("q-{}", self.notified_questions.len()), kind: "question", title: "Worth asking".into(), message: q.clone() });
            }
        }
        if let Some(first) = you_run.first() {
            let run_start = first.start;
            if monologue_due(you_run) && self.monologue_nudged_run_start != Some(run_start) && self.last_monologue_nudge.map(|t| now.duration_since(t) >= MONOLOGUE_NUDGE_MIN_INTERVAL).unwrap_or(true) {
                self.monologue_nudged_run_start = Some(run_start);
                self.last_monologue_nudge = Some(now);
                let secs = (you_run.last().map(|s| s.end).unwrap_or(run_start) - run_start).round() as i64;
                deliver(RecordingNudge { id: format!("m-{run_start}"), kind: "monologue", title: "Long stretch".into(), message: format!("You've been speaking for {secs}s. Hand the conversation back?") });
            }
        } else {
            self.monologue_nudged_run_start = None;
        }
        if let Some(rate) = filler_rate_due(you_recent, transcript_end) {
            if self.last_filler_nudge.map(|t| now.duration_since(t) >= FILLER_NUDGE_MIN_INTERVAL).unwrap_or(true) {
                self.last_filler_nudge = Some(now);
                deliver(RecordingNudge { id: format!("f-{}", transcript_end as i64), kind: "filler", title: "Filler burst".into(), message: format!("{rate:.0} fillers a minute in the last minute. Try a silent beat.") });
            }
        }
    }

    /// Apply the user's choice on the current prompt.
    pub fn decide(&mut self, app: &AppHandle, prompt_id: &str, choice: &str, now: Instant) -> Vec<Action> {
        let _ = prompt_id;
        let mut actions = Vec::new();
        let Some(pending) = self.pending.clone() else { return actions };
        match (pending, choice) {
            (Pending::CallStart(appc), "primary") => {
                self.start_from_prompt = Some(appc.clone());
                actions.push(Action::StartFromCall(Some(appc)));
            }
            (Pending::CallStart(appc), _) => self.engine.note_start_prompt_dismissed(&appc.id),
            (Pending::Quiet, "primary") | (Pending::CallEndAsk(_), "primary") | (Pending::EndingGrace { .. }, "primary") => {
                actions.push(Action::Stop { reason: "user" });
            }
            (Pending::Quiet, _) => self.suppress_until = Some(now + KEEP_RECORDING_SUPPRESSION),
            (Pending::CallEndAsk(_), _) | (Pending::EndingGrace { .. }, _) => {}
            (Pending::Transition(_), "primary") => actions.push(Action::EndAndStartNew),
            (Pending::Transition(_), _) => {}
            (Pending::CalendarTransition { event, .. }, "primary") => actions.push(Action::EndAndStartEvent(event)),
            (Pending::CalendarTransition { event, .. }, "tertiary") => {
                self.snoozed_events.insert(event.id.clone(), now + CALENDAR_SNOOZE);
            }
            (Pending::CalendarTransition { event, .. }, _) => {
                self.dismissed_events.insert(event.id.clone());
            }
            (Pending::CalendarStart { event, .. }, "primary") => {
                self.dismissed_events.insert(event.id.clone());
                actions.push(Action::StartFromEvent(event));
            }
            (Pending::CalendarStart { event, .. }, _) => {
                self.dismissed_events.insert(event.id.clone());
            }
        }
        self.clear_prompt(app);
        actions
    }

    pub fn prompt_kind(&self) -> Option<&'static str> {
        self.current_prompt_kind()
    }
}

/// Convenience for the runtime: managed slot on the app.
pub fn slot(app: &AppHandle) -> Option<tauri::State<'_, MonitorSlot>> {
    app.try_state::<MonitorSlot>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str, browser: bool) -> ActiveCall {
        ActiveCall { id: id.into(), name: id.into(), confidence: if browser { CallConfidence::Browser } else { CallConfidence::Native } }
    }
    fn snap(active: Vec<ActiveCall>, at: Instant) -> CallSnapshot {
        CallSnapshot { active, reliable: true, at }
    }
    fn ctx(recording: bool, gap: f64) -> LifecycleContext {
        LifecycleContext { is_recording: recording, smart_enabled: true, has_meaningful_content: true, recording_duration: Duration::from_secs(600), transcript_gap: Duration::from_secs_f64(gap), audio_gap: Duration::from_secs_f64(gap), in_ending_grace: false }
    }

    #[test]
    fn quiet_thresholds_follow_the_table() {
        assert_eq!(smart_quiet_threshold_secs(0), 300.0);
        assert_eq!(smart_quiet_threshold_secs(3), 120.0);
        assert_eq!(smart_quiet_threshold_secs(5), 180.0);
        assert_eq!(smart_quiet_threshold_secs(10), 300.0);
        assert_eq!(smart_quiet_threshold_secs(15), 300.0);
        assert!(should_offer_quiet_prompt(true, 300.0, 300.0, 0, false, false, false));
        assert!(!should_offer_quiet_prompt(true, 300.0, 10.0, 0, false, false, false), "both transcript and audio must be quiet");
        assert!(should_offer_quiet_prompt(true, 130.0, 130.0, 0, true, false, false), "boundary shortens to 2 min");
        assert!(!should_offer_quiet_prompt(true, 400.0, 400.0, 0, false, true, false), "one prompt per episode");
        assert!(!should_offer_quiet_prompt(true, 400.0, 400.0, 0, false, false, true), "suppressed");
        assert!(!should_offer_quiet_prompt(false, 400.0, 400.0, 0, false, false, false), "needs content");
    }

    #[test]
    fn common_boundary_rule() {
        // 2024-01-01T10:20:00Z quiet start; now 10:32Z → crossed 10:30 (if local tz aligns to :00/:30 boundaries it still does).
        let start = 1_704_104_400; // 2024-01-01 10:20:00 UTC
        assert!(!quiet_crossed_common_boundary(start, start + 60, 20.0 * 60.0), "needs ≥2 min quiet");
        assert!(!quiet_crossed_common_boundary(start, start + 12 * 60, 10.0 * 60.0), "needs ≥15 min recording");
        assert!(quiet_crossed_common_boundary(start, start + 12 * 60, 20.0 * 60.0));
        assert!(!quiet_crossed_common_boundary(start, start + 5 * 60, 20.0 * 60.0), "10:25 has not crossed :30");
    }

    #[test]
    fn handoff_requires_every_condition() {
        let now = 1_000_000;
        assert!(can_auto_handoff(Some(now - 10), Some(now + 5), now, 200.0, 200.0, true, true, false));
        assert!(!can_auto_handoff(Some(now - 10), Some(now + 5), now, 200.0, 200.0, false, true, false));
        assert!(!can_auto_handoff(Some(now - 10), Some(now + 5), now, 30.0, 200.0, true, true, false));
        assert!(!can_auto_handoff(Some(now - 10), Some(now + 5), now, 200.0, 200.0, true, true, true));
        assert!(!can_auto_handoff(Some(now + 60), Some(now + 90), now, 200.0, 200.0, true, true, false), "current not ended");
        assert!(!can_auto_handoff(Some(now - 10), Some(now + 20 * 60), now, 200.0, 200.0, true, true, false), "gap > 15 min");
    }

    #[test]
    fn lifecycle_offers_start_after_debounce_and_clears() {
        let mut e = CallLifecycleEngine::default();
        let t0 = Instant::now();
        assert!(e.ingest(&snap(vec![call("zoom", false)], t0), ctx(false, 0.0)).is_empty());
        let evs = e.ingest(&snap(vec![call("zoom", false)], t0 + Duration::from_secs(3)), ctx(false, 0.0));
        assert_eq!(evs, vec![LifecycleEvent::OfferStartPrompt(call("zoom", false))]);
        assert!(e.ingest(&snap(vec![call("zoom", false)], t0 + Duration::from_secs(4)), ctx(false, 0.0)).is_empty(), "prompted once");
        e.note_start_prompt_dismissed("zoom");
        assert!(e.ingest(&snap(vec![], t0 + Duration::from_secs(5)), ctx(false, 0.0)).is_empty());
        let evs = e.ingest(&snap(vec![], t0 + Duration::from_secs(9)), ctx(false, 0.0));
        assert!(evs.is_empty(), "prompt already dismissed → nothing to clear");
        // A new call after the reset gap prompts again.
        assert!(e.ingest(&snap(vec![call("zoom", false)], t0 + Duration::from_secs(10)), ctx(false, 0.0)).is_empty());
        let evs = e.ingest(&snap(vec![call("zoom", false)], t0 + Duration::from_secs(13)), ctx(false, 0.0));
        assert_eq!(evs.len(), 1);
    }

    #[test]
    fn lifecycle_ending_grace_needs_debounce_and_quiet() {
        let mut e = CallLifecycleEngine::default();
        let t0 = Instant::now();
        e.note_recording_started(&[call("zoom", false)], None);
        assert_eq!(e.associated_app.as_ref().map(|a| a.id.as_str()), Some("zoom"));
        e.ingest(&snap(vec![call("zoom", false)], t0), ctx(true, 0.0));
        // App releases mic; first tick starts the candidate.
        assert!(e.ingest(&snap(vec![], t0 + Duration::from_secs(1)), ctx(true, 1.0)).is_empty());
        // Debounce passed but transcript not quiet enough → no grace yet.
        assert!(e.ingest(&snap(vec![], t0 + Duration::from_secs(6)), ctx(true, 2.0)).is_empty());
        let evs = e.ingest(&snap(vec![], t0 + Duration::from_secs(7)), ctx(true, 6.0));
        assert_eq!(evs, vec![LifecycleEvent::BeginEndingGrace { app: call("zoom", false), ask_only: false }]);
        // Browser calls are more sceptical.
        let mut b = CallLifecycleEngine::default();
        b.note_recording_started(&[call("firefox", true)], None);
        b.ingest(&snap(vec![call("firefox", true)], t0), ctx(true, 0.0));
        b.ingest(&snap(vec![], t0 + Duration::from_secs(1)), ctx(true, 1.0));
        assert!(b.ingest(&snap(vec![], t0 + Duration::from_secs(7)), ctx(true, 6.0)).is_empty());
        assert_eq!(b.ingest(&snap(vec![], t0 + Duration::from_secs(10)), ctx(true, 9.0)).len(), 1);
    }

    #[test]
    fn lifecycle_short_session_asks_and_reactivation_cancels() {
        let mut e = CallLifecycleEngine::default();
        let t0 = Instant::now();
        e.note_recording_started(&[call("teams", false)], None);
        e.ingest(&snap(vec![call("teams", false)], t0), ctx(true, 0.0));
        e.ingest(&snap(vec![], t0 + Duration::from_secs(1)), ctx(true, 1.0));
        let mut short = ctx(true, 10.0);
        short.recording_duration = Duration::from_secs(30);
        let evs = e.ingest(&snap(vec![], t0 + Duration::from_secs(6)), short);
        assert_eq!(evs, vec![LifecycleEvent::BeginEndingGrace { app: call("teams", false), ask_only: true }]);
        let mut grace = ctx(true, 10.0);
        grace.in_ending_grace = true;
        let evs = e.ingest(&snap(vec![call("teams", false)], t0 + Duration::from_secs(8)), grace);
        assert_eq!(evs, vec![LifecycleEvent::ReactivateDuringGrace]);
    }

    #[test]
    fn lifecycle_offers_transition_once() {
        let mut e = CallLifecycleEngine::default();
        let t0 = Instant::now();
        e.note_recording_started(&[call("zoom", false)], None);
        e.ingest(&snap(vec![call("zoom", false), call("teams", false)], t0), ctx(true, 0.0));
        let evs = e.ingest(&snap(vec![call("zoom", false), call("teams", false)], t0 + Duration::from_secs(3)), ctx(true, 0.0));
        assert_eq!(evs, vec![LifecycleEvent::OfferTransition(call("teams", false))]);
        assert!(e.ingest(&snap(vec![call("zoom", false), call("teams", false)], t0 + Duration::from_secs(9)), ctx(true, 0.0)).is_empty());
    }

    #[test]
    fn unreliable_snapshots_freeze_the_engine() {
        let mut e = CallLifecycleEngine::default();
        let t0 = Instant::now();
        let unreliable = CallSnapshot { active: vec![], reliable: false, at: t0 };
        e.ingest(&snap(vec![call("zoom", false)], t0), ctx(false, 0.0));
        assert!(e.ingest(&unreliable, ctx(false, 0.0)).is_empty());
        assert!(e.ingest(&snap(vec![call("zoom", false)], t0 + Duration::from_secs(1)), ctx(false, 0.0)).is_empty(), "debounce restarted");
    }

    #[test]
    fn snapshot_from_clients_prefers_native_apps() {
        let clients = vec![
            CaptureClient { app_name: "Firefox".into(), is_capturing: true },
            CaptureClient { app_name: "ZOOM VoiceEngine".into(), is_capturing: true },
            CaptureClient { app_name: "parec".into(), is_capturing: true },
            CaptureClient { app_name: "Teams".into(), is_capturing: false },
        ];
        let s = CallSnapshot::from_clients(Some(clients), Instant::now());
        assert!(s.reliable);
        assert_eq!(s.active.len(), 2);
        assert_eq!(s.active[0].confidence, CallConfidence::Native);
        assert!(!CallSnapshot::from_clients(None, Instant::now()).reliable);
    }

    #[test]
    fn nudge_rules() {
        let run: Vec<YouSegment> = (0..10).map(|i| YouSegment { start: i as f64 * 7.0, end: i as f64 * 7.0 + 6.5, words: 20, fillers: 0 }).collect();
        assert!(monologue_due(&run), "63 s and 200 words");
        assert!(!monologue_due(&run[..3]));
        let recent = vec![YouSegment { start: 100.0, end: 130.0, words: 30, fillers: 5 }, YouSegment { start: 130.0, end: 160.0, words: 30, fillers: 4 }];
        assert!(filler_rate_due(&recent, 160.0).is_some(), "9/min over 60 words");
        assert!(filler_rate_due(&recent[..1], 160.0).is_none(), "5/min is under threshold");
        let few = vec![YouSegment { start: 100.0, end: 160.0, words: 10, fillers: 9 }];
        assert!(filler_rate_due(&few, 160.0).is_none(), "needs 20 words");
    }
}
