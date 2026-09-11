//! Opt-out reliability reporting (`POST /api/client-events`).
//!
//! Structured events about how the client is holding up — reconnects, audio
//! recovery, enrollment failures — so a fleet problem is visible without
//! asking someone to send a log. Never transcript or audio content: an event
//! is an allowlisted name, a level, a category, the diagnostics session id,
//! the meeting id, the app mode, and a few bounded key/value details.
//!
//! On in managed mode unless the user turned it off, and never sent in BYOK,
//! where there is no miniti account to attach it to.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::prefs::{AppMode, Prefs};

/// Server-side limit, mirrored here so a loop cannot spend a device's budget.
pub const MAX_EVENTS_PER_MINUTE: usize = 30;
/// Send when this many are queued, or when the flusher next ticks.
pub const BATCH_SIZE: usize = 10;
pub const FLUSH_INTERVAL: Duration = Duration::from_secs(30);
/// Details are bounded: a handful of short values, never free text.
pub const MAX_DETAILS: usize = 6;
pub const MAX_DETAIL_CHARS: usize = 120;

/// The vocabulary. An event not in this list is dropped rather than sent, so
/// what leaves this machine is reviewable by reading one array.
pub const EVENT_NAMES: [&str; 10] = [
    "deepgram_unexpected_disconnect",
    "deepgram_reconnect_attempt",
    "deepgram_connect_failed",
    "deepgram_stream_failed",
    "audio_recovery_attempt",
    "audio_recovery_degraded",
    "audio_recovery_succeeded",
    "transcript_starvation_detected",
    "enrollment_failed",
    "session_start_failed",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Transcription,
    Audio,
    Auth,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ClientEvent {
    pub name: String,
    pub level: Level,
    pub category: Category,
    /// Groups the events of one app run; not the Deepgram session id.
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_id: Option<String>,
    /// byok | managed (an event is only ever sent in managed).
    pub app_mode: String,
    pub app_version: String,
    /// ISO 8601.
    pub at: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, String>,
}

/// On unless the user turned it off, and never in BYOK. A pref that was never
/// set reads as on, so 0.7.0 installs start reporting; one that was explicitly
/// switched off stays off.
pub fn enabled(prefs: &Prefs) -> bool {
    prefs.app_mode == AppMode::Managed && prefs.share_diagnostics.unwrap_or(true)
}

/// A transport failure as a short, contentless code. The raw text can quote
/// the Deepgram URL, whose query carries the meeting's keyterms, so only
/// plain words survive.
pub fn reason_code(reason: &str) -> String {
    reason
        .split_whitespace()
        .filter(|w| !w.contains("://") && !w.contains('?') && !w.contains('='))
        .map(|w| {
            w.chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .take(5)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(48)
        .collect()
}

/// Trim details to the bounded shape the endpoint accepts.
fn bounded(details: Vec<(&str, String)>) -> BTreeMap<String, String> {
    details
        .into_iter()
        .take(MAX_DETAILS)
        .map(|(k, v)| {
            let v: String = v.chars().take(MAX_DETAIL_CHARS).collect();
            (k.to_string(), v)
        })
        .collect()
}

#[derive(Debug, Default)]
struct Queue {
    events: Vec<ClientEvent>,
    /// Start of the current minute-long budget window.
    window_start: Option<Instant>,
    sent_in_window: usize,
    dropped_in_window: usize,
}

struct Hub {
    session_id: String,
    queue: Mutex<Queue>,
}

fn hub() -> &'static Hub {
    static HUB: OnceLock<Hub> = OnceLock::new();
    HUB.get_or_init(|| Hub {
        session_id: uuid::Uuid::new_v4().to_string(),
        queue: Mutex::new(Queue::default()),
    })
}

/// The diagnostics session id for this app run.
pub fn session_id() -> &'static str {
    &hub().session_id
}

/// Whether another event fits in this minute's budget; counts it when it does.
fn take_budget(q: &mut Queue, now: Instant) -> bool {
    let fresh = q
        .window_start
        .map(|start| now.duration_since(start) >= Duration::from_secs(60))
        .unwrap_or(true);
    if fresh {
        if q.dropped_in_window > 0 {
            tracing::debug!("diagnostics: dropped {} over budget", q.dropped_in_window);
        }
        q.window_start = Some(now);
        q.sent_in_window = 0;
        q.dropped_in_window = 0;
    }
    if q.sent_in_window >= MAX_EVENTS_PER_MINUTE {
        q.dropped_in_window += 1;
        return false;
    }
    q.sent_in_window += 1;
    true
}

/// Queue one event. Cheap and non-blocking: the flusher sends it.
pub fn record(
    app: &AppHandle,
    name: &str,
    level: Level,
    category: Category,
    meeting_id: Option<String>,
    details: Vec<(&str, String)>,
) {
    if !EVENT_NAMES.contains(&name) {
        debug_assert!(false, "diagnostics: {name} is not in the vocabulary");
        return;
    }
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let Ok(prefs) = state.prefs_snapshot() else {
        return;
    };
    if !enabled(&prefs) || !state.enrolled() {
        return;
    }
    let event = ClientEvent {
        name: name.to_string(),
        level,
        category,
        session_id: hub().session_id.clone(),
        meeting_id,
        app_mode: "managed".into(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        details: bounded(details),
    };
    let ready = {
        let Ok(mut q) = hub().queue.lock() else { return };
        if !take_budget(&mut q, Instant::now()) {
            return;
        }
        q.events.push(event);
        q.events.len() >= BATCH_SIZE
    };
    if ready {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { flush(&app).await });
    }
}

/// Send whatever is queued. A failed send drops the batch rather than
/// retrying: diagnostics must never cost more than the problem they report.
pub async fn flush(app: &AppHandle) {
    let batch = {
        let Ok(mut q) = hub().queue.lock() else { return };
        if q.events.is_empty() {
            return;
        }
        std::mem::take(&mut q.events)
    };
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let Ok(prefs) = state.prefs_snapshot() else {
        return;
    };
    if !enabled(&prefs) {
        return;
    }
    let Ok(client) = state.api_client(&prefs) else {
        return;
    };
    let body = serde_json::json!({ "events": batch });
    match client.post_client_events(&body).await {
        Ok(()) => tracing::debug!("diagnostics: sent {} events", batch.len()),
        Err(e) => tracing::debug!("diagnostics: {} events not sent ({e})", batch.len()),
    }
}

/// Flush on a timer so a quiet session still reports what little it had.
pub fn spawn_flusher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(FLUSH_INTERVAL);
        loop {
            tick.tick().await;
            flush(&app).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn managed() -> Prefs {
        Prefs {
            app_mode: AppMode::Managed,
            ..Default::default()
        }
    }

    #[test]
    fn on_by_default_in_managed_off_when_the_user_said_so_never_in_byok() {
        assert!(enabled(&managed()), "a pref that was never set is on");
        assert!(!enabled(&Prefs {
            share_diagnostics: Some(false),
            ..managed()
        }));
        assert!(enabled(&Prefs {
            share_diagnostics: Some(true),
            ..managed()
        }));
        for share in [None, Some(true), Some(false)] {
            assert!(
                !enabled(&Prefs {
                    app_mode: AppMode::Byok,
                    share_diagnostics: share,
                    ..Default::default()
                }),
                "BYOK never reports"
            );
        }
    }

    #[test]
    fn the_minute_budget_matches_the_server_limit() {
        let mut q = Queue::default();
        let t0 = Instant::now();
        for _ in 0..MAX_EVENTS_PER_MINUTE {
            assert!(take_budget(&mut q, t0));
        }
        assert!(!take_budget(&mut q, t0), "the 31st is dropped");
        assert_eq!(q.dropped_in_window, 1);
        // The window rolls over.
        assert!(take_budget(&mut q, t0 + Duration::from_secs(61)));
        assert_eq!(q.sent_in_window, 1);
        assert_eq!(q.dropped_in_window, 0);
    }

    #[test]
    fn a_failure_reason_is_reduced_to_plain_words() {
        assert_eq!(
            reason_code("IO error: connection reset by peer"),
            "io error connection reset by"
        );
        let with_url = reason_code(
            "handshake failed for wss://api.deepgram.com/v1/listen?keyterms=Acme%20Corp",
        );
        assert!(!with_url.contains("deepgram"), "{with_url}");
        assert!(!with_url.contains("Acme"), "{with_url}");
        assert!(reason_code("").is_empty());
    }

    #[test]
    fn details_are_bounded_and_never_free_text() {
        let long = "x".repeat(500);
        let many: Vec<(&str, String)> = (0..20).map(|_| ("k", long.clone())).collect();
        let out = bounded(many);
        assert!(out.len() <= MAX_DETAILS);
        assert!(out.values().all(|v| v.chars().count() <= MAX_DETAIL_CHARS));
    }

    #[test]
    fn an_event_carries_no_content() {
        let e = ClientEvent {
            name: "deepgram_reconnect_attempt".into(),
            level: Level::Warn,
            category: Category::Transcription,
            session_id: "diag-1".into(),
            meeting_id: Some("m1".into()),
            app_mode: "managed".into(),
            app_version: "0.8.0".into(),
            at: "2026-09-11T00:00:00Z".into(),
            details: bounded(vec![("attempt", "2".into())]),
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains(r#""level":"warn""#));
        assert!(json.contains(r#""category":"transcription""#));
        assert!(json.contains(r#""attempt":"2""#));
        assert!(
            EVENT_NAMES.contains(&e.name.as_str()),
            "only allowlisted names are sent"
        );
    }
}
