//! The one state model every external surface sees: `state.json` in the
//! runtime dir, the `subscribe` stream on the socket, the D-Bus
//! `StateChanged` signal and `miniti status` all carry this shape.
//!
//! The hub owns the last published snapshot and a broadcast channel; the
//! shell ticker calls `publish` once a second and only a real change (not the
//! wall clock) is written or broadcast.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Listener, Manager};
use tokio::sync::broadcast;

use crate::state::{AppState, RecordingPresence};

/// Live JSON model written to `state.json` and streamed to subscribers.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub schema: u32,
    pub app_version: &'static str,
    pub pid: u32,
    /// Unix seconds of the last change (not of the last tick).
    pub updated_at: i64,
    #[serde(flatten)]
    pub presence: RecordingPresence,
    /// Questions worth asking now (`{question, type, context?, priority?}`).
    pub questions: Vec<Value>,
    /// Rolling summary of the live meeting ("" when none).
    pub summary: String,
    /// Pending Smart-meeting prompt (`{id, kind, message, primary, secondary, …}`).
    pub prompt: Option<Value>,
    /// Latest live-guidance nudge (`{kind, title, body, …}`), cleared on stop.
    pub nudge: Option<Value>,
}

impl Snapshot {
    /// Serialisation used to detect change: everything except `updated_at`.
    fn change_key(&self) -> String {
        let mut v = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Some(o) = v.as_object_mut() {
            o.remove("updated_at");
        }
        v.to_string()
    }
}

#[derive(Default)]
struct InsightsCache {
    meeting_id: Option<String>,
    questions: Vec<Value>,
    summary: String,
}

pub struct Hub {
    tx: broadcast::Sender<String>,
    last_key: Mutex<Option<String>>,
    last_json: Mutex<Option<String>>,
    prompt: Mutex<Option<Value>>,
    nudge: Mutex<Option<Value>>,
    insights_dirty: AtomicBool,
    insights: Mutex<InsightsCache>,
    pub dbus: Mutex<Option<zbus::Connection>>,
    pub inhibit: tokio::sync::Mutex<super::inhibit::Inhibitor>,
}

pub type HubSlot = Arc<Hub>;

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

impl Hub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            tx,
            last_key: Mutex::new(None),
            last_json: Mutex::new(None),
            prompt: Mutex::new(None),
            nudge: Mutex::new(None),
            insights_dirty: AtomicBool::new(true),
            insights: Mutex::new(InsightsCache::default()),
            dbus: Mutex::new(None),
            inhibit: tokio::sync::Mutex::new(super::inhibit::Inhibitor::default()),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Last published snapshot as JSON (None before the first tick).
    pub fn last_json(&self) -> Option<String> {
        self.last_json.lock().ok().and_then(|g| g.clone())
    }
}

pub fn hub(app: &AppHandle) -> Option<HubSlot> {
    app.try_state::<HubSlot>().map(|h| h.inner().clone())
}

/// Wire the hub to the in-process events it mirrors. Call once at setup.
pub fn listen(app: &AppHandle) {
    let Some(h) = hub(app) else { return };
    let hub = h.clone();
    app.listen("smart_prompt", move |ev| {
        let v: Value = serde_json::from_str(ev.payload()).unwrap_or(Value::Null);
        let cleared = v.get("kind").and_then(|k| k.as_str()) == Some("clear");
        if let Ok(mut p) = hub.prompt.lock() {
            *p = if cleared || v.is_null() { None } else { Some(v) };
        }
    });
    let hub = h.clone();
    app.listen("recording_nudge", move |ev| {
        let mut v: Value = serde_json::from_str(ev.payload()).unwrap_or(Value::Null);
        if let Some(o) = v.as_object_mut() {
            o.insert("at".into(), Value::from(chrono::Utc::now().timestamp()));
        }
        if let Ok(mut n) = hub.nudge.lock() {
            *n = if v.is_null() { None } else { Some(v) };
        }
    });
    let hub = h.clone();
    app.listen("insights_updated", move |_| {
        hub.insights_dirty.store(true, Ordering::Relaxed);
    });
    let hub = h;
    app.listen("meeting_saved", move |_| {
        hub.insights_dirty.store(true, Ordering::Relaxed);
        if let Ok(mut n) = hub.nudge.lock() {
            *n = None;
        }
    });
}

fn parse_questions(raw: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|q| {
            q.get("question")
                .and_then(|s| s.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
        })
        .collect()
}

/// Questions + summary for the live meeting, re-read from the database only
/// when the insights engine reported a change (or the meeting changed).
fn insights_for(app: &AppHandle, hub: &Hub, meeting_id: Option<&str>) -> (Vec<Value>, String) {
    let Ok(mut cache) = hub.insights.lock() else {
        return (Vec::new(), String::new());
    };
    let meeting_changed = cache.meeting_id.as_deref() != meeting_id;
    if !meeting_changed && !hub.insights_dirty.swap(false, Ordering::Relaxed) {
        return (cache.questions.clone(), cache.summary.clone());
    }
    hub.insights_dirty.store(false, Ordering::Relaxed);
    cache.meeting_id = meeting_id.map(str::to_string);
    cache.questions.clear();
    cache.summary.clear();
    if let Some(id) = meeting_id {
        let state = app.state::<AppState>();
        let row = state
            .db
            .lock()
            .ok()
            .and_then(|conn| crate::db::get_meeting(&conn, id).ok().flatten());
        if let Some(m) = row {
            cache.questions = parse_questions(&m.suggested_questions);
            cache.summary = m.summary;
        }
    }
    (cache.questions.clone(), cache.summary.clone())
}

/// Build the snapshot from live state. `presence` comes from the caller so the
/// ticker, the tray and the surface all describe the same second.
pub fn build(app: &AppHandle, presence: RecordingPresence) -> Snapshot {
    let hub = hub(app);
    let (questions, summary) = match &hub {
        Some(h) if presence.is_recording => insights_for(app, h, presence.meeting_id.as_deref()),
        Some(h) => insights_for(app, h, None),
        None => (Vec::new(), String::new()),
    };
    let prompt = hub
        .as_ref()
        .and_then(|h| h.prompt.lock().ok().and_then(|p| p.clone()));
    let nudge = hub
        .as_ref()
        .and_then(|h| h.nudge.lock().ok().and_then(|n| n.clone()));
    Snapshot {
        schema: super::protocol::SCHEMA,
        app_version: crate::state::APP_VERSION,
        pid: std::process::id(),
        updated_at: chrono::Utc::now().timestamp(),
        presence,
        questions,
        summary,
        prompt,
        nudge,
    }
}

/// Fresh snapshot for one-shot readers (`status`, D-Bus properties).
pub fn current(app: &AppHandle) -> Snapshot {
    build(app, crate::state::build_presence(app))
}

/// Publish if anything changed: rewrite `state.json` atomically, feed
/// subscribers and the D-Bus signal. Returns true when a change went out.
pub fn publish(app: &AppHandle, presence: RecordingPresence) -> bool {
    let Some(hub) = hub(app) else { return false };
    let was_recording = hub
        .last_json
        .lock()
        .ok()
        .and_then(|j| j.as_deref().map(|j| j.contains("\"is_recording\":true")))
        .unwrap_or(false);
    let snap = build(app, presence);
    let key = snap.change_key();
    {
        let Ok(mut last) = hub.last_key.lock() else { return false };
        if last.as_deref() == Some(key.as_str()) {
            return false;
        }
        *last = Some(key);
    }
    let json = serde_json::to_string(&snap).unwrap_or_default();
    if let Ok(mut l) = hub.last_json.lock() {
        *l = Some(json.clone());
    }
    write_state_file(&json);
    let _ = hub.tx.send(json.clone());
    let recording_changed = was_recording != snap.presence.is_recording;
    if let Some(conn) = hub.dbus.lock().ok().and_then(|c| c.clone()) {
        tauri::async_runtime::spawn(async move {
            super::dbus::broadcast(&conn, &json, recording_changed).await;
        });
    }
    true
}

/// Atomic replace so a reader never sees a half-written file.
fn write_state_file(json: &str) {
    let Ok(dir) = super::ensure_runtime_dir() else { return };
    let path = dir.join(super::STATE_FILE);
    let tmp = dir.join(format!(".{}.{}", super::STATE_FILE, std::process::id()));
    let mut body = json.to_string();
    body.push('\n');
    if std::fs::write(&tmp, body).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Remove the runtime files on exit so nothing reports a dead instance.
pub fn cleanup() {
    let _ = std::fs::remove_file(super::state_path());
    let _ = std::fs::remove_file(super::socket_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn questions_keep_only_non_empty_entries() {
        let q = parse_questions(
            r#"[{"question":"Who owns the budget?","type":"clarify"},{"question":"  "},{"nope":1}]"#,
        );
        assert_eq!(q.len(), 1);
        assert_eq!(q[0]["question"], "Who owns the budget?");
        assert!(parse_questions("not json").is_empty());
    }

    #[test]
    fn change_key_ignores_the_clock() {
        let presence = RecordingPresence {
            is_recording: false,
            elapsed_seconds: 0.0,
            elapsed_text: "00:00".into(),
            meeting_id: None,
            meeting_title: String::new(),
            lifecycle_status: None,
            transcription_status: "Transcription: off".into(),
            audio_status: "Audio: healthy".into(),
            audio_health: crate::state::AudioHealth::Healthy,
            stream_state: None,
            grace_remaining_seconds: None,
            grace_app_name: None,
            call_app_name: None,
        };
        let a = Snapshot {
            schema: 1,
            app_version: "x",
            pid: 1,
            updated_at: 1,
            presence: presence.clone(),
            questions: vec![],
            summary: String::new(),
            prompt: None,
            nudge: None,
        };
        let mut b = a.clone();
        b.updated_at = 2;
        assert_eq!(a.change_key(), b.change_key());
        b.summary = "x".into();
        assert_ne!(a.change_key(), b.change_key());
        let v: Value = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(v["is_recording"], false, "presence is flattened into the root");
        assert_eq!(v["schema"], 1);
    }
}
