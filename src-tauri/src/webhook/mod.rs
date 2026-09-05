//! Outbound webhook (PLAN.md §11). Fire-and-forget POST on meeting save/update
//! with a 10 s timeout; payload shape mirrors the Apple `WebhookService`.

use std::time::Duration;

use serde::Serialize;

pub const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize)]
pub struct WebhookTranscriptSegment {
    pub speaker: i64,
    pub text: String,
    pub start: f64,
    pub end: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    pub event: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    pub duration_seconds: f64,
    pub language: String,
    pub summary: String,
    /// Insight blobs kept as raw JSON so the shape matches macOS exactly.
    pub action_items: serde_json::Value,
    pub key_decisions: serde_json::Value,
    pub suggested_questions: serde_json::Value,
    pub speaker_names: serde_json::Value,
    /// Local coaching metrics blob.
    pub training: serde_json::Value,
    pub transcript: Vec<WebhookTranscriptSegment>,
}

/// Send the payload, ignoring the result beyond logging (fire-and-forget).
/// Any transport error is swallowed so a bad webhook never blocks a save.
pub async fn send(url: &str, payload: &WebhookPayload) {
    let client = match reqwest::Client::builder().timeout(WEBHOOK_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("webhook client build failed: {e}");
            return;
        }
    };
    match client.post(url).json(payload).send().await {
        Ok(resp) => tracing::info!("webhook posted: {}", resp.status()),
        Err(e) => tracing::warn!("webhook post failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_serializes_with_expected_keys() {
        let payload = WebhookPayload {
            event: "meeting.saved".into(),
            title: "Sync".into(),
            started_at: Some(1000),
            ended_at: Some(2000),
            duration_seconds: 1000.0,
            language: "en".into(),
            summary: "did things".into(),
            action_items: serde_json::json!(["ship it"]),
            key_decisions: serde_json::json!([]),
            suggested_questions: serde_json::json!([]),
            speaker_names: serde_json::json!({"1000": "Me"}),
            training: serde_json::json!({"talk_ratio": 0.5}),
            transcript: vec![WebhookTranscriptSegment {
                speaker: 1000,
                text: "hello".into(),
                start: 0.0,
                end: 1.0,
                source: Some("microphone".into()),
            }],
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["event"], "meeting.saved");
        assert_eq!(json["transcript"][0]["speaker"], 1000);
        assert_eq!(json["transcript"][0]["source"], "microphone");
        assert_eq!(json["training"]["talk_ratio"], 0.5);
    }

    #[test]
    fn optional_source_is_omitted_when_absent() {
        let seg = WebhookTranscriptSegment {
            speaker: 0,
            text: "x".into(),
            start: 0.0,
            end: 0.1,
            source: None,
        };
        let json = serde_json::to_value(&seg).unwrap();
        assert!(json.get("source").is_none());
    }
}
