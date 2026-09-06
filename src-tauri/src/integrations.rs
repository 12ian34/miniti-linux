//! Google Calendar, Attio and Twenty integrations. All OAuth and third-party
//! traffic goes through the Miniti backend (contract in
//! `../miniti-api/docs/agents/04-api-reference.md`); the app opens the
//! authorize URL in the browser and receives the return via a deep link
//! (`miniti-google://oauth-callback?status=success`, etc.).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::{endpoint_url, ApiClient, ApiError};
use crate::db::{Meeting, TranscriptSegment};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrmProvider {
    Attio,
    Twenty,
}

impl CrmProvider {
    pub fn path(self) -> &'static str {
        match self {
            CrmProvider::Attio => "attio",
            CrmProvider::Twenty => "twenty",
        }
    }
    pub fn scheme(self) -> &'static str {
        match self {
            CrmProvider::Attio => "miniti-attio",
            CrmProvider::Twenty => "miniti-twenty",
        }
    }
    pub fn objects(self) -> &'static [&'static str] {
        match self {
            CrmProvider::Attio => &["people", "companies"],
            CrmProvider::Twenty => &["people", "companies", "opportunities"],
        }
    }
}

pub const GOOGLE_SCHEME: &str = "miniti-google";

// ---- Response models -----------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ConnectStart {
    pub auth_url: String,
    #[serde(default)]
    pub callback_scheme: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct GoogleStatus {
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CrmStatus {
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub account_label: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
pub struct CalendarAttendee {
    #[serde(default)]
    pub email: String,
    #[serde(default, rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(default, rename = "responseStatus")]
    pub response_status: Option<String>,
    #[serde(default)]
    pub organizer: bool,
    #[serde(default, rename = "self")]
    pub is_self: bool,
    #[serde(default)]
    pub domain: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
pub struct CalendarEvent {
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// ISO 8601.
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub end: String,
    #[serde(default, rename = "isAllDay")]
    pub is_all_day: bool,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, rename = "meetLink")]
    pub meet_link: Option<String>,
    #[serde(default, rename = "conferenceUrl")]
    pub conference_url: Option<String>,
    #[serde(default)]
    pub attendees: Vec<CalendarAttendee>,
}

impl CalendarEvent {
    pub fn start_ts(&self) -> Option<i64> {
        crate::api::parse_iso8601(&self.start)
    }
    pub fn end_ts(&self) -> Option<i64> {
        crate::api::parse_iso8601(&self.end)
    }
    /// Attendees as the insights `{name, domain, role}` + email shape stored on the meeting.
    pub fn attendees_json(&self) -> String {
        let list: Vec<Value> = self
            .attendees
            .iter()
            .filter(|a| !a.email.is_empty())
            .map(|a| {
                json!({
                    "email": a.email,
                    "name": a.display_name,
                    "domain": a.domain.clone().unwrap_or_else(|| a.email.split('@').nth(1).unwrap_or_default().to_string()),
                    "is_organizer": a.organizer,
                    "is_self": a.is_self,
                })
            })
            .collect();
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CalendarEvents {
    #[serde(default)]
    pub events: Vec<CalendarEvent>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CrmRecordId {
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub object_id: String,
    #[serde(default)]
    pub record_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CrmRecord {
    pub id: CrmRecordId,
    #[serde(default)]
    pub record_text: String,
    #[serde(default)]
    pub record_image: Option<String>,
    #[serde(default)]
    pub object_slug: String,
    #[serde(default)]
    pub record_email: Option<String>,
    #[serde(default)]
    pub record_domain: Option<String>,
    #[serde(default)]
    pub record_detail: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CrmSearch {
    #[serde(default)]
    pub data: Vec<CrmRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CrmTask {
    pub content: String,
    /// `YYYY-MM-DD`, Attio applies it; Twenty ignores deadlines today.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<String>,
}

// ---- Calendar cache -----------------------------------------------------------

#[derive(Default)]
pub struct CalendarState {
    pub connected: bool,
    pub email: Option<String>,
    pub events: Vec<CalendarEvent>,
    pub fetched_at: Option<Instant>,
    pub last_error: Option<String>,
}

pub type CalendarSlot = Mutex<CalendarState>;

/// Next non-all-day events, soonest first (upcoming list shows five).
pub fn upcoming(events: &[CalendarEvent], now: i64, limit: usize) -> Vec<CalendarEvent> {
    let mut list: Vec<CalendarEvent> = events
        .iter()
        .filter(|e| !e.is_all_day && e.status.as_deref() != Some("cancelled"))
        .filter(|e| e.end_ts().map(|end| end > now).unwrap_or(false))
        .cloned()
        .collect();
    list.sort_by_key(|e| e.start_ts().unwrap_or(i64::MAX));
    list.truncate(limit);
    list
}

/// Port of `unambiguousCurrentCalendarEvent`: exactly one event whose window
/// (start − 5 min, end) contains `now`.
pub fn unambiguous_current(events: &[CalendarEvent], now: i64) -> Option<CalendarEvent> {
    let candidates: Vec<&CalendarEvent> = events
        .iter()
        .filter(|e| !e.is_all_day)
        .filter(|e| match (e.start_ts(), e.end_ts()) {
            (Some(s), Some(en)) => s - 300 <= now && now < en,
            _ => false,
        })
        .collect();
    if candidates.len() == 1 { Some(candidates[0].clone()) } else { None }
}

// ---- Backend calls -----------------------------------------------------------

impl ApiClient {
    fn base(&self) -> &str {
        // `endpoint_url` needs the base; expose via context-free accessor.
        self.base_url()
    }

    pub async fn google_connect_start(&self) -> Result<ConnectStart, ApiError> {
        self.post_json(&endpoint_url(self.base(), "api/google/connect/start"), &json!({ "callback_scheme": GOOGLE_SCHEME })).await
    }
    pub async fn google_status(&self) -> Result<GoogleStatus, ApiError> {
        self.get_json(&endpoint_url(self.base(), "api/google/status")).await
    }
    pub async fn google_disconnect(&self) -> Result<Value, ApiError> {
        self.post_json(&endpoint_url(self.base(), "api/google/disconnect"), &json!({})).await
    }
    pub async fn google_events(&self, time_min: &str, time_max: &str, max_results: u32) -> Result<CalendarEvents, ApiError> {
        let url = format!(
            "{}?time_min={}&time_max={}&max_results={}",
            endpoint_url(self.base(), "api/google/events"),
            urlencoding(time_min),
            urlencoding(time_max),
            max_results
        );
        self.get_json(&url).await
    }
    pub async fn crm_connect_start(&self, provider: CrmProvider) -> Result<ConnectStart, ApiError> {
        self.post_json(&endpoint_url(self.base(), &format!("api/{}/connect/start", provider.path())), &json!({ "callback_scheme": provider.scheme() })).await
    }
    pub async fn crm_status(&self, provider: CrmProvider) -> Result<CrmStatus, ApiError> {
        self.get_json(&endpoint_url(self.base(), &format!("api/{}/status", provider.path()))).await
    }
    pub async fn crm_search(&self, provider: CrmProvider, query: &str, objects: &[&str]) -> Result<CrmSearch, ApiError> {
        self.post_json(&endpoint_url(self.base(), &format!("api/{}/search", provider.path())), &json!({ "query": query, "objects": objects })).await
    }
    pub async fn crm_send(&self, provider: CrmProvider, body: &Value) -> Result<Value, ApiError> {
        self.post_json(&endpoint_url(self.base(), &format!("api/{}/send", provider.path())), body).await
    }
}

fn urlencoding(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---- CRM meeting payload (port of `AttioMeetingPayload`) ----------------------

fn iso(ts: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

pub fn formatted_duration(seconds: i64) -> String {
    let s = seconds.max(0);
    if s >= 3600 {
        format!("{}h {}m", s / 3600, (s % 3600) / 60)
    } else {
        format!("{}m {}s", s / 60, s % 60)
    }
}

/// Strip checkbox / bullet prefixes and blanks (port of `normalizedActionItems`).
pub fn normalized_action_items(items: &[String]) -> Vec<String> {
    items
        .iter()
        .map(|i| {
            let mut t = i.trim();
            for p in ["- [ ]", "- [x]", "- [X]", "[ ]", "[x]", "[X]", "-", "•", "*", "☐", "☑"] {
                if let Some(rest) = t.strip_prefix(p) {
                    t = rest.trim();
                    break;
                }
            }
            t.to_string()
        })
        .filter(|t| !t.is_empty())
        .collect()
}

pub fn crm_meeting_payload(meeting: &Meeting, segments: &[TranscriptSegment]) -> Value {
    let arr = |raw: &str| serde_json::from_str::<Vec<String>>(raw).unwrap_or_default();
    let med: HashMap<String, Value> = serde_json::from_str(&meeting.meddpicc).unwrap_or_default();
    let meddpicc: serde_json::Map<String, Value> = med
        .into_iter()
        .filter_map(|(k, v)| v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(|s| (k, Value::String(s.to_string()))))
        .collect();
    let start = meeting.started_at.unwrap_or(meeting.created_at);
    let mut payload = json!({
        "title": meeting.display_title(),
        "started_at": iso(start),
        "duration_text": formatted_duration(meeting.duration_seconds()),
        "discussion_flow": arr(&meeting.discussion_flow),
        "action_items": normalized_action_items(&arr(&meeting.action_items)),
        "key_decisions": arr(&meeting.key_decisions),
        "topics": arr(&meeting.topics),
        "meddpicc": Value::Object(meddpicc),
        "insights_stale": false,
    });
    if let Some(e) = meeting.ended_at {
        payload["ended_at"] = Value::String(iso(e));
    }
    if !meeting.summary.trim().is_empty() {
        payload["summary"] = Value::String(meeting.summary.trim().to_string());
    }
    if !meeting.notes.trim().is_empty() {
        payload["notes"] = Value::String(meeting.notes.trim().to_string());
    }
    // Transcript is deliberately omitted (backend ignores it; macOS no longer sends it).
    let _ = segments;
    payload
}

/// Default task preview: one task per action item, deadline today (Attio).
pub fn default_tasks(meeting: &Meeting, today: &str) -> Vec<CrmTask> {
    let items: Vec<String> = serde_json::from_str(&meeting.action_items).unwrap_or_default();
    normalized_action_items(&items)
        .into_iter()
        .map(|content| CrmTask { content, deadline_at: Some(today.to_string()) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: &str, start: i64, end: i64) -> CalendarEvent {
        CalendarEvent { id: id.into(), title: id.into(), start: iso(start), end: iso(end), ..Default::default() }
    }

    #[test]
    fn upcoming_filters_and_sorts() {
        let now = 1_700_000_000;
        let events = vec![
            ev("past", now - 7200, now - 3600),
            ev("later", now + 7200, now + 9000),
            ev("soon", now + 600, now + 1800),
            CalendarEvent { is_all_day: true, ..ev("allday", now, now + 86400) },
            CalendarEvent { status: Some("cancelled".into()), ..ev("cancelled", now + 60, now + 600) },
        ];
        let up = upcoming(&events, now, 5);
        assert_eq!(up.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["soon", "later"]);
        assert_eq!(upcoming(&events, now, 1).len(), 1);
    }

    #[test]
    fn unambiguous_current_event() {
        let now = 1_700_000_000;
        assert!(unambiguous_current(&[ev("a", now - 60, now + 600)], now).is_some());
        assert!(unambiguous_current(&[ev("a", now + 240, now + 600)], now).is_some(), "5-minute lead-in counts");
        assert!(unambiguous_current(&[ev("a", now - 60, now + 600), ev("b", now - 30, now + 900)], now).is_none(), "overlap is ambiguous");
        assert!(unambiguous_current(&[ev("a", now + 900, now + 1200)], now).is_none());
    }

    #[test]
    fn calendar_event_decodes_backend_shape() {
        let e: CalendarEvent = serde_json::from_value(json!({
            "id": "x", "title": "Sync", "start": "2026-04-02T10:00:00.000Z", "end": "2026-04-02T11:00:00.000Z",
            "isAllDay": false, "status": "confirmed", "meetLink": "https://meet.google.com/abc",
            "attendees": [{"email": "a@acme.com", "displayName": "Alice", "responseStatus": "accepted", "organizer": false, "self": true, "domain": "acme.com"}]
        })).unwrap();
        assert_eq!(e.attendees[0].display_name.as_deref(), Some("Alice"));
        assert!(e.attendees[0].is_self);
        assert!(e.start_ts().is_some());
        let a: Vec<Value> = serde_json::from_str(&e.attendees_json()).unwrap();
        assert_eq!(a[0]["domain"], "acme.com");
    }

    #[test]
    fn crm_payload_matches_swift_dictionary() {
        let mut m = Meeting::new("Deal review", "en");
        m.started_at = Some(1_700_000_000);
        m.ended_at = Some(1_700_003_700);
        m.summary = " Went well ".into();
        m.action_items = r#"["- [ ] Send proposal", "• Book demo", "   "]"#.into();
        m.meddpicc = r#"{"champion":"Sam","metrics":"  "}"#.into();
        let p = crm_meeting_payload(&m, &[]);
        assert_eq!(p["title"], "Deal review");
        assert_eq!(p["started_at"], "2023-11-14T22:13:20Z");
        assert_eq!(p["duration_text"], "1h 1m");
        assert_eq!(p["summary"], "Went well");
        assert_eq!(p["action_items"], json!(["Send proposal", "Book demo"]));
        assert_eq!(p["meddpicc"], json!({"champion": "Sam"}));
        assert!(p.get("notes").is_none());
        assert!(p.get("transcript").is_none());
        assert_eq!(p["insights_stale"], false);
        let tasks = default_tasks(&m, "2026-09-06");
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].deadline_at.as_deref(), Some("2026-09-06"));
    }

    #[test]
    fn provider_paths_and_objects() {
        assert_eq!(CrmProvider::Attio.objects(), &["people", "companies"]);
        assert_eq!(CrmProvider::Twenty.objects().len(), 3);
        assert_eq!(CrmProvider::Twenty.scheme(), "miniti-twenty");
        assert_eq!(formatted_duration(125), "2m 5s");
    }
}
