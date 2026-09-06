//! Outbound webhook (PLAN.md §11). Fire-and-forget POST on `meeting.saved` /
//! `meeting.updated` with a 10 s timeout. The payload is a field-for-field port
//! of the Apple `WebhookService.MeetingPayload` (snake_case keys, `meeting`
//! envelope) so existing receivers work unchanged.

use std::collections::HashMap;
use std::time::Duration;

use serde::Serialize;

use crate::coaching::{self, TrainingMetrics};
use crate::db::{Meeting, TranscriptSegment};

pub const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    pub event: String,
    pub meeting: MeetingData,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingData {
    pub id: String,
    pub title: String,
    /// ISO 8601 start.
    pub date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    pub duration_seconds: i64,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub action_items: Vec<String>,
    pub key_decisions: Vec<String>,
    pub topics: Vec<String>,
    pub discussion_flow: Vec<String>,
    pub notes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meddpicc: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training: Option<TrainingData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub questions: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docs: Option<Vec<serde_json::Value>>,
    pub speaker_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_names: Option<HashMap<String, String>>,
    pub transcript: Vec<TranscriptEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_edited_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_revision: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insights_stale: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendar_event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attendees: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptEntry {
    /// Resolved display label ("You", a name, or "Speaker N").
    pub speaker: String,
    pub text: String,
    /// Seconds from meeting start.
    pub timestamp: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrainingData {
    pub talk_ratio_you: f64,
    pub duration_minutes: f64,
    pub speakers: Vec<TrainingSpeaker>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrainingSpeaker {
    pub speaker: String,
    pub is_you: bool,
    pub word_count: usize,
    pub words_per_minute: f64,
    pub fillers_per_minute: f64,
    pub total_fillers: usize,
    pub fillers: HashMap<String, usize>,
    pub longest_monologue_words: usize,
    pub questions_asked: usize,
    pub avg_words_per_turn: f64,
}

/// Port of `WebhookService.trainingData(from:)`.
pub fn training_data(metrics: &TrainingMetrics) -> TrainingData {
    TrainingData {
        talk_ratio_you: metrics.talk_ratio_you,
        duration_minutes: metrics.duration_minutes,
        speakers: metrics
            .speakers
            .iter()
            .map(|s| TrainingSpeaker {
                speaker: s.speaker_label.clone(),
                is_you: s.is_local_mic,
                word_count: s.word_count,
                words_per_minute: s.words_per_minute,
                fillers_per_minute: s.fillers_per_minute,
                total_fillers: s.total_fillers,
                fillers: s
                    .fillers
                    .iter()
                    .map(|f| (f.word.clone(), f.count))
                    .collect(),
                longest_monologue_words: s.longest_monologue_words,
                questions_asked: s.questions_asked,
                avg_words_per_turn: s.avg_words_per_turn,
            })
            .collect(),
    }
}

fn iso(ts: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

fn json_array_of_strings(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn json_array(raw: &str) -> Option<Vec<serde_json::Value>> {
    serde_json::from_str::<Vec<serde_json::Value>>(raw)
        .ok()
        .filter(|v| !v.is_empty())
}

fn meddpicc_or_none(raw: &str) -> Option<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let obj = v.as_object()?;
    let any_value = obj
        .values()
        .any(|x| x.as_str().map(|s| !s.trim().is_empty()).unwrap_or(false));
    any_value.then_some(v)
}

/// Build the payload from a persisted meeting (port of `payloadFromMeeting`),
/// with `event` = `meeting.saved` at stop or `meeting.updated` after insights.
pub fn payload_from_meeting(
    event: &str,
    meeting: &Meeting,
    segments: &[TranscriptSegment],
    fillers: &[String],
) -> WebhookPayload {
    let names: HashMap<String, String> =
        serde_json::from_str(&meeting.speaker_names).unwrap_or_default();
    let names_opt = if names.is_empty() { None } else { Some(&names) };
    let self_ids = meeting.self_speaker_ids();
    let self_slice = self_ids.as_deref();

    let mut finals: Vec<&TranscriptSegment> = segments.iter().collect();
    finals.sort_by(|a, b| {
        a.start_s
            .partial_cmp(&b.start_s)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let transcript: Vec<TranscriptEntry> = finals
        .iter()
        .map(|s| TranscriptEntry {
            speaker: coaching::resolved_speaker_label(s.speaker, names_opt, self_slice),
            text: s.text.clone(),
            timestamp: s.start_s,
        })
        .collect();
    let speaker_count = {
        let mut ids: Vec<i64> = finals.iter().map(|s| s.speaker).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    };

    let duration = match (meeting.started_at, meeting.ended_at) {
        (Some(s), Some(e)) => (e - s).max(0),
        _ => 0,
    };
    let coaching_segments: Vec<coaching::Segment> = finals
        .iter()
        .map(|s| coaching::Segment {
            text: s.text.clone(),
            speaker: s.speaker,
            is_final: true,
            timestamp: s.start_s,
        })
        .collect();
    let metrics = coaching::compute(
        &coaching_segments,
        duration as f64,
        fillers,
        names_opt,
        self_slice,
    );

    WebhookPayload {
        event: event.to_string(),
        meeting: MeetingData {
            id: meeting.id.clone(),
            title: meeting.display_title(),
            date: meeting
                .started_at
                .map(iso)
                .unwrap_or_else(|| iso(meeting.created_at)),
            end_time: meeting.ended_at.map(iso),
            duration_seconds: duration,
            language: meeting.language.clone(),
            summary: Some(meeting.summary.clone()).filter(|s| !s.trim().is_empty()),
            action_items: json_array_of_strings(&meeting.action_items),
            key_decisions: json_array_of_strings(&meeting.key_decisions),
            topics: json_array_of_strings(&meeting.topics),
            discussion_flow: json_array_of_strings(&meeting.discussion_flow),
            notes: meeting.notes.clone(),
            meddpicc: meddpicc_or_none(&meeting.meddpicc),
            training: Some(training_data(&metrics)),
            questions: json_array(&meeting.suggested_questions),
            docs: json_array(&meeting.docs),
            speaker_count,
            speaker_names: names_opt.cloned(),
            transcript,
            transcript_edited_at: None,
            transcript_revision: None,
            insights_stale: Some(false),
            calendar_event_id: meeting.calendar_event_id.clone(),
            attendees: json_array(&meeting.attendees),
        },
    }
}

/// Send the payload, ignoring the result beyond logging (fire-and-forget).
/// Any transport error is swallowed so a bad webhook never blocks a save.
pub async fn send(url: &str, payload: &WebhookPayload) {
    if url.trim().is_empty() || url::Url::parse(url).is_err() {
        return;
    }
    let client = match reqwest::Client::builder().timeout(WEBHOOK_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("webhook client build failed: {e}");
            return;
        }
    };
    match client.post(url).json(payload).send().await {
        Ok(resp) if resp.status().is_success() => {
            tracing::info!("webhook sent ({}): {}", resp.status(), payload.event)
        }
        Ok(resp) => tracing::warn!("webhook non-2xx ({}): {}", resp.status(), payload.event),
        Err(e) => tracing::warn!("webhook failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meeting() -> Meeting {
        let mut m = Meeting::new("Sync", "en");
        m.started_at = Some(1_000);
        m.ended_at = Some(1_600);
        m.summary = "did things".into();
        m.action_items = r#"["ship it"]"#.into();
        m.speaker_names = r#"{"0":"Alex"}"#.into();
        m.meddpicc = r#"{"metrics":"","champion":"Sam"}"#.into();
        m
    }

    #[test]
    fn payload_matches_apple_envelope_and_keys() {
        let m = meeting();
        let segs = vec![
            TranscriptSegment::new(&m.id, 1000, "um hello", 0.0, 1.0, "microphone"),
            TranscriptSegment::new(&m.id, 0, "hi there", 1.0, 2.0, "system"),
        ];
        let fillers: Vec<String> = coaching::default_fillers("en")
            .into_iter()
            .map(String::from)
            .collect();
        let json = serde_json::to_value(payload_from_meeting("meeting.saved", &m, &segs, &fillers))
            .unwrap();

        assert_eq!(json["event"], "meeting.saved");
        let mt = &json["meeting"];
        for key in [
            "id",
            "title",
            "date",
            "end_time",
            "duration_seconds",
            "language",
            "summary",
            "action_items",
            "key_decisions",
            "topics",
            "discussion_flow",
            "notes",
            "meddpicc",
            "training",
            "speaker_count",
            "speaker_names",
            "transcript",
            "insights_stale",
        ] {
            assert!(mt.get(key).is_some(), "missing key {key}");
        }
        assert_eq!(mt["date"], "1970-01-01T00:16:40Z");
        assert_eq!(mt["duration_seconds"], 600);
        assert_eq!(mt["action_items"][0], "ship it");
        assert_eq!(mt["speaker_count"], 2);
        assert_eq!(mt["transcript"][0]["speaker"], "You");
        assert_eq!(mt["transcript"][1]["speaker"], "Alex");
        assert_eq!(mt["transcript"][0]["timestamp"], 0.0);
        assert!(
            mt["transcript"][0].get("source").is_none(),
            "Apple entries carry no source"
        );
        assert_eq!(mt["meddpicc"]["champion"], "Sam");
        assert_eq!(mt["training"]["speakers"][0]["speaker"], "You");
        assert_eq!(mt["training"]["speakers"][0]["is_you"], true);
        assert_eq!(mt["training"]["speakers"][0]["total_fillers"], 1);
        assert!(
            mt.get("questions").is_none(),
            "empty arrays are omitted like Apple's nil"
        );
        assert!(mt.get("attendees").is_none());
        assert!(mt.get("calendar_event_id").is_none());
    }

    #[test]
    fn empty_summary_and_meddpicc_are_omitted() {
        let mut m = meeting();
        m.summary = String::new();
        m.meddpicc = r#"{"metrics":"","champion":""}"#.into();
        let json =
            serde_json::to_value(payload_from_meeting("meeting.updated", &m, &[], &[])).unwrap();
        assert!(json["meeting"].get("summary").is_none());
        assert!(json["meeting"].get("meddpicc").is_none());
        assert_eq!(json["meeting"]["speaker_count"], 0);
    }
}
